//! Single sign-on per organization (HITOP Cloud C5, Business): each org may bring its own OpenID
//! Connect provider, for the addresses of the domains it proved it owns.
//!
//! - **Configuration** (`GET/PUT/DELETE /v1/orgs/:org/sso`, owners; `POST .../sso/test`): issuer,
//!   client id, client secret, scopes, button name, groups claim and required groups, "SSO
//!   required", "assume verified". The client secret is sealed at rest under the host key
//!   (hostkey.rs, label `org-sso-secret:<org>`) and never sent back. The flow is oidc.rs's, keyed
//!   by org: one callback URL (`<public URL>/v1/auth/sso/callback`) for every org and for the
//!   server's own SSO; the sign-in's `state` names the org.
//! - **Domains** (`GET/POST /v1/orgs/:org/domains`, `POST .../domains/:domain/verify`, `DELETE
//!   .../domains/:domain`, admins): an org claims `corp.example` by publishing the TXT record
//!   `_hitop-verify.corp.example` = `hitop-verify=<token>`; the server looks it up over DNS over
//!   HTTPS (`HITOP_SYNC_DNS_URL`, a resolver's JSON API, e.g. `https://cloudflare-dns.com/dns-query`;
//!   unset = verification off). A domain is verified for one org at a time; free-mail domains
//!   can't be claimed.
//! - **The rule that keeps orgs apart**: an org's provider may sign in only addresses in that
//!   org's *verified* domains (its `allowed_domains` are exactly those, never "any"). So org B's
//!   provider can't sign in `ada@a.example` of org A, whatever it asserts; and with "SSO required"
//!   the addresses of an org's domains sign in only through that org's provider (no mail codes,
//!   passwords, passkeys, Google, GitHub, or the server's own SSO).
//! - **Which provider**: `POST /v1/auth/sso/start` takes `org`, or finds it from the `email` hint's
//!   domain; without either it uses the server's own SSO (`HITOP_SYNC_OIDC_*`). Old clients (1.1,
//!   1.2) send the hint with `hitop sync login --sso --email <address>`.
//!
//! All of it is off unless the server has a host key (`HITOP_SYNC_HOST_KEY_FILE`): every endpoint
//! here then answers 404 `org_sso_off`. The hosted test server keeps it unset (owner decision
//! Q15). On HITOP Cloud (`HITOP_SYNC_LIMITS=plans`) configuring needs the Business or Enterprise
//! plan (402 `sso`); signing in keeps working when a plan lapses, so nobody is locked out.

use std::collections::HashMap;
use std::sync::{Arc, Mutex, RwLock};

use rust_web_server::request::Request;
use rust_web_server::router::PathParams;
use serde::Deserialize;
use serde_json::{json, Value as Json};

use crate::api::{body, random_hex, ApiError, ApiResult, State};
use crate::audit::{self, Event};
use crate::db::{DbError, Tx};
use crate::oidc::{OidcConfig, Sso};
use crate::orgs::{self, OrgRole};

/// Plans that may configure SSO and claim domains (with `HITOP_SYNC_LIMITS=plans`).
pub const SSO_PLANS: &[&str] = &["business", "enterprise"];
const MAX_DOMAINS: i64 = 20;
const TXT_PREFIX: &str = "hitop-verify=";

/// What the server knows about a verified domain, in memory (sign-in checks run where no database
/// transaction can be opened).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DomainPolicy {
    pub org: String,
    /// The org has SSO configured and on.
    pub sso: bool,
    /// ... and "SSO required".
    pub required: bool,
    pub name: String,
}

/// Per-org SSO state: verified domains, and one provider (metadata and key cache) per org.
#[derive(Default)]
pub struct OrgSso {
    domains: RwLock<HashMap<String, DomainPolicy>>,
    providers: Mutex<HashMap<String, Arc<Sso>>>,
}

impl OrgSso {
    /// The verified domain `domain`'s policy.
    pub fn domain(&self, domain: &str) -> Option<DomainPolicy> {
        self.domains
            .read()
            .unwrap_or_else(|e| e.into_inner())
            .get(domain)
            .cloned()
    }

    /// The policy of the domain of `email`.
    pub fn for_email(&self, email: &str) -> Option<DomainPolicy> {
        self.domain(&domain_of(email))
    }

