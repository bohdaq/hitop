//! OAuth 2.0 auth (RFC 6749): the token request for the client credentials and password grants,
//! its answer, and (native) a cache that keeps each token in memory until shortly before it
//! expires. The token goes out as `Authorization: Bearer`; it is never written anywhere.
use std::collections::HashMap;

use anyhow::{bail, Result};
use base64::Engine;
use serde_json::Value;

use crate::model::{Auth, OAuth2};

/// The token request: POST `url` with a form body.
#[derive(Debug, Clone, PartialEq)]
pub struct TokenRequest {
    pub url: String,
    pub headers: Vec<(String, String)>,
    pub body: String,
    /// What the token is cached by (URL, client, scope, user).
    pub key: String,
}

fn form(pairs: &[(&str, &str)]) -> String {
    pairs
        .iter()
        .filter(|(_, v)| !v.is_empty())
        .map(|(k, v)| format!("{}={}", enc(k), enc(v)))
        .collect::<Vec<_>>()
        .join("&")
}

fn enc(s: &str) -> String {
    s.bytes()
        .map(|b| match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => (b as char).to_string(),
            _ => format!("%{b:02X}"),
        })
        .collect()
}

/// The request for `o`, its values filled in by `interpolate`.
pub fn token_request(o: &OAuth2, interpolate: &dyn Fn(&str) -> Result<String>) -> Result<TokenRequest> {
    let url = interpolate(&o.token_url)?;
    if url.trim().is_empty() {
        bail!("OAuth 2.0: the token URL is empty");
    }
    let client_id = interpolate(&o.client_id)?;
    let secret = interpolate(&o.client_secret)?;
    let scope = interpolate(&o.scope)?;
    let grant = if o.grant.is_empty() { "client_credentials" } else { o.grant.as_str() };
    let mut pairs: Vec<(&str, String)> = vec![("grant_type", grant.to_string())];
    let (user, pass) = match grant {
        "client_credentials" => (String::new(), String::new()),
        "password" => (interpolate(&o.username)?, interpolate(&o.password)?),
        other => bail!("OAuth 2.0: the grant {other} isn't supported (client_credentials or password)"),
    };
    if grant == "password" {
        pairs.push(("username", user.clone()));
        pairs.push(("password", pass));
    }
    pairs.push(("scope", scope.clone()));
    let mut headers = vec![("Content-Type".to_string(), "application/x-www-form-urlencoded".to_string()), ("Accept".to_string(), "application/json".to_string())];
    if o.client_auth == "body" {
        pairs.push(("client_id", client_id.clone()));
        pairs.push(("client_secret", secret));
    } else {
        // RFC 6749 §2.3.1: each part form-encoded, then Basic.
        let creds = format!("{}:{}", enc(&client_id), enc(&secret));
        headers.push(("Authorization".into(), format!("Basic {}", base64::engine::general_purpose::STANDARD.encode(creds))));
    }
    let refs: Vec<(&str, &str)> = pairs.iter().map(|(k, v)| (*k, v.as_str())).collect();
    Ok(TokenRequest { key: format!("{url}\n{client_id}\n{scope}\n{user}"), url, headers, body: form(&refs) })
}

/// The access token and its lifetime in seconds from the token endpoint's answer.
pub fn parse_token(status: u16, body: &str) -> Result<(String, Option<u64>)> {
    let json: Value = serde_json::from_str(body).unwrap_or(Value::Null);
    if !(200..300).contains(&status) {
        let why = json["error_description"].as_str().or(json["error"].as_str()).unwrap_or("no reason given");
        bail!("OAuth 2.0: the token endpoint answered {status}: {why}");
    }
    let Some(token) = json["access_token"].as_str().filter(|t| !t.is_empty()) else {
        bail!("OAuth 2.0: the token endpoint's answer has no access_token");
    };
    Ok((token.to_string(), json["expires_in"].as_u64().or_else(|| json["expires_in"].as_str().and_then(|s| s.parse().ok()))))
}

/// Tokens by `TokenRequest::key`: (token, valid until, as seconds of `now`).
#[derive(Default)]
pub struct Tokens(HashMap<String, (String, Option<u64>)>);

