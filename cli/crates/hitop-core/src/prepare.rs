//! A request as it is sent: variables interpolated, auth applied, then the
//! pre-request script run (so a script can still override the auth).
use std::collections::HashMap;

use anyhow::Result;
use serde_json::Value;

use crate::{
    auth,
    interpolation::interpolate,
    model::{Auth, Environment, Header},
    scripting::{run_pre_request_script, PreRequestCtx, PreRequestResult},
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
    let url = interpolate(draft.url, variables)?;
    let headers = draft
        .headers
        .iter()
        .filter(|h| !h.name.is_empty())
        .map(|h| Ok((interpolate(&h.name, variables)?, interpolate(&h.value, variables)?)))
        .collect::<Result<Vec<_>>>()?;
    let body = interpolate(draft.body, variables)?;
    let (url, headers) = auth::apply(draft.auth, url, headers, &|s| interpolate(s, variables))?;
    run_pre_request_script(
        draft.pre_request_script,
        PreRequestCtx {
            url,
            method: draft.method.to_string(),
            headers,
            body,
            variables: variables.clone(),
            context: context.clone(),
        },
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::ApiKeyLocation;

    #[test]
    fn environment_overrides_collection_and_feeds_auth() {
        let col: HashMap<String, String> = [("base".into(), "https://col".into()), ("key".into(), "c".into())].into();
        let env = Environment { id: "1".into(), name: "Staging".into(), variables: [("key".into(), "stg".into())].into() };
        let vars = variables(&col, Some(&env));
        let auth = Auth::ApiKey { key: "api_key".into(), value: "${key}".into(), location: ApiKeyLocation::Query };
        let draft = Draft { method: "GET", url: "${base}/x?a=1", headers: &[], body: "", pre_request_script: "", auth: Some(&auth) };
        let out = prepare(&draft, &vars, &HashMap::new()).unwrap();
        assert_eq!(out.url, "https://col/x?a=1&api_key=stg");
    }

    #[test]
    fn script_runs_after_auth() {
        let auth = Auth::Bearer { token: "t".into() };
        let draft = Draft { method: "GET", url: "https://x", headers: &[], body: "", pre_request_script: "setHeader('Authorization', 'Bearer from-script')", auth: Some(&auth) };
        let out = prepare(&draft, &HashMap::new(), &HashMap::new()).unwrap();
        assert_eq!(out.headers, vec![("Authorization".to_string(), "Bearer from-script".to_string())]);
    }
}
