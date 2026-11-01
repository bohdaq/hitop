#![cfg(feature = "secret-managers")]
//! Resolves `${secret:...}` references ([`crate::secret_refs`]) at run time:
//!
//! - **Vault** (and OpenBao): KV v2 or v1 over HTTP. `VAULT_ADDR`, `VAULT_TOKEN`
//!   (else `~/.vault-token`), `VAULT_NAMESPACE`; `BAO_ADDR`, `BAO_TOKEN`,
//!   `BAO_NAMESPACE` work too.
//! - **AWS Secrets Manager**: `GetSecretValue`, signed with SigV4 here (no AWS
//!   SDK). Credentials: `AWS_ACCESS_KEY_ID`/`AWS_SECRET_ACCESS_KEY`/
//!   `AWS_SESSION_TOKEN`, then the profile (`AWS_PROFILE`, else `default`) in
//!   `~/.aws/credentials` and `~/.aws/config` (static keys or
//!   `credential_process`), then `aws configure export-credentials` (SSO,
//!   assumed roles: whatever the AWS CLI can sign in with). Region from an ARN,
//!   `AWS_REGION`, `AWS_DEFAULT_REGION` or the profile; endpoint
//!   `AWS_ENDPOINT_URL_SECRETS_MANAGER` or `AWS_ENDPOINT_URL` when set.
//! - **Azure Key Vault**: a token from `AZURE_TENANT_ID`/`AZURE_CLIENT_ID`/
//!   `AZURE_CLIENT_SECRET` (client credentials, `AZURE_AUTHORITY_HOST`), else
//!   from `az account get-access-token --resource https://vault.azure.net`.
//! - **1Password**: `op read op://...` (the user's signed-in `op` CLI).
//!
//! Values are cached for the life of a [`Resolver`] (one run). Errors name the
//! reference, never a value. `HITOP_SECRET_MANAGERS=off` turns it all off
//! ([`enabled`]); `HITOP_SECRET_MANAGER_TIMEOUT` (seconds) sets the time limit
//! of each call (default 15 s for HTTP, 60 s for `op`, `az` and `aws`).
use std::collections::{BTreeSet, HashMap};
use std::io::Read;
use std::path::PathBuf;
use std::process::{Command, Stdio};
use std::time::{Duration, Instant, SystemTime};

use serde_json::Value;

use crate::secret_refs::{display, parse, Reference};

/// Reads a process environment variable (`std::env::var` outside tests).
pub type EnvLookup = Box<dyn Fn(&str) -> Option<String> + Send + Sync>;

/// False when `HITOP_SECRET_MANAGERS` is `off` (or `0`, `false`).
pub fn enabled() -> bool {
    !matches!(std::env::var("HITOP_SECRET_MANAGERS").ok().as_deref().map(str::to_ascii_lowercase).as_deref(), Some("off" | "0" | "false" | "no"))
}

/// Resolves references, one call per secret, cached for its lifetime.
pub struct Resolver {
    env: EnvLookup,
    http: Option<reqwest::blocking::Client>,
    http_timeout: Duration,
    command_timeout: Duration,
    values: HashMap<String, String>,
    /// Whole secrets already read (`vault:<path>`, `aws:<name>`), so two keys
    /// of one secret make one call.
    documents: HashMap<String, Value>,
    azure_token: Option<String>,
    aws_credentials: Option<AwsCredentials>,
}

impl Default for Resolver {
    fn default() -> Self {
        Self::from_env()
    }
}

impl Resolver {
    /// Reads the process environment.
    pub fn from_env() -> Self {
        Self::with_env(Box::new(|k| std::env::var(k).ok().filter(|v| !v.is_empty())))
    }

    /// Reads `env` instead of the process environment (tests).
    pub fn with_env(env: EnvLookup) -> Self {
        let custom = env("HITOP_SECRET_MANAGER_TIMEOUT").and_then(|s| s.trim().parse::<f64>().ok()).filter(|s| *s > 0.0).map(Duration::from_secs_f64);
        Resolver {
            env,
            http: None,
            http_timeout: custom.unwrap_or(Duration::from_secs(15)),
            command_timeout: custom.unwrap_or(Duration::from_secs(60)),
            values: HashMap::new(),
            documents: HashMap::new(),
            azure_token: None,
            aws_credentials: None,
        }
    }

    /// One reference's value, by its inner text (`secret:vault:kv/app#token`).
    /// The error names the reference.
    pub fn resolve(&mut self, inner: &str) -> Result<String, String> {
        if let Some(v) = self.values.get(inner) {
            return Ok(v.clone());
        }
        let reference = parse(inner)?;
        let value = match &reference {
            Reference::Vault { path, key } => self.vault(path, key.as_deref()),
            Reference::Aws { name, key } => self.aws(name, key.as_deref()),
            Reference::Azure { vault, name, version } => self.azure(vault, name, version.as_deref()),
            Reference::OnePassword { uri } => self.op(uri),
        }
        .map_err(|e| format!("{} ({}): {e}", display(inner), reference.provider().name()))?;
        self.values.insert(inner.to_string(), value.clone());
        Ok(value)
    }

    /// Every reference: the values found and an error line for each that
    /// failed.
    pub fn resolve_all(&mut self, refs: &BTreeSet<String>) -> (Vec<(String, String)>, Vec<String>) {
        let (mut values, mut errors) = (vec![], vec![]);
        for r in refs {
            match self.resolve(r) {
                Ok(v) => values.push((r.clone(), v)),
                Err(e) => errors.push(e),
            }
        }
        (values, errors)
    }

    fn var(&self, name: &str) -> Option<String> {
        (self.env)(name).filter(|v| !v.is_empty())
    }

    fn home(&self) -> Option<PathBuf> {
        self.var("HOME").or_else(|| self.var("USERPROFILE")).map(PathBuf::from).or_else(dirs::home_dir)
    }

