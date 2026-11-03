//! SCIM 2.0 provisioning per organization (HITOP Cloud C6; RFC 7643 and RFC 7644).
//!
//! An org owner creates a SCIM token in the portal (Business or Enterprise on HITOP Cloud; every
//! team org on a self-hosted server). Without a token an org has no SCIM at all (off by default).
//! The identity provider (Okta, Microsoft Entra ID, Keycloak, ...) then calls `/scim/v2/...` with
//! `Authorization: Bearer hitop_scim_...`; the token names the org, so one base URL serves them all.
//!
//! What the directory maps to:
//!
//! - **Users** are the org's members, by address: `userName` must be the person's email address
//!   (HITOP accounts are addresses). A user with an account joins the org as a member; an address
//!   without one gets an org invite (it joins on first sign-in, orgs.rs). Either takes a seat.
//!   `active: false` (or DELETE) takes the address out of the org and out of the org's workspaces
//!   SCIM gave it. The account itself stays (it may hold personal workspaces) and so do its sessions.
//! - **Groups** carry the roles, set by an org admin in the portal (`PUT /v1/orgs/:org/scim/groups/:id`):
//!   an org role (`admin` or `member`; when any group of the org has one, members without such a
//!   group are `member`) and workspace roles (`viewer` or `editor` in workspaces of the org).
//! - **Workspace roles.** The server can't add anyone to a workspace: that takes the workspace key
//!   wrapped for the person's devices, which only a member's client makes (teams.rs). So a group's
//!   workspace role becomes an invite the person has already accepted (once they have a device key),
//!   and the workspace owner's client adds them as for any accepted invite (`hitop sync members`,
//!   then `hitop sync add <email>`, which checks the device fingerprints). For a member already in
//!   the workspace SCIM sets the role. When a group stops giving a workspace, SCIM takes back only
//!   what it gave (`scim_grants`): the pending invite, or the membership it brought (with the
//!   member's wrapped keys) plus a key rotation request (admin.rs) for the owners, since the server
//!   can't rotate. A person who was in the workspace before SCIM stays in it.
//! - **Owners** (of the org or of a workspace) are never created, demoted or removed by SCIM:
//!   deactivating an org owner is refused (409), a workspace owner keeps the workspace (audited).
//!
//! Every change goes to the org's audit chain (and the workspace's), with `scim:<token name>` as
//! the actor. The server stores what the IdP sends that it needs (userName, externalId,
//! displayName, name parts, active, group names and members); other attributes are ignored.

use std::collections::{BTreeMap, BTreeSet};

use rust_web_server::header::Header;
use rust_web_server::range::Range;
use rust_web_server::request::Request;
use rust_web_server::response::Response;
use rust_web_server::router::PathParams;
use serde::Deserialize;
use serde_json::{json, Value as Json};

use crate::api::{body, new_token, query, random_hex, token_hash, ApiError, ApiResult, State};
use crate::audit::{self, Event};
use crate::auth::normalize_email;
use crate::db::{DbError, Tx};
use crate::orgs::{self, OrgRole};

pub const TOKEN_PREFIX: &str = "hitop_scim_";
const USER: &str = "urn:ietf:params:scim:schemas:core:2.0:User";
const GROUP: &str = "urn:ietf:params:scim:schemas:core:2.0:Group";
const LIST: &str = "urn:ietf:params:scim:api:messages:2.0:ListResponse";
const PATCH: &str = "urn:ietf:params:scim:api:messages:2.0:PatchOp";
const ERROR: &str = "urn:ietf:params:scim:api:messages:2.0:Error";
const MAX_TOKENS: i64 = 5;
const MAX_GROUPS: i64 = 1000;
const MAX_PAGE: usize = 200;
/// SCIM invites stay open while the group gives the workspace (refreshed on every change).
const INVITE_TTL: i64 = 365 * 24 * 3600;

/// The marker in `invites.created_by` for invites SCIM made for `org`.
fn invite_marker(org: &str) -> String {
    format!("scim:{org}")
}

/// A SCIM error (RFC 7644 section 3.12): `status` is a string there.
#[derive(Debug)]
pub struct ScimError {
    status: u16,
    scim_type: Option<&'static str>,
    detail: String,
}

impl ScimError {
    fn new(status: u16, scim_type: Option<&'static str>, detail: impl Into<String>) -> Self {
        ScimError { status, scim_type, detail: detail.into() }
    }
    fn bad(scim_type: &'static str, detail: impl Into<String>) -> Self {
        Self::new(400, Some(scim_type), detail)
    }
    fn not_found(what: &str) -> Self {
        Self::new(404, None, format!("no such {what}"))
    }
}

impl From<ApiError> for ScimError {
    fn from(e: ApiError) -> Self {
        let scim_type = match e.status {
            409 => Some("uniqueness"),
            400 => Some("invalidValue"),
            _ => None,
        };
        ScimError { status: e.status, scim_type, detail: e.message }
    }
}

impl From<DbError> for ScimError {
    fn from(e: DbError) -> Self {
        ApiError::from(e).into()
    }
}

type ScimResult = Result<(u16, Json), ScimError>;

/// Who is calling: the org a token belongs to and the actor named in the audit log.
struct Ctx {
    org: String,
    actor: String,
    now: i64,
    base: String,
}

fn scim_response(status: u16, body: &Json) -> Response {
    if status == 204 {
        return crate::api::json_response(204, &Json::Null);
    }
    let mut r = crate::api::json_response(status, &Json::Null);
    r.content_range_list = vec![Range::get_content_range(body.to_string().into_bytes(), "application/scim+json".into())];
    r
}

fn error_response(e: &ScimError) -> Response {
    let mut body = json!({ "schemas": [ERROR], "status": e.status.to_string(), "detail": e.detail });
    if let Some(t) = e.scim_type {
        body["scimType"] = json!(t);
    }
    let mut r = scim_response(e.status, &body);
    if e.status == 401 {
        r.headers.push(Header { name: "WWW-Authenticate".into(), value: "Bearer realm=\"hitop-scim\"".into() });
    }
    r
}

/// Every `/scim/v2/...` request (api.rs sends them here: the router has no PATCH).
pub fn handle(state: &State, request: &Request, path: &str) -> Response {
    let rest = path.strip_prefix("/scim/v2").unwrap_or("").trim_end_matches('/').to_string();
    let result = state.db.tx(|tx| -> ScimResult {
        let ctx = authenticate(state, tx, request)?;
        route(state, tx, request, &ctx, &rest)
    });
    match result {
        Ok((status, body)) => scim_response(status, &body),
        Err(e) => error_response(&e),
    }
}

fn authenticate(state: &State, tx: &mut Tx, request: &Request) -> Result<Ctx, ScimError> {
    let unauthorized = || ScimError::new(401, None, "a SCIM token is needed (Authorization: Bearer hitop_scim_...): create one in the portal, Admin → SCIM");
    let token = crate::api::bearer(request).filter(|t| t.starts_with(TOKEN_PREFIX)).ok_or_else(unauthorized)?;
    let row = tx.query_one(
        "SELECT t.id, t.org_id, t.name, t.last_used_at FROM scim_tokens t JOIN orgs o ON o.id = t.org_id WHERE t.token_hash = $1 AND o.personal_of = ''",
        &[token_hash(&token).into()],
    )?;
    let row = row.ok_or_else(unauthorized)?;
    let org = row.text(1);
    crate::limits::require_plan(state, tx, &org, "scim", "SCIM provisioning", crate::admin::FULL_PLANS)?;
    let now = state.now();
    if now - row.int(3) >= 60 {
        tx.execute("UPDATE scim_tokens SET last_used_at = $1 WHERE id = $2", &[now.into(), row.text(0).into()])?;
    }
    let base = state.config.public_url.join("scim/v2").map(|u| u.to_string()).unwrap_or_default();
    Ok(Ctx { org, actor: format!("scim:{}", row.text(2)), now, base })
}