impl Tokens {
    /// For `auth` when it is OAuth 2.0: a Bearer preset with a token from the cache or from
    /// `fetch(request) -> (status, body)`; None for any other auth. `now`: seconds.
    pub fn resolve(
        &mut self,
        auth: Option<&Auth>,
        interpolate: &dyn Fn(&str) -> Result<String>,
        now: u64,
        fetch: &mut dyn FnMut(&TokenRequest) -> Result<(u16, String)>,
    ) -> Result<Option<Auth>> {
        let Some(Auth::OAuth2(o)) = auth else { return Ok(None) };
        let req = token_request(o, interpolate)?;
        if let Some((token, until)) = self.0.get(&req.key) {
            if until.is_none_or(|u| now < u) {
                return Ok(Some(Auth::Bearer { token: token.clone() }));
            }
        }
        let (status, body) = fetch(&req)?;
        let (token, lifetime) = parse_token(status, &body)?;
        // Taken again 30 s before it runs out.
        self.0.insert(req.key, (token.clone(), lifetime.map(|l| now + l.saturating_sub(30))));
        Ok(Some(Auth::Bearer { token }))
    }
}

/// One request's OAuth 2.0 token, fetched now (`hitop send`, `hitop requests send`): a Bearer
/// preset, or None when `auth` isn't OAuth 2.0.
#[cfg(feature = "native")]
pub fn fetch_once(auth: Option<&Auth>, variables: &HashMap<String, String>, timeout: Option<std::time::Duration>) -> Result<Option<Auth>> {
    Tokens::default().resolve(auth, &|s| crate::interpolation::interpolate(s, variables), 0, &mut |r| {
        let res = crate::http::execute_opts("POST", &r.url, &r.headers, r.body.clone().into_bytes(), crate::http::Options { timeout, jar: None })?;
        Ok((res.status, res.body))
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn same(s: &str) -> Result<String> {
        Ok(s.replace("${secret}", "s3 cret"))
    }

    fn cc() -> OAuth2 {
        OAuth2 { grant: "client_credentials".into(), token_url: "https://id.test/token".into(), client_id: "app".into(), client_secret: "${secret}".into(), scope: "read write".into(), ..Default::default() }
    }

    #[test]
    fn client_credentials_with_basic_client_auth() {
        let r = token_request(&cc(), &same).unwrap();
        assert_eq!(r.body, "grant_type=client_credentials&scope=read%20write");
        let basic = base64::engine::general_purpose::STANDARD.encode("app:s3%20cret");
        assert!(r.headers.contains(&("Authorization".into(), format!("Basic {basic}"))));
    }

    #[test]
    fn password_grant_with_the_client_in_the_body() {
        let o = OAuth2 { grant: "password".into(), username: "ada".into(), password: "p&w".into(), client_auth: "body".into(), scope: String::new(), ..cc() };
        let r = token_request(&o, &same).unwrap();
        assert_eq!(r.body, "grant_type=password&username=ada&password=p%26w&client_id=app&client_secret=s3%20cret");
        assert!(!r.headers.iter().any(|(k, _)| k == "Authorization"));
        assert!(token_request(&OAuth2 { grant: "implicit".into(), ..cc() }, &same).is_err());
    }

    #[test]
    fn answers_and_the_cache() {
        assert_eq!(parse_token(200, r#"{"access_token":"t1","expires_in":3600}"#).unwrap(), ("t1".into(), Some(3600)));
        assert!(parse_token(401, r#"{"error":"invalid_client"}"#).unwrap_err().to_string().contains("invalid_client"));
        assert!(parse_token(200, "{}").is_err());
        let auth = Auth::OAuth2(Box::new(cc()));
        let mut tokens = Tokens::default();
        let mut calls = 0;
        let mut fetch = |_: &TokenRequest| -> Result<(u16, String)> {
            calls += 1;
            Ok((200, format!(r#"{{"access_token":"tok{calls}","expires_in":100}}"#)))
        };
        let first = tokens.resolve(Some(&auth), &same, 1000, &mut fetch).unwrap();
        let again = tokens.resolve(Some(&auth), &same, 1050, &mut fetch).unwrap();
        let later = tokens.resolve(Some(&auth), &same, 1080, &mut fetch).unwrap();
        assert_eq!(first, Some(Auth::Bearer { token: "tok1".into() }));
        assert_eq!(again, first, "cached while valid");
        assert_eq!(later, Some(Auth::Bearer { token: "tok2".into() }), "taken again 30 s before it runs out");
        assert_eq!(tokens.resolve(Some(&Auth::None), &same, 0, &mut fetch).unwrap(), None);
    }
}