    fn client(&mut self) -> Result<reqwest::blocking::Client, String> {
        if self.http.is_none() {
            let c = reqwest::blocking::Client::builder()
                .timeout(self.http_timeout)
                .user_agent(concat!("hitop/", env!("CARGO_PKG_VERSION")))
                .build()
                .map_err(|e| format!("HTTP client: {e}"))?;
            self.http = Some(c);
        }
        Ok(self.http.clone().expect("client"))
    }

    fn program(&self, name: &str) -> String {
        // HITOP_OP_BIN, HITOP_AZ_BIN, HITOP_AWS_BIN: where the CLI is, when not on PATH.
        self.var(&format!("HITOP_{}_BIN", name.to_ascii_uppercase())).unwrap_or_else(|| name.to_string())
    }

    // ── Vault ────────────────────────────────────────────────────────────

    fn vault(&mut self, path: &str, key: Option<&str>) -> Result<String, String> {
        let doc_key = format!("vault:{path}");
        if !self.documents.contains_key(&doc_key) {
            let doc = self.vault_read(path)?;
            self.documents.insert(doc_key.clone(), doc);
        }
        pick_field(&self.documents[&doc_key], key, "#<field>")
    }

    fn vault_read(&mut self, path: &str) -> Result<Value, String> {
        let addr = self.var("VAULT_ADDR").or_else(|| self.var("BAO_ADDR")).ok_or("VAULT_ADDR is not set")?;
        let addr = addr.trim_end_matches('/').to_string();
        let token = match self.var("VAULT_TOKEN").or_else(|| self.var("BAO_TOKEN")) {
            Some(t) => t,
            None => {
                let file = self.home().map(|h| h.join(".vault-token"));
                file.as_ref()
                    .and_then(|f| std::fs::read_to_string(f).ok())
                    .map(|t| t.trim().to_string())
                    .filter(|t| !t.is_empty())
                    .ok_or("no Vault token: set VAULT_TOKEN or sign in with `vault login` (~/.vault-token)")?
            }
        };
        let namespace = self.var("VAULT_NAMESPACE").or_else(|| self.var("BAO_NAMESPACE"));
        let client = self.client()?;
        let get = |p: &str| {
            let mut req = client.get(format!("{addr}/v1/{p}")).header("X-Vault-Token", &token).header("X-Vault-Request", "true");
            if let Some(ns) = &namespace {
                req = req.header("X-Vault-Namespace", ns);
            }
            req.send().map_err(|e| format!("Vault at {addr}: {}", reqwest_reason(&e)))
        };
        // Which mount and KV version: what the vault CLI asks too. A token that
        // may not ask falls back on the path (`.../data/...` = KV v2).
        let (read_path, v2) = match get(&format!("sys/internal/ui/mounts/{path}")) {
            Ok(r) if r.status().is_success() => {
                let body: Value = r.json().unwrap_or(Value::Null);
                let mount = body["data"]["path"].as_str().unwrap_or_default().trim_matches('/').to_string();
                let v2 = body["data"]["options"]["version"].as_str() == Some("2");
                if v2 && !mount.is_empty() {
                    let rest = path.strip_prefix(&mount).unwrap_or(path).trim_start_matches('/');
                    let rest = rest.strip_prefix("data/").unwrap_or(rest);
                    (format!("{mount}/data/{rest}"), true)
                } else {
                    (path.to_string(), false)
                }
            }
            Ok(_) | Err(_) => (path.to_string(), path.contains("/data/")),
        };
        let r = get(&read_path)?;
        let status = r.status();
        let body: Value = r.json().unwrap_or(Value::Null);
        if !status.is_success() {
            let errors: Vec<&str> = body["errors"].as_array().map(|a| a.iter().filter_map(Value::as_str).collect()).unwrap_or_default();
            return Err(match status.as_u16() {
                404 => format!("no secret at {read_path}"),
                403 => format!("permission denied reading {read_path} (check the token and its policy{})", if namespace.is_some() { " and VAULT_NAMESPACE" } else { "" }),
                _ => format!("Vault answered {status} for {read_path}{}", if errors.is_empty() { String::new() } else { format!(": {}", errors.join("; ")) }),
            });
        }
        let data = &body["data"];
        // KV v2 wraps the fields in data.data, next to data.metadata.
        let fields = if v2 || (data["data"].is_object() && data["metadata"].is_object()) { &data["data"] } else { data };
        if !fields.is_object() {
            return Err(format!("{read_path} holds no key/value data (deleted, or not a KV secret)"));
        }
        Ok(fields.clone())
    }

    // ── AWS Secrets Manager ──────────────────────────────────────────────

    fn aws(&mut self, name: &str, key: Option<&str>) -> Result<String, String> {
        let doc_key = format!("aws:{name}");
        if !self.documents.contains_key(&doc_key) {
            let text = self.aws_get(name)?;
            self.documents.insert(doc_key.clone(), Value::String(text));
        }
        let Value::String(text) = &self.documents[&doc_key] else { unreachable!() };
        match key {
            None => Ok(text.clone()),
            Some(k) => {
                let json: Value = serde_json::from_str(text).map_err(|_| format!("#{k} needs a JSON secret, and this one isn't JSON"))?;
                pick_field(&json, Some(k), "#<jsonKey>")
            }
        }
    }

    fn aws_region(&self, name: &str, profile_region: Option<String>) -> Result<String, String> {
        if let Some(rest) = name.strip_prefix("arn:") {
            // arn:<partition>:secretsmanager:<region>:<account>:secret:<name>
            if let Some(region) = rest.split(':').nth(2).filter(|r| !r.is_empty()) {
                return Ok(region.to_string());
            }
        }
        self.var("AWS_REGION")
            .or_else(|| self.var("AWS_DEFAULT_REGION"))
            .or(profile_region)
            .ok_or_else(|| "no AWS region: set AWS_REGION, give one in the profile, or use the secret's ARN".to_string())
    }