fn route(state: &State, tx: &mut Tx, request: &Request, ctx: &Ctx, rest: &str) -> ScimResult {
    let parts: Vec<&str> = rest.trim_start_matches('/').split('/').filter(|p| !p.is_empty()).collect();
    let method = request.method.to_ascii_uppercase();
    match (method.as_str(), parts.as_slice()) {
        ("GET", ["ServiceProviderConfig"]) => Ok((200, service_provider_config(ctx))),
        ("GET", ["ResourceTypes"]) => Ok((200, list_of(resource_types(ctx), 1))),
        ("GET", ["ResourceTypes", name]) => resource_types(ctx).into_iter().find(|r| r["id"] == *name).map(|r| (200, r)).ok_or_else(|| ScimError::not_found("resource type")),
        ("GET", ["Schemas"]) => Ok((200, list_of(schemas(), 1))),
        ("GET", ["Schemas", id]) => schemas().into_iter().find(|s| s["id"] == *id).map(|s| (200, s)).ok_or_else(|| ScimError::not_found("schema")),
        ("GET", ["Users"]) => list_users(tx, request, ctx),
        ("POST", ["Users"]) => create_user(state, tx, request, ctx),
        ("GET", ["Users", id]) => {
            let u = load_user(tx, ctx, id)?;
            Ok((200, user_json(tx, ctx, &u)?))
        }
        ("PUT", ["Users", id]) => replace_user(state, tx, request, ctx, id),
        ("PATCH", ["Users", id]) => patch_user(state, tx, request, ctx, id),
        ("DELETE", ["Users", id]) => delete_user(state, tx, ctx, id),
        ("GET", ["Groups"]) => list_groups(tx, request, ctx),
        ("POST", ["Groups"]) => create_group(state, tx, request, ctx),
        ("GET", ["Groups", id]) => {
            let g = load_group(tx, ctx, id)?;
            Ok((200, group_json(tx, ctx, &g, true)?))
        }
        ("PUT", ["Groups", id]) => replace_group(state, tx, request, ctx, id),
        ("PATCH", ["Groups", id]) => patch_group(state, tx, request, ctx, id),
        ("DELETE", ["Groups", id]) => delete_group(state, tx, ctx, id),
        ("POST", ["Bulk"]) | ("POST", [".search"]) | ("POST", ["Users", ".search"]) | ("POST", ["Groups", ".search"]) => Err(ScimError::new(501, None, "not supported: see /scim/v2/ServiceProviderConfig")),
        ("GET" | "POST" | "PUT" | "PATCH" | "DELETE", _) => Err(ScimError::not_found("endpoint")),
        _ => Err(ScimError::new(405, None, "method not allowed")),
    }
}

// --- discovery -------------------------------------------------------------------------------

fn service_provider_config(ctx: &Ctx) -> Json {
    json!({
        "schemas": ["urn:ietf:params:scim:schemas:core:2.0:ServiceProviderConfig"],
        "documentationUri": "https://github.com/bohdaq/hitop/blob/main/sync-server/README.md#scim-provisioning",
        "patch": { "supported": true },
        "bulk": { "supported": false, "maxOperations": 0, "maxPayloadSize": 0 },
        "filter": { "supported": true, "maxResults": MAX_PAGE },
        "changePassword": { "supported": false },
        "sort": { "supported": false },
        "etag": { "supported": false },
        "authenticationSchemes": [{ "type": "oauthbearertoken", "name": "Bearer token", "description": "A per-organization SCIM token from the HITOP Cloud portal (Admin → SCIM)", "primary": true }],
        "meta": { "resourceType": "ServiceProviderConfig", "location": format!("{}/ServiceProviderConfig", ctx.base) },
    })
}

fn resource_types(ctx: &Ctx) -> Vec<Json> {
    vec![
        json!({ "schemas": ["urn:ietf:params:scim:schemas:core:2.0:ResourceType"], "id": "User", "name": "User", "endpoint": "/Users", "description": "An organization member, by email address", "schema": USER, "meta": { "resourceType": "ResourceType", "location": format!("{}/ResourceTypes/User", ctx.base) } }),
        json!({ "schemas": ["urn:ietf:params:scim:schemas:core:2.0:ResourceType"], "id": "Group", "name": "Group", "endpoint": "/Groups", "description": "A group whose org and workspace roles are set in the portal", "schema": GROUP, "meta": { "resourceType": "ResourceType", "location": format!("{}/ResourceTypes/Group", ctx.base) } }),
    ]
}

fn attr(name: &str, kind: &str, required: bool, uniqueness: &str, mutability: &str) -> Json {
    json!({ "name": name, "type": kind, "multiValued": false, "required": required, "caseExact": false, "mutability": mutability, "returned": "default", "uniqueness": uniqueness })
}

fn schemas() -> Vec<Json> {
    let mut members = attr("members", "complex", false, "none", "readWrite");
    members["multiValued"] = json!(true);
    members["subAttributes"] = json!([attr("value", "string", true, "none", "immutable"), attr("display", "string", false, "none", "readOnly")]);
    let mut name = attr("name", "complex", false, "none", "readWrite");
    name["subAttributes"] = json!([attr("formatted", "string", false, "none", "readWrite"), attr("givenName", "string", false, "none", "readWrite"), attr("familyName", "string", false, "none", "readWrite")]);
    let mut emails = attr("emails", "complex", false, "none", "readOnly");
    emails["multiValued"] = json!(true);
    emails["description"] = json!("The userName; other addresses are ignored");
    let mut groups = attr("groups", "complex", false, "none", "readOnly");
    groups["multiValued"] = json!(true);
    let mut user_name = attr("userName", "string", true, "server", "readWrite");
    user_name["description"] = json!("The person's email address");
    let schema_meta = |id: &str| json!({ "resourceType": "Schema", "location": format!("/scim/v2/Schemas/{id}") });
    vec![
        json!({ "schemas": ["urn:ietf:params:scim:schemas:core:2.0:Schema"], "id": USER, "name": "User", "description": "Organization member", "attributes": [user_name, attr("externalId", "string", false, "none", "readWrite"), attr("displayName", "string", false, "none", "readWrite"), name, attr("active", "boolean", false, "none", "readWrite"), emails, groups], "meta": schema_meta(USER) }),
        json!({ "schemas": ["urn:ietf:params:scim:schemas:core:2.0:Schema"], "id": GROUP, "name": "Group", "description": "Group", "attributes": [attr("displayName", "string", true, "none", "readWrite"), attr("externalId", "string", false, "none", "readWrite"), members], "meta": schema_meta(GROUP) }),
    ]
}

fn list_of(resources: Vec<Json>, start: usize) -> Json {
    json!({ "schemas": [LIST], "totalResults": resources.len(), "startIndex": start, "itemsPerPage": resources.len(), "Resources": resources })
}

// --- filters and paging ----------------------------------------------------------------------

/// `attr eq "value"`, the one filter IdPs send for provisioning (also `id`, `externalId`).
fn parse_filter(filter: &str) -> Result<(String, String), ScimError> {
    let invalid = || ScimError::bad("invalidFilter", "only `<attribute> eq \"<value>\"` filters are supported");
    let f = filter.trim();
    let (attr, rest) = f.split_once(char::is_whitespace).ok_or_else(invalid)?;
    let (op, value) = rest.trim_start().split_once(char::is_whitespace).ok_or_else(invalid)?;
    if !op.eq_ignore_ascii_case("eq") {
        return Err(invalid());
    }
    let value = value.trim();
    let value = value.strip_prefix('"').and_then(|v| v.strip_suffix('"')).ok_or_else(invalid)?;
    Ok((attr.to_string(), value.replace("\\\"", "\"").replace("\\\\", "\\")))
}

fn page(request: &Request, all: Vec<Json>) -> Json {
    let q = query(request);
    let start = q.get("startIndex").and_then(|s| s.parse::<usize>().ok()).unwrap_or(1).max(1);
    let count = q.get("count").and_then(|s| s.parse::<usize>().ok()).unwrap_or(100).min(MAX_PAGE);
    let total = all.len();
    let items: Vec<Json> = all.into_iter().skip(start - 1).take(count).collect();
    json!({ "schemas": [LIST], "totalResults": total, "startIndex": start, "itemsPerPage": items.len(), "Resources": items })
}

// --- users -----------------------------------------------------------------------------------

struct ScimUser {
    id: String,
    user_name: String,
    external_id: String,
    display_name: String,
    name: Json,
    active: bool,
    created_at: i64,
    updated_at: i64,
}

const USER_COLUMNS: &str = "id, user_name, external_id, display_name, name_json, active, created_at, updated_at";

fn user_of(r: &crate::db::Row) -> ScimUser {
    ScimUser {
        id: r.text(0),
        user_name: r.text(1),
        external_id: r.text(2),
        display_name: r.text(3),
        name: serde_json::from_str(&r.text(4)).unwrap_or_else(|_| json!({})),
        active: r.int(5) != 0,
        created_at: r.int(6),
        updated_at: r.int(7),
    }
}

