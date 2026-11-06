//! Hosted MCP (HITOP Cloud, owner decision 2026-11-06): `POST /mcp` lets an AI assistant (Claude
//! Code, Claude Desktop, Cursor) read and edit the person's HITOP Cloud library without a local
//! folder: the tools of `hitop mcp` that need no request to be sent. Stateless JSON-RPC 2.0 over
//! the Streamable HTTP transport's POST (no session, no SSE): `initialize`, `ping`,
//! `tools/list`, `tools/call`; notifications get `202`.
//!
//! Every request carries `Authorization: Bearer` with a personal access token from the portal
//! (`hitop_pat_…`, scope `read`, or `read write` for the tools that change things) or a session.
//! Shared workspaces stay end to end encrypted: the server can't read them, so they aren't here.
//! Tool names and arguments are a contract once released: change them only additively.
//! Library writes go through `library::store_from_server` (a new version the apps pull) and
//! `gitsync::touched` (a collection saved to GitHub is pushed), as a save from the apps would.

use rust_web_server::request::Request;
use rust_web_server::router::PathParams;
use serde_json::{json, Map, Value as Json};

use crate::api::{bearer, ApiError, ApiResult, State};
use crate::auth::User;
use crate::db::Tx;

const VERSIONS: [&str; 4] = ["2025-11-25", "2025-06-18", "2025-03-26", "2024-11-05"];

struct Tool {
    name: &'static str,
    description: &'static str,
    write: bool,
    schema: fn() -> Json,
}

fn s(desc: &str) -> Json {
    json!({ "type": "string", "description": desc })
}

const COLLECTION: &str = "The collection's name or id (list_collections)";

fn tools() -> Vec<Tool> {
    vec![
        Tool { name: "list_collections", description: "List the collections in your HITOP Cloud library: id, name, organization and number of requests. Shared (end-to-end encrypted) collections aren't readable by the server and aren't listed.", write: false, schema: || json!({ "type": "object", "properties": {} }) },
        Tool {
            name: "list_requests",
            description: "List a collection's requests in order: name, method and URL.",
            write: false,
            schema: || json!({ "type": "object", "properties": { "collection": s(COLLECTION) }, "required": ["collection"] }),
        },
        Tool {
            name: "get_request",
            description: "Show one saved request: method, URL, headers, body, auth and the scripts before and after it. ${variables} are shown as written.",
            write: false,
            schema: || json!({ "type": "object", "properties": { "collection": s(COLLECTION), "request": s("The request's name") }, "required": ["collection", "request"] }),
        },
        Tool {
            name: "create_request",
            description: "Add a request to a collection (saved at once; the apps get it on their next sync). Use ${variables} for base URLs and secrets.",
            write: true,
            schema: || {
                json!({ "type": "object", "properties": {
                    "collection": s(COLLECTION), "name": s("The new request's name"), "method": s("GET, POST, PUT, PATCH, DELETE, HEAD or OPTIONS (default GET)"),
                    "url": s("The URL, e.g. ${baseUrl}/orders"), "headers": { "type": "array", "items": { "type": "object", "properties": { "name": { "type": "string" }, "value": { "type": "string" } }, "required": ["name", "value"] } },
                    "body": s("The raw body"), "scriptAfter": s("JavaScript run after the response, e.g. pm.test(...) checks")
                }, "required": ["collection", "name", "url"] })
            },
        },
        Tool {
            name: "update_request",
            description: "Change a saved request: give only the fields to change (headers replace all headers).",
            write: true,
            schema: || {
                json!({ "type": "object", "properties": {
                    "collection": s(COLLECTION), "request": s("The request's current name"), "name": s("A new name"), "method": s("A new method"), "url": s("A new URL"),
                    "headers": { "type": "array", "items": { "type": "object", "properties": { "name": { "type": "string" }, "value": { "type": "string" } }, "required": ["name", "value"] } },
                    "body": s("A new body"), "scriptBefore": s("A new script before the request"), "scriptAfter": s("A new script after the response")
                }, "required": ["collection", "request"] })
            },
        },
        Tool { name: "list_environments", description: "List your environments (from your HITOP Cloud library) with their variable names; secret values are never shown.", write: false, schema: || json!({ "type": "object", "properties": {} }) },
    ]
}