    fn aws_get(&mut self, name: &str) -> Result<String, String> {
        let profile = AwsProfile::load(self);
        let region = self.aws_region(name, profile.get("region"))?;
        let creds = match self.aws_credentials.clone() {
            Some(c) => c,
            None => {
                let c = self.aws_credentials_chain(&profile)?;
                self.aws_credentials = Some(c.clone());
                c
            }
        };
        let endpoint = self.var("AWS_ENDPOINT_URL_SECRETS_MANAGER").or_else(|| self.var("AWS_ENDPOINT_URL")).unwrap_or_else(|| {
            let suffix = if region.starts_with("cn-") { "amazonaws.com.cn" } else { "amazonaws.com" };
            format!("https://secretsmanager.{region}.{suffix}")
        });
        let url = reqwest::Url::parse(&endpoint).map_err(|e| format!("endpoint {endpoint}: {e}"))?;
        let host = match (url.host_str(), url.port()) {
            (Some(h), Some(p)) => format!("{h}:{p}"),
            (Some(h), None) => h.to_string(),
            _ => return Err(format!("endpoint {endpoint} has no host")),
        };
        let body = serde_json::json!({ "SecretId": name }).to_string();
        let amz_date = amz_date(SystemTime::now());
        let mut headers = vec![
            ("content-type".to_string(), "application/x-amz-json-1.1".to_string()),
            ("host".to_string(), host),
            ("x-amz-date".to_string(), amz_date.clone()),
            ("x-amz-target".to_string(), "secretsmanager.GetSecretValue".to_string()),
        ];
        if let Some(t) = &creds.session_token {
            headers.push(("x-amz-security-token".to_string(), t.clone()));
        }
        let path = if url.path().is_empty() { "/" } else { url.path() };
        let authorization = sigv4(&SigV4 { method: "POST", path, query: "", headers: &headers, body: body.as_bytes(), access_key: &creds.access_key, secret_key: &creds.secret_key, region: &region, service: "secretsmanager", amz_date: &amz_date });
        let client = self.client()?;
        let mut req = client.post(url.clone()).header("Authorization", authorization).body(body);
        for (k, v) in headers.iter().filter(|(k, _)| k != "host") {
            req = req.header(k.as_str(), v.as_str());
        }
        let r = req.send().map_err(|e| format!("Secrets Manager at {endpoint}: {}", reqwest_reason(&e)))?;
        let status = r.status();
        let body: Value = r.json().unwrap_or(Value::Null);
        if !status.is_success() {
            let kind = body["__type"].as_str().unwrap_or_default().rsplit('#').next().unwrap_or_default().to_string();
            let message = body["message"].as_str().or(body["Message"].as_str()).unwrap_or_default();
            return Err(format!("Secrets Manager answered {status}{}{}", if kind.is_empty() { String::new() } else { format!(" {kind}") }, if message.is_empty() { String::new() } else { format!(": {message}") }));
        }
        if let Some(s) = body["SecretString"].as_str() {
            return Ok(s.to_string());
        }
        if let Some(b) = body["SecretBinary"].as_str() {
            use base64::Engine;
            let bytes = base64::engine::general_purpose::STANDARD.decode(b).map_err(|_| "SecretBinary is not base64")?;
            return String::from_utf8(bytes).map_err(|_| "the secret is binary, not text".to_string());
        }
        Err("the answer has no SecretString".into())
    }

    fn aws_credentials_chain(&self, profile: &AwsProfile) -> Result<AwsCredentials, String> {
        if let (Some(access_key), Some(secret_key)) = (self.var("AWS_ACCESS_KEY_ID"), self.var("AWS_SECRET_ACCESS_KEY")) {
            return Ok(AwsCredentials { access_key, secret_key, session_token: self.var("AWS_SESSION_TOKEN") });
        }
        if let (Some(access_key), Some(secret_key)) = (profile.get("aws_access_key_id"), profile.get("aws_secret_access_key")) {
            return Ok(AwsCredentials { access_key, secret_key, session_token: profile.get("aws_session_token") });
        }
        if let Some(process) = profile.get("credential_process") {
            let (shell, flag) = if cfg!(windows) { ("cmd", "/C") } else { ("sh", "-c") };
            let out = run_command(shell, &[flag, &process], self.command_timeout).map_err(|e| format!("credential_process of profile {}: {e}", profile.name))?;
            return process_credentials(&out).map_err(|e| format!("credential_process of profile {}: {e}", profile.name));
        }
        // SSO, assumed roles, web identity: the AWS CLI signs in, we take the keys.
        let aws = self.program("aws");
        let mut args = vec!["configure", "export-credentials", "--format", "process"];
        if self.var("AWS_PROFILE").is_some() || profile.found {
            args.extend(["--profile", profile.name.as_str()]);
        }
        match run_command(&aws, &args, self.command_timeout) {
            Ok(out) => process_credentials(&out).map_err(|e| format!("`aws configure export-credentials`: {e}")),
            Err(e) => Err(format!(
                "no AWS credentials: set AWS_ACCESS_KEY_ID and AWS_SECRET_ACCESS_KEY, add keys or credential_process to profile {} in ~/.aws, or sign in with the AWS CLI (`aws sso login`); `aws configure export-credentials` failed: {e}",
                profile.name
            )),
        }
    }

    // ── Azure Key Vault ──────────────────────────────────────────────────