fn load_user(tx: &mut Tx, ctx: &Ctx, id: &str) -> Result<ScimUser, ScimError> {
    tx.query_one(&format!("SELECT {USER_COLUMNS} FROM scim_users WHERE id = $1 AND org_id = $2"), &[id.into(), (&ctx.org).into()])?.map(|r| user_of(&r)).ok_or_else(|| ScimError::not_found("user"))
}

fn meta(ctx: &Ctx, kind: &str, id: &str, created: i64, updated: i64) -> Json {
    json!({ "resourceType": kind, "created": iso(created), "lastModified": iso(updated), "location": format!("{}/{kind}s/{id}", ctx.base), "version": format!("W/\"{updated}\"") })
}

fn user_json(tx: &mut Tx, ctx: &Ctx, u: &ScimUser) -> Result<Json, ScimError> {
    let groups: Vec<Json> = tx
        .query("SELECT g.id, g.display_name FROM scim_group_members m JOIN scim_groups g ON g.id = m.group_id WHERE m.user_id = $1 ORDER BY g.display_name, g.id", &[(&u.id).into()])?
        .iter()
        .map(|r| json!({ "value": r.text(0), "display": r.text(1), "$ref": format!("{}/Groups/{}", ctx.base, r.text(0)) }))
        .collect();
    let mut out = json!({
        "schemas": [USER],
        "id": u.id,
        "userName": u.user_name,
        "active": u.active,
        "emails": [{ "value": u.user_name, "type": "work", "primary": true }],
        "groups": groups,
        "meta": meta(ctx, "User", &u.id, u.created_at, u.updated_at),
    });
    if !u.external_id.is_empty() {
        out["externalId"] = json!(u.external_id);
    }
    if !u.display_name.is_empty() {
        out["displayName"] = json!(u.display_name);
    }
    if u.name.as_object().is_some_and(|o| !o.is_empty()) {
        out["name"] = u.name.clone();
    }
    Ok(out)
}

fn list_users(tx: &mut Tx, request: &Request, ctx: &Ctx) -> ScimResult {
    let q = query(request);
    let mut sql = format!("SELECT {USER_COLUMNS} FROM scim_users WHERE org_id = $1");
    let mut params: Vec<crate::db::Value> = vec![(&ctx.org).into()];
    if let Some(f) = q.get("filter").filter(|f| !f.trim().is_empty()) {
        let (a, v) = parse_filter(f)?;
        match a.to_ascii_lowercase().as_str() {
            "username" | "emails.value" | "emails[type eq \"work\"].value" => {
                sql.push_str(" AND user_name = $2");
                params.push(v.trim().to_lowercase().into());
            }
            "externalid" => {
                sql.push_str(" AND external_id = $2");
                params.push(v.into());
            }
            "id" => {
                sql.push_str(" AND id = $2");
                params.push(v.into());
            }
            _ => return Err(ScimError::bad("invalidFilter", format!("filtering on {a} isn't supported (userName, externalId, id)"))),
        }
    }
    sql.push_str(" ORDER BY created_at, id");
    let rows = tx.query(&sql, &params)?;
    let mut all = Vec::new();
    for r in rows {
        all.push(user_json(tx, ctx, &user_of(&r))?);
    }
    Ok((200, page(request, all)))
}

/// The user attributes HITOP keeps, from a full resource (POST, PUT) or a PATCH value.
#[derive(Default)]
struct UserIn {
    user_name: Option<String>,
    external_id: Option<String>,
    display_name: Option<String>,
    name: Option<Json>,
    active: Option<bool>,
}

/// `true`, `false`, or the strings some IdPs send ("True", "False").
fn boolean(v: &Json) -> Result<bool, ScimError> {
    match v {
        Json::Bool(b) => Ok(*b),
        Json::String(s) if s.eq_ignore_ascii_case("true") => Ok(true),
        Json::String(s) if s.eq_ignore_ascii_case("false") => Ok(false),
        _ => Err(ScimError::bad("invalidValue", "active must be true or false")),
    }
}

fn text(v: &Json, what: &str) -> Result<String, ScimError> {
    match v {
        Json::String(s) if s.chars().count() <= 256 && !s.chars().any(char::is_control) => Ok(s.trim().to_string()),
        Json::Null => Ok(String::new()),
        _ => Err(ScimError::bad("invalidValue", format!("{what} must be a string of at most 256 characters"))),
    }
}

fn name_part(v: &Json) -> Result<Json, ScimError> {
    let mut out = serde_json::Map::new();
    if let Some(o) = v.as_object() {
        for (k, val) in o {
            if let Some(key) = ["formatted", "givenName", "familyName"].iter().find(|n| n.eq_ignore_ascii_case(k)) {
                let t = text(val, "name")?;
                if !t.is_empty() {
                    out.insert(key.to_string(), json!(t));
                }
            }
        }
    }
    Ok(Json::Object(out))
}

/// Reads the attributes HITOP keeps from a JSON object (names are case-insensitive in SCIM).
fn user_attrs(v: &Json, into: &mut UserIn) -> Result<(), ScimError> {
    let Some(o) = v.as_object() else { return Err(ScimError::bad("invalidSyntax", "a JSON object is needed")) };
    for (k, val) in o {
        match k.to_ascii_lowercase().as_str() {
            "username" => into.user_name = Some(text(val, "userName")?),
            "externalid" => into.external_id = Some(text(val, "externalId")?),
            "displayname" => into.display_name = Some(text(val, "displayName")?),
            "name" => into.name = Some(name_part(val)?),
            "name.givenname" | "name.familyname" | "name.formatted" => {
                let mut n = into.name.clone().unwrap_or_else(|| json!({}));
                let key = match k.to_ascii_lowercase().as_str() {
                    "name.givenname" => "givenName",
                    "name.familyname" => "familyName",
                    _ => "formatted",
                };
                n[key] = json!(text(val, "name")?);
                into.name = Some(n);
            }
            "active" => into.active = Some(boolean(val)?),
            _ => {}
        }
    }
    Ok(())
}

fn valid_user_name(s: &str) -> Result<String, ScimError> {
    normalize_email(s).map_err(|_| ScimError::bad("invalidValue", "userName must be the person's email address"))
}

fn create_user(state: &State, tx: &mut Tx, request: &Request, ctx: &Ctx) -> ScimResult {
    let v: Json = body(request)?;
    let mut input = UserIn::default();
    user_attrs(&v, &mut input)?;
    let user_name = valid_user_name(input.user_name.as_deref().unwrap_or(""))?;
    if tx.query_one("SELECT id FROM scim_users WHERE org_id = $1 AND user_name = $2", &[(&ctx.org).into(), (&user_name).into()])?.is_some() {
        return Err(ScimError::new(409, Some("uniqueness"), "a user with this userName exists"));
    }
    let id = random_hex::<16>();
    let active = input.active.unwrap_or(true);
    tx.execute(
        "INSERT INTO scim_users (id, org_id, user_name, external_id, display_name, name_json, active, created_at, updated_at) VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $8)",
        &[
            (&id).into(),
            (&ctx.org).into(),
            (&user_name).into(),
            input.external_id.unwrap_or_default().into(),
            input.display_name.unwrap_or_default().into(),
            input.name.unwrap_or_else(|| json!({})).to_string().into(),
            i64::from(active).into(),
            ctx.now.into(),
        ],
    )?;
    record(tx, state, ctx, &orgs::chain(&ctx.org), "scim_user_created", &user_name, json!({ "id": id, "active": active }))?;
    apply(state, tx, ctx)?;
    let u = load_user(tx, ctx, &id)?;
    Ok((201, user_json(tx, ctx, &u)?))
}