/// The caller, with the `write` scope when `write` (a token) — sessions may do everything.
fn caller(state: &State, tx: &mut Tx, request: &Request, write: bool) -> Result<User, ApiError> {
    let token = bearer(request).ok_or_else(ApiError::unauthorized)?;
    let user = if token.starts_with(crate::tokens::PREFIX) { crate::tokens::authenticate_token_for(state, tx, &token, write)? } else { crate::auth::authenticate(state, tx, request)? };
    user.person()?;
    Ok(user)
}

fn rpc_result(id: &Json, result: Json) -> ApiResult {
    Ok((200, json!({ "jsonrpc": "2.0", "id": id, "result": result })))
}

fn rpc_error(id: &Json, code: i64, message: &str) -> ApiResult {
    Ok((200, json!({ "jsonrpc": "2.0", "id": id, "error": { "code": code, "message": message } })))
}

/// `POST /mcp`.
pub fn handle(state: &State, request: &Request, _: &PathParams) -> ApiResult {
    let msg: Json = serde_json::from_slice(&request.body).map_err(|_| ApiError::bad("a JSON-RPC 2.0 request"))?;
    if msg.is_array() {
        return rpc_error(&Json::Null, -32600, "batches aren't supported: send one request at a time");
    }
    let id = msg.get("id").cloned().unwrap_or(Json::Null);
    let method = msg["method"].as_str().unwrap_or_default();
    // Notifications (no id) need no answer.
    if msg.get("id").is_none() {
        return Ok((202, json!({})));
    }
    // Every request is the person's: an unknown or missing token is a 401 the client shows.
    let tool = (method == "tools/call").then(|| tools().into_iter().find(|t| Some(t.name) == msg["params"]["name"].as_str())).flatten();
    let write = tool.as_ref().is_some_and(|t| t.write);
    state.db.tx(|tx| {
        let user = caller(state, tx, request, write)?;
        match method {
            "initialize" => {
                let asked = msg["params"]["protocolVersion"].as_str().unwrap_or_default();
                let version = VERSIONS.iter().find(|v| **v == asked).copied().unwrap_or(VERSIONS[0]);
                rpc_result(&id, json!({
                    "protocolVersion": version,
                    "capabilities": { "tools": { "listChanged": false } },
                    "serverInfo": { "name": "hitop-cloud", "title": "HITOP Cloud", "version": env!("CARGO_PKG_VERSION") },
                    "instructions": "Your HITOP Cloud library: API collections and environments. Read with list_collections, list_requests and get_request; change with create_request and update_request (a token with the write scope). Requests aren't sent from here: use the HITOP apps or `hitop run`."
                }))
            }
            "ping" => rpc_result(&id, json!({})),
            "tools/list" => rpc_result(&id, json!({ "tools": tools().iter().map(|t| json!({
                "name": t.name, "description": t.description, "inputSchema": (t.schema)(),
                "annotations": { "readOnlyHint": !t.write, "destructiveHint": false, "openWorldHint": false }
            })).collect::<Vec<_>>() })),
            "tools/call" => {
                let Some(t) = tool else { return rpc_error(&id, -32602, "unknown tool") };
                let args = msg["params"]["arguments"].as_object().cloned().unwrap_or_default();
                let out = match call(state, tx, &user, t.name, &args) {
                    Ok(v) => json!({ "content": [{ "type": "text", "text": serde_json::to_string_pretty(&v).unwrap_or_default() }], "structuredContent": v, "isError": false }),
                    Err(e) => json!({ "content": [{ "type": "text", "text": e }], "isError": true }),
                };
                rpc_result(&id, out)
            }
            _ => rpc_error(&id, -32601, "method not found"),
        }
    })
}

