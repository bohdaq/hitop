//! Settings from the environment (all optional for a local run):
//!
//! - `HITOP_SYNC_DATABASE`: `sqlite:<path>` (default `sqlite:hitop-sync.db`) or `postgres://...`
//! - `HITOP_SYNC_PUBLIC_URL`: where clients reach this server (default `http://localhost:7881`);
//!   sign-in links point at `<url>/signin`
//! - `HITOP_SYNC_RP_ID`: passkey relying party id (default: the public URL's host). Set it to a
//!   parent domain (`hitop.rws8.tech`) so the web app and this server share passkeys.
//! - `HITOP_SYNC_RP_ORIGINS`: comma separated origins allowed to use passkeys (default: the
//!   public URL)
//! - `HITOP_SYNC_ALLOWED_ORIGINS`: comma separated web origins allowed by CORS (default none)
//! - `HITOP_SYNC_MAX_BLOB_BYTES` (default 4 MiB), `HITOP_SYNC_MAX_WORKSPACE_BYTES` (256 MiB),
//!   `HITOP_SYNC_MAX_WORKSPACES` per user (50)
//! - `HITOP_SYNC_TRUSTED_PROXIES`: comma separated peer IPs whose `X-Forwarded-For` names the
//!   client (default `127.0.0.1,::1`: the front proxy on the same host)
//! - `HITOP_SYNC_AUDIT_IP`: `truncated` (default: IPv4 /24, IPv6 /48) or `off`
//! - `HITOP_SYNC_AUDIT_RETENTION_DAYS`: audit rows older than this are purged (default 365; 0
//!   keeps them forever)
//! - `HITOP_SYNC_LIMITS`: `off` (default: a self-hosted server has no plans, seats or per-org
//!   limits) or `plans` (HITOP Cloud: each org's plan limits are enforced, limits.rs)
//! - `HITOP_SYNC_PORTAL_URL`: the account portal, named in limit errors (default
//!   `https://cloud.hitop.rws8.tech/`)
//! - SSO (OpenID Connect): see oidc.rs (`HITOP_SYNC_OIDC_*`, `HITOP_SYNC_SSO_REQUIRED`)
//! - mail: see mail.rs

use url::Url;

use crate::audit::IpPolicy;
use crate::oidc::OidcConfig;

#[derive(Clone, Debug)]
pub struct Config {
    pub database: String,
    pub public_url: Url,
    pub rp_id: String,
    pub rp_origins: Vec<Url>,
    pub cors_origins: Vec<String>,
    pub max_blob_bytes: usize,
    pub max_workspace_bytes: i64,
    pub max_workspaces: i64,
    /// Sign-in links per address per hour, and in total per hour.
    pub links_per_email_hour: i64,
    pub links_per_hour: i64,
    pub link_ttl_secs: i64,
    pub session_ttl_secs: i64,
    pub trusted_proxies: Vec<String>,
    pub audit_ip: IpPolicy,
    pub audit_retention_days: i64,
    /// Sign-in with an OpenID Connect provider; `None` when not configured.
    pub oidc: Option<OidcConfig>,
    /// Enforce each org's plan limits (`HITOP_SYNC_LIMITS=plans`); off on a self-hosted server.
    pub plan_limits: bool,
    /// The account portal (limit errors link to it).
    pub portal_url: String,
}

fn list(value: &str) -> Vec<String> {
    value.split(',').map(|s| s.trim().trim_end_matches('/').to_string()).filter(|s| !s.is_empty()).collect()
}

impl Config {
    pub fn local(public_url: &str) -> Config {
        let public_url = Url::parse(public_url).expect("public URL");
        Config {
            database: "sqlite:hitop-sync.db".into(),
            rp_id: public_url.host_str().unwrap_or("localhost").to_string(),
            rp_origins: vec![public_url.clone()],
            public_url,
            cors_origins: vec![],
            max_blob_bytes: 4 << 20,
            max_workspace_bytes: 256 << 20,
            max_workspaces: 50,
            links_per_email_hour: 5,
            links_per_hour: 500,
            link_ttl_secs: 15 * 60,
            session_ttl_secs: 30 * 24 * 3600,
            trusted_proxies: vec!["127.0.0.1".into(), "::1".into()],
            audit_ip: IpPolicy::Truncated,
            audit_retention_days: 365,
            oidc: None,
            plan_limits: false,
            portal_url: "https://cloud.hitop.rws8.tech/".into(),
        }
    }

