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
    scripting::{run_post_request_script_with, PostRequestCtx, SentRequest},
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
    let mut context: HashMap<String, Value> = HashMap::new();
    let mut upper = variables(&collection.variables, environment);
    upper.extend(options.overrides.iter().map(|(k, v)| (k.clone(), v.clone())));
    secrets.apply(&mut upper);
    let mut scopes = Scopes { globals: options.globals.clone(), upper };
    let masker = secrets.masker_with(&options.global_secrets);
    let mut results = Vec::new();

    for request in &collection.requests {
        let started = Instant::now();
        let draft = Draft {
            method: &request.method,
            url: &request.url,
            headers: &request.headers,
            body: &request.body,
            pre_request_script: &request.pre_request_script,
            // A request's own auth (even `none`) wins over the collection's.
            auth: request.auth.as_ref().or(collection.auth.as_ref()),
        };
        let step = match prepare_in(&draft, &scopes, &context) {
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
                context.extend(pre.context_updates.clone());
                // Variables a script sets (pm.environment.set, ...) last for the run.
                scopes.apply(&pre.variables, &pre.globals);
                let variables = scopes.merged();
                let result = form::outgoing(request, &pre.headers, &pre.body, &variables).and_then(|(h, b)| {
                    sent_parts = (h.clone(), b.clone());
                    http::execute_opts(&request.method, &pre.url, &h, b, http::Options { timeout: options.timeout, jar: jar.as_deref_mut() })
                });
                let (headers, body) = sent_parts;
                let mut tests = vec![];
                if let Ok(resp) = &result {
                    let sent = SentRequest { url: pre.url.clone(), method: request.method.clone(), headers, body: String::from_utf8(body).unwrap_or_default() };
                    match run_post_request_script_with(
                        &request.post_request_script,
                        &PostRequestCtx {
                            response_body: &resp.body,
                            response_headers: &resp.headers,
                            status_code: resp.status,
                            duration_ms: resp.duration_ms,
                            variables: &variables,
                            context: &context,
                            request: Some(&sent),
                            globals: Some(&scopes.globals),
                        },
                    ) {
                        Ok(post) => {
                            context.extend(post.context_updates);
                            scopes.apply(&post.variables, &post.globals);
                            tests = post.tests;
                        }
                        Err(e) => tests.push(TestResult { name: "post-request script".into(), pass: false, detail: e.to_string() }),
                    }
                }
                RunStepResult { request_name: request.name.clone(), method: request.method.clone(), url: pre.url, result, tests, duration_ms: 0 }
            }
        };
        let mut step = step;
        step.duration_ms = started.elapsed().as_millis() as u64;
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
