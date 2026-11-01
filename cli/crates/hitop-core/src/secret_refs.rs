//! Secret manager references (spec/collection-format.md, section 5b):
//! `${secret:vault:path#key}`, `${secret:aws:name[#jsonKey]}`,
//! `${secret:azure:vault/name[/version]}` and `${secret:op:vault/item/field}`
//! (also `${secret:op://vault/item/field}`).
//!
//! A reference sits wherever a `${variable}` can: URL, headers, body, auth,
//! GraphQL and form fields, and the values of collection, environment and
//! global variables. Its inner text (`secret:vault:path#key`) is used as a
//! variable name, so once resolved (the `secret-managers` feature,
//! `secret_managers.rs`) the value goes into the variables and the secrets
//! like any other secret: interpolation needs no change and every masker
//! covers it. This module only finds and parses references, with no
//! dependencies, so the web app (hitop-wasm) can name them too.
use std::collections::{BTreeSet, HashMap};

use serde_json::Value;

use crate::model::{Collection, Environment, Request};

/// The start of a reference's inner text.
pub const PREFIX: &str = "secret:";

/// Which secret manager a reference reads from.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Provider {
    /// HashiCorp Vault or OpenBao, KV v1 or v2.
    Vault,
    /// AWS Secrets Manager.
    Aws,
    /// Azure Key Vault.
    Azure,
    /// 1Password, through the `op` CLI.
    OnePassword,
}

impl Provider {
    pub fn name(self) -> &'static str {
        match self {
            Provider::Vault => "vault",
            Provider::Aws => "aws",
            Provider::Azure => "azure",
            Provider::OnePassword => "op",
        }
    }
}

/// A parsed reference.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Reference {
    /// `vault:<path>[#key]`: the secret at `path` (mount included, `data/`
    /// optional on KV v2), one field of it.
    Vault { path: String, key: Option<String> },
    /// `aws:<name or ARN>[#jsonKey]`.
    Aws { name: String, key: Option<String> },
    /// `azure:<vault>/<name>[/<version>]`; a vault with a dot is a host name.
    Azure { vault: String, name: String, version: Option<String> },
    /// `op:<vault>/<item>/[<section>/]<field>`, as `op read op://...` takes it.
    OnePassword { uri: String },
}

impl Reference {
    pub fn provider(&self) -> Provider {
        match self {
            Reference::Vault { .. } => Provider::Vault,
            Reference::Aws { .. } => Provider::Aws,
            Reference::Azure { .. } => Provider::Azure,
            Reference::OnePassword { .. } => Provider::OnePassword,
        }
    }
}

/// `${secret:...}` as written, from its inner text.
pub fn display(inner: &str) -> String {
    format!("${{{inner}}}")
}

/// True when a variable name is a reference's inner text.
pub fn is_reference(name: &str) -> bool {
    name.starts_with(PREFIX)
}

/// Parses a reference's inner text (`secret:vault:kv/app#token`).
pub fn parse(inner: &str) -> Result<Reference, String> {
    let rest = inner.strip_prefix(PREFIX).ok_or_else(|| format!("{} is not a secret reference", display(inner)))?;
    let (provider, body) = rest.split_once(':').ok_or_else(|| {
        format!("{}: expected secret:<manager>:<path>, where the manager is vault, aws, azure or op", display(inner))
    })?;
    let bad = |why: &str| format!("{}: {why}", display(inner));
    let key_split = |s: &str| -> (String, Option<String>) {
        match s.split_once('#') {
            Some((a, k)) => (a.to_string(), Some(k.to_string()).filter(|k| !k.is_empty())),
            None => (s.to_string(), None),
        }
    };
    match provider {
        "vault" => {
            let (path, key) = key_split(body);
            let path = path.trim_matches('/').to_string();
            if path.is_empty() {
                return Err(bad("expected vault:<mount>/<path>[#key]"));
            }
            Ok(Reference::Vault { path, key })
        }
        "aws" => {
            let (name, key) = key_split(body);
            if name.is_empty() {
                return Err(bad("expected aws:<secret name or ARN>[#jsonKey]"));
            }
            Ok(Reference::Aws { name, key })
        }
        "azure" => {
            let parts: Vec<&str> = body.split('/').collect();
            match parts.as_slice() {
                [vault, name] if !vault.is_empty() && !name.is_empty() => Ok(Reference::Azure { vault: vault.to_string(), name: name.to_string(), version: None }),
                [vault, name, version] if !vault.is_empty() && !name.is_empty() && !version.is_empty() => {
                    Ok(Reference::Azure { vault: vault.to_string(), name: name.to_string(), version: Some(version.to_string()) })
                }
                _ => Err(bad("expected azure:<vault>/<secret name>[/<version>]")),
            }
        }
        "op" => {
            let path = body.trim_start_matches('/');
            let segments = path.split('?').next().unwrap_or_default().split('/').filter(|s| !s.is_empty()).count();
            if !(3..=4).contains(&segments) {
                return Err(bad("expected op:<vault>/<item>/[<section>/]<field> (or op://...)"));
            }
            Ok(Reference::OnePassword { uri: format!("op://{path}") })
        }
        other => Err(bad(&format!("unknown secret manager '{other}' (vault, aws, azure or op)"))),
    }
}

/// Adds the inner text of every `${secret:...}` in `text` to `out`.
pub fn find(text: &str, out: &mut BTreeSet<String>) {
    let mut rest = text;
    while let Some(start) = rest.find("${secret:") {
        let after = &rest[start + 2..];
        match after.find('}') {
            Some(end) => {
                out.insert(after[..end].to_string());
                rest = &after[end + 1..];
            }
            None => break,
        }
    }
}