    pub fn set_domains(&self, map: HashMap<String, DomainPolicy>) {
        *self.domains.write().unwrap_or_else(|e| e.into_inner()) = map;
        self.providers
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .clear();
    }

    /// Does any org sign in by SSO here?
    pub fn any(&self) -> bool {
        self.domains
            .read()
            .unwrap_or_else(|e| e.into_inner())
            .values()
            .any(|p| p.sso)
    }
}

pub fn domain_of(email: &str) -> String {
    email
        .rsplit_once('@')
        .map(|(_, d)| d.to_lowercase())
        .unwrap_or_default()
}

/// Rebuilds the in-memory view from the database (at start and after every change here).
pub fn reload(state: &State) -> Result<(), DbError> {
    let map = state.db.tx(load)?;
    state.org_sso.set_domains(map);
    Ok(())
}

pub fn load(tx: &mut Tx) -> Result<HashMap<String, DomainPolicy>, DbError> {
    let rows = tx.query(
        "SELECT d.domain, d.org_id, COALESCE(s.enabled, 0), COALESCE(s.required, 0), COALESCE(s.name, '') FROM org_domains d LEFT JOIN org_sso s ON s.org_id = d.org_id WHERE d.verified_at > 0",
        &[],
    )?;
    Ok(rows
        .iter()
        .map(|r| {
            (
                r.text(0),
                DomainPolicy {
                    org: r.text(1),
                    sso: r.int(2) != 0,
                    required: r.int(2) != 0 && r.int(3) != 0,
                    name: r.text(4),
                },
            )
        })
        .collect())
}

fn off() -> ApiError {
    ApiError::new(
        404,
        "org_sso_off",
        "single sign-on per organization isn't available on this server",
    )
}

fn host_key(state: &State) -> Result<&crate::hostkey::HostKey, ApiError> {
    state.config.host_key.as_ref().ok_or_else(off)
}

fn secret_label(org: &str) -> String {
    format!("org-sso-secret:{org}")
}

fn verified_domains(tx: &mut Tx, org: &str) -> Result<Vec<String>, DbError> {
    Ok(tx
        .query(
            "SELECT domain FROM org_domains WHERE org_id = $1 AND verified_at > 0 ORDER BY domain",
            &[org.into()],
        )?
        .iter()
        .map(|r| r.text(0))
        .collect())
}

fn split(s: &str) -> Vec<String> {
    s.split(',')
        .map(|g| g.trim().to_string())
        .filter(|g| !g.is_empty())
        .collect()
}

/// The org's provider configuration with its secret opened; `Err` (a message) when it can't sign
/// anyone in.
fn org_config(state: &State, tx: &mut Tx, org: &str) -> Result<OidcConfig, String> {
    let key = state
        .config
        .host_key
        .as_ref()
        .ok_or("single sign-on per organization is off on this server")?;
    let r = tx
        .query_one("SELECT issuer, client_id, client_secret, scopes, name, groups_claim, required_groups, required, assume_verified, enabled FROM org_sso WHERE org_id = $1", &[org.into()])
        .map_err(|e| e.to_string())?
        .ok_or("this organization has no single sign-on")?;
    if r.int(9) == 0 {
        return Err("this organization's single sign-on is turned off".into());
    }
    let domains = verified_domains(tx, org).map_err(|e| e.to_string())?;
    if domains.is_empty() {
        return Err("this organization has no verified domain yet".into());
    }
    let secret = key
        .open(&secret_label(org), &r.bytes(2))
        .map_err(|e| format!("the client secret: {e}"))?;
    Ok(OidcConfig {
        issuer: r.text(0),
        client_id: r.text(1),
        client_secret: String::from_utf8(secret).map_err(|_| "the client secret isn't text")?,
        scopes: r.text(3),
        name: r.text(4),
        allowed_domains: domains,
        groups_claim: r.text(5),
        required_groups: split(&r.text(6)),
        sso_required: r.int(7) != 0,
        assume_verified: r.int(8) != 0,
    })
}

/// The org's provider (cached until its configuration or domains change).
pub fn provider(state: &State, org: &str) -> Result<Arc<Sso>, String> {
    if let Some(p) = state
        .org_sso
        .providers
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .get(org)
    {
        return Ok(p.clone());
    }
    let c = state
        .db
        .tx(|tx| Ok::<_, DbError>(org_config(state, tx, org)))
        .map_err(|e| e.to_string())??;
    let sso = Arc::new(Sso::provider_of("sso", Some(c), vec![])?);
    state
        .org_sso
        .providers
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .insert(org.to_string(), sso.clone());
    Ok(sso)
}