/// The person's collections in the library: (item id, data, version).
fn collections(tx: &mut Tx, user: &str) -> Result<Vec<(String, Json, i64)>, ApiError> {
    let rows = tx.query("SELECT item_id, data, version FROM library_items WHERE user_id = $1 AND kind = 'collection' AND deleted = 0 ORDER BY seq", &[user.into()])?;
    Ok(rows.iter().map(|r| (r.text(0), serde_json::from_str(&r.text(1)).unwrap_or(Json::Null), r.int(2))).collect())
}

fn find_collection(tx: &mut Tx, user: &str, wanted: &str) -> Result<(String, Json, i64), String> {
    let all = collections(tx, user).map_err(|e| e.message)?;
    let wanted = wanted.trim();
    all.iter()
        .find(|(id, _, _)| id == wanted)
        .or_else(|| all.iter().find(|(_, d, _)| d["name"].as_str() == Some(wanted)))
        .or_else(|| all.iter().find(|(_, d, _)| d["name"].as_str().is_some_and(|n| n.eq_ignore_ascii_case(wanted))))
        .cloned()
        .ok_or_else(|| format!("no collection {wanted:?} in your HITOP Cloud library (list_collections lists them)"))
}

fn arg<'a>(args: &'a Map<String, Json>, key: &str) -> Option<&'a str> {
    args.get(key).and_then(Json::as_str)
}

fn need<'a>(args: &'a Map<String, Json>, key: &str) -> Result<&'a str, String> {
    arg(args, key).filter(|v| !v.trim().is_empty()).ok_or_else(|| format!("{key} is required"))
}

fn headers_of(v: &Json) -> Json {
    Json::Array(
        v.as_array()
            .into_iter()
            .flatten()
            .filter_map(|h| Some(json!({ "name": h["name"].as_str()?, "value": h["value"].as_str().unwrap_or_default() })))
            .collect(),
    )
}

fn method_of(m: Option<&str>) -> Result<String, String> {
    let m = m.unwrap_or("GET").trim().to_uppercase();
    if ["GET", "POST", "PUT", "PATCH", "DELETE", "HEAD", "OPTIONS"].contains(&m.as_str()) {
        Ok(m)
    } else {
        Err(format!("{m} isn't a method HITOP sends (GET, POST, PUT, PATCH, DELETE, HEAD, OPTIONS)"))
    }
}

/// Saves `data` as the collection's next version, as an app save would (the apps pull it; a
/// collection saved to GitHub is pushed).
fn save(state: &State, tx: &mut Tx, user: &str, id: &str, data: &Json, version: i64) -> Result<(), String> {
    if !crate::library::store_from_server(state, tx, user, id, data, version).map_err(|e| e.message)? {
        return Err("the collection changed meanwhile: try again".into());
    }
    crate::gitsync::touched(state, tx, user, "collection", id).map_err(|e| e.to_string())
}

