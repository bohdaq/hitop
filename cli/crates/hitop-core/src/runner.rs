#![cfg(feature = "native")]
use std::collections::HashMap;
use std::time::{Duration, Instant};
use anyhow::Result;
use serde_json::Value;

use crate::{
    cookies::Jar,
    form,
    http,
    model::{Collection, Environment, RequestResult, TestResult},
    prepare::{prepare_in, variables, Draft, Scopes},
    scripting::{run_post_request_script_with, PostRequestCtx, SentRequest, VariableUpdates},
    secrets::{Masker, Secrets},
};

#[derive(Debug)]
pub struct RunStepResult {
    pub request_name: String,
    pub method: String,
    pub url: String,
    pub result: Result<RequestResult>,
    /// The post-request script's `assert.*` results.
    pub tests: Vec<TestResult>,
    /// The whole step: scripts, request and response, in milliseconds.
    /// The response time alone is `result`'s `duration_ms`.
    pub duration_ms: u64,
}

/// How [`run_collection_opts`] runs.
#[derive(Debug, Clone, Default)]
pub struct RunOptions {
    /// Stop after the first request that fails.
    pub stop_on_fail: bool,
    /// Time limit for each request.
    pub timeout: Option<Duration>,
    /// Variables that override the collection's and the environment's
    /// (a data row, `--var`); secrets still override these.
    pub overrides: HashMap<String, String>,
    /// The globals scope, below the collection's variables, with its secret
    /// values filled in (see `global_secrets`).
    pub globals: HashMap<String, String>,
    /// The globals' resolved secrets: already in `globals`, here so every
    /// step masks them too.
    pub global_secrets: Secrets,
}

impl RunStepResult {
    /// Sent, answered below 400, and every assertion passed.
    pub fn passed(&self) -> bool {
        self.result.as_ref().map(|r| r.status < 400).unwrap_or(false) && self.tests.iter().all(|t| t.pass)
    }

    /// Replaces secret values everywhere in the step: URL, error, response
    /// headers and body, assertion names and details.
    pub fn mask(&mut self, m: &Masker) {
        if m.is_empty() {
            return;
        }
        self.url = m.mask(&self.url);
        match &mut self.result {
            Ok(r) => {
                r.body = m.mask(&r.body);
                for v in r.headers.values_mut() {
                    *v = m.mask(v);
                }
            }
            Err(e) => *e = anyhow::anyhow!(m.mask(&format!("{e:#}"))),
        }
        for t in &mut self.tests {
            t.name = m.mask(&t.name);
            t.detail = m.mask(&t.detail);
        }
    }
}

/// Whether a run sends this request (one step per such request, in collection order).
/// WebSocket and gRPC requests are made on demand (`hitop ws`, `hitop grpc`): a run
/// has nothing to send for them. Anything mapping steps back to requests filters by this.
pub fn is_run_step(request: &crate::model::Request) -> bool {
    !request.is_websocket() && !request.is_grpc()
}

/// Runs the requests in order, passing script context from one to the next.
/// `environment` overrides collection variables.
pub fn run_collection(
    collection: &Collection,
    environment: Option<&Environment>,
    stop_on_fail: bool,
    on_step: &mut dyn FnMut(&RunStepResult),
) -> Vec<RunStepResult> {
    run_collection_with(collection, environment, &Secrets::default(), stop_on_fail, on_step)
}

/// [`run_collection`] with resolved secrets: they override the variables,
/// and every step is masked before `on_step` sees it or it is returned, so
/// what reports and logs get never holds a secret value.
pub fn run_collection_with(
    collection: &Collection,
    environment: Option<&Environment>,
    secrets: &Secrets,
    stop_on_fail: bool,
    on_step: &mut dyn FnMut(&RunStepResult),
) -> Vec<RunStepResult> {
    let options = RunOptions { stop_on_fail, ..Default::default() };
    run_collection_opts(collection, environment, secrets, &options, on_step)
}

/// [`run_collection_with`] with a per-request timeout and variable
/// overrides. Script context starts empty on every call.
pub fn run_collection_opts(
    collection: &Collection,
    environment: Option<&Environment>,
    secrets: &Secrets,
    options: &RunOptions,
    on_step: &mut dyn FnMut(&RunStepResult),
) -> Vec<RunStepResult> {
    run_collection_jar(collection, environment, secrets, options, None, on_step)
}