/// The caller (a person with a session, not an access token, for changes) with at least `need`
/// in a team org, on a server with a host key.
fn caller(
    state: &State,
    tx: &mut Tx,
    request: &Request,
    org: &str,
    need: OrgRole,
    change: bool,
) -> Result<(crate::auth::User, orgs::Org), ApiError> {
    host_key(state)?;
    let (user, o) = orgs::org_caller(state, tx, request, org, need, change)?;
    if o.personal {
        return Err(ApiError::bad(
            "single sign-on is for an organization, not a personal account",
        ));
    }
    Ok((user, o))
}

fn plan(state: &State, tx: &mut Tx, org: &str) -> Result<(), ApiError> {
    crate::limits::require_plan(
        state,
        tx,
        org,
        "sso",
        "single sign-on and domain claims",
        SSO_PLANS,
    )
}

fn sso_json(state: &State, tx: &mut Tx, org: &str) -> Result<Json, ApiError> {
    let row = tx.query_one("SELECT issuer, client_id, scopes, name, groups_claim, required_groups, required, assume_verified, enabled, updated_at FROM org_sso WHERE org_id = $1", &[org.into()])?;
    let domains = domains_json(tx, org)?;
    let allowed = !state.config.plan_limits || {
        let e = crate::limits::entitlement(tx, org)?;
        SSO_PLANS.contains(&e.plan_in_force())
    };
    let mut out = json!({
        "available": true,
        "allowed": allowed,
        "redirectUri": crate::oidc::redirect_uri(state),
        "dnsVerification": state.config.dns_url.is_some(),
        "domains": domains,
        "configured": row.is_some(),
    });
    if let Some(r) = row {
        out["config"] = json!({
            "issuer": r.text(0),
            "clientId": r.text(1),
            "hasSecret": true,
            "scopes": r.text(2),
            "name": r.text(3),
            "groupsClaim": r.text(4),
            "requiredGroups": split(&r.text(5)),
            "required": r.int(6) != 0,
            "assumeVerified": r.int(7) != 0,
            "enabled": r.int(8) != 0,
            "updatedAt": r.int(9),
        });
    }
    Ok(out)
}