    pub fn from_env() -> Result<Config, String> {
        let var = |name: &str| std::env::var(name).ok().filter(|s| !s.is_empty());
        let number = |name: &str, default: i64| -> Result<i64, String> {
            var(name).map_or(Ok(default), |v| v.parse().map_err(|_| format!("{name} must be a number")))
        };
        let mut c = Config::local(&var("HITOP_SYNC_PUBLIC_URL").unwrap_or_else(|| "http://localhost:7881".into()));
        if let Some(db) = var("HITOP_SYNC_DATABASE") {
            c.database = db;
        }
        if let Some(rp) = var("HITOP_SYNC_RP_ID") {
            c.rp_id = rp;
        }
        if let Some(origins) = var("HITOP_SYNC_RP_ORIGINS") {
            c.rp_origins = list(&origins).iter().map(|o| Url::parse(o).map_err(|_| format!("HITOP_SYNC_RP_ORIGINS: {o} is not a URL"))).collect::<Result<_, _>>()?;
        }
        if let Some(origins) = var("HITOP_SYNC_ALLOWED_ORIGINS") {
            c.cors_origins = list(&origins);
        }
        c.max_blob_bytes = number("HITOP_SYNC_MAX_BLOB_BYTES", c.max_blob_bytes as i64)? as usize;
        c.max_workspace_bytes = number("HITOP_SYNC_MAX_WORKSPACE_BYTES", c.max_workspace_bytes)?;
        c.max_workspaces = number("HITOP_SYNC_MAX_WORKSPACES", c.max_workspaces)?;
        if let Some(proxies) = var("HITOP_SYNC_TRUSTED_PROXIES") {
            c.trusted_proxies = list(&proxies);
        }
        c.audit_ip = match var("HITOP_SYNC_AUDIT_IP").as_deref() {
            None | Some("truncated") => IpPolicy::Truncated,
            Some("off") => IpPolicy::Off,
            Some(other) => return Err(format!("HITOP_SYNC_AUDIT_IP must be truncated or off, not {other}")),
        };
        c.audit_retention_days = number("HITOP_SYNC_AUDIT_RETENTION_DAYS", c.audit_retention_days)?;
        c.oidc = OidcConfig::from_env(&var)?;
        c.plan_limits = match var("HITOP_SYNC_LIMITS").as_deref() {
            None | Some("off") => false,
            Some("plans") => true,
            Some(other) => return Err(format!("HITOP_SYNC_LIMITS must be off or plans, not {other}")),
        };
        if let Some(url) = var("HITOP_SYNC_PORTAL_URL") {
            c.portal_url = if url.ends_with('/') { url } else { format!("{url}/") };
        }
        Ok(c)
    }

    /// A blob travels base64 encoded inside JSON: leave room for that.
    pub fn max_request_bytes(&self) -> usize {
        self.max_blob_bytes / 3 * 4 + 64 * 1024
    }

    pub fn webauthn(&self) -> Result<webauthn_rs::Webauthn, String> {
        let first = self.rp_origins.first().ok_or("HITOP_SYNC_RP_ORIGINS is empty")?;
        let mut builder = webauthn_rs::WebauthnBuilder::new(&self.rp_id, first).map_err(|e| format!("passkeys: {e} (rp id {} for {first})", self.rp_id))?.rp_name("hitop sync");
        for origin in &self.rp_origins[1..] {
            builder = builder.append_allowed_origin(origin);
        }
        builder.build().map_err(|e| format!("passkeys: {e}"))
    }
}