/// What one request of a run hands to the next: the scripts' context, every variable scope with
/// the values scripts set applied, and the OAuth 2.0 tokens fetched so far. [`run_collection_jar`]
/// keeps one for the whole run; an app sending one request at a time ([`run_step`]) keeps the
/// script-set values ([`RunState::script_variables`], [`RunState::script_globals`]) itself.
pub struct RunState {
    pub context: HashMap<String, Value>,
    pub scopes: Scopes,
    /// Every variable the scripts of this state's steps set (`None` = unset; the last value wins).
    pub script_variables: VariableUpdates,
    /// The same for globals (`pm.globals.set`).
    pub script_globals: VariableUpdates,
    /// The request as the last step sent it (after scripts, auth, form bodies and signing).
    pub last_sent: Option<SentRequest>,
    tokens: crate::oauth2::Tokens,
}

impl RunState {
    /// The scopes a run starts with: collection variables, the environment, `overrides`, then
    /// secrets (references filled in); globals below them all.
    pub fn new(collection: &Collection, environment: Option<&Environment>, secrets: &Secrets, options: &RunOptions) -> Self {
        let mut upper = variables(&collection.variables, environment);
        upper.extend(options.overrides.iter().map(|(k, v)| (k.clone(), v.clone())));
        secrets.apply(&mut upper);
        // `token: ${secret:vault:...}` in a variable's value: the resolved
        // reference (in `secrets`) takes its place.
        secrets.expand_refs(&mut upper);
        let mut globals = options.globals.clone();
        secrets.expand_refs(&mut globals);
        options.global_secrets.expand_refs(&mut globals);
        RunState {
            context: HashMap::new(),
            scopes: Scopes { globals, upper },
            script_variables: VariableUpdates::new(),
            script_globals: VariableUpdates::new(),
            last_sent: None,
            tokens: crate::oauth2::Tokens::interactive(options.timeout),
        }
    }

    fn apply(&mut self, variables: &VariableUpdates, globals: &VariableUpdates) {
        self.scopes.apply(variables, globals);
        self.script_variables.extend(variables.iter().map(|(k, v)| (k.clone(), v.clone())));
        self.script_globals.extend(globals.iter().map(|(k, v)| (k.clone(), v.clone())));
    }
}

/// [`run_collection_opts`] with a cookie jar: cookies set by one response are
/// sent with the next requests (and stay in `jar` for the caller to save).
pub fn run_collection_jar(
    collection: &Collection,
    environment: Option<&Environment>,
    secrets: &Secrets,
    options: &RunOptions,
    mut jar: Option<&mut Jar>,
    on_step: &mut dyn FnMut(&RunStepResult),
) -> Vec<RunStepResult> {
    let stop_on_fail = options.stop_on_fail;
    let mut state = RunState::new(collection, environment, secrets, options);
    let masker = secrets.masker_with(&options.global_secrets);
    let mut results = Vec::new();

    for saved in &collection.requests {
        if !is_run_step(saved) {
            continue;
        }
        let mut step = run_step(collection, saved, &mut state, options, jar.as_deref_mut());
        step.mask(&masker);
        let failed = !step.passed();
        on_step(&step);
        results.push(step);
        if failed && stop_on_fail {
            break;
        }
    }

    results
}

