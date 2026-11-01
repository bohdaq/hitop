#![cfg(feature = "native")]
//! A request as it is sent: variables interpolated, auth applied, then the
//! pre-request script run (so a script can still override the auth).
use std::collections::HashMap;

use anyhow::{bail, Result};
use serde_json::Value;

use crate::{
    auth,
    interpolation::{fill, interpolate, interpolate_lenient},
    model::{Auth, Environment, Header},
    scripting::{run_pre_request_script, PreRequestCtx, PreRequestResult, VariableUpdates},
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
}

/// Collection variables, overridden by the environment's.
pub fn variables(collection: &HashMap<String, String>, environment: Option<&Environment>) -> HashMap<String, String> {
    let mut vars = collection.clone();
    if let Some(env) = environment {
        vars.extend(env.variables.iter().map(|(k, v)| (k.clone(), v.clone())));
    }
    vars
}

pub fn prepare(draft: &Draft, variables: &HashMap<String, String>, context: &HashMap<String, Value>) -> Result<PreRequestResult> {
    // A `${name}` with no value yet is kept for the script: a Postman
    // pre-request script often sets the variable the same request uses.
    let missing = std::cell::RefCell::new(Vec::new());
    let lenient = |s: &str| -> Result<String> { Ok(interpolate_lenient(s, variables, &mut missing.borrow_mut())) };
    let first = build(draft, &lenient)?;
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
        },
    )?;
    if pre.variables.is_empty() {
        if let Some(name) = missing.first() {
            bail!("variable '{name}' is not defined");
        }
        return Ok(pre);
    }
    // The script set variables: what it left as built is built again with
    // them; what it changed only gets the missing ones filled in.
    let mut vars = variables.clone();
    apply_updates(&mut vars, &pre.variables);
    let strict = |s: &str| interpolate(s, &vars);
    let (url, headers, body) = build(draft, &strict)?;
    pre.url = if pre.url == first.0 { url } else { fill(&pre.url, &missing, &vars)? };
    pre.body = if pre.body == first.2 { body } else { fill(&pre.body, &missing, &vars)? };
    pre.headers = if pre.headers == first.1 {
        headers
    } else {
        pre.headers.iter().map(|(k, v)| Ok((fill(k, &missing, &vars)?, fill(v, &missing, &vars)?))).collect::<Result<_>>()?
    };
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

/// URL, enabled headers and body interpolated, then the auth applied.
fn build(draft: &Draft, interpolate: &dyn Fn(&str) -> Result<String>) -> Result<Built> {
    let url = interpolate(draft.url)?;
    let headers = draft
        .headers
        .iter()
        .filter(|h| h.enabled && !h.name.is_empty())
        .map(|h| Ok((interpolate(&h.name)?, interpolate(&h.value)?)))
        .collect::<Result<Vec<_>>>()?;
    let body = interpolate(draft.body)?;
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
        let draft = Draft { method: "GET", url: "${base}/x?a=1", headers: &[], body: "", pre_request_script: "", auth: Some(&auth) };
        let out = prepare(&draft, &vars, &HashMap::new()).unwrap();
        assert_eq!(out.url, "https://col/x?a=1&api_key=stg");
    }

    #[test]
    fn disabled_headers_are_not_sent() {
        let mut off = Header::new("X-Debug", "1");
        off.enabled = false;
        let headers = [Header::new("Accept", "*/*"), off];
        let draft = Draft { method: "GET", url: "https://x", headers: &headers, body: "", pre_request_script: "", auth: None };
        let out = prepare(&draft, &HashMap::new(), &HashMap::new()).unwrap();
        assert_eq!(out.headers, vec![("Accept".to_string(), "*/*".to_string())]);
    }

    #[test]
    fn script_keeps_the_request_headers() {
        let headers = [Header::new("Content-Type", "application/json")];
        let auth = Auth::ApiKey { key: "X-API-Key".into(), value: "k".into(), location: ApiKeyLocation::Header };
        let draft = Draft { method: "GET", url: "https://x", headers: &headers, body: "", pre_request_script: "setHeader('X-Trace', '1')", auth: Some(&auth) };
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
    fn script_runs_after_auth() {
        let auth = Auth::Bearer { token: "t".into() };
        let draft = Draft { method: "GET", url: "https://x", headers: &[], body: "", pre_request_script: "setHeader('Authorization', 'Bearer from-script')", auth: Some(&auth) };
        let out = prepare(&draft, &HashMap::new(), &HashMap::new()).unwrap();
        assert_eq!(out.headers, vec![("Authorization".to_string(), "Bearer from-script".to_string())]);
    }
}
