//! Mock servers (HITOP Cloud, owner decision 2026-11-06): a library collection answered at
//! `/mock/<id>/<path>` from its requests' saved examples, so a frontend can be built before the
//! API exists. Paid plans (with plan limits on), a random unguessable id, at most
//! `PER_MINUTE` calls a minute per mock, and never HTML: an example whose type is HTML (or
//! anything a browser would run) goes out as `text/plain`, so a mock can't host a phishing page.
//!
//! A request matches by method and path: the saved URL's path after its host or a leading
//! `${variable}`; a `${name}`, `:name` or `{name}` segment matches any one segment; the query
//! is ignored. The answer is the request's first saved example (status, body, `Content-Type`),
//! or `?example=<name>` picks one by name. CORS is open (`*`): a mock is public by design.
//!
//!   GET/PUT/DELETE /v1/library/collection/:id/mock   the collection's mock (PUT turns it on)

use std::collections::HashMap;
use std::sync::Mutex;

use rust_web_server::header::Header;
use rust_web_server::request::Request;
use rust_web_server::response::Response;
use rust_web_server::router::PathParams;
use serde_json::{json, Value as Json};

use crate::api::{random_hex, ApiError, ApiResult, State};
use crate::auth::authenticate;
use crate::db::Tx;

pub const PER_MINUTE: u32 = 600;
const ID_HEX: usize = 24;

fn mock_url(state: &State, id: &str) -> String {
    format!("{}/mock/{id}", state.config.public_url.as_str().trim_end_matches('/'))
}

fn collection_id(params: &PathParams) -> Result<String, ApiError> {
    let id = params.get("id").unwrap_or_default().to_string();
    if id.is_empty() || id.len() > 64 {
        return Err(ApiError::bad("a collection id"));
    }
    Ok(id)
}

/// Mocks need a paid plan when the server enforces plans.
fn plan_allows(state: &State, tx: &mut Tx, org: &str) -> Result<(), ApiError> {
    plan_allows_feature(state, tx, org, "mock servers")
}

/// A paid-plan feature (mock servers, monitors): refused on Free when the server enforces plans.
pub(crate) fn plan_allows_feature(state: &State, tx: &mut Tx, org: &str, feature: &str) -> Result<(), ApiError> {
    if !state.config.plan_limits {
        return Ok(());
    }
    // The plan in force: a lapsed, refunded or paused subscription is on Free (as every other gate).
    if crate::limits::entitlement(tx, org)?.plan_in_force() == "free" {
        return Err(ApiError::new(402, "limit", format!("{feature} come with the Pro, Team, Business and Enterprise plans")));
    }
    Ok(())
}

fn row(tx: &mut Tx, user: &str, collection: &str) -> Result<Option<String>, ApiError> {
    Ok(tx.query_one("SELECT id FROM mocks WHERE user_id = $1 AND collection_id = $2", &[user.into(), collection.into()])?.map(|r| r.text(0)))
}

/// `GET /v1/library/collection/:id/mock` → `{url}` or 404.
pub fn get(state: &State, request: &Request, params: &PathParams) -> ApiResult {
    let collection = collection_id(params)?;
    state.db.tx(|tx| {
        let user = authenticate(state, tx, request)?;
        user.person()?;
        let id = row(tx, &user.id, &collection)?.ok_or_else(|| ApiError::not_found("no mock for this collection"))?;
        Ok((200, json!({ "id": id, "url": mock_url(state, &id) })))
    })
}

/// `PUT /v1/library/collection/:id/mock` → `{url}`: turns the mock on (again: the same URL).
pub fn put(state: &State, request: &Request, params: &PathParams) -> ApiResult {
    let collection = collection_id(params)?;
    state.db.tx(|tx| {
        let user = authenticate(state, tx, request)?;
        user.person()?;
        let org = tx
            .query_one("SELECT org_id FROM library_items WHERE user_id = $1 AND kind = 'collection' AND item_id = $2 AND deleted = 0", &[(&user.id).into(), (&collection).into()])?
            .map(|r| r.text(0))
            .ok_or_else(|| ApiError::not_found("no such collection in your HITOP Cloud library"))?;
        plan_allows(state, tx, &org)?;
        if let Some(id) = row(tx, &user.id, &collection)? {
            return Ok((200, json!({ "id": id, "url": mock_url(state, &id) })));
        }
        let id = random_hex::<12>();
        tx.execute("INSERT INTO mocks (id, user_id, collection_id, created_at) VALUES ($1, $2, $3, $4)", &[(&id).into(), (&user.id).into(), (&collection).into(), state.now().into()])?;
        Ok((201, json!({ "id": id, "url": mock_url(state, &id) })))
    })
}