fn call(state: &State, tx: &mut Tx, user: &User, tool: &str, args: &Map<String, Json>) -> Result<Json, String> {
    match tool {
        "list_collections" => {
            let all = collections(tx, &user.id).map_err(|e| e.message)?;
            Ok(json!({ "collections": all.iter().map(|(id, d, _)| json!({
                "id": id, "name": d["name"], "organization": d.get("org").cloned().unwrap_or(Json::Null),
                "requests": d["requests"].as_array().map_or(0, Vec::len)
            })).collect::<Vec<_>>() }))
        }
        "list_requests" => {
            let (_, d, _) = find_collection(tx, &user.id, need(args, "collection")?)?;
            Ok(json!({ "collection": d["name"], "requests": d["requests"].as_array().into_iter().flatten().map(|r| json!({
                "name": r["name"], "method": r["method"], "url": r["url"]
            })).collect::<Vec<_>>() }))
        }
        "get_request" => {
            let (_, d, _) = find_collection(tx, &user.id, need(args, "collection")?)?;
            let name = need(args, "request")?;
            let r = d["requests"].as_array().into_iter().flatten().find(|r| r["name"].as_str() == Some(name)).ok_or_else(|| format!("no request {name:?} in {}", d["name"]))?;
            Ok(json!({
                "name": r["name"], "method": r["method"], "url": r["url"], "headers": headers_of(&r["headers"]), "body": r["body"],
                "auth": r.get("auth").cloned().unwrap_or(Json::Null), "scriptBefore": r["preRequestScript"], "scriptAfter": r["postRequestScript"]
            }))
        }
        "create_request" => {
            let (id, mut d, version) = find_collection(tx, &user.id, need(args, "collection")?)?;
            let name = need(args, "name")?.trim().to_string();
            let requests = d["requests"].as_array().cloned().unwrap_or_default();
            if requests.iter().any(|r| r["name"].as_str() == Some(name.as_str())) {
                return Err(format!("{name:?} is already in {}: use update_request", d["name"]));
            }
            let next = requests.iter().filter_map(|r| r["id"].as_u64()).max().unwrap_or(0).max(state.now() as u64 * 1000) + 1;
            let mut headers = headers_of(args.get("headers").unwrap_or(&Json::Null));
            headers.as_array_mut().expect("array").push(json!({ "name": "", "value": "" }));
            let request = json!({
                "id": next, "name": name, "method": method_of(arg(args, "method"))?, "url": need(args, "url")?, "headers": headers,
                "body": arg(args, "body").unwrap_or_default(), "preRequestScript": "", "postRequestScript": arg(args, "scriptAfter").unwrap_or_default()
            });
            let mut list = requests;
            list.push(request);
            d["requests"] = Json::Array(list);
            save(state, tx, &user.id, &id, &d, version)?;
            Ok(json!({ "created": name, "collection": d["name"] }))
        }
        "update_request" => {
            let (id, mut d, version) = find_collection(tx, &user.id, need(args, "collection")?)?;
            let name = need(args, "request")?.to_string();
            let collection_name = d["name"].clone();
            let requests = d["requests"].as_array_mut().ok_or("the collection has no requests")?;
            if let Some(new) = arg(args, "name").filter(|n| *n != name) {
                if requests.iter().any(|r| r["name"].as_str() == Some(new)) {
                    return Err(format!("{new:?} is already in {collection_name}"));
                }
            }
            let r = requests.iter_mut().find(|r| r["name"].as_str() == Some(name.as_str())).ok_or_else(|| format!("no request {name:?} in {collection_name}"))?;
            for (key, field) in [("name", "name"), ("url", "url"), ("body", "body"), ("scriptBefore", "preRequestScript"), ("scriptAfter", "postRequestScript")] {
                if let Some(v) = arg(args, key) {
                    r[field] = json!(v);
                }
            }
            if let Some(m) = arg(args, "method") {
                r["method"] = json!(method_of(Some(m))?);
            }
            if let Some(h) = args.get("headers") {
                let mut headers = headers_of(h);
                headers.as_array_mut().expect("array").push(json!({ "name": "", "value": "" }));
                r["headers"] = headers;
            }
            save(state, tx, &user.id, &id, &d, version)?;
            Ok(json!({ "updated": arg(args, "name").unwrap_or(&name), "collection": collection_name }))
        }
        "list_environments" => {
            let row = tx
                .query_one("SELECT data FROM library_items WHERE user_id = $1 AND kind = 'environments' AND item_id = 'all' AND deleted = 0", &[(&user.id).into()])
                .map_err(|e| e.to_string())?;
            let data: Json = row.map(|r| serde_json::from_str(&r.text(0)).unwrap_or(Json::Null)).unwrap_or(Json::Null);
            Ok(json!({ "environments": data["environments"].as_array().into_iter().flatten().map(|e| {
                let secrets: Vec<&str> = e["secrets"].as_array().into_iter().flatten().filter_map(Json::as_str).collect();
                let vars: Vec<&String> = e["variables"].as_object().map(|m| m.keys().collect()).unwrap_or_default();
                json!({ "name": e["name"], "variables": vars, "secret": secrets, "organization": e.get("org").cloned().unwrap_or(Json::Null) })
            }).collect::<Vec<_>>() }))
        }
        other => Err(format!("unknown tool {other}")),
    }
}
