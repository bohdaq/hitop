#![cfg(feature = "native")]
//! A request as it is sent: variables interpolated, auth applied, then the
//! pre-request script run (so a script can still override the auth). The URL
//! then gets a scheme if it has none (`sending::with_scheme`).
use std::collections::HashMap;

use anyhow::{bail, Result};
use serde_json::Value;

use crate::{
    auth,
    graphql::{self, Operation},
    interpolation::{fill, interpolate, interpolate_lenient},
    model::{Auth, Environment, Header, Request},
    scripting::{run_pre_request_script, PreRequestCtx, PreRequestResult, VariableUpdates},
    sending::with_scheme,
};

/// What a request needs to be prepared; borrowed from a saved `Request` or
/// built from command-line flags.
pub struct Draft<'a> {
    pub method: &'a str,
    pub url: &'a str,
    pub headers: &'a [Header],
    pub body: &'a str,
    pub pre_request_script: &'a str,
    pub auth: Option<&'a Auth>,
    /// A GraphQL request: sent as the JSON body built from it (`body` is
    /// then not used), with `Content-Type: application/json` unless a
    /// header sets one.
    pub graphql: Option<Operation<'a>>,
}

impl<'a> Draft<'a> {
    /// A saved request; `default_auth` (the collection's) applies when the
    /// request has no auth of its own (a request's `none` wins).
    pub fn of(request: &'a Request, default_auth: Option<&'a Auth>) -> Self {
        Draft {
            method: &request.method,
            url: &request.url,
            headers: &request.headers,
            body: &request.body,
            pre_request_script: &request.pre_request_script,
            auth: request.auth.as_ref().or(default_auth),
            graphql: request.graphql_operation(),
        }
    }
}

/// Collection variables, overridden by the environment's.
pub fn variables(collection: &HashMap<String, String>, environment: Option<&Environment>) -> HashMap<String, String> {
    let mut vars = collection.clone();
    if let Some(env) = environment {
        vars.extend(env.variables.iter().map(|(k, v)| (k.clone(), v.clone())));
    }
    vars
}

/// Variables in two layers: the globals scope, and everything above it
/// (collection, environment, data row, `--var`, script-set values, secrets).
/// A name in `upper` hides the global of that name.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Scopes {
    pub globals: HashMap<String, String>,
    pub upper: HashMap<String, String>,
}

impl Scopes {
    /// `upper` alone, no globals.
    pub fn plain(upper: HashMap<String, String>) -> Self {
        Scopes { globals: HashMap::new(), upper }
    }

    /// Every variable, the upper layer winning.
    pub fn merged(&self) -> HashMap<String, String> {
        let mut out = self.globals.clone();
        out.extend(self.upper.iter().map(|(k, v)| (k.clone(), v.clone())));
        out
    }

    /// A script's updates: `variables` go to the upper layer, `globals` to the globals.
    pub fn apply(&mut self, variables: &VariableUpdates, globals: &VariableUpdates) {
        apply_updates(&mut self.upper, variables);
        apply_updates(&mut self.globals, globals);
    }
}

/// [`prepare_in`] without globals.
pub fn prepare(draft: &Draft, variables: &HashMap<String, String>, context: &HashMap<String, Value>) -> Result<PreRequestResult> {
    prepare_in(draft, &Scopes::plain(variables.clone()), context)
}

pub fn prepare_in(draft: &Draft, scopes: &Scopes, context: &HashMap<String, Value>) -> Result<PreRequestResult> {
    let variables = &scopes.merged();
    // A `${name}` with no value yet is kept for the script: a Postman
    // pre-request script often sets the variable the same request uses.
    let missing = std::cell::RefCell::new(Vec::new());
    let lenient = |s: &str| -> Result<String> { Ok(interpolate_lenient(s, variables, &mut missing.borrow_mut())) };
    // GraphQL variables with a `${name}` still missing may not be JSON yet:
    // that waits for the script too.
    let first = build(draft, &lenient, &|| !missing.borrow().is_empty())?;
    let missing = missing.into_inner();
    let mut pre = run_pre_request_script(
        draft.pre_request_script,
        PreRequestCtx {
            url: first.0.clone(),
            method: draft.method.to_string(),
            headers: first.1.clone(),
            body: first.2.clone(),
            variables: variables.clone(),
            context: context.clone(),
            globals: scopes.globals.clone(),
        },
    )?;
    if pre.variables.is_empty() && pre.globals.is_empty() {
        if let Some(name) = missing.first() {
            bail!("variable '{name}' is not defined");
        }
        pre.url = with_scheme(&pre.url)?;
        return Ok(pre);
    }
    // The script set variables: what it left as built is built again with
    // them; what it changed only gets the missing ones filled in.
    let mut after = scopes.clone();
    after.apply(&pre.variables, &pre.globals);
    let vars = after.merged();
    let strict = |s: &str| interpolate(s, &vars);
    let (url, headers, body) = build(draft, &strict, &|| false)?;
    pre.url = if pre.url == first.0 { url } else { fill(&pre.url, &missing, &vars)? };
    pre.body = if pre.body == first.2 { body } else { fill(&pre.body, &missing, &vars)? };
    pre.headers = if pre.headers == first.1 {
        headers
    } else {
        pre.headers.iter().map(|(k, v)| Ok((fill(k, &missing, &vars)?, fill(v, &missing, &vars)?))).collect::<Result<_>>()?
    };
    pre.url = with_scheme(&pre.url)?;
    Ok(pre)
}