    fn azure(&mut self, vault: &str, name: &str, version: Option<&str>) -> Result<String, String> {
        let token = match self.azure_token.clone() {
            Some(t) => t,
            None => {
                let t = self.azure_token()?;
                self.azure_token = Some(t.clone());
                t
            }
        };
        let base = match self.var("HITOP_AZURE_VAULT_URL") {
            Some(template) => template.replace("{vault}", vault),
            None if vault.contains('.') => format!("https://{vault}"),
            None => format!("https://{vault}.vault.azure.net"),
        };
        let base = base.trim_end_matches('/');
        let url = match version {
            Some(v) => format!("{base}/secrets/{name}/{v}?api-version=7.4"),
            None => format!("{base}/secrets/{name}?api-version=7.4"),
        };
        let client = self.client()?;
        let r = client.get(&url).bearer_auth(&token).send().map_err(|e| format!("Key Vault {base}: {}", reqwest_reason(&e)))?;
        let status = r.status();
        let body: Value = r.json().unwrap_or(Value::Null);
        if !status.is_success() {
            let code = body["error"]["code"].as_str().unwrap_or_default();
            let message = body["error"]["message"].as_str().unwrap_or_default();
            return Err(format!("Key Vault answered {status}{}{}", if code.is_empty() { String::new() } else { format!(" {code}") }, if message.is_empty() { String::new() } else { format!(": {message}") }));
        }
        body["value"].as_str().map(str::to_string).ok_or_else(|| "the answer has no value".into())
    }

    fn azure_token(&mut self) -> Result<String, String> {
        if let (Some(tenant), Some(client_id), Some(secret)) = (self.var("AZURE_TENANT_ID"), self.var("AZURE_CLIENT_ID"), self.var("AZURE_CLIENT_SECRET")) {
            let authority = self.var("AZURE_AUTHORITY_HOST").unwrap_or_else(|| "https://login.microsoftonline.com".into());
            let url = format!("{}/{tenant}/oauth2/v2.0/token", authority.trim_end_matches('/'));
            let client = self.client()?;
            let form = [("grant_type", "client_credentials"), ("client_id", client_id.as_str()), ("client_secret", secret.as_str()), ("scope", "https://vault.azure.net/.default")];
            let r = client.post(&url).form(&form).send().map_err(|e| format!("Azure sign-in at {authority}: {}", reqwest_reason(&e)))?;
            let status = r.status();
            let body: Value = r.json().unwrap_or(Value::Null);
            return match body["access_token"].as_str() {
                Some(t) if status.is_success() => Ok(t.to_string()),
                _ => {
                    let why = body["error_description"].as_str().or(body["error"].as_str()).unwrap_or_default();
                    Err(format!("Azure sign-in (AZURE_CLIENT_ID) answered {status}{}", if why.is_empty() { String::new() } else { format!(": {}", why.lines().next().unwrap_or_default()) }))
                }
            };
        }
        let az = self.program("az");
        let out = run_command(&az, &["account", "get-access-token", "--resource", "https://vault.azure.net", "--output", "json"], self.command_timeout)
            .map_err(|e| format!("no Azure token: set AZURE_TENANT_ID, AZURE_CLIENT_ID and AZURE_CLIENT_SECRET, or sign in with `az login`; `az account get-access-token` failed: {e}"))?;
        let body: Value = serde_json::from_str(&out).map_err(|_| "`az account get-access-token` printed no JSON")?;
        body["accessToken"].as_str().map(str::to_string).ok_or_else(|| "`az account get-access-token` gave no accessToken".into())
    }

    // ── 1Password ────────────────────────────────────────────────────────

    fn op(&mut self, uri: &str) -> Result<String, String> {
        let op = self.program("op");
        run_command(&op, &["read", uri, "--no-newline"], self.command_timeout).map_err(|e| format!("`op read`: {e}"))
    }
}

/// A secret's field: the one named, or the only one there is.
fn pick_field(doc: &Value, key: Option<&str>, hint: &str) -> Result<String, String> {
    let text = |v: &Value| match v {
        Value::String(s) => s.clone(),
        other => other.to_string(),
    };
    let Some(map) = doc.as_object() else {
        return match key {
            None => Ok(text(doc)),
            Some(k) => Err(format!("no field '{k}': the secret is not an object")),
        };
    };
    let names = || map.keys().map(String::as_str).collect::<Vec<_>>().join(", ");
    match key {
        Some(k) => map.get(k).map(text).ok_or_else(|| format!("no field '{k}' (fields: {})", names())),
        None if map.len() == 1 => Ok(text(map.values().next().expect("one field"))),
        None => Err(format!("the secret has fields {}: name one with {hint}", names())),
    }
}

fn reqwest_reason(e: &reqwest::Error) -> String {
    if e.is_timeout() {
        return "timed out".into();
    }
    if e.is_connect() {
        return "could not connect".into();
    }
    let mut s = e.to_string();
    let mut src = std::error::Error::source(e);
    while let Some(inner) = src {
        s.push_str(&format!(": {inner}"));
        src = inner.source();
    }
    s
}

/// Runs a program with a time limit; stdout on success, stderr's first lines
/// (never stdout, which may hold the value) in the error.
pub(crate) fn run_command(program: &str, args: &[&str], timeout: Duration) -> Result<String, String> {
    let mut child = Command::new(program).args(args).stdin(Stdio::null()).stdout(Stdio::piped()).stderr(Stdio::piped()).spawn().map_err(|e| {
        if e.kind() == std::io::ErrorKind::NotFound {
            format!("`{program}` was not found on PATH")
        } else {
            format!("could not start `{program}`: {e}")
        }
    })?;
    let mut stdout = child.stdout.take().expect("stdout");
    let mut stderr = child.stderr.take().expect("stderr");
    let out = std::thread::spawn(move || {
        let mut b = vec![];
        let _ = stdout.read_to_end(&mut b);
        b
    });
    let err = std::thread::spawn(move || {
        let mut b = vec![];
        let _ = stderr.read_to_end(&mut b);
        b
    });
    let started = Instant::now();
    let status = loop {
        match child.try_wait() {
            Ok(Some(s)) => break s,
            Ok(None) if started.elapsed() > timeout => {
                let _ = child.kill();
                let _ = child.wait();
                return Err(format!("`{program}` timed out after {} s", timeout.as_secs_f64()));
            }
            Ok(None) => std::thread::sleep(Duration::from_millis(10)),
            Err(e) => return Err(format!("`{program}`: {e}")),
        }
    };
    let out = String::from_utf8(out.join().unwrap_or_default()).map_err(|_| format!("`{program}` printed something that isn't text"))?;
    let err = String::from_utf8_lossy(&err.join().unwrap_or_default()).into_owned();
    if !status.success() {
        let why: Vec<&str> = err.lines().map(str::trim).filter(|l| !l.is_empty()).take(3).collect();
        return Err(format!("`{program}` failed ({status}){}", if why.is_empty() { String::new() } else { format!(": {}", why.join(" ")) }));
    }
    Ok(out)
}