/// `DELETE /v1/library/collection/:id/mock`: the URL stops answering.
pub fn delete(state: &State, request: &Request, params: &PathParams) -> ApiResult {
    let collection = collection_id(params)?;
    state.db.tx(|tx| {
        let user = authenticate(state, tx, request)?;
        tx.execute("DELETE FROM mocks WHERE user_id = $1 AND collection_id = $2", &[(&user.id).into(), (&collection).into()])?;
        Ok((200, json!({ "deleted": true })))
    })
}

// --- serving ------------------------------------------------------------------------------

static CALLS: Mutex<Option<HashMap<String, (i64, u32)>>> = Mutex::new(None);

/// One more call to mock `id` this minute; false past `PER_MINUTE`.
fn admit(id: &str, now: i64) -> bool {
    let mut guard = CALLS.lock().unwrap_or_else(|p| p.into_inner());
    let map = guard.get_or_insert_with(HashMap::new);
    let minute = now / 60;
    if map.len() > 10_000 {
        map.retain(|_, (m, _)| *m == minute);
    }
    let entry = map.entry(id.to_string()).or_insert((minute, 0));
    if entry.0 != minute {
        *entry = (minute, 0);
    }
    entry.1 += 1;
    entry.1 <= PER_MINUTE
}

/// The path of a saved URL: after `scheme://host` or a leading `${variable}` (a base URL).
pub fn saved_path(url: &str) -> String {
    let u = url.trim();
    let rest = if let Some(after) = u.split_once("://").map(|(_, r)| r) {
        after.find('/').map_or("", |i| &after[i..])
    } else if let Some(r) = u.strip_prefix("${") {
        r.find('}').map_or(u, |i| &r[i + 1..])
    } else {
        u
    };
    let path = rest.split(['?', '#']).next().unwrap_or("");
    format!("/{}", path.trim_matches('/'))
}

fn is_param(seg: &str) -> bool {
    (seg.starts_with("${") && seg.ends_with('}')) || seg.starts_with(':') || (seg.starts_with('{') && seg.ends_with('}'))
}

/// Whether the saved `pattern` path matches the called `path`.
pub fn path_matches(pattern: &str, path: &str) -> bool {
    let a: Vec<&str> = pattern.trim_matches('/').split('/').collect();
    let b: Vec<&str> = path.trim_matches('/').split('/').collect();
    a.len() == b.len() && a.iter().zip(&b).all(|(p, s)| is_param(p) || p.eq_ignore_ascii_case(s))
}

fn plain(status: i16, body: &str) -> Response {
    let mut r = Response::get_response(crate::api::phrase(status as u16), None, None);
    r.headers.push(Header::new(Header::_CONTENT_TYPE, "application/json"));
    r.content_range_list = vec![rust_web_server::range::Range::get_content_range(json!({ "error": body }).to_string().into_bytes(), "application/json".into())];
    r
}

/// A content type a mock may send as is; anything a browser would render as a page is text.
fn safe_type(t: &str) -> String {
    let lower = t.to_ascii_lowercase();
    let risky = ["html", "xml", "javascript", "ecmascript", "svg", "xhtml"];
    if t.is_empty() {
        "application/json".into()
    } else if risky.iter().any(|r| lower.contains(r)) {
        "text/plain; charset=utf-8".into()
    } else {
        t.to_string()
    }
}

/// `ANY /mock/<id>/<path>`.
pub fn serve(state: &State, request: &Request, path: &str) -> Response {
    let mut response = serve_inner(state, request, path);
    let mut add = |name: &'static str, value: &str| response.headers.push(Header::new(name, value.to_string()));
    add("Access-Control-Allow-Origin", "*");
    add("Access-Control-Allow-Methods", "GET, POST, PUT, PATCH, DELETE, HEAD, OPTIONS");
    add("Access-Control-Allow-Headers", "*");
    add("X-Content-Type-Options", "nosniff");
    add("Content-Security-Policy", "default-src 'none'; sandbox");
    add("X-Robots-Tag", "noindex");
    response
}