/// Applies a script's variable updates (`None` unsets).
pub fn apply_updates(variables: &mut HashMap<String, String>, updates: &VariableUpdates) {
    for (k, v) in updates {
        match v {
            Some(v) => variables.insert(k.clone(), v.clone()),
            None => variables.remove(k),
        };
    }
}

type Built = (String, Vec<(String, String)>, String);

/// URL, enabled headers and body interpolated, then the auth applied. A
/// GraphQL body whose variables aren't JSON is an error, unless `defer()`
/// (a variable is still missing): then its variables are left out for now.
fn build(draft: &Draft, interpolate: &dyn Fn(&str) -> Result<String>, defer: &dyn Fn() -> bool) -> Result<Built> {
    let url = interpolate(draft.url)?;
    let mut headers = draft
        .headers
        .iter()
        .filter(|h| h.enabled && !h.name.is_empty())
        .map(|h| Ok((interpolate(&h.name)?, interpolate(&h.value)?)))
        .collect::<Result<Vec<_>>>()?;
    let body = match &draft.graphql {
        None => interpolate(draft.body)?,
        Some(op) => {
            let (query, variables, name) = (interpolate(op.query)?, interpolate(op.variables)?, interpolate(op.operation_name)?);
            let body = match graphql::request_body(&query, &variables, &name) {
                Err(_) if defer() => graphql::request_body(&query, "", &name)?,
                other => other?,
            };
            if !headers.iter().any(|(k, _)| k.eq_ignore_ascii_case("content-type")) {
                headers.push(("Content-Type".into(), graphql::CONTENT_TYPE.into()));
            }
            body
        }
    };
    let (url, headers) = auth::apply(draft.auth, url, headers, interpolate)?;
    Ok((url, headers, body))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::ApiKeyLocation;

    #[test]
    fn environment_overrides_collection_and_feeds_auth() {
        let col: HashMap<String, String> = [("base".into(), "https://col".into()), ("key".into(), "c".into())].into();
        let env = Environment { id: "1".into(), name: "Staging".into(), variables: [("key".into(), "stg".into())].into(), ..Default::default() };
        let vars = variables(&col, Some(&env));
        let auth = Auth::ApiKey { key: "api_key".into(), value: "${key}".into(), location: ApiKeyLocation::Query };
        let draft = Draft { method: "GET", url: "${base}/x?a=1", headers: &[], body: "", pre_request_script: "", auth: Some(&auth), graphql: None };
        let out = prepare(&draft, &vars, &HashMap::new()).unwrap();
        assert_eq!(out.url, "https://col/x?a=1&api_key=stg");
    }

    #[test]
    fn disabled_headers_are_not_sent() {
        let mut off = Header::new("X-Debug", "1");
        off.enabled = false;
        let headers = [Header::new("Accept", "*/*"), off];
        let draft = Draft { method: "GET", url: "https://x", headers: &headers, body: "", pre_request_script: "", auth: None, graphql: None };
        let out = prepare(&draft, &HashMap::new(), &HashMap::new()).unwrap();
        assert_eq!(out.headers, vec![("Accept".to_string(), "*/*".to_string())]);
    }

    #[test]
    fn script_keeps_the_request_headers() {
        let headers = [Header::new("Content-Type", "application/json")];
        let auth = Auth::ApiKey { key: "X-API-Key".into(), value: "k".into(), location: ApiKeyLocation::Header };
        let draft = Draft { method: "GET", url: "https://x", headers: &headers, body: "", pre_request_script: "setHeader('X-Trace', '1')", auth: Some(&auth), graphql: None };
        let out = prepare(&draft, &HashMap::new(), &HashMap::new()).unwrap();
        let names: Vec<&str> = out.headers.iter().map(|(n, _)| n.as_str()).collect();
        assert_eq!(names, ["Content-Type", "X-API-Key", "X-Trace"]);
    }

    #[test]
    fn a_variable_set_by_the_script_is_used_by_the_same_request() {
        let headers = [Header::new("X-Ts", "${ts}")];
        let vars: HashMap<String, String> = [("host".into(), "h".into()), ("v".into(), "old".into())].into();
        let draft = Draft {
            method: "POST",
            url: "http://${host}/${ts}/${v}",
            headers: &headers,
            body: "${ts}",
            pre_request_script: "pm.environment.set('ts', 42); pm.variables.set('v', 'new')",
            auth: None,
            graphql: None,
        };
        let out = prepare(&draft, &vars, &HashMap::new()).unwrap();
        assert_eq!(out.url, "http://h/42/new");
        assert_eq!(out.body, "42");
        assert_eq!(out.headers, vec![("X-Ts".to_string(), "42".to_string())]);
        assert_eq!(out.variables["ts"].as_deref(), Some("42"));
        // Without the script the missing variable is still an error.
        let plain = Draft { pre_request_script: "", ..draft };
        assert_eq!(prepare(&plain, &vars, &HashMap::new()).unwrap_err().to_string(), "variable 'ts' is not defined");
    }

    #[test]
    fn graphql_body_and_content_type() {
        let vars: HashMap<String, String> = [("id".into(), "7".into()), ("q".into(), "say \"hi\"".into())].into();
        let op = Operation { query: "query U($id: ID!) { user(id: $id) { note(s: \"${q}\") } }", variables: "{\"id\": ${id}}", operation_name: "U" };
        let draft = Draft { method: "POST", url: "https://x/graphql", headers: &[], body: "ignored", pre_request_script: "", auth: None, graphql: Some(op) };
        let out = prepare(&draft, &vars, &HashMap::new()).unwrap();
        let body: Value = serde_json::from_str(&out.body).unwrap();
        assert_eq!(body["query"], "query U($id: ID!) { user(id: $id) { note(s: \"say \"hi\"\") } }");
        assert_eq!(body["variables"], serde_json::json!({"id": 7}));
        assert_eq!(body["operationName"], "U");
        assert_eq!(out.headers, vec![("Content-Type".to_string(), "application/json".to_string())]);

        // A Content-Type header of the request's own wins.
        let headers = [Header::new("content-type", "application/graphql+json")];
        let own = Draft { headers: &headers, ..draft };
        assert_eq!(prepare(&own, &vars, &HashMap::new()).unwrap().headers, vec![("content-type".to_string(), "application/graphql+json".to_string())]);

        // Variables that aren't JSON are an error; a missing variable is named.
        let bad = Draft { graphql: Some(Operation { variables: "{id: 1}", ..op }), ..draft };
        assert!(prepare(&bad, &vars, &HashMap::new()).unwrap_err().to_string().contains("not valid JSON"));
        assert_eq!(prepare(&draft, &HashMap::new(), &HashMap::new()).unwrap_err().to_string(), "variable 'q' is not defined");
    }

    #[test]
    fn graphql_variables_a_script_sets() {
        let op = Operation { query: "{ me }", variables: "{\"n\": ${n}}", operation_name: "" };
        let draft = Draft { method: "POST", url: "https://x", headers: &[], body: "", pre_request_script: "pm.variables.set('n', 3)", auth: None, graphql: Some(op) };
        let out = prepare(&draft, &HashMap::new(), &HashMap::new()).unwrap();
        assert_eq!(out.body, r#"{"query":"{ me }","variables":{"n": 3}}"#);
    }

    #[test]
    fn a_url_without_a_scheme_gets_one() {
        let vars: HashMap<String, String> = [("host".into(), "api.example.com".into())].into();
        let draft = Draft { method: "GET", url: "${host}/users", headers: &[], body: "", pre_request_script: "", auth: None, graphql: None };
        assert_eq!(prepare(&draft, &vars, &HashMap::new()).unwrap().url, "https://api.example.com/users");
        let local = Draft { url: "localhost:3000/x", ..draft };
        assert_eq!(prepare(&local, &vars, &HashMap::new()).unwrap().url, "http://localhost:3000/x");
        // Also after a script that set the variable.
        let scripted = Draft { url: "${later}/x", pre_request_script: "pm.variables.set('later', 'localhost')", ..draft };
        assert_eq!(prepare(&scripted, &vars, &HashMap::new()).unwrap().url, "http://localhost/x");
        let bad = Draft { url: "users", ..draft };
        assert_eq!(prepare(&bad, &vars, &HashMap::new()).unwrap_err().to_string(), "\"users\" isn't a URL. Enter a full URL, such as https://api.example.com/users");
    }

    #[test]
    fn script_runs_after_auth() {
        let auth = Auth::Bearer { token: "t".into() };
        let draft = Draft { method: "GET", url: "https://x", headers: &[], body: "", pre_request_script: "setHeader('Authorization', 'Bearer from-script')", auth: Some(&auth), graphql: None };
        let out = prepare(&draft, &HashMap::new(), &HashMap::new()).unwrap();
        assert_eq!(out.headers, vec![("Authorization".to_string(), "Bearer from-script".to_string())]);
    }
}
