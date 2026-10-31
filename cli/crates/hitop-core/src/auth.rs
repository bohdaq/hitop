//! Applying auth presets to a request, and their Postman v2.1 form.
use base64::Engine;
use serde_json::{json, Value};

use crate::model::{ApiKeyLocation, Auth};
use crate::params::{parse_query, with_query};

/// Adds the auth header (or query parameter) to an already interpolated
/// request. `interpolate` resolves `${variables}` in the auth values. A
/// header set by hand with the same name wins.
pub fn apply(
    auth: Option<&Auth>,
    url: String,
    headers: Vec<(String, String)>,
    interpolate: &dyn Fn(&str) -> anyhow::Result<String>,
) -> anyhow::Result<(String, Vec<(String, String)>)> {
    let has = |name: &str| headers.iter().any(|(k, v)| k.eq_ignore_ascii_case(name) && !v.is_empty());
    let mut headers = headers.clone();
    match auth {
        Some(Auth::Bearer { token }) if !token.is_empty() => {
            if !has("Authorization") {
                headers.push(("Authorization".into(), format!("Bearer {}", interpolate(token)?)));
            }
            Ok((url, headers))
        }
        Some(Auth::Basic { username, password }) if !username.is_empty() || !password.is_empty() => {
            if !has("Authorization") {
                let creds = format!("{}:{}", interpolate(username)?, interpolate(password)?);
                let encoded = base64::engine::general_purpose::STANDARD.encode(creds);
                headers.push(("Authorization".into(), format!("Basic {encoded}")));
            }
            Ok((url, headers))
        }
        Some(Auth::ApiKey { key, value, location }) if !key.is_empty() && !value.is_empty() => {
            let key = interpolate(key)?;
            let value = interpolate(value)?;
            match location {
                ApiKeyLocation::Query => {
                    let mut rows: Vec<(String, String)> = parse_query(&url).into_iter().filter(|(k, _)| *k != key).collect();
                    rows.push((key, urlencode(&value)));
                    Ok((with_query(&url, &rows), headers))
                }
                ApiKeyLocation::Header => {
                    if !has(&key) {
                        headers.push((key, value));
                    }
                    Ok((url, headers))
                }
            }
        }
        _ => Ok((url, headers)),
    }
}

fn urlencode(s: &str) -> String {
    s.bytes()
        .map(|b| match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => (b as char).to_string(),
            _ => format!("%{b:02X}"),
        })
        .collect()
}

/// Postman v2.1 `auth`, or `None` for no auth.
pub fn to_postman(auth: Option<&Auth>) -> Option<Value> {
    let kv = |pairs: &[(&str, &str)]| -> Value { pairs.iter().map(|(k, v)| json!({ "key": k, "value": v, "type": "string" })).collect() };
    match auth? {
        Auth::None => None,
        Auth::Bearer { token } => Some(json!({ "type": "bearer", "bearer": kv(&[("token", token)]) })),
        Auth::Basic { username, password } => Some(json!({ "type": "basic", "basic": kv(&[("username", username), ("password", password)]) })),
        Auth::ApiKey { key, value, location } => {
            let at = if *location == ApiKeyLocation::Query { "query" } else { "header" };
            Some(json!({ "type": "apikey", "apikey": kv(&[("key", key), ("value", value), ("in", at)]) }))
        }
    }
}

/// A Postman v2.1 `auth` object as a preset (`None` for unsupported types).
pub fn from_postman(auth: &Value) -> Option<Auth> {
    let kind = auth["type"].as_str()?;
    let get = |k: &str| -> String {
        auth[kind]
            .as_array()
            .and_then(|a| a.iter().find(|p| p["key"] == k))
            .and_then(|p| p["value"].as_str())
            .unwrap_or("")
            .to_string()
    };
    match kind {
        "bearer" => Some(Auth::Bearer { token: get("token") }),
        "basic" => Some(Auth::Basic { username: get("username"), password: get("password") }),
        "apikey" => Some(Auth::ApiKey {
            key: get("key"),
            value: get("value"),
            location: if get("in") == "query" { ApiKeyLocation::Query } else { ApiKeyLocation::Header },
        }),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn same(s: &str) -> anyhow::Result<String> {
        Ok(s.replace("${t}", "tok"))
    }

    #[test]
    fn bearer_and_basic_add_authorization() {
        let (_, h) = apply(Some(&Auth::Bearer { token: "${t}".into() }), "https://x".into(), vec![], &same).unwrap();
        assert_eq!(h, vec![("Authorization".to_string(), "Bearer tok".to_string())]);
        let (_, h) = apply(Some(&Auth::Basic { username: "ada".into(), password: "s3cret".into() }), "https://x".into(), vec![], &same).unwrap();
        assert_eq!(h[0].1, "Basic YWRhOnMzY3JldA==");
    }

    #[test]
    fn a_header_set_by_hand_wins() {
        let manual = vec![("authorization".to_string(), "Bearer mine".to_string())];
        let (_, h) = apply(Some(&Auth::Bearer { token: "x".into() }), "https://x".into(), manual.clone(), &same).unwrap();
        assert_eq!(h, manual);
    }

    #[test]
    fn api_key_in_query_replaces_an_existing_value() {
        let auth = Auth::ApiKey { key: "k".into(), value: "a b".into(), location: ApiKeyLocation::Query };
        let (url, h) = apply(Some(&auth), "https://x/?k=old&q=1".into(), vec![], &same).unwrap();
        assert_eq!(url, "https://x/?q=1&k=a%20b");
        assert!(h.is_empty());
    }

    #[test]
    fn postman_round_trip() {
        for auth in [
            Auth::Bearer { token: "t".into() },
            Auth::Basic { username: "u".into(), password: "p".into() },
            Auth::ApiKey { key: "k".into(), value: "v".into(), location: ApiKeyLocation::Query },
        ] {
            assert_eq!(from_postman(&to_postman(Some(&auth)).unwrap()), Some(auth));
        }
        assert!(to_postman(Some(&Auth::None)).is_none());
    }
}