/// Stores `input` over `u` and applies what changed.
fn update_user(state: &State, tx: &mut Tx, ctx: &Ctx, u: ScimUser, input: UserIn, full: bool) -> ScimResult {
    let user_name = match input.user_name.as_deref() {
        Some(n) => valid_user_name(n)?,
        None if full => return Err(ScimError::bad("invalidValue", "userName is required")),
        None => u.user_name.clone(),
    };
    if user_name != u.user_name && tx.query_one("SELECT id FROM scim_users WHERE org_id = $1 AND user_name = $2", &[(&ctx.org).into(), (&user_name).into()])?.is_some() {
        return Err(ScimError::new(409, Some("uniqueness"), "a user with this userName exists"));
    }
    let pick = |new: Option<String>, old: &str| new.unwrap_or_else(|| if full { String::new() } else { old.to_string() });
    let external_id = pick(input.external_id, &u.external_id);
    let display_name = pick(input.display_name, &u.display_name);
    let name = input.name.unwrap_or_else(|| if full { json!({}) } else { u.name.clone() });
    let active = input.active.unwrap_or(if full { true } else { u.active });
    // The old address leaves first (a new address, or a deactivation).
    if u.active && (!active || user_name != u.user_name) {
        deprovision(state, tx, ctx, &u.user_name)?;
    }
    tx.execute(
        "UPDATE scim_users SET user_name = $1, external_id = $2, display_name = $3, name_json = $4, active = $5, updated_at = $6 WHERE id = $7",
        &[(&user_name).into(), external_id.into(), display_name.into(), name.to_string().into(), i64::from(active).into(), ctx.now.into(), (&u.id).into()],
    )?;
    let event = match (u.active, active) {
        (true, false) => "scim_user_deactivated",
        (false, true) => "scim_user_reactivated",
        _ => "scim_user_updated",
    };
    let mut detail = json!({ "id": u.id });
    if user_name != u.user_name {
        detail["from"] = json!(u.user_name);
    }
    record(tx, state, ctx, &orgs::chain(&ctx.org), event, &user_name, detail)?;
    apply(state, tx, ctx)?;
    let u = load_user(tx, ctx, &u.id)?;
    Ok((200, user_json(tx, ctx, &u)?))
}

fn replace_user(state: &State, tx: &mut Tx, request: &Request, ctx: &Ctx, id: &str) -> ScimResult {
    let u = load_user(tx, ctx, id)?;
    let v: Json = body(request)?;
    let mut input = UserIn::default();
    user_attrs(&v, &mut input)?;
    update_user(state, tx, ctx, u, input, true)
}

#[derive(Deserialize)]
struct PatchIn {
    #[serde(default)]
    schemas: Vec<String>,
    #[serde(rename = "Operations", alias = "operations")]
    operations: Vec<PatchOp>,
}

#[derive(Deserialize)]
struct PatchOp {
    op: String,
    #[serde(default)]
    path: Option<String>,
    #[serde(default)]
    value: Option<Json>,
}

fn patch_body(request: &Request) -> Result<PatchIn, ScimError> {
    let p: PatchIn = body(request).map_err(|e| ScimError::bad("invalidSyntax", e.message))?;
    if !p.schemas.is_empty() && !p.schemas.iter().any(|s| s == PATCH) {
        return Err(ScimError::bad("invalidSyntax", format!("a PATCH body has the schema {PATCH}")));
    }
    if p.operations.is_empty() || p.operations.len() > 100 {
        return Err(ScimError::bad("invalidSyntax", "1 to 100 operations"));
    }
    Ok(p)
}

fn patch_user(state: &State, tx: &mut Tx, request: &Request, ctx: &Ctx, id: &str) -> ScimResult {
    let u = load_user(tx, ctx, id)?;
    let p = patch_body(request)?;
    let mut input = UserIn::default();
    for op in &p.operations {
        let value = op.value.clone().unwrap_or(Json::Null);
        match (op.op.to_ascii_lowercase().as_str(), op.path.as_deref().map(str::trim).filter(|s| !s.is_empty())) {
            ("add" | "replace", None) => user_attrs(&value, &mut input)?,
            ("add" | "replace", Some(path)) => user_attrs(&json!({ path: value }), &mut input)?,
            ("remove", Some(path)) => match path.to_ascii_lowercase().as_str() {
                "externalid" => input.external_id = Some(String::new()),
                "displayname" => input.display_name = Some(String::new()),
                "name" => input.name = Some(json!({})),
                "username" | "active" => return Err(ScimError::bad("mutability", format!("{path} can't be removed"))),
                _ => {}
            },
            ("remove", None) => return Err(ScimError::bad("noTarget", "remove needs a path")),
            _ => return Err(ScimError::bad("invalidSyntax", format!("unknown op {}", op.op))),
        }
    }
    update_user(state, tx, ctx, u, input, false)
}

fn delete_user(state: &State, tx: &mut Tx, ctx: &Ctx, id: &str) -> ScimResult {
    let u = load_user(tx, ctx, id)?;
    if u.active {
        deprovision(state, tx, ctx, &u.user_name)?;
    }
    tx.execute("DELETE FROM scim_users WHERE id = $1", &[(&u.id).into()])?;
    record(tx, state, ctx, &orgs::chain(&ctx.org), "scim_user_deleted", &u.user_name, json!({ "id": u.id }))?;
    apply(state, tx, ctx)?;
    Ok((204, Json::Null))
}

// --- groups ----------------------------------------------------------------------------------

struct ScimGroup {
    id: String,
    display_name: String,
    external_id: String,
    org_role: String,
    created_at: i64,
    updated_at: i64,
}

const GROUP_COLUMNS: &str = "id, display_name, external_id, org_role, created_at, updated_at";

fn group_of(r: &crate::db::Row) -> ScimGroup {
    ScimGroup { id: r.text(0), display_name: r.text(1), external_id: r.text(2), org_role: r.text(3), created_at: r.int(4), updated_at: r.int(5) }
}

fn load_group(tx: &mut Tx, ctx: &Ctx, id: &str) -> Result<ScimGroup, ScimError> {
    load_group_of(tx, &ctx.org, id)?.ok_or_else(|| ScimError::not_found("group"))
}

fn load_group_of(tx: &mut Tx, org: &str, id: &str) -> Result<Option<ScimGroup>, DbError> {
    Ok(tx.query_one(&format!("SELECT {GROUP_COLUMNS} FROM scim_groups WHERE id = $1 AND org_id = $2"), &[id.into(), org.into()])?.map(|r| group_of(&r)))
}

fn group_json(tx: &mut Tx, ctx: &Ctx, g: &ScimGroup, with_members: bool) -> Result<Json, ScimError> {
    let mut out = json!({ "schemas": [GROUP], "id": g.id, "displayName": g.display_name, "meta": meta(ctx, "Group", &g.id, g.created_at, g.updated_at) });
    if !g.external_id.is_empty() {
        out["externalId"] = json!(g.external_id);
    }
    if with_members {
        let members: Vec<Json> = tx
            .query("SELECT u.id, u.user_name FROM scim_group_members m JOIN scim_users u ON u.id = m.user_id WHERE m.group_id = $1 ORDER BY u.user_name", &[(&g.id).into()])?
            .iter()
            .map(|r| json!({ "value": r.text(0), "display": r.text(1), "$ref": format!("{}/Users/{}", ctx.base, r.text(0)) }))
            .collect();
        out["members"] = json!(members);
    }
    Ok(out)
}

fn list_groups(tx: &mut Tx, request: &Request, ctx: &Ctx) -> ScimResult {
    let q = query(request);
    let excluded = q.get("excludedAttributes").is_some_and(|e| e.split(',').any(|a| a.trim().eq_ignore_ascii_case("members")));
    let mut sql = format!("SELECT {GROUP_COLUMNS} FROM scim_groups WHERE org_id = $1");
    let mut params: Vec<crate::db::Value> = vec![(&ctx.org).into()];
    if let Some(f) = q.get("filter").filter(|f| !f.trim().is_empty()) {
        let (a, v) = parse_filter(f)?;
        let column = match a.to_ascii_lowercase().as_str() {
            "displayname" => "display_name",
            "externalid" => "external_id",
            "id" => "id",
            _ => return Err(ScimError::bad("invalidFilter", format!("filtering on {a} isn't supported (displayName, externalId, id)"))),
        };
        sql.push_str(&format!(" AND {column} = $2"));
        params.push(v.into());
    }
    sql.push_str(" ORDER BY created_at, id");
    let rows = tx.query(&sql, &params)?;
    let mut all = Vec::new();
    for r in rows {
        all.push(group_json(tx, ctx, &group_of(&r), !excluded)?);
    }
    Ok((200, page(request, all)))
}