/// `GET /v1/orgs/:org/sso` (admin): the org's SSO settings (never the client secret), its domains
/// and whether the plan allows changing them. `{"available": false}` on a server without a host
/// key.
pub fn get(state: &State, request: &Request, params: &PathParams) -> ApiResult {
    let org = params.get("org").unwrap_or_default().to_string();
    state.db.tx(|tx| {
        if state.config.host_key.is_none() {
            orgs::org_caller(state, tx, request, &org, OrgRole::Admin, false)?;
            return Ok((200, json!({ "available": false, "reason": "single sign-on per organization isn't available on this server" })));
        }
        caller(state, tx, request, &org, OrgRole::Admin, false)?;
        Ok((200, sso_json(state, tx, &org)?))
    })
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
struct SsoIn {
    issuer: String,
    client_id: String,
    #[serde(default)]
    client_secret: Option<String>,
    #[serde(default)]
    scopes: Option<String>,
    #[serde(default)]
    name: Option<String>,
    #[serde(default)]
    groups_claim: Option<String>,
    #[serde(default)]
    required_groups: Option<Vec<String>>,
    #[serde(default)]
    required: Option<bool>,
    #[serde(default)]
    assume_verified: Option<bool>,
    #[serde(default)]
    enabled: Option<bool>,
}

fn clean(field: &str, value: &str, max: usize) -> Result<String, ApiError> {
    let v = value.trim();
    if v.is_empty() || v.len() > max || v.chars().any(char::is_control) {
        return Err(ApiError::bad(format!(
            "{field} must be 1 to {max} characters"
        )));
    }
    Ok(v.to_string())
}

/// `PUT /v1/orgs/:org/sso {issuer, clientId, clientSecret?, scopes?, name?, groupsClaim?,
/// requiredGroups?, required?, assumeVerified?, enabled?}` (owner, a session): sets the org's
/// provider. `clientSecret` is needed the first time; left out, the stored one stays. "SSO
/// required" and "assume verified" need a verified domain.
pub fn put(state: &State, request: &Request, params: &PathParams) -> ApiResult {
    let org = params.get("org").unwrap_or_default().to_string();
    let input: SsoIn = body(request)?;
    let out = state.db.tx(|tx| {
        let (user, _) = caller(state, tx, request, &org, OrgRole::Owner, true)?;
        plan(state, tx, &org)?;
        let key = host_key(state)?;
        let existing = tx.query_one("SELECT client_secret, scopes, name, groups_claim, required_groups, required, assume_verified, enabled FROM org_sso WHERE org_id = $1", &[(&org).into()])?;
        let secret = match (&input.client_secret, &existing) {
            (Some(s), _) => key.seal(&secret_label(&org), clean("clientSecret", s, 1000)?.as_bytes()),
            (None, Some(r)) => r.bytes(0),
            (None, None) => return Err(ApiError::bad("clientSecret is needed the first time")),
        };
        let prev = |i: usize| existing.as_ref().map(|r| r.text(i));
        let prev_flag = |i: usize, default: bool| existing.as_ref().map_or(default, |r| r.int(i) != 0);
        let c = OidcConfig {
            issuer: clean("issuer", &input.issuer, 500)?.trim_end_matches('/').to_string(),
            client_id: clean("clientId", &input.client_id, 500)?,
            client_secret: "-".into(),
            scopes: match &input.scopes {
                Some(s) => clean("scopes", s, 500)?,
                None => prev(1).unwrap_or_else(|| "openid email profile".into()),
            },
            name: match &input.name {
                Some(s) => clean("name", s, 60)?,
                None => prev(2).unwrap_or_else(|| "SSO".into()),
            },
            allowed_domains: verified_domains(tx, &org)?,
            groups_claim: match &input.groups_claim {
                Some(s) => clean("groupsClaim", s, 100)?,
                None => prev(3).unwrap_or_else(|| "groups".into()),
            },
            required_groups: match &input.required_groups {
                Some(g) => g.iter().map(|s| clean("requiredGroups", s, 200)).collect::<Result<Vec<_>, _>>()?,
                None => prev(4).map(|s| split(&s)).unwrap_or_default(),
            },
            sso_required: input.required.unwrap_or_else(|| prev_flag(5, false)),
            assume_verified: input.assume_verified.unwrap_or_else(|| prev_flag(6, false)),
        };
        let enabled = input.enabled.unwrap_or_else(|| prev_flag(7, true));
        c.check().map_err(|e| ApiError::bad(e.replace("HITOP_SYNC_OIDC_ASSUME_VERIFIED needs HITOP_SYNC_OIDC_ALLOWED_DOMAINS (domains your provider owns)", "assumeVerified needs a verified domain").replace("HITOP_SYNC_OIDC_ISSUER", "issuer").replace("HITOP_SYNC_OIDC_SCOPES", "scopes")))?;
        if c.sso_required && c.allowed_domains.is_empty() {
            return Err(ApiError::bad("\"SSO required\" needs a verified domain"));
        }
        let now = state.now();
        tx.execute(
            "INSERT INTO org_sso (org_id, issuer, client_id, client_secret, scopes, name, groups_claim, required_groups, required, assume_verified, enabled, updated_by, updated_at) \
             VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12, $13) ON CONFLICT (org_id) DO UPDATE SET issuer = $2, client_id = $3, client_secret = $4, scopes = $5, name = $6, \
             groups_claim = $7, required_groups = $8, required = $9, assume_verified = $10, enabled = $11, updated_by = $12, updated_at = $13",
            &[
                (&org).into(),
                (&c.issuer).into(),
                (&c.client_id).into(),
                secret.into(),
                (&c.scopes).into(),
                (&c.name).into(),
                (&c.groups_claim).into(),
                c.required_groups.join(",").into(),
                c.sso_required.into(),
                c.assume_verified.into(),
                enabled.into(),
                (&user.id).into(),
                now.into(),
            ],
        )?;
        // Never the secret, in the log or anywhere else.
        audit::record(
            tx,
            state,
            Event::new(&orgs::chain(&org), "org_sso_updated", Some(&user)).target(&org).detail(json!({ "issuer": c.issuer, "clientId": c.client_id, "required": c.sso_required, "enabled": enabled, "secretChanged": input.client_secret.is_some() })),
        )?;
        sso_json(state, tx, &org)
    })?;
    reload(state)?;
    Ok((200, out))
}

/// `DELETE /v1/orgs/:org/sso` (owner, a session): removes the org's provider (people sign in as
/// before SSO; accounts and links stay).
pub fn delete(state: &State, request: &Request, params: &PathParams) -> ApiResult {
    let org = params.get("org").unwrap_or_default().to_string();
    state.db.tx(|tx| {
        let (user, _) = caller(state, tx, request, &org, OrgRole::Owner, true)?;
        if tx.execute("DELETE FROM org_sso WHERE org_id = $1", &[(&org).into()])? == 0 {
            return Err(ApiError::not_found(
                "this organization has no single sign-on",
            ));
        }
        audit::record(
            tx,
            state,
            Event::new(&orgs::chain(&org), "org_sso_deleted", Some(&user)).target(&org),
        )?;
        Ok(())
    })?;
    reload(state)?;
    Ok((204, Json::Null))
}

/// `POST /v1/orgs/:org/sso/test` (admin): fetches the provider's discovery document and keys, so
/// a wrong issuer shows before anyone signs in. `{ok, error?}`.
pub fn test(state: &State, request: &Request, params: &PathParams) -> ApiResult {
    let org = params.get("org").unwrap_or_default().to_string();
    let c = state.db.tx(|tx| {
        caller(state, tx, request, &org, OrgRole::Admin, false)?;
        Ok::<_, ApiError>(org_config(state, tx, &org))
    })?;
    let result = c.and_then(|c| {
        let sso = Sso::provider_of("sso", Some(c.clone()), vec![])?;
        sso.check_provider(&c, state.now())
    });
    Ok((
        200,
        match result {
            Ok(()) => json!({ "ok": true }),
            Err(e) => json!({ "ok": false, "error": e.chars().take(300).collect::<String>() }),
        },
    ))
}

// --- domains ---------------------------------------------------------------------------------

fn domains_json(tx: &mut Tx, org: &str) -> Result<Vec<Json>, DbError> {
    Ok(tx
        .query("SELECT domain, token, verified_at, checked_at, created_at FROM org_domains WHERE org_id = $1 ORDER BY domain", &[org.into()])?
        .iter()
        .map(|r| {
            json!({
                "domain": r.text(0),
                "record": format!("_hitop-verify.{}", r.text(0)),
                "value": format!("{TXT_PREFIX}{}", r.text(1)),
                "verified": r.int(2) > 0,
                "verifiedAt": if r.int(2) > 0 { json!(r.int(2)) } else { Json::Null },
                "checkedAt": if r.int(3) > 0 { json!(r.int(3)) } else { Json::Null },
                "createdAt": r.int(4),
            })
        })
        .collect())
}

/// A domain an org may claim: lower case, letters, digits and hyphens between dots, two labels
/// at least, no free-mail provider.
pub fn valid_domain(d: &str) -> Result<String, ApiError> {
    let d = d.trim().trim_end_matches('.').to_lowercase();
    let bad = || ApiError::bad("domain must be a DNS name such as corp.example");
    if d.len() > 253 || d.split('.').count() < 2 {
        return Err(bad());
    }
    for label in d.split('.') {
        if label.is_empty()
            || label.len() > 63
            || label.starts_with('-')
            || label.ends_with('-')
            || !label
                .bytes()
                .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'-')
        {
            return Err(bad());
        }
    }
    if crate::billing::free_mail(&d) {
        return Err(ApiError::bad(format!(
            "{d} is a public mail provider: an organization can't claim it"
        )));
    }
    Ok(d)
}