/// One request of `collection` as a run sends it: folder and collection auth and headers
/// inherited, OAuth 2.0 tokens, the pre-request script, the request, Digest's retry, the
/// post-request script and its assertions, with `state` carried in and out. The step is not
/// masked: mask it with the run's [`Masker`] before anything shows or keeps it.
pub fn run_step(collection: &Collection, saved: &crate::model::Request, state: &mut RunState, options: &RunOptions, mut jar: Option<&mut Jar>) -> RunStepResult {
    // Folder and collection auth and headers, inherited (model.rs `Collection::effective`).
    let inherited = collection.effective(saved);
    let request = &inherited;
    let settings = request.settings();
    let timeout = settings.timeout_ms.filter(|ms| *ms > 0).map(std::time::Duration::from_millis).or(options.timeout);
    let no_redirects = settings.follow_redirects == Some(false);
    let started = Instant::now();
    state.last_sent = None;
    // OAuth 2.0: a token (from the cache, or the token endpoint now) in place of the preset.
    let oauth = {
        let vars = state.scopes.merged();
        let now = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map_or(0, |d| d.as_secs());
        state.tokens.resolve(request.auth.as_ref().or(collection.auth.as_ref()), &|s| crate::interpolation::interpolate(s, &vars), now, &mut |r| {
            let res = http::execute_opts("POST", &r.url, &r.headers, r.body.clone().into_bytes(), http::Options { timeout: options.timeout, jar: None, ..Default::default() })?;
            Ok((res.status, res.body))
        })
    };
    let (oauth_auth, oauth_error) = match oauth {
        Ok(a) => (a, None),
        Err(e) => (None, Some(e)),
    };
    // A request's own auth (even `none`) wins over the collection's.
    let mut draft = Draft::of(request, collection.auth.as_ref());
    if let Some(a) = &oauth_auth {
        draft.auth = Some(a);
    }
    let prepared = match oauth_error {
        Some(e) => Err(e),
        None => prepare_in(&draft, &state.scopes, &state.context),
    };
    let mut step = match prepared {
        Err(e) => RunStepResult {
            request_name: request.name.clone(),
            method: request.method.clone(),
            url: request.url.clone(),
            result: Err(e),
            tests: vec![],
            duration_ms: 0,
        },
        Ok(pre) => {
            let mut sent_parts = (pre.headers.clone(), pre.body.clone().into_bytes());
            state.context.extend(pre.context_updates.clone());
            // Variables a script sets (pm.environment.set, ...) last for the run.
            state.apply(&pre.variables, &pre.globals);
            let variables = state.scopes.merged();
            let result = form::outgoing(request, &pre.headers, &pre.body, &variables).and_then(|(mut h, b)| {
                // AWS Signature v4 signs the request as it goes out.
                crate::aws_sigv4::sign_if_aws(request.auth.as_ref().or(collection.auth.as_ref()), &|s| crate::interpolation::interpolate(s, &variables), &request.method, &pre.url, &mut h, &b)?;
                sent_parts = (h.clone(), b.clone());
                let first = http::execute_opts(&request.method, &pre.url, &h, b.clone(), http::Options { timeout, jar: jar.as_deref_mut(), no_redirects, ..Default::default() })?;
                // Digest: the server's 401 challenge answered once.
                let fill = |s: &str| crate::interpolation::interpolate(s, &variables);
                match crate::digest_auth::retry_headers(request.auth.as_ref().or(collection.auth.as_ref()), &fill, first.status, &first.headers, &request.method, &pre.url, &h)? {
                    Some(h2) => {
                        sent_parts = (h2.clone(), b.clone());
                        http::execute_opts(&request.method, &pre.url, &h2, b, http::Options { timeout, jar, no_redirects, ..Default::default() })
                    }
                    None => Ok(first),
                }
            });
            let (headers, body) = sent_parts;
            let sent = SentRequest { url: pre.url.clone(), method: request.method.clone(), headers, body: String::from_utf8(body).unwrap_or_default() };
            let mut tests = vec![];
            if let Ok(resp) = &result {
                match run_post_request_script_with(
                    &request.post_request_script,
                    &PostRequestCtx {
                        response_body: &resp.body,
                        response_headers: &resp.headers,
                        status_code: resp.status,
                        duration_ms: resp.duration_ms,
                        variables: &variables,
                        context: &state.context,
                        request: Some(&sent),
                        globals: Some(&state.scopes.globals),
                    },
                ) {
                    Ok(post) => {
                        state.context.extend(post.context_updates);
                        state.apply(&post.variables, &post.globals);
                        tests = post.tests;
                    }
                    Err(e) => tests.push(TestResult { name: "post-request script".into(), pass: false, detail: e.to_string() }),
                }
            }
            state.last_sent = Some(sent);
            RunStepResult { request_name: request.name.clone(), method: request.method.clone(), url: pre.url, result, tests, duration_ms: 0 }
        }
    };
    step.duration_ms = started.elapsed().as_millis() as u64;
    step
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::{Header, Request};
    use crate::secrets::{resolve, Lookup, MemoryStore};

    #[test]
    fn steps_never_carry_a_secret_value() {
        // Port 9 on localhost refuses the connection, so the step fails
        // with an error that names the URL.
        let collection = Collection {
            requests: vec![Request {
                name: "r".into(),
                method: "GET".into(),
                url: "http://127.0.0.1:9/x?key=${apiKey}".into(),
                headers: vec![Header::new("Authorization", "Bearer ${apiKey}")],
                ..Default::default()
            }],
            ..Default::default()
        };
        let env = Environment { id: "prod".into(), name: "prod".into(), secrets: vec!["apiKey".into()], ..Default::default() };
        let store = MemoryStore::new();
        let lookup = |k: &str| (k == "HITOP_SECRET_PROD_APIKEY").then(|| "s3cr3t-key".to_string());
        let resolved = resolve(&Lookup { workspace: "w", environment: Some(&env), flags: &[], store: &store, lookup_env: &lookup, prompt: None }).unwrap();
        let mut seen = vec![];
        let results = run_collection_with(&collection, Some(&env), &resolved.secrets, false, &mut |s| {
            seen.push(format!("{} {:?}", s.url, s.result.as_ref().err().map(|e| e.to_string())));
        });
        assert_eq!(results[0].url, "http://127.0.0.1:9/x?key=••••");
        let all = format!("{seen:?} {:?}", results);
        assert!(!all.contains("s3cr3t"), "{all}");
    }

    #[test]
    fn secret_references_fill_requests_and_variable_values() {
        use crate::secrets::Origin;
        let collection = Collection {
            requests: vec![Request { name: "r".into(), method: "GET".into(), url: "http://127.0.0.1:9/${secret:vault:kv/app#id}/${token}/${g}".into(), ..Default::default() }],
            ..Default::default()
        };
        let env = Environment { id: "e".into(), name: "e".into(), variables: [("token".into(), "t-${secret:aws:api}".into())].into(), ..Default::default() };
        let mut secrets = Secrets::default();
        secrets.insert("secret:vault:kv/app#id", "vault-value".into(), Origin::SecretManager);
        secrets.insert("secret:aws:api", "aws-value".into(), Origin::SecretManager);
        secrets.insert("secret:op:v/i/f", "op-value".into(), Origin::SecretManager);
        let options = RunOptions { globals: [("g".into(), "${secret:op:v/i/f}".into())].into(), ..Default::default() };
        let mut sent = String::new();
        let results = run_collection_opts(&collection, Some(&env), &secrets, &options, &mut |s| sent = s.url.clone());
        assert_eq!(results[0].url, "http://127.0.0.1:9/••••/t-••••/••••");
        assert_eq!(sent, results[0].url);
        let all = format!("{results:?}");
        assert!(!all.contains("-value"), "{all}");
    }

    #[test]
    fn websocket_requests_are_skipped() {
        let collection = Collection {
            requests: vec![
                Request { name: "socket".into(), method: "GET".into(), url: "ws://127.0.0.1:9/".into(), kind: Some("websocket".into()), ..Default::default() },
                Request { name: "call".into(), method: "GET".into(), url: "127.0.0.1:9".into(), kind: Some("grpc".into()), ..Default::default() },
            ],
            ..Default::default()
        };
        assert!(run_collection(&collection, None, false, &mut |_| panic!("no step")).is_empty());
    }

    #[test]
    fn overrides_beat_the_environment() {
        let collection = Collection {
            variables: [("host".to_string(), "127.0.0.1:1".to_string())].into(),
            requests: vec![Request { name: "r".into(), method: "GET".into(), url: "http://${host}/${who}?k=${key}".into(), ..Default::default() }],
            ..Default::default()
        };
        let env = Environment { id: "e".into(), name: "e".into(), variables: [("host".into(), "127.0.0.1:2".into()), ("who".into(), "env".into())].into(), ..Default::default() };
        let options = RunOptions {
            overrides: [("host".to_string(), "127.0.0.1:9".to_string()), ("key".to_string(), "plain".to_string())].into(),
            timeout: Some(Duration::from_millis(500)),
            ..Default::default()
        };
        let results = run_collection_opts(&collection, Some(&env), &Secrets::default(), &options, &mut |_| {});
        assert_eq!(results[0].url, "http://127.0.0.1:9/env?k=plain");
        assert!(results[0].result.is_err());
    }
}