/// Member ids from `[{"value": id}, ...]`; each must be a user of this org.
fn member_ids(tx: &mut Tx, ctx: &Ctx, v: &Json) -> Result<Vec<String>, ScimError> {
    let list = match v {
        Json::Array(a) => a.clone(),
        Json::Object(_) => vec![v.clone()],
        Json::Null => vec![],
        _ => return Err(ScimError::bad("invalidValue", "members is a list of {\"value\": <user id>}")),
    };
    let mut out = Vec::new();
    for m in list {
        let id = m.get("value").and_then(Json::as_str).ok_or_else(|| ScimError::bad("invalidValue", "a member needs a value (the user's id)"))?;
        if tx.query_one("SELECT id FROM scim_users WHERE id = $1 AND org_id = $2", &[id.into(), (&ctx.org).into()])?.is_none() {
            return Err(ScimError::bad("invalidValue", format!("no user {id} in this organization's directory")));
        }
        out.push(id.to_string());
    }
    Ok(out)
}

fn set_members(tx: &mut Tx, group: &str, ids: &BTreeSet<String>) -> Result<(), DbError> {
    tx.execute("DELETE FROM scim_group_members WHERE group_id = $1", &[group.into()])?;
    for id in ids {
        tx.execute("INSERT INTO scim_group_members (group_id, user_id) VALUES ($1, $2)", &[group.into(), id.into()])?;
    }
    Ok(())
}

fn members_of(tx: &mut Tx, group: &str) -> Result<BTreeSet<String>, DbError> {
    Ok(tx.query("SELECT user_id FROM scim_group_members WHERE group_id = $1", &[group.into()])?.iter().map(|r| r.text(0)).collect())
}

fn create_group(state: &State, tx: &mut Tx, request: &Request, ctx: &Ctx) -> ScimResult {
    let v: Json = body(request)?;
    let o = v.as_object().ok_or_else(|| ScimError::bad("invalidSyntax", "a JSON object is needed"))?;
    let get = |k: &str| o.iter().find(|(n, _)| n.eq_ignore_ascii_case(k)).map(|(_, v)| v.clone()).unwrap_or(Json::Null);
    let display_name = text(&get("displayName"), "displayName")?;
    if display_name.is_empty() {
        return Err(ScimError::bad("invalidValue", "displayName is required"));
    }
    let n = tx.query_one("SELECT COUNT(*) FROM scim_groups WHERE org_id = $1", &[(&ctx.org).into()])?.map_or(0, |r| r.int(0));
    if n >= MAX_GROUPS {
        return Err(ScimError::new(403, None, format!("an organization has at most {MAX_GROUPS} SCIM groups")));
    }
    let members: BTreeSet<String> = member_ids(tx, ctx, &get("members"))?.into_iter().collect();
    let id = random_hex::<16>();
    tx.execute(
        "INSERT INTO scim_groups (id, org_id, display_name, external_id, org_role, created_at, updated_at) VALUES ($1, $2, $3, $4, '', $5, $5)",
        &[(&id).into(), (&ctx.org).into(), (&display_name).into(), text(&get("externalId"), "externalId")?.into(), ctx.now.into()],
    )?;
    set_members(tx, &id, &members)?;
    record(tx, state, ctx, &orgs::chain(&ctx.org), "scim_group_created", &id, json!({ "displayName": display_name, "members": members.len() }))?;
    apply(state, tx, ctx)?;
    let g = load_group(tx, ctx, &id)?;
    Ok((201, group_json(tx, ctx, &g, true)?))
}

fn save_group(state: &State, tx: &mut Tx, ctx: &Ctx, g: &ScimGroup, display_name: String, external_id: String, members: BTreeSet<String>) -> ScimResult {
    if display_name.is_empty() {
        return Err(ScimError::bad("invalidValue", "displayName is required"));
    }
    let before = members_of(tx, &g.id)?;
    tx.execute("UPDATE scim_groups SET display_name = $1, external_id = $2, updated_at = $3 WHERE id = $4", &[(&display_name).into(), external_id.into(), ctx.now.into(), (&g.id).into()])?;
    set_members(tx, &g.id, &members)?;
    let added = members.difference(&before).count();
    let removed = before.difference(&members).count();
    record(tx, state, ctx, &orgs::chain(&ctx.org), "scim_group_updated", &g.id, json!({ "displayName": display_name, "added": added, "removed": removed }))?;
    apply(state, tx, ctx)?;
    let g = load_group(tx, ctx, &g.id)?;
    Ok((200, group_json(tx, ctx, &g, true)?))
}

fn replace_group(state: &State, tx: &mut Tx, request: &Request, ctx: &Ctx, id: &str) -> ScimResult {
    let g = load_group(tx, ctx, id)?;
    let v: Json = body(request)?;
    let o = v.as_object().ok_or_else(|| ScimError::bad("invalidSyntax", "a JSON object is needed"))?;
    let get = |k: &str| o.iter().find(|(n, _)| n.eq_ignore_ascii_case(k)).map(|(_, v)| v.clone()).unwrap_or(Json::Null);
    let members = member_ids(tx, ctx, &get("members"))?.into_iter().collect();
    save_group(state, tx, ctx, &g, text(&get("displayName"), "displayName")?, text(&get("externalId"), "externalId")?, members)
}

/// `members[value eq "<id>"]` → the id.
fn member_filter(path: &str) -> Option<String> {
    let inner = path.trim().strip_prefix("members[")?.strip_suffix(']')?;
    let (a, v) = parse_filter(inner).ok()?;
    a.eq_ignore_ascii_case("value").then_some(v)
}

fn patch_group(state: &State, tx: &mut Tx, request: &Request, ctx: &Ctx, id: &str) -> ScimResult {
    let g = load_group(tx, ctx, id)?;
    let p = patch_body(request)?;
    let mut display_name = g.display_name.clone();
    let mut external_id = g.external_id.clone();
    let mut members = members_of(tx, &g.id)?;
    for op in &p.operations {
        let value = op.value.clone().unwrap_or(Json::Null);
        let path = op.path.as_deref().map(str::trim).filter(|s| !s.is_empty());
        let lower = path.map(str::to_ascii_lowercase);
        match (op.op.to_ascii_lowercase().as_str(), lower.as_deref()) {
            ("add", Some("members")) => members.extend(member_ids(tx, ctx, &value)?),
            ("replace", Some("members")) => members = member_ids(tx, ctx, &value)?.into_iter().collect(),
            ("remove", Some("members")) => {
                if value.is_null() {
                    members.clear();
                } else {
                    for m in value.as_array().cloned().unwrap_or_else(|| vec![value.clone()]) {
                        if let Some(v) = m.get("value").and_then(Json::as_str) {
                            members.remove(v);
                        }
                    }
                }
            }
            ("remove", Some(p)) if p.starts_with("members[") => {
                let id = member_filter(path.unwrap_or("")).ok_or_else(|| ScimError::bad("invalidPath", "only members[value eq \"<id>\"] is supported"))?;
                members.remove(&id);
            }
            ("add" | "replace", Some("displayname")) => display_name = text(&value, "displayName")?,
            ("add" | "replace", Some("externalid")) => external_id = text(&value, "externalId")?,
            ("remove", Some("externalid")) => external_id.clear(),
            ("add" | "replace", None) => {
                let Some(o) = value.as_object() else { return Err(ScimError::bad("invalidSyntax", "a value object is needed without a path")) };
                for (k, v) in o {
                    match k.to_ascii_lowercase().as_str() {
                        "displayname" => display_name = text(v, "displayName")?,
                        "externalid" => external_id = text(v, "externalId")?,
                        "members" if op.op.eq_ignore_ascii_case("replace") => members = member_ids(tx, ctx, v)?.into_iter().collect(),
                        "members" => members.extend(member_ids(tx, ctx, v)?),
                        _ => {}
                    }
                }
            }
            ("add" | "replace" | "remove", Some(_)) => return Err(ScimError::bad("invalidPath", format!("unsupported path {}", path.unwrap_or("")))),
            ("remove", None) => return Err(ScimError::bad("noTarget", "remove needs a path")),
            _ => return Err(ScimError::bad("invalidSyntax", format!("unknown op {}", op.op))),
        }
    }
    save_group(state, tx, ctx, &g, display_name, external_id, members)
}

fn delete_group(state: &State, tx: &mut Tx, ctx: &Ctx, id: &str) -> ScimResult {
    let g = load_group(tx, ctx, id)?;
    tx.execute("DELETE FROM scim_groups WHERE id = $1", &[(&g.id).into()])?;
    record(tx, state, ctx, &orgs::chain(&ctx.org), "scim_group_deleted", &g.id, json!({ "displayName": g.display_name }))?;
    apply(state, tx, ctx)?;
    Ok((204, Json::Null))
}