#[derive(Clone)]
struct AwsCredentials {
    access_key: String,
    secret_key: String,
    session_token: Option<String>,
}

/// `credential_process` output: `{"Version": 1, "AccessKeyId", "SecretAccessKey", "SessionToken"}`.
fn process_credentials(out: &str) -> Result<AwsCredentials, String> {
    let v: Value = serde_json::from_str(out).map_err(|_| "printed no JSON".to_string())?;
    match (v["AccessKeyId"].as_str(), v["SecretAccessKey"].as_str()) {
        (Some(a), Some(s)) => Ok(AwsCredentials { access_key: a.into(), secret_key: s.into(), session_token: v["SessionToken"].as_str().map(str::to_string) }),
        _ => Err("printed no AccessKeyId and SecretAccessKey".into()),
    }
}

/// The settings of one AWS profile, from the credentials file and the
/// config file (`[default]`, `[profile name]` or `[name]`).
struct AwsProfile {
    name: String,
    found: bool,
    values: HashMap<String, String>,
}

impl AwsProfile {
    fn load(r: &Resolver) -> Self {
        let name = r.var("AWS_PROFILE").or_else(|| r.var("AWS_DEFAULT_PROFILE")).unwrap_or_else(|| "default".into());
        let home = r.home();
        let file = |var: &str, rel: &str| r.var(var).map(PathBuf::from).or_else(|| home.as_ref().map(|h| h.join(".aws").join(rel)));
        let mut values = HashMap::new();
        let mut found = false;
        // The credentials file wins over the config file.
        for (path, config) in [(file("AWS_CONFIG_FILE", "config"), true), (file("AWS_SHARED_CREDENTIALS_FILE", "credentials"), false)] {
            if let Some(text) = path.and_then(|p| std::fs::read_to_string(p).ok()) {
                if let Some(section) = ini_section(&text, &name, config) {
                    found = true;
                    values.extend(section);
                }
            }
        }
        AwsProfile { name, found, values }
    }

    fn get(&self, key: &str) -> Option<String> {
        self.values.get(key).cloned().filter(|v| !v.is_empty())
    }
}

fn ini_section(text: &str, profile: &str, config: bool) -> Option<HashMap<String, String>> {
    let mut current: Option<bool> = None;
    let mut out = HashMap::new();
    let mut found = false;
    for line in text.lines() {
        let line = line.trim();
        if line.is_empty() || line.starts_with('#') || line.starts_with(';') {
            continue;
        }
        if let Some(header) = line.strip_prefix('[').and_then(|l| l.strip_suffix(']')) {
            let header = header.trim();
            let name = if config { header.strip_prefix("profile ").map(str::trim).unwrap_or(header) } else { header };
            let is = name == profile && !(config && header.starts_with("sso-session"));
            found |= is;
            current = Some(is);
            continue;
        }
        if current == Some(true) {
            if let Some((k, v)) = line.split_once('=') {
                out.insert(k.trim().to_ascii_lowercase(), v.trim().to_string());
            }
        }
    }
    found.then_some(out)
}

/// `YYYYMMDDTHHMMSSZ` in UTC.
fn amz_date(t: SystemTime) -> String {
    let secs = t.duration_since(SystemTime::UNIX_EPOCH).map(|d| d.as_secs()).unwrap_or_default() as i64;
    let (days, rem) = (secs.div_euclid(86_400), secs.rem_euclid(86_400));
    // Howard Hinnant's civil_from_days.
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let day = doy - (153 * mp + 2) / 5 + 1;
    let month = if mp < 10 { mp + 3 } else { mp - 9 };
    let year = yoe + era * 400 + i64::from(month <= 2);
    format!("{year:04}{month:02}{day:02}T{:02}{:02}{:02}Z", rem / 3600, rem % 3600 / 60, rem % 60)
}

/// What [`sigv4`] signs. `headers` are lower-case names, all of them signed.
struct SigV4<'a> {
    method: &'a str,
    path: &'a str,
    query: &'a str,
    headers: &'a [(String, String)],
    body: &'a [u8],
    access_key: &'a str,
    secret_key: &'a str,
    region: &'a str,
    service: &'a str,
    amz_date: &'a str,
}

fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}

fn sha256_hex(data: &[u8]) -> String {
    hex(ring::digest::digest(&ring::digest::SHA256, data).as_ref())
}

fn hmac(key: &[u8], data: &str) -> Vec<u8> {
    ring::hmac::sign(&ring::hmac::Key::new(ring::hmac::HMAC_SHA256, key), data.as_bytes()).as_ref().to_vec()
}

