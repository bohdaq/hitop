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
//! - Request rate limits: see ratelimit.rs (`HITOP_SYNC_RATE_LIMITS=off`, `HITOP_SYNC_RATE_*`)
//! - `HITOP_SYNC_MAX_JSON_BYTES`: the largest request body except a blob upload (default 1 MiB)
//! - SSO (OpenID Connect): see oidc.rs (`HITOP_SYNC_OIDC_*`, `HITOP_SYNC_SSO_REQUIRED`)
//! - Passwords: see password.rs (`HITOP_SYNC_PASSWORDS=off`, `HITOP_SYNC_PASSWORD_PEPPER[_FILE]`)
//! - "Continue with Google": `HITOP_SYNC_GOOGLE_CLIENT_ID` and `HITOP_SYNC_GOOGLE_CLIENT_SECRET`
//!   (or `_FILE`), redirect URI `<public URL>/v1/auth/google/callback`; off when unset (oidc.rs)
//! - "Continue with GitHub": `HITOP_SYNC_GITHUB_CLIENT_ID` and `HITOP_SYNC_GITHUB_CLIENT_SECRET`
//!   (or `_FILE`), callback URL `<public URL>/v1/auth/github/callback`; off when unset (github.rs)
//! - Billing (HITOP Cloud, Stripe Managed Payments): see billing.rs (`HITOP_SYNC_STRIPE_*`, off
//!   unless `HITOP_SYNC_STRIPE_SECRET_KEY` and `HITOP_SYNC_STRIPE_WEBHOOK_SECRET` are set) and `HITOP_SYNC_TRIAL_DAYS` (14; 0 = no Team trials)
//! - Run history and alerts: see runs.rs (`HITOP_SYNC_MAX_RUN_BYTES`, `HITOP_SYNC_RUN_RETENTION_DAYS`,
//!   `HITOP_SYNC_ALERTS_PER_HOUR`, `HITOP_SYNC_ALERT_WEBHOOKS`)
//! - `HITOP_SYNC_HOST_KEY` (64 hex characters) or `HITOP_SYNC_HOST_KEY_FILE`: the host key the
//!   server seals its own secrets under (hostkey.rs). Without it, single sign-on per organization
//!   is off.
//! - `HITOP_SYNC_DNS_URL`: a DNS-over-HTTPS resolver's JSON API for domain verification (orgsso.rs,
//!   e.g. `https://cloudflare-dns.com/dns-query`; unset = domains can't be verified)
//! - Audit anchors: `HITOP_SYNC_AUDIT_ANCHORS` (`on` default, `off`), see anchors.rs
//! - mail: see mail.rs

use url::Url;

use crate::audit::IpPolicy;
use crate::billing::BillingConfig;
use crate::github::GithubConfig;
use crate::oidc::OidcConfig;
use crate::password::PasswordConfig;
use crate::ratelimit::RateLimits;

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
    /// Request rate limits (ratelimit.rs).
    pub rate_limits: RateLimits,
    /// The largest request body of any request but a blob upload.
    pub max_json_bytes: usize,
    /// Password sign-in (password.rs).
    pub password: PasswordConfig,
    /// "Continue with Google" (an OpenID Connect provider); `None` when not configured.
    pub google: Option<OidcConfig>,
    /// "Continue with GitHub" (OAuth 2); `None` when not configured.
    pub github: Option<GithubConfig>,
    /// Stripe billing (billing.rs); `None` when not configured.
    pub billing: Option<BillingConfig>,
    /// Days of the Team trial (0 = none).
    pub trial_days: i64,
    /// Run history and alerts (runs.rs).
    pub runs: crate::runs::RunsConfig,
    /// The host key (hostkey.rs); `None` = per-org SSO off.
    pub host_key: Option<crate::hostkey::HostKey>,
    /// DNS-over-HTTPS JSON endpoint for domain verification; `None` = off.
    pub dns_url: Option<String>,
    /// Daily audit anchors (anchors.rs).
    pub audit_anchors: bool,
    /// Git sync of the personal library through a GitHub App (gitsync.rs); `None` = off.
    pub git: Option<crate::gitsync::GitConfig>,
    /// Monitors (monitors.rs): the binary run as `<bin> monitor-run` for each run
    /// (`HITOP_SYNC_MONITOR_BIN`, default this executable; `None` = monitors off).
    pub monitor_bin: Option<std::path::PathBuf>,
    /// Addresses monitors may never reach besides the private ranges (`HITOP_SYNC_DENY_IPS`: the
    /// host's own public IP).
    pub deny_ips: Vec<String>,
    /// Tests only (`HITOP_SYNC_MONITOR_ALLOW_PRIVATE=1`): monitors may reach private addresses.
    pub monitor_allow_private: bool,
}