// --- applying the directory ------------------------------------------------------------------

fn record(tx: &mut Tx, state: &State, ctx: &Ctx, chain: &str, event: &'static str, target: &str, detail: Json) -> Result<(), DbError> {
    audit::record(tx, state, Event::new(chain, event, None).email(&ctx.actor).target(target).detail(detail))
}

fn account_of(tx: &mut Tx, email: &str) -> Result<Option<String>, DbError> {
    Ok(tx.query_one("SELECT id FROM users WHERE email = $1 AND runner_of = ''", &[email.into()])?.map(|r| r.text(0)))
}

fn has_device(tx: &mut Tx, user: &str) -> Result<bool, DbError> {
    Ok(tx.query_one("SELECT id FROM device_keys WHERE user_id = $1", &[user.into()])?.is_some())
}

fn rank(role: &str) -> u8 {
    match role {
        "admin" | "editor" => 2,
        "member" | "viewer" => 1,
        _ => 0,
    }
}

/// Makes the org, its invites and the workspaces match the directory: every active user is a
/// member (or invited) with its groups' org role, and has the workspace roles its groups give.
fn apply(state: &State, tx: &mut Tx, ctx: &Ctx) -> Result<(), ScimError> {
    let org = ctx.org.clone();
    let users: Vec<(String, String)> = tx.query("SELECT id, user_name FROM scim_users WHERE org_id = $1 AND active = 1 ORDER BY user_name", &[(&org).into()])?.iter().map(|r| (r.text(0), r.text(1))).collect();
    let roles_used = tx.query_one("SELECT COUNT(*) FROM scim_groups WHERE org_id = $1 AND org_role <> ''", &[(&org).into()])?.map_or(0, |r| r.int(0)) > 0;
    let mut org_role: BTreeMap<String, String> = BTreeMap::new();
    for r in tx.query("SELECT m.user_id, g.org_role FROM scim_group_members m JOIN scim_groups g ON g.id = m.group_id WHERE g.org_id = $1 AND g.org_role <> ''", &[(&org).into()])? {
        let e = org_role.entry(r.text(0)).or_default();
        if rank(&r.text(1)) > rank(e) {
            *e = r.text(1);
        }
    }
    let mut want: BTreeMap<(String, String), String> = BTreeMap::new();
    let emails: BTreeMap<String, String> = users.iter().cloned().collect();
    for r in tx.query(
        "SELECT m.user_id, gw.workspace_id, gw.role FROM scim_group_members m JOIN scim_groups g ON g.id = m.group_id JOIN scim_group_workspaces gw ON gw.group_id = g.id JOIN workspaces w ON w.id = gw.workspace_id WHERE g.org_id = $1 AND w.org_id = $1",
        &[(&org).into()],
    )? {
        let Some(email) = emails.get(&r.text(0)) else { continue };
        let e = want.entry((r.text(1), email.clone())).or_default();
        if rank(&r.text(2)) > rank(e) {
            *e = r.text(2);
        }
    }
    for (id, email) in &users {
        let role = org_role.get(id).cloned().or_else(|| roles_used.then(|| "member".to_string()));
        ensure_org_member(state, tx, ctx, email, role.as_deref())?;
    }
    let grants: Vec<(String, String, String)> = tx.query("SELECT workspace_id, email, kind FROM scim_grants WHERE org_id = $1", &[(&org).into()])?.iter().map(|r| (r.text(0), r.text(1), r.text(2))).collect();
    for (ws, email, kind) in grants {
        if !want.contains_key(&(ws.clone(), email.clone())) {
            revoke(state, tx, ctx, &ws, &email, &kind)?;
        }
    }
    for ((ws, email), role) in &want {
        grant(state, tx, ctx, ws, email, role)?;
    }
    Ok(())
}

/// Adds `email` to the org (or invites it) and sets its org role (`None`: leave it as it is).
fn ensure_org_member(state: &State, tx: &mut Tx, ctx: &Ctx, email: &str, role: Option<&str>) -> Result<(), ScimError> {
    let org = &ctx.org;
    let chain = orgs::chain(org);
    match account_of(tx, email)? {
        Some(user) => match tx.query_one("SELECT role FROM org_members WHERE org_id = $1 AND user_id = $2", &[org.into(), (&user).into()])? {
            None => {
                let role = role.unwrap_or("member");
                crate::limits::check_seat(state, tx, org, &user, true)?;
                tx.execute("INSERT INTO org_members (org_id, user_id, role, created_at) VALUES ($1, $2, $3, $4)", &[org.into(), (&user).into(), role.into(), ctx.now.into()])?;
                tx.execute("DELETE FROM org_invites WHERE org_id = $1 AND email = $2", &[org.into(), email.into()])?;
                record(tx, state, ctx, &chain, "org_member_added", &user, json!({ "email": email, "role": role, "by": "scim" }))?;
            }
            Some(r) => {
                let current = r.text(0);
                if let Some(role) = role.filter(|r| *r != current && current != OrgRole::Owner.as_str()) {
                    tx.execute("UPDATE org_members SET role = $1 WHERE org_id = $2 AND user_id = $3", &[role.into(), org.into(), (&user).into()])?;
                    record(tx, state, ctx, &chain, "org_role_changed", &user, json!({ "from": current, "to": role, "by": "scim" }))?;
                }
            }
        },
        None => match tx.query_one("SELECT role FROM org_invites WHERE org_id = $1 AND email = $2", &[org.into(), email.into()])? {
            None => {
                let role = role.unwrap_or("member");
                crate::limits::check_seat(state, tx, org, &format!("invite:{email}"), true)?;
                tx.execute("INSERT INTO org_invites (org_id, email, role, created_by, created_at) VALUES ($1, $2, $3, $4, $5)", &[org.into(), email.into(), role.into(), invite_marker(org).into(), ctx.now.into()])?;
                record(tx, state, ctx, &chain, "org_invite_created", email, json!({ "role": role, "mailed": false, "by": "scim" }))?;
            }
            Some(r) => {
                if let Some(role) = role.filter(|r2| *r2 != r.text(0) && r.text(0) != OrgRole::Owner.as_str()) {
                    tx.execute("UPDATE org_invites SET role = $1 WHERE org_id = $2 AND email = $3", &[role.into(), org.into(), email.into()])?;
                }
            }
        },
    }
    Ok(())
}

/// Takes `email` out of the org (a deactivated or deleted user, or an old address) and out of every
/// org workspace SCIM gave it. Owners stay: refused for the org, kept (audited) for a workspace.
fn deprovision(state: &State, tx: &mut Tx, ctx: &Ctx, email: &str) -> Result<(), ScimError> {
    let org = &ctx.org;
    let chain = orgs::chain(org);
    if let Some(user) = account_of(tx, email)? {
        if let Some(r) = tx.query_one("SELECT role FROM org_members WHERE org_id = $1 AND user_id = $2", &[org.into(), (&user).into()])? {
            if r.text(0) == OrgRole::Owner.as_str() {
                return Err(ScimError::new(409, Some("mutability"), "this person owns the organization: transfer or remove the owner role in the portal first (SCIM never removes owners)"));
            }
            tx.execute("DELETE FROM org_members WHERE org_id = $1 AND user_id = $2", &[org.into(), (&user).into()])?;
            record(tx, state, ctx, &chain, "org_member_removed", &user, json!({ "email": email, "role": r.text(0), "by": "scim" }))?;
        }
    }
    if tx.execute("DELETE FROM org_invites WHERE org_id = $1 AND email = $2", &[org.into(), email.into()])? > 0 {
        record(tx, state, ctx, &chain, "org_invite_deleted", email, json!({ "by": "scim" }))?;
    }
    let grants: Vec<(String, String)> = tx.query("SELECT workspace_id, kind FROM scim_grants WHERE org_id = $1 AND email = $2", &[org.into(), email.into()])?.iter().map(|r| (r.text(0), r.text(1))).collect();
    for (ws, kind) in grants {
        revoke(state, tx, ctx, &ws, email, &kind)?;
    }
    Ok(())
}