fn serve_inner(state: &State, request: &Request, path: &str) -> Response {
    let rest = path.strip_prefix("/mock/").unwrap_or("");
    let (id, sub) = rest.split_once('/').map_or((rest, ""), |(a, b)| (a, b));
    if id.len() != ID_HEX || !id.bytes().all(|b| b.is_ascii_hexdigit()) {
        return plain(404, "no such mock");
    }
    if request.method == "OPTIONS" {
        return Response::get_response(crate::api::phrase(204), None, None);
    }
    if !admit(id, state.now()) {
        return plain(429, "this mock answers at most 600 calls a minute");
    }
    let sub = format!("/{}", sub.trim_matches('/'));
    let query = request.request_uri.split_once('?').map(|(_, q)| q).unwrap_or("");
    let wanted_example = query.split('&').find_map(|kv| kv.strip_prefix("example=")).map(|v| percent_encoding::percent_decode_str(v).decode_utf8_lossy().into_owned());
    let found = state.db.tx(|tx| {
        let Some(r) = tx.query_one("SELECT user_id, collection_id FROM mocks WHERE id = $1", &[id.into()])? else { return Ok(None) };
        let (user, collection) = (r.text(0), r.text(1));
        let Some((data, _)) = crate::library::collection_item(tx, &user, &collection)? else { return Ok(None) };
        let org = tx.query_one("SELECT org_id FROM library_items WHERE user_id = $1 AND kind = 'collection' AND item_id = $2", &[(&user).into(), (&collection).into()])?.map(|r| r.text(0)).unwrap_or_default();
        if plan_allows(state, tx, &org).is_err() {
            return Ok(Some(Err("this mock's plan no longer includes mock servers")));
        }
        Ok::<_, ApiError>(Some(Ok(data)))
    });
    let data = match found {
        Ok(Some(Ok(d))) => d,
        Ok(Some(Err(why))) => return plain(402, why),
        Ok(None) => return plain(404, "no such mock"),
        Err(_) => return plain(500, "the mock couldn't be read"),
    };
    let method = request.method.to_uppercase();
    let requests = data["requests"].as_array().cloned().unwrap_or_default();
    let candidates: Vec<&Json> = requests.iter().filter(|r| path_matches(&saved_path(r["url"].as_str().unwrap_or("")), &sub)).collect();
    let Some(r) = candidates.iter().find(|r| r["method"].as_str().unwrap_or("GET").eq_ignore_ascii_case(&method)).or_else(|| (method == "HEAD").then(|| candidates.iter().find(|r| r["method"] == "GET")).flatten()) else {
        let msg = if candidates.is_empty() { format!("no request in this collection matches {sub}") } else { format!("{sub} has no {method} request in this collection") };
        return plain(if candidates.is_empty() { 404 } else { 405 }, &msg);
    };
    let examples = r["examples"].as_array().cloned().unwrap_or_default();
    let example = match &wanted_example {
        Some(name) => examples.iter().find(|e| e["name"].as_str() == Some(name.as_str())),
        None => examples.first(),
    };
    let Some(e) = example else {
        return plain(501, &format!("{} has no saved example: send it in HITOP and choose Save as example", r["name"].as_str().unwrap_or("this request")));
    };
    let status = e["status"].as_u64().filter(|s| (200..600).contains(s)).unwrap_or(200) as i16;
    let headers = e["headers"].as_object();
    let ctype = headers.and_then(|h| h.iter().find(|(k, _)| k.eq_ignore_ascii_case("content-type")).and_then(|(_, v)| v.as_str())).unwrap_or("");
    let body = e["body"].as_str().unwrap_or_default().to_string();
    let mut out = Response::get_response(crate::api::phrase(status as u16), None, None);
    let t = safe_type(ctype);
    out.headers.push(Header::new(Header::_CONTENT_TYPE, t.clone()));
    if method != "HEAD" {
        out.content_range_list = vec![rust_web_server::range::Range::get_content_range(body.into_bytes(), t)];
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn paths_and_types() {
        assert_eq!(saved_path("${baseUrl}/orders/${id}?x=1"), "/orders/${id}");
        assert_eq!(saved_path("https://api.test/v1/users/"), "/v1/users");
        assert_eq!(saved_path("${baseUrl}"), "/");
        assert!(path_matches("/orders/${id}", "/orders/7"));
        assert!(path_matches("/orders/:id/items", "/orders/7/items"));
        assert!(!path_matches("/orders/${id}", "/orders/7/items"));
        assert!(!path_matches("/orders", "/users"));
        assert_eq!(safe_type("text/html; charset=utf-8"), "text/plain; charset=utf-8");
        assert_eq!(safe_type("image/svg+xml"), "text/plain; charset=utf-8");
        assert_eq!(safe_type("application/json"), "application/json");
    }
}