/// `<PREFIX>_CLIENT_ID` and `<PREFIX>_CLIENT_SECRET` (or `<PREFIX>_CLIENT_SECRET_FILE`): both or
/// neither.
pub fn client_pair(var: &dyn Fn(&str) -> Option<String>, prefix: &str) -> Result<Option<(String, String)>, String> {
    let id = var(&format!("{prefix}_CLIENT_ID"));
    let secret = match (var(&format!("{prefix}_CLIENT_SECRET")), var(&format!("{prefix}_CLIENT_SECRET_FILE"))) {
        (Some(s), _) => Some(s),
        (None, Some(path)) => Some(std::fs::read_to_string(&path).map_err(|e| format!("{prefix}_CLIENT_SECRET_FILE {path}: {e}"))?.trim().to_string()),
        (None, None) => None,
    };
    match (id, secret) {
        (None, None) => Ok(None),
        (Some(i), Some(s)) if !s.is_empty() => Ok(Some((i, s))),
        _ => Err(format!("{prefix}_CLIENT_ID and {prefix}_CLIENT_SECRET (or _FILE) go together")),
    }
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
            rate_limits: RateLimits::defaults(),
            max_json_bytes: 1 << 20,
            password: PasswordConfig::default(),
            google: None,
            github: None,
            billing: None,
            trial_days: 14,
            runs: crate::runs::RunsConfig::default(),
            host_key: None,
            dns_url: None,
            audit_anchors: true,
            git: None,
            monitor_bin: None,
            deny_ips: Vec::new(),
            monitor_allow_private: false,
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
        c.password = PasswordConfig::from_env(&var)?;
        c.google = crate::oidc::google_from_env(&var)?;
        c.github = GithubConfig::from_env(&var)?;
    c.git = crate::gitsync::GitConfig::from_env(&var)?;
        c.billing = BillingConfig::from_env(&var)?;
        c.trial_days = number("HITOP_SYNC_TRIAL_DAYS", c.trial_days)?;
        c.runs = crate::runs::RunsConfig::from_env(&var)?;
        c.host_key = crate::hostkey::HostKey::from_env(&var)?;
        c.monitor_bin = match var("HITOP_SYNC_MONITOR_BIN") {
            Some(p) if p == "off" => None,
            Some(p) => Some(p.into()),
            None => std::env::current_exe().ok(),
        };
        c.deny_ips = var("HITOP_SYNC_DENY_IPS").map(|v| list(&v)).unwrap_or_default();
        c.monitor_allow_private = var("HITOP_SYNC_MONITOR_ALLOW_PRIVATE").as_deref() == Some("1");
        if let Some(u) = var("HITOP_SYNC_DNS_URL").filter(|u| u != "off") {
            if !crate::oidc::secure_url(&u) {
                return Err(format!("HITOP_SYNC_DNS_URL must be an https:// URL (http:// only on localhost): {u}"));
            }
            c.dns_url = Some(u);
        }
        c.audit_anchors = match var("HITOP_SYNC_AUDIT_ANCHORS").as_deref() {
            None | Some("on") => true,
            Some("off") => false,
            Some(other) => return Err(format!("HITOP_SYNC_AUDIT_ANCHORS must be on or off, not {other}")),
        };
        match var("HITOP_SYNC_RATE_LIMITS").as_deref() {
            None | Some("on") => {}
            Some("off") => {
                c.rate_limits = RateLimits::off();
                c.password.backoff = false;
            }
            Some(other) => return Err(format!("HITOP_SYNC_RATE_LIMITS must be on or off, not {other}")),
        }
        let per = |name: &str, default: u32| -> Result<u32, String> { number(name, default.into()).and_then(|n| u32::try_from(n).map_err(|_| format!("{name} must be 0 or more"))) };
        c.rate_limits.auth_per_minute = per("HITOP_SYNC_RATE_AUTH", c.rate_limits.auth_per_minute)?;
        c.rate_limits.write_per_minute = per("HITOP_SYNC_RATE_WRITE", c.rate_limits.write_per_minute)?;
        c.rate_limits.write_ip_per_minute = per("HITOP_SYNC_RATE_WRITE_IP", c.rate_limits.write_ip_per_minute)?;
        c.rate_limits.create_per_hour = per("HITOP_SYNC_RATE_CREATE", c.rate_limits.create_per_hour)?;
        c.max_json_bytes = number("HITOP_SYNC_MAX_JSON_BYTES", c.max_json_bytes as i64)? as usize;
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