/// Every string in a JSON value, skipping script and example keys (a script
/// is code, an example a saved response: neither is interpolated).
pub fn find_in_json(value: &Value, out: &mut BTreeSet<String>) {
    match value {
        Value::String(s) => find(s, out),
        Value::Array(a) => a.iter().for_each(|v| find_in_json(v, out)),
        Value::Object(o) => {
            for (k, v) in o {
                if !matches!(k.as_str(), "preRequestScript" | "postRequestScript" | "examples" | "scripts") {
                    find_in_json(v, out);
                }
            }
        }
        _ => {}
    }
}

/// The references a request uses: URL, headers, body, auth, GraphQL,
/// form fields, WebSocket messages, gRPC fields (not its scripts).
pub fn find_in_request(request: &Request, out: &mut BTreeSet<String>) {
    if let Ok(v) = serde_json::to_value(request) {
        find_in_json(&v, out);
    }
}

/// The references in variable values.
pub fn find_in_variables(variables: &HashMap<String, String>, out: &mut BTreeSet<String>) {
    variables.values().for_each(|v| find(v, out));
}

/// The references a collection's requests, its variables and its default
/// auth use.
pub fn find_in_collection(collection: &Collection, out: &mut BTreeSet<String>) {
    collection.requests.iter().for_each(|r| find_in_request(r, out));
    find_in_variables(&collection.variables, out);
    if let Some(auth) = &collection.auth {
        if let Ok(v) = serde_json::to_value(auth) {
            find_in_json(&v, out);
        }
    }
}

/// The references in an environment's (or the globals') variable values.
pub fn find_in_environment(environment: &Environment, out: &mut BTreeSet<String>) {
    find_in_variables(&environment.variables, out);
}

/// Replaces each reference inside `text` whose value `values` has (keyed by
/// inner text); the others are left as written.
pub fn expand(text: &str, values: &dyn Fn(&str) -> Option<String>) -> String {
    if !text.contains("${secret:") {
        return text.to_string();
    }
    let mut out = String::new();
    let mut rest = text;
    while let Some(start) = rest.find("${secret:") {
        out.push_str(&rest[..start]);
        let after = &rest[start + 2..];
        match after.find('}') {
            Some(end) => {
                match values(&after[..end]) {
                    Some(v) => out.push_str(&v),
                    None => out.push_str(&rest[start..start + 2 + end + 1]),
                }
                rest = &after[end + 1..];
            }
            None => {
                out.push_str(&rest[start..]);
                rest = "";
            }
        }
    }
    out.push_str(rest);
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_every_manager() {
        assert_eq!(parse("secret:vault:secret/data/app#token").unwrap(), Reference::Vault { path: "secret/data/app".into(), key: Some("token".into()) });
        assert_eq!(parse("secret:vault:/kv/app/").unwrap(), Reference::Vault { path: "kv/app".into(), key: None });
        assert_eq!(
            parse("secret:aws:arn:aws:secretsmanager:eu-west-1:123:secret:db-AbC#password").unwrap(),
            Reference::Aws { name: "arn:aws:secretsmanager:eu-west-1:123:secret:db-AbC".into(), key: Some("password".into()) }
        );
        assert_eq!(parse("secret:aws:prod/api").unwrap(), Reference::Aws { name: "prod/api".into(), key: None });
        assert_eq!(parse("secret:azure:my-vault/api-key").unwrap(), Reference::Azure { vault: "my-vault".into(), name: "api-key".into(), version: None });
        assert_eq!(parse("secret:azure:v/n/abc").unwrap(), Reference::Azure { vault: "v".into(), name: "n".into(), version: Some("abc".into()) });
        assert_eq!(parse("secret:op:Dev/Stripe/api key").unwrap(), Reference::OnePassword { uri: "op://Dev/Stripe/api key".into() });
        assert_eq!(parse("secret:op://Dev/Stripe/section/key").unwrap(), Reference::OnePassword { uri: "op://Dev/Stripe/section/key".into() });
        for bad in ["secret:vault:", "secret:aws:", "secret:azure:onlyvault", "secret:op:a/b", "secret:gcp:x", "secret:nomanager", "plain"] {
            assert!(parse(bad).is_err(), "{bad}");
        }
        assert!(parse("secret:gcp:x").unwrap_err().contains("${secret:gcp:x}"));
    }

    #[test]
    fn finds_references_outside_scripts() {
        let mut out = BTreeSet::new();
        find("a ${secret:vault:a#b} ${x} ${secret:op:v/i/f}${secret:unterminated", &mut out);
        assert_eq!(out.iter().map(String::as_str).collect::<Vec<_>>(), ["secret:op:v/i/f", "secret:vault:a#b"]);

        let request = Request {
            url: "https://x/${secret:aws:u}".into(),
            headers: vec![crate::model::Header::new("Authorization", "Bearer ${secret:azure:v/t}")],
            pre_request_script: "// ${secret:aws:not-this}".into(),
            examples: vec![serde_json::json!({"body": "${secret:aws:nor-this}"})],
            ..Default::default()
        };
        let mut out = BTreeSet::new();
        find_in_request(&request, &mut out);
        assert_eq!(out.iter().map(String::as_str).collect::<Vec<_>>(), ["secret:aws:u", "secret:azure:v/t"]);
    }

    #[test]
    fn expands_known_references_only() {
        let values = |k: &str| (k == "secret:aws:a").then(|| "VALUE".to_string());
        assert_eq!(expand("x${secret:aws:a}y${secret:aws:b}z${secret:aws:a", &values), "xVALUEy${secret:aws:b}z${secret:aws:a");
        assert_eq!(expand("plain ${v}", &values), "plain ${v}");
    }
}