/// Gives `email` `role` in `ws`: sets the role of a member, or opens an invite the person has
/// accepted (once they have a device key) for the owners to add.
fn grant(state: &State, tx: &mut Tx, ctx: &Ctx, ws: &str, email: &str, role: &str) -> Result<(), ScimError> {
    let marker = invite_marker(&ctx.org);
    let existing = tx.query_one("SELECT kind, role FROM scim_grants WHERE workspace_id = $1 AND email = $2", &[ws.into(), email.into()])?;
    let account = account_of(tx, email)?;
    let member = match &account {
        Some(user) => tx.query_one("SELECT role FROM members WHERE workspace_id = $1 AND user_id = $2", &[ws.into(), user.into()])?.map(|r| r.text(0)),
        None => None,
    };
    let kind = match (&member, &existing) {
        (Some(current), _) => {
            let user = account.clone().unwrap_or_default();
            if current != "owner" && current != role {
                if role != "viewer" {
                    crate::limits::check_seat(state, tx, &ctx.org, &user, true)?;
                }
                tx.execute("UPDATE members SET role = $1 WHERE workspace_id = $2 AND user_id = $3", &[role.into(), ws.into(), (&user).into()])?;
                record(tx, state, ctx, ws, "role_changed", &user, json!({ "from": current, "to": role, "by": "scim" }))?;
            }
            tx.execute("DELETE FROM invites WHERE workspace_id = $1 AND email = $2 AND created_by = $3", &[ws.into(), email.into(), (&marker).into()])?;
            match &existing {
                None => "preexisting".to_string(),
                Some(g) if g.text(0) == "invite" => "member".to_string(),
                Some(g) => g.text(0),
            }
        }
        (None, _) => {
            let accepted = match &account {
                Some(user) if has_device(tx, user)? => user.clone(),
                _ => String::new(),
            };
            let expires = ctx.now + INVITE_TTL;
            match tx.query_one("SELECT id, accepted_by FROM invites WHERE workspace_id = $1 AND email = $2 AND created_by = $3", &[ws.into(), email.into(), (&marker).into()])? {
                Some(i) => {
                    let accepted_by = if i.text(1).is_empty() { accepted } else { i.text(1) };
                    tx.execute("UPDATE invites SET role = $1, expires_at = $2, accepted_by = $3, accepted_at = CASE WHEN accepted_at = 0 AND $3 <> '' THEN $4 ELSE accepted_at END WHERE id = $5", &[role.into(), expires.into(), accepted_by.into(), ctx.now.into(), i.text(0).into()])?;
                }
                None => {
                    let id = random_hex::<16>();
                    let accepted_at = if accepted.is_empty() { 0 } else { ctx.now };
                    tx.execute(
                        "INSERT INTO invites (id, workspace_id, token_hash, email, role, created_by, created_at, expires_at, accepted_by, accepted_at) VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10)",
                        &[(&id).into(), ws.into(), token_hash(&new_token()).into(), email.into(), role.into(), (&marker).into(), ctx.now.into(), expires.into(), accepted.into(), accepted_at.into()],
                    )?;
                    record(tx, state, ctx, ws, "invite_created", &id, json!({ "email": email, "role": role, "mailed": false, "by": "scim" }))?;
                }
            }
            "invite".to_string()
        }
    };
    tx.execute(
        "INSERT INTO scim_grants (org_id, workspace_id, email, role, kind, created_at) VALUES ($1, $2, $3, $4, $5, $6) ON CONFLICT (workspace_id, email) DO UPDATE SET role = $4, kind = $5",
        &[(&ctx.org).into(), ws.into(), email.into(), role.into(), kind.into(), ctx.now.into()],
    )?;
    Ok(())
}

/// Takes back what SCIM gave `email` in `ws` (`kind` from `scim_grants`).
fn revoke(state: &State, tx: &mut Tx, ctx: &Ctx, ws: &str, email: &str, kind: &str) -> Result<(), ScimError> {
    let marker = invite_marker(&ctx.org);
    if tx.execute("DELETE FROM invites WHERE workspace_id = $1 AND email = $2 AND created_by = $3", &[ws.into(), email.into(), (&marker).into()])? > 0 {
        record(tx, state, ctx, ws, "invite_deleted", email, json!({ "by": "scim" }))?;
    }
    if kind != "preexisting" {
        if let Some(user) = account_of(tx, email)? {
            if let Some(m) = tx.query_one("SELECT role FROM members WHERE workspace_id = $1 AND user_id = $2", &[ws.into(), (&user).into()])? {
                if m.text(0) == "owner" {
                    record(tx, state, ctx, ws, "scim_owner_kept", &user, json!({ "email": email }))?;
                } else {
                    tx.execute("DELETE FROM members WHERE workspace_id = $1 AND user_id = $2", &[ws.into(), (&user).into()])?;
                    tx.execute("DELETE FROM wraps WHERE workspace_id = $1 AND key_id IN (SELECT id FROM device_keys WHERE user_id = $2)", &[ws.into(), (&user).into()])?;
                    tx.execute("DELETE FROM env_share_wraps WHERE workspace_id = $1 AND user_id = $2", &[ws.into(), (&user).into()])?;
                    tx.execute("UPDATE workspaces SET rotation_requested_at = $1, rotation_requested_by = $2 WHERE id = $3", &[ctx.now.into(), (&ctx.actor).into(), ws.into()])?;
                    record(tx, state, ctx, ws, "member_removed", &user, json!({ "email": email, "role": m.text(0), "by": "scim" }))?;
                    record(tx, state, ctx, ws, "rotation_requested", ws, json!({ "reason": "scim_removed" }))?;
                }
            }
        }
    }
    tx.execute("DELETE FROM scim_grants WHERE workspace_id = $1 AND email = $2", &[ws.into(), email.into()])?;
    Ok(())
}

// --- the portal's endpoints (`/v1/orgs/:org/scim...`) -------------------------------------------

fn scim_caller(state: &State, tx: &mut Tx, request: &Request, org: &str, need: OrgRole, change: bool) -> Result<crate::auth::User, ApiError> {
    let (user, o) = orgs::org_caller(state, tx, request, org, need, change)?;
    if o.personal {
        return Err(ApiError::bad("SCIM is for an organization, not a personal account"));
    }
    crate::limits::require_plan(state, tx, org, "scim", "SCIM provisioning", crate::admin::FULL_PLANS)?;
    Ok(user)
}

fn portal_group(tx: &mut Tx, g: &ScimGroup) -> Result<Json, DbError> {
    let members = tx.query_one("SELECT COUNT(*) FROM scim_group_members WHERE group_id = $1", &[(&g.id).into()])?.map_or(0, |r| r.int(0));
    let workspaces: Vec<Json> = tx.query("SELECT workspace_id, role FROM scim_group_workspaces WHERE group_id = $1 ORDER BY workspace_id", &[(&g.id).into()])?.iter().map(|r| json!({ "workspace": r.text(0), "role": r.text(1) })).collect();
    Ok(json!({ "id": g.id, "displayName": g.display_name, "externalId": g.external_id, "members": members, "orgRole": g.org_role, "workspaces": workspaces }))
}

