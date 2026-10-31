//! Flags shared by `send` and `requests`: auth presets and query parameters.
use anyhow::{bail, Result};
use clap::{Args, ValueEnum};

use hitop_core::model::{ApiKeyLocation, Auth};

#[derive(Clone, Copy, ValueEnum)]
pub enum KeyIn {
    Header,
    Query,
}

#[derive(Args, Default)]
pub struct AuthFlags {
    /// Bearer token (sent as "Authorization: Bearer <token>")
    #[arg(long, value_name = "TOKEN", group = "auth_kind")]
    pub bearer: Option<String>,

    /// Basic auth credentials
    #[arg(long, value_name = "USER:PASSWORD", group = "auth_kind")]
    pub basic: Option<String>,

    /// API key, sent as a header unless --api-key-in query
    #[arg(long, value_name = "NAME=VALUE", group = "auth_kind")]
    pub api_key: Option<String>,

    /// Where the API key goes
    #[arg(long, value_enum, default_value = "header", requires = "api_key")]
    pub api_key_in: Option<KeyIn>,
}

impl AuthFlags {
    /// The preset the flags describe, or None when none was given.
    pub fn auth(&self) -> Result<Option<Auth>> {
        if let Some(token) = &self.bearer {
            return Ok(Some(Auth::Bearer { token: token.clone() }));
        }
        if let Some(creds) = &self.basic {
            let (username, password) = creds.split_once(':').unwrap_or((creds.as_str(), ""));
            return Ok(Some(Auth::Basic { username: username.into(), password: password.into() }));
        }
        if let Some(pair) = &self.api_key {
            let Some((key, value)) = pair.split_once('=') else { bail!("--api-key takes NAME=VALUE") };
            let location = match self.api_key_in {
                Some(KeyIn::Query) => ApiKeyLocation::Query,
                _ => ApiKeyLocation::Header,
            };
            return Ok(Some(Auth::ApiKey { key: key.into(), value: value.into(), location }));
        }
        Ok(None)
    }
}

/// "key=value" (or a bare "key") as a query row.
pub fn query_pair(s: &str) -> (String, String) {
    match s.split_once('=') {
        Some((k, v)) => (k.to_string(), v.to_string()),
        None => (s.to_string(), String::new()),
    }
}

/// A script given inline, or read from a file when it starts with "@".
pub fn script_arg(s: &Option<String>) -> Result<String> {
    match s {
        None => Ok(String::new()),
        Some(v) if v.starts_with('@') => Ok(std::fs::read_to_string(&v[1..])?),
        Some(v) => Ok(v.clone()),
    }
}