/// `GET /v1/orgs/:org/domains` (admin).
pub fn list_domains(state: &State, request: &Request, params: &PathParams) -> ApiResult {
    let org = params.get("org").unwrap_or_default().to_string();
    state.db.tx(|tx| {
        caller(state, tx, request, &org, OrgRole::Admin, false)?;
        Ok((200, json!({ "domains": domains_json(tx, &org)? })))
    })
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct DomainIn {
    domain: String,
}

/// `POST /v1/orgs/:org/domains {"domain"}` (admin, a session) → the TXT record to publish.
pub fn add_domain(state: &State, request: &Request, params: &PathParams) -> ApiResult {
    let org = params.get("org").unwrap_or_default().to_string();
    let input: DomainIn = body(request)?;
    let domain = valid_domain(&input.domain)?;
    state.db.tx(|tx| {
        let (user, _) = caller(state, tx, request, &org, OrgRole::Admin, true)?;
        plan(state, tx, &org)?;
        if tx.query_one("SELECT domain FROM org_domains WHERE org_id = $1 AND domain = $2", &[(&org).into(), (&domain).into()])?.is_some() {
            return Err(ApiError::conflict("domain_exists", format!("{domain} is already claimed by this organization"), json!({})));
        }
        let n = tx.query_one("SELECT COUNT(*) FROM org_domains WHERE org_id = $1", &[(&org).into()])?.map_or(0, |r| r.int(0));
        if n >= MAX_DOMAINS {
            return Err(ApiError::bad(format!("an organization claims at most {MAX_DOMAINS} domains")));
        }
        let token = random_hex::<16>();
        tx.execute("INSERT INTO org_domains (org_id, domain, token, created_by, created_at) VALUES ($1, $2, $3, $4, $5)", &[(&org).into(), (&domain).into(), (&token).into(), (&user.id).into(), state.now().into()])?;
        audit::record(tx, state, Event::new(&orgs::chain(&org), "org_domain_added", Some(&user)).target(&domain))?;
        let all = domains_json(tx, &org)?;
        Ok((201, all.into_iter().find(|d| d["domain"] == domain.as_str()).unwrap_or(Json::Null)))
    })
}

/// The TXT strings of `name`, from the DNS-over-HTTPS resolver (`HITOP_SYNC_DNS_URL`, JSON API:
/// `?name=&type=TXT`, `Accept: application/dns-json`).
pub fn txt_records(state: &State, name: &str) -> Result<Vec<String>, String> {
    let base = state
        .config
        .dns_url
        .as_ref()
        .ok_or("domain verification is off on this server (HITOP_SYNC_DNS_URL)")?;
    let mut url = url::Url::parse(base).map_err(|e| format!("HITOP_SYNC_DNS_URL: {e}"))?;
    url.query_pairs_mut()
        .append_pair("name", name)
        .append_pair("type", "TXT");
    let mut res = state
        .billing_http
        .get(url.as_str())
        .header("Accept", "application/dns-json")
        .call()
        .map_err(|e| format!("DNS lookup: {e}"))?;
    let status = res.status().as_u16();
    let text = res
        .body_mut()
        .with_config()
        .limit(1 << 20)
        .read_to_string()
        .map_err(|e| format!("DNS lookup: {e}"))?;
    if status != 200 {
        return Err(format!("DNS lookup: HTTP {status}"));
    }
    let doc: Json = serde_json::from_str(&text).map_err(|_| "DNS lookup: not JSON".to_string())?;
    if doc["Status"].as_i64().unwrap_or(-1) != 0 {
        return Ok(vec![]);
    }
    Ok(doc["Answer"]
        .as_array()
        .map(|a| {
            a.iter()
                .filter(|r| r["type"].as_i64() == Some(16))
                .filter_map(|r| r["data"].as_str().map(txt_data))
                .collect()
        })
        .unwrap_or_default())
}

/// A TXT record's `data` as resolvers print it: one or more quoted strings (`"a" "b"` = `ab`).
pub fn txt_data(data: &str) -> String {
    let d = data.trim();
    if !d.starts_with('"') {
        return d.to_string();
    }
    let mut out = String::new();
    let mut inside = false;
    let mut chars = d.chars();
    while let Some(c) = chars.next() {
        match (inside, c) {
            (false, '"') => inside = true,
            (true, '"') => inside = false,
            (true, '\\') => {
                if let Some(n) = chars.next() {
                    out.push(n);
                }
            }
            (true, c) => out.push(c),
            _ => {}
        }
    }
    out
}

/// `POST /v1/orgs/:org/domains/:domain/verify` (admin, a session): looks the TXT record up; the
/// domain is verified when one of its strings is the expected value.
pub fn verify_domain(state: &State, request: &Request, params: &PathParams) -> ApiResult {
    let org = params.get("org").unwrap_or_default().to_string();
    let domain = valid_domain(params.get("domain").unwrap_or_default())?;
    let token = state.db.tx(|tx| {
        caller(state, tx, request, &org, OrgRole::Admin, true)?;
        plan(state, tx, &org)?;
        tx.query_one(
            "SELECT token FROM org_domains WHERE org_id = $1 AND domain = $2",
            &[(&org).into(), (&domain).into()],
        )?
        .map(|r| r.text(0))
        .ok_or_else(|| ApiError::not_found("this organization hasn't claimed that domain"))
    })?;
    if state.config.dns_url.is_none() {
        return Err(ApiError::new(
            503,
            "dns_off",
            "domain verification is off on this server (HITOP_SYNC_DNS_URL isn't set)",
        ));
    }
    let record = format!("_hitop-verify.{domain}");
    let found = txt_records(state, &record).map_err(|e| {
        eprintln!("hitop-sync-server: {e}");
        ApiError::new(503, "dns_unreachable", "the DNS lookup failed; try again")
    })?;
    let expected = format!("{TXT_PREFIX}{token}");
    let ok = found.iter().any(|t| t.trim() == expected);
    let out = state.db.tx(|tx| {
        let (user, _) = orgs::org_caller(state, tx, request, &org, OrgRole::Admin, true)?;
        let now = state.now();
        tx.execute("UPDATE org_domains SET checked_at = $1 WHERE org_id = $2 AND domain = $3", &[now.into(), (&org).into(), (&domain).into()])?;
        if ok {
            if let Some(other) = tx.query_one("SELECT org_id FROM org_domains WHERE domain = $1 AND verified_at > 0 AND org_id <> $2", &[(&domain).into(), (&org).into()])? {
                let _ = other;
                return Err(ApiError::conflict("domain_taken", format!("{domain} is verified for another organization; its owners must remove it first (or ask support)"), json!({})));
            }
            let newly = tx.execute("UPDATE org_domains SET verified_at = $1 WHERE org_id = $2 AND domain = $3 AND verified_at = 0", &[now.into(), (&org).into(), (&domain).into()])? > 0;
            if newly {
                audit::record(tx, state, Event::new(&orgs::chain(&org), "org_domain_verified", Some(&user)).target(&domain))?;
            }
        }
        let all = domains_json(tx, &org)?;
        let mut d = all.into_iter().find(|d| d["domain"] == domain.as_str()).unwrap_or(Json::Null);
        if !ok {
            d["problem"] = json!(if found.is_empty() { format!("no TXT record at {record} yet (DNS changes can take a while)") } else { format!("the TXT record at {record} doesn't hold {expected}") });
        }
        Ok::<_, ApiError>(d)
    })?;
    reload(state)?;
    Ok((200, out))
}

/// `DELETE /v1/orgs/:org/domains/:domain` (admin, a session).
pub fn delete_domain(state: &State, request: &Request, params: &PathParams) -> ApiResult {
    let org = params.get("org").unwrap_or_default().to_string();
    let domain = params
        .get("domain")
        .unwrap_or_default()
        .trim()
        .to_lowercase();
    state.db.tx(|tx| {
        let (user, _) = caller(state, tx, request, &org, OrgRole::Admin, true)?;
        if tx.execute(
            "DELETE FROM org_domains WHERE org_id = $1 AND domain = $2",
            &[(&org).into(), (&domain).into()],
        )? == 0
        {
            return Err(ApiError::not_found(
                "this organization hasn't claimed that domain",
            ));
        }
        audit::record(
            tx,
            state,
            Event::new(&orgs::chain(&org), "org_domain_removed", Some(&user)).target(&domain),
        )?;
        Ok(())
    })?;
    reload(state)?;
    Ok((204, Json::Null))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn txt_data_joins_quoted_strings() {
        assert_eq!(txt_data("\"hitop-verify=ab\" \"cd\""), "hitop-verify=abcd");
        assert_eq!(txt_data("\"a\\\"b\""), "a\"b");
        assert_eq!(txt_data("plain"), "plain");
    }

    #[test]
    fn domains_are_checked() {
        assert_eq!(valid_domain(" Corp.Example. ").unwrap(), "corp.example");
        for bad in [
            "corp",
            "-a.example",
            "a..example",
            "a_b.example",
            "gmail.com",
            "x.example/y",
        ] {
            assert!(valid_domain(bad).is_err(), "{bad}");
        }
    }
}