/// `GET /v1/orgs/:org/scim` (admin): tokens (never their values), the directory's size, groups and
/// their roles.
pub fn get(state: &State, request: &Request, params: &PathParams) -> ApiResult {
    let org = params.get("org").unwrap_or_default().to_string();
    state.db.tx(|tx| {
        scim_caller(state, tx, request, &org, OrgRole::Admin, false)?;
        let tokens: Vec<Json> = tx
            .query("SELECT id, name, created_at, last_used_at FROM scim_tokens WHERE org_id = $1 ORDER BY created_at, id", &[(&org).into()])?
            .iter()
            .map(|r| json!({ "id": r.text(0), "name": r.text(1), "createdAt": r.int(2), "lastUsedAt": r.int(3) }))
            .collect();
        let users = tx.query_one("SELECT COUNT(*) FROM scim_users WHERE org_id = $1", &[(&org).into()])?.map_or(0, |r| r.int(0));
        let active = tx.query_one("SELECT COUNT(*) FROM scim_users WHERE org_id = $1 AND active = 1", &[(&org).into()])?.map_or(0, |r| r.int(0));
        let rows = tx.query(&format!("SELECT {GROUP_COLUMNS} FROM scim_groups WHERE org_id = $1 ORDER BY display_name, id"), &[(&org).into()])?;
        let mut groups = Vec::new();
        for r in rows {
            groups.push(portal_group(tx, &group_of(&r))?);
        }
        let pending = tx.query_one("SELECT COUNT(*) FROM scim_grants WHERE org_id = $1 AND kind = 'invite'", &[(&org).into()])?.map_or(0, |r| r.int(0));
        let base = state.config.public_url.join("scim/v2").map(|u| u.to_string()).unwrap_or_default();
        Ok((200, json!({ "org": org, "enabled": !tokens.is_empty(), "baseUrl": base, "tokens": tokens, "users": users, "activeUsers": active, "groups": groups, "pendingWorkspaceAdds": pending })))
    })
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct NewToken {
    #[serde(default)]
    name: String,
}

/// `POST /v1/orgs/:org/scim/tokens {"name"}` (owner, signed-in session) → the token, shown once.
pub fn create_token(state: &State, request: &Request, params: &PathParams) -> ApiResult {
    let org = params.get("org").unwrap_or_default().to_string();
    let input: NewToken = body(request)?;
    let name = input.name.trim();
    let name = if name.is_empty() { "SCIM" } else { name };
    if name.chars().count() > 64 || name.chars().any(char::is_control) {
        return Err(ApiError::bad("a token name is at most 64 characters"));
    }
    state.db.tx(|tx| {
        let user = scim_caller(state, tx, request, &org, OrgRole::Owner, true)?;
        let n = tx.query_one("SELECT COUNT(*) FROM scim_tokens WHERE org_id = $1", &[(&org).into()])?.map_or(0, |r| r.int(0));
        if n >= MAX_TOKENS {
            return Err(ApiError::forbidden(format!("an organization has at most {MAX_TOKENS} SCIM tokens: revoke one first")));
        }
        let id = random_hex::<8>();
        let token = format!("{TOKEN_PREFIX}{}", new_token());
        tx.execute(
            "INSERT INTO scim_tokens (id, org_id, name, token_hash, created_by, created_at) VALUES ($1, $2, $3, $4, $5, $6)",
            &[(&id).into(), (&org).into(), name.into(), token_hash(&token).into(), (&user.id).into(), state.now().into()],
        )?;
        audit::record(tx, state, Event::new(&orgs::chain(&org), "scim_token_created", Some(&user)).target(&id).detail(json!({ "name": name })))?;
        let base = state.config.public_url.join("scim/v2").map(|u| u.to_string()).unwrap_or_default();
        Ok((201, json!({ "id": id, "name": name, "token": token, "baseUrl": base })))
    })
}

/// `DELETE /v1/orgs/:org/scim/tokens/:id` (owner, signed-in session).
pub fn delete_token(state: &State, request: &Request, params: &PathParams) -> ApiResult {
    let org = params.get("org").unwrap_or_default().to_string();
    let id = params.get("id").unwrap_or_default().to_string();
    state.db.tx(|tx| {
        // No plan check: revoking always works, also after a downgrade.
        let (user, o) = orgs::org_caller(state, tx, request, &org, OrgRole::Owner, true)?;
        let _ = o;
        if tx.execute("DELETE FROM scim_tokens WHERE id = $1 AND org_id = $2", &[(&id).into(), (&org).into()])? == 0 {
            return Err(ApiError::not_found("no such SCIM token"));
        }
        audit::record(tx, state, Event::new(&orgs::chain(&org), "scim_token_revoked", Some(&user)).target(&id))?;
        Ok((204, Json::Null))
    })
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
struct GroupRolesIn {
    #[serde(default)]
    org_role: String,
    #[serde(default)]
    workspaces: Vec<WorkspaceRoleIn>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct WorkspaceRoleIn {
    workspace: String,
    role: String,
}

/// `PUT /v1/orgs/:org/scim/groups/:group {"orgRole": ""|"member"|"admin", "workspaces": [{"workspace",
/// "role": "viewer"|"editor"}]}` (admin, signed-in session): what the group gives; applied at once.
pub fn put_group(state: &State, request: &Request, params: &PathParams) -> ApiResult {
    let org = params.get("org").unwrap_or_default().to_string();
    let group = params.get("group").unwrap_or_default().to_string();
    let input: GroupRolesIn = body(request)?;
    if !["", "member", "admin"].contains(&input.org_role.as_str()) {
        return Err(ApiError::bad("orgRole is \"\", member or admin (owners are set in the portal, never by SCIM)"));
    }
    if input.workspaces.len() > 200 {
        return Err(ApiError::bad("at most 200 workspaces per group"));
    }
    state.db.tx(|tx| {
        let user = scim_caller(state, tx, request, &org, OrgRole::Admin, true)?;
        let Some(g) = load_group_of(tx, &org, &group)? else { return Err(ApiError::not_found("no such SCIM group")) };
        let mut roles = BTreeMap::new();
        for w in &input.workspaces {
            if !["viewer", "editor"].contains(&w.role.as_str()) {
                return Err(ApiError::bad("a workspace role is viewer or editor (owners add owners themselves)"));
            }
            if tx.query_one("SELECT id FROM workspaces WHERE id = $1 AND org_id = $2", &[(&w.workspace).into(), (&org).into()])?.is_none() {
                return Err(ApiError::not_found(format!("no workspace {} in this organization", w.workspace)));
            }
            roles.insert(w.workspace.clone(), w.role.clone());
        }
        tx.execute("UPDATE scim_groups SET org_role = $1 WHERE id = $2", &[(&input.org_role).into(), (&g.id).into()])?;
        tx.execute("DELETE FROM scim_group_workspaces WHERE group_id = $1", &[(&g.id).into()])?;
        for (ws, role) in &roles {
            tx.execute("INSERT INTO scim_group_workspaces (group_id, workspace_id, role) VALUES ($1, $2, $3)", &[(&g.id).into(), ws.into(), role.into()])?;
        }
        let detail = json!({ "displayName": g.display_name, "orgRole": input.org_role, "workspaces": roles });
        audit::record(tx, state, Event::new(&orgs::chain(&org), "scim_group_roles_set", Some(&user)).target(&g.id).detail(detail))?;
        let ctx = Ctx { org: org.clone(), actor: format!("scim:group roles by {}", user.email), now: state.now(), base: String::new() };
        apply(state, tx, &ctx).map_err(|e| ApiError { status: e.status, code: if e.status == 402 { "limit" } else { "scim" }, message: e.detail, extra: None })?;
        let g = load_group_of(tx, &org, &group)?.ok_or_else(|| ApiError::not_found("no such SCIM group"))?;
        Ok((200, portal_group(tx, &g)?))
    })
}

/// Fills in SCIM invites that a person can accept now (they have an account with a device key):
/// called before an owner lists a workspace's invites (teams.rs), so they show as accepted.
pub fn accept_pending(state: &State, tx: &mut Tx, ws: &str) -> Result<(), DbError> {
    tx.execute(
        "UPDATE invites SET accepted_by = (SELECT u.id FROM users u WHERE u.email = invites.email AND u.runner_of = ''), accepted_at = $1
         WHERE workspace_id = $2 AND created_by LIKE 'scim:%' AND accepted_by = ''
         AND EXISTS (SELECT 1 FROM users u JOIN device_keys k ON k.user_id = u.id WHERE u.email = invites.email AND u.runner_of = '')",
        &[state.now().into(), ws.into()],
    )?;
    Ok(())
}

/// Seconds since 1970 → `2026-11-03T12:00:00Z`.
fn iso(secs: i64) -> String {
    let days = secs.div_euclid(86_400);
    let rem = secs.rem_euclid(86_400);
    // Civil from days (Howard Hinnant's algorithm).
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let y = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    let y = if m <= 2 { y + 1 } else { y };
    format!("{y:04}-{m:02}-{d:02}T{:02}:{:02}:{:02}Z", rem / 3600, rem % 3600 / 60, rem % 60)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn iso_dates() {
        assert_eq!(iso(0), "1970-01-01T00:00:00Z");
        assert_eq!(iso(1_793_664_000), "2026-11-03T00:00:00Z");
        assert_eq!(iso(951_782_400 + 3661), "2000-02-29T01:01:01Z");
    }

    #[test]
    fn filters() {
        assert_eq!(parse_filter("userName eq \"ada@corp.test\"").unwrap(), ("userName".into(), "ada@corp.test".into()));
        assert_eq!(parse_filter("externalId EQ \"a \\\"b\\\"\"").unwrap(), ("externalId".into(), "a \"b\"".into()));
        assert!(parse_filter("userName sw \"a\"").is_err());
        assert!(parse_filter("userName eq ada").is_err());
        assert_eq!(member_filter("members[value eq \"abc\"]").as_deref(), Some("abc"));
    }
}