/// The `Authorization` header of AWS Signature Version 4.
fn sigv4(s: &SigV4) -> String {
    let mut headers: Vec<(String, String)> = s.headers.iter().map(|(k, v)| (k.to_ascii_lowercase(), v.split_whitespace().collect::<Vec<_>>().join(" "))).collect();
    headers.sort();
    let canonical_headers: String = headers.iter().map(|(k, v)| format!("{k}:{v}\n")).collect();
    let signed: Vec<&str> = headers.iter().map(|(k, _)| k.as_str()).collect();
    let signed = signed.join(";");
    let canonical = format!("{}\n{}\n{}\n{canonical_headers}\n{signed}\n{}", s.method, s.path, s.query, sha256_hex(s.body));
    let date = &s.amz_date[..8];
    let scope = format!("{date}/{}/{}/aws4_request", s.region, s.service);
    let to_sign = format!("AWS4-HMAC-SHA256\n{}\n{scope}\n{}", s.amz_date, sha256_hex(canonical.as_bytes()));
    let k = hmac(format!("AWS4{}", s.secret_key).as_bytes(), date);
    let k = hmac(&k, s.region);
    let k = hmac(&k, s.service);
    let k = hmac(&k, "aws4_request");
    let signature = hex(&hmac(&k, &to_sign));
    format!("AWS4-HMAC-SHA256 Credential={}/{scope}, SignedHeaders={signed}, Signature={signature}", s.access_key)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn resolver(vars: &[(&str, &str)]) -> Resolver {
        let map: HashMap<String, String> = vars.iter().map(|(k, v)| (k.to_string(), v.to_string())).collect();
        // HOME points nowhere, so no real ~/.vault-token or ~/.aws is read.
        let mut map = map;
        map.entry("HOME".into()).or_insert_with(|| "/nonexistent-hitop-home".into());
        map.entry("HITOP_AWS_BIN".into()).or_insert_with(|| "/nonexistent-hitop-aws".into());
        map.entry("HITOP_AZ_BIN".into()).or_insert_with(|| "/nonexistent-hitop-az".into());
        Resolver::with_env(Box::new(move |k| map.get(k).cloned()))
    }

    #[test]
    fn utc_dates() {
        assert_eq!(amz_date(SystemTime::UNIX_EPOCH), "19700101T000000Z");
        assert_eq!(amz_date(SystemTime::UNIX_EPOCH + Duration::from_secs(1_440_938_160)), "20150830T123600Z");
        assert_eq!(amz_date(SystemTime::UNIX_EPOCH + Duration::from_secs(951_782_400)), "20000229T000000Z");
    }

    #[test]
    fn sigv4_matches_the_aws_test_suite() {
        // aws-sig-v4-test-suite, get-vanilla.
        let headers = [("Host".to_string(), "example.amazonaws.com".to_string()), ("X-Amz-Date".to_string(), "20150830T123600Z".to_string())];
        let auth = sigv4(&SigV4 {
            method: "GET",
            path: "/",
            query: "",
            headers: &headers,
            body: b"",
            access_key: "AKIDEXAMPLE",
            secret_key: "wJalrXUtnFEMI/K7MDENG+bPxRfiCYEXAMPLEKEY",
            region: "us-east-1",
            service: "service",
            amz_date: "20150830T123600Z",
        });
        assert_eq!(
            auth,
            "AWS4-HMAC-SHA256 Credential=AKIDEXAMPLE/20150830/us-east-1/service/aws4_request, SignedHeaders=host;x-amz-date, Signature=5fa00fa31553b73ebf1942676e86291e8372ff2a2260956d9b8aae1d763fbf31"
        );
    }

    #[test]
    fn sigv4_of_get_secret_value_matches_botocore() {
        // Signed by botocore 1.34 (SigV4Auth) with the same inputs.
        let headers = [
            ("content-type".to_string(), "application/x-amz-json-1.1".to_string()),
            ("host".to_string(), "secretsmanager.eu-west-1.amazonaws.com".to_string()),
            ("x-amz-date".to_string(), "20261101T120000Z".to_string()),
            ("x-amz-security-token".to_string(), "session-token-example".to_string()),
            ("x-amz-target".to_string(), "secretsmanager.GetSecretValue".to_string()),
        ];
        let auth = sigv4(&SigV4 {
            method: "POST",
            path: "/",
            query: "",
            headers: &headers,
            body: br#"{"SecretId":"prod/api"}"#,
            access_key: "AKIDEXAMPLE",
            secret_key: "wJalrXUtnFEMI/K7MDENG+bPxRfiCYEXAMPLEKEY",
            region: "eu-west-1",
            service: "secretsmanager",
            amz_date: "20261101T120000Z",
        });
        assert_eq!(
            auth,
            "AWS4-HMAC-SHA256 Credential=AKIDEXAMPLE/20261101/eu-west-1/secretsmanager/aws4_request, SignedHeaders=content-type;host;x-amz-date;x-amz-security-token;x-amz-target, Signature=f70becee2a9644acc9a5120c10ddd11b7aa991391fc57ecc52851dcff4e2891d"
        );
    }

    #[test]
    fn fields_and_errors_name_no_value() {
        let doc = serde_json::json!({"user": "u", "port": 5432});
        assert_eq!(pick_field(&doc, Some("port"), "#k").unwrap(), "5432");
        let e = pick_field(&doc, Some("nope"), "#k").unwrap_err();
        assert!(e.contains("port, user") && !e.contains("5432"), "{e}");
        assert!(pick_field(&doc, None, "#k").unwrap_err().contains("name one with #k"));
        assert_eq!(pick_field(&serde_json::json!({"only": "v"}), None, "#k").unwrap(), "v");
    }

    #[test]
    fn aws_profiles_and_regions() {
        let ini = "[default]\nregion = us-east-1\n[profile dev]\nregion=eu-west-1\ncredential_process = x\n[sso-session dev]\nsso_region=x\n";
        assert_eq!(ini_section(ini, "dev", true).unwrap()["region"], "eu-west-1");
        assert_eq!(ini_section(ini, "default", true).unwrap()["region"], "us-east-1");
        assert!(ini_section(ini, "none", true).is_none());
        let r = resolver(&[("AWS_REGION", "ap-south-1")]);
        assert_eq!(r.aws_region("arn:aws:secretsmanager:eu-central-1:1:secret:x", None).unwrap(), "eu-central-1");
        assert_eq!(r.aws_region("x", Some("us-west-2".into())).unwrap(), "ap-south-1");
        assert!(resolver(&[]).aws_region("x", None).unwrap_err().contains("AWS_REGION"));
    }

    #[test]
    fn missing_configuration_is_explained() {
        let mut r = resolver(&[]);
        let e = r.resolve("secret:vault:kv/app#k").unwrap_err();
        assert!(e.starts_with("${secret:vault:kv/app#k} (vault): VAULT_ADDR"), "{e}");
        let mut r = resolver(&[("VAULT_ADDR", "http://127.0.0.1:1")]);
        assert!(r.resolve("secret:vault:kv/app#k").unwrap_err().contains("no Vault token"));
        let mut r = resolver(&[("AWS_REGION", "us-east-1")]);
        assert!(r.resolve("secret:aws:x").unwrap_err().contains("no AWS credentials"));
        let mut r = resolver(&[]);
        assert!(r.resolve("secret:azure:v/n").unwrap_err().contains("az login"));
        let mut r = resolver(&[("HITOP_OP_BIN", "/nonexistent-hitop-op")]);
        assert!(r.resolve("secret:op:a/b/c").unwrap_err().contains("not found on PATH"));
    }

    #[test]
    fn vault_kv2_kv1_and_fallback() {
        use httpmock::prelude::*;
        let server = MockServer::start();
        let mounts_v2 = server.mock(|w, t| {
            w.method(GET).path("/v1/sys/internal/ui/mounts/secret/app").header("X-Vault-Token", "tok").header("X-Vault-Namespace", "team");
            t.status(200).json_body(serde_json::json!({"data": {"path": "secret/", "type": "kv", "options": {"version": "2"}}}));
        });
        let read_v2 = server.mock(|w, t| {
            w.method(GET).path("/v1/secret/data/app");
            t.status(200).json_body(serde_json::json!({"data": {"data": {"user": "svc", "token": "s3cr3t-v2"}, "metadata": {"version": 3}}}));
        });
        server.mock(|w, t| {
            w.method(GET).path("/v1/sys/internal/ui/mounts/kv1/app");
            t.status(200).json_body(serde_json::json!({"data": {"path": "kv1/", "type": "kv", "options": {"version": "1"}}}));
        });
        server.mock(|w, t| {
            w.method(GET).path("/v1/kv1/app");
            t.status(200).json_body(serde_json::json!({"data": {"token": "s3cr3t-v1"}}));
        });
        server.mock(|w, t| {
            w.method(GET).path("/v1/sys/internal/ui/mounts/locked/data/app");
            t.status(403).json_body(serde_json::json!({"errors": ["permission denied"]}));
        });
        server.mock(|w, t| {
            w.method(GET).path("/v1/locked/data/app");
            t.status(200).json_body(serde_json::json!({"data": {"data": {"k": "s3cr3t-locked"}, "metadata": {}}}));
        });
        server.mock(|w, t| {
            w.method(GET).path("/v1/sys/internal/ui/mounts/secret/denied");
            t.status(200).json_body(serde_json::json!({"data": {"path": "secret/", "options": {"version": "2"}}}));
        });
        server.mock(|w, t| {
            w.method(GET).path("/v1/secret/data/denied");
            t.status(403).json_body(serde_json::json!({"errors": ["1 error occurred:\n\t* permission denied\n\n"]}));
        });
        let mut r = resolver(&[("VAULT_ADDR", &server.base_url()), ("VAULT_TOKEN", "tok"), ("VAULT_NAMESPACE", "team")]);
        assert_eq!(r.resolve("secret:vault:secret/app#token").unwrap(), "s3cr3t-v2");
        // A second key of the same secret: no second call.
        assert_eq!(r.resolve("secret:vault:secret/app#user").unwrap(), "svc");
        read_v2.assert_calls(1);
        mounts_v2.assert_calls(1);
        assert_eq!(r.resolve("secret:vault:kv1/app").unwrap(), "s3cr3t-v1");
        assert_eq!(r.resolve("secret:vault:locked/data/app#k").unwrap(), "s3cr3t-locked");
        let e = r.resolve("secret:vault:secret/app#nope").unwrap_err();
        assert!(e.contains("no field 'nope'") && e.contains("token, user") && !e.contains("s3cr3t"), "{e}");
        assert!(r.resolve("secret:vault:secret/app").unwrap_err().contains("name one with #<field>"));
        assert!(r.resolve("secret:vault:secret/denied#k").unwrap_err().contains("permission denied"));

        // The token file when VAULT_TOKEN isn't set; BAO_ADDR works too.
        let home = tempfile::tempdir().unwrap();
        std::fs::write(home.path().join(".vault-token"), "tok\n").unwrap();
        let home_s = home.path().to_string_lossy().to_string();
        let mut r = resolver(&[("BAO_ADDR", &server.base_url()), ("HOME", &home_s), ("VAULT_NAMESPACE", "team")]);
        assert_eq!(r.resolve("secret:vault:secret/app#token").unwrap(), "s3cr3t-v2");
    }

    #[cfg(unix)]
    fn script(dir: &std::path::Path, name: &str, body: &str) -> String {
        use std::os::unix::fs::PermissionsExt;
        let path = dir.join(name);
        std::fs::write(&path, format!("#!/bin/sh\n{body}\n")).unwrap();
        std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o755)).unwrap();
        path.to_string_lossy().to_string()
    }

    #[cfg(unix)]
    #[test]
    fn azure_with_client_credentials_and_az() {
        use httpmock::prelude::*;
        let server = MockServer::start();
        let token = server.mock(|w, t| {
            w.method(POST).path("/tenant-1/oauth2/v2.0/token").body_includes("grant_type=client_credentials").body_includes("client_id=app").body_includes("scope=https%3A%2F%2Fvault.azure.net%2F.default");
            t.status(200).json_body(serde_json::json!({"access_token": "aad-token", "token_type": "Bearer"}));
        });
        let secret = server.mock(|w, t| {
            w.method(GET).path("/kv-prod/secrets/api-key").query_param("api-version", "7.4").header("Authorization", "Bearer aad-token");
            t.status(200).json_body(serde_json::json!({"value": "az-s3cr3t", "id": "x"}));
        });
        server.mock(|w, t| {
            w.method(GET).path("/kv-prod/secrets/api-key/v2").header("Authorization", "Bearer cli-token");
            t.status(200).json_body(serde_json::json!({"value": "az-s3cr3t-v2"}));
        });
        server.mock(|w, t| {
            w.method(GET).path("/kv-prod/secrets/missing");
            t.status(404).json_body(serde_json::json!({"error": {"code": "SecretNotFound", "message": "A secret with (name/id) missing was not found in this key vault."}}));
        });
        let template = format!("{}/{{vault}}", server.base_url());
        let mut r = resolver(&[
            ("AZURE_TENANT_ID", "tenant-1"),
            ("AZURE_CLIENT_ID", "app"),
            ("AZURE_CLIENT_SECRET", "client-secret"),
            ("AZURE_AUTHORITY_HOST", &server.base_url()),
            ("HITOP_AZURE_VAULT_URL", &template),
        ]);
        assert_eq!(r.resolve("secret:azure:kv-prod/api-key").unwrap(), "az-s3cr3t");
        let e = r.resolve("secret:azure:kv-prod/missing").unwrap_err();
        assert!(e.contains("404") && e.contains("SecretNotFound"), "{e}");
        token.assert_calls(1);
        secret.assert_calls(1);

        // No client credentials: the az CLI's token.
        let dir = tempfile::tempdir().unwrap();
        let az = script(dir.path(), "az", r#"[ "$1 $2 $4" = "account get-access-token https://vault.azure.net" ] || exit 9; echo '{"accessToken": "cli-token", "expiresOn": "x"}'"#);
        let mut r = resolver(&[("HITOP_AZ_BIN", &az), ("HITOP_AZURE_VAULT_URL", &template)]);
        assert_eq!(r.resolve("secret:azure:kv-prod/api-key/v2").unwrap(), "az-s3cr3t-v2");
        let failing = script(dir.path(), "az-out", "echo 'Please run az login to setup account.' >&2; exit 1");
        let mut r = resolver(&[("HITOP_AZ_BIN", &failing), ("HITOP_AZURE_VAULT_URL", &template)]);
        assert!(r.resolve("secret:azure:kv-prod/api-key").unwrap_err().contains("Please run az login"));
    }

    #[cfg(unix)]
    #[test]
    fn one_password_through_op_read() {
        let dir = tempfile::tempdir().unwrap();
        let op = script(dir.path(), "op", r#"[ "$1" = read ] && [ "$3" = --no-newline ] || exit 9; case "$2" in "op://Dev/Stripe/api key") printf 'op-s3cr3t';; *) echo "[ERROR] could not read secret '$2': item not found" >&2; exit 1;; esac"#);
        let mut r = resolver(&[("HITOP_OP_BIN", &op)]);
        assert_eq!(r.resolve("secret:op://Dev/Stripe/api key").unwrap(), "op-s3cr3t");
        assert_eq!(r.resolve("secret:op:Dev/Stripe/api key").unwrap(), "op-s3cr3t");
        let e = r.resolve("secret:op:Dev/Nope/field").unwrap_err();
        assert!(e.starts_with("${secret:op:Dev/Nope/field} (op): `op read`") && e.contains("item not found"), "{e}");
    }

    #[cfg(unix)]
    #[test]
    fn aws_credentials_from_profile_process_and_cli() {
        let dir = tempfile::tempdir().unwrap();
        let aws_dir = dir.path().join(".aws");
        std::fs::create_dir(&aws_dir).unwrap();
        std::fs::write(aws_dir.join("credentials"), "[default]\naws_access_key_id = AKIDFILE\naws_secret_access_key = file-secret\n").unwrap();
        std::fs::write(aws_dir.join("config"), "[profile proc]\nregion = eu-west-1\ncredential_process = echo '{\"Version\": 1, \"AccessKeyId\": \"AKIDPROC\", \"SecretAccessKey\": \"p\", \"SessionToken\": \"t\"}'\n[profile sso]\nsso_session = corp\n").unwrap();
        let home = dir.path().to_string_lossy().to_string();
        let r = resolver(&[("HOME", &home)]);
        assert_eq!(r.aws_credentials_chain(&AwsProfile::load(&r)).unwrap().access_key, "AKIDFILE");
        let r = resolver(&[("HOME", &home), ("AWS_PROFILE", "proc")]);
        let p = AwsProfile::load(&r);
        assert_eq!(p.get("region").as_deref(), Some("eu-west-1"));
        let c = r.aws_credentials_chain(&p).unwrap();
        assert_eq!((c.access_key.as_str(), c.session_token.as_deref()), ("AKIDPROC", Some("t")));
        let aws = script(dir.path(), "aws", r#"[ "$1 $2 $5 $6" = "configure export-credentials --profile sso" ] || exit 9; echo '{"Version": 1, "AccessKeyId": "AKIDSSO", "SecretAccessKey": "s"}'"#);
        let r = resolver(&[("HOME", &home), ("AWS_PROFILE", "sso"), ("HITOP_AWS_BIN", &aws)]);
        assert_eq!(r.aws_credentials_chain(&AwsProfile::load(&r)).unwrap().access_key, "AKIDSSO");
        let r = resolver(&[("HOME", &home), ("AWS_ACCESS_KEY_ID", "AKIDENV"), ("AWS_SECRET_ACCESS_KEY", "e")]);
        assert_eq!(r.aws_credentials_chain(&AwsProfile::load(&r)).unwrap().access_key, "AKIDENV");
    }

    #[test]
    fn commands_time_out() {
        if cfg!(windows) {
            return;
        }
        let started = Instant::now();
        let e = run_command("sh", &["-c", "sleep 5"], Duration::from_millis(200)).unwrap_err();
        assert!(e.contains("timed out"), "{e}");
        assert!(started.elapsed() < Duration::from_secs(3));
        let e = run_command("sh", &["-c", "echo VALUE; echo 'not signed in' >&2; exit 3"], Duration::from_secs(5)).unwrap_err();
        assert!(e.contains("not signed in") && !e.contains("VALUE"), "{e}");
    }
}
