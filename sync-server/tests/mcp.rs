//! Hosted MCP (mcp.rs): an AI assistant reads and edits the HITOP Cloud library over `POST /mcp`
//! with a personal access token; a read-only token can't change anything; no token, no access.

mod common;

use common::*;
use serde_json::{json, Value};

fn rpc(s: &Server, token: Option<&str>, method: &str, params: Value) -> Reply {
    s.call("POST", "/mcp", token, Some(json!({ "jsonrpc": "2.0", "id": 1, "method": method, "params": params })))
}

fn tool(s: &Server, token: &str, name: &str, args: Value) -> Value {
    let r = rpc(s, Some(token), "tools/call", json!({ "name": name, "arguments": args }));
    assert_eq!(r.status, 200, "{}", r.json);
    r.json["result"].clone()
}

#[test]
fn an_assistant_reads_and_edits_the_library_with_a_token() {
    let s = Server::new("hosted-mcp");
    let ada = s.sign_in("ada@example.com");
    let put = s.call("PUT", "/v1/library/collection/1", Some(&ada), Some(json!({ "baseVersion": 0, "data": {
        "id": 1, "name": "Shop API", "variables": { "baseUrl": "https://shop.test" },
        "requests": [{ "id": 11, "name": "List orders", "method": "GET", "url": "${baseUrl}/orders", "headers": [], "body": "", "preRequestScript": "", "postRequestScript": "" }]
    } })));
    assert_eq!(put.status, 200, "{}", put.json);
    let token = |scopes: Value| s.call("POST", "/v1/me/tokens", Some(&ada), Some(json!({ "name": "assistant", "scopes": scopes, "expiresInDays": 30 }))).json["token"].as_str().unwrap().to_string();
    let read = token(json!(["read"]));
    let write = token(json!(["read", "write"]));

    assert_eq!(rpc(&s, None, "initialize", json!({})).status, 401, "no token, no access");
    let init = rpc(&s, Some(&read), "initialize", json!({ "protocolVersion": "2025-06-18" }));
    assert_eq!(init.json["result"]["protocolVersion"], "2025-06-18");
    let names: Vec<String> = rpc(&s, Some(&read), "tools/list", json!({})).json["result"]["tools"].as_array().unwrap().iter().map(|t| t["name"].as_str().unwrap().to_string()).collect();
    assert_eq!(names, ["list_collections", "list_requests", "get_request", "create_request", "update_request", "list_environments"]);

    let list = tool(&s, &read, "list_collections", json!({}));
    assert_eq!(list["structuredContent"]["collections"][0]["name"], "Shop API", "{list}");
    let req = tool(&s, &read, "get_request", json!({ "collection": "shop api", "request": "List orders" }));
    assert_eq!(req["structuredContent"]["url"], "${baseUrl}/orders");

    // A read-only token can't change the library.
    let refused = rpc(&s, Some(&read), "tools/call", json!({ "name": "create_request", "arguments": { "collection": "Shop API", "name": "Get order", "url": "${baseUrl}/orders/1" } }));
    assert_eq!(refused.status, 403, "{}", refused.json);

    let made = tool(&s, &write, "create_request", json!({ "collection": "Shop API", "name": "Get order", "method": "get", "url": "${baseUrl}/orders/1", "scriptAfter": "pm.test('ok', () => pm.response.to.have.status(200));" }));
    assert_eq!(made["isError"], false, "{made}");
    let changed = tool(&s, &write, "update_request", json!({ "collection": "1", "request": "List orders", "url": "${baseUrl}/orders?limit=5", "headers": [{ "name": "Accept", "value": "application/json" }] }));
    assert_eq!(changed["isError"], false, "{changed}");
    let dup = tool(&s, &write, "create_request", json!({ "collection": "Shop API", "name": "Get order", "url": "x" }));
    assert_eq!(dup["isError"], true, "a second request of the same name is refused");

    // The apps see the new version.
    let lib = s.call("GET", "/v1/library", Some(&ada), None).json;
    let item = lib["items"].as_array().unwrap().iter().find(|i| i["kind"] == "collection").unwrap().clone();
    assert_eq!(item["version"], 3, "{item}");
    let reqs = item["data"]["requests"].as_array().unwrap();
    assert_eq!(reqs.len(), 2);
    assert_eq!(reqs[0]["url"], "${baseUrl}/orders?limit=5");
    assert_eq!(reqs[1]["method"], "GET");
    assert!(reqs[1]["postRequestScript"].as_str().unwrap().contains("pm.test"));

    // Another account's token sees nothing of Ada's.
    let bob = s.sign_in("bob@example.com");
    let bob_token = s.call("POST", "/v1/me/tokens", Some(&bob), Some(json!({ "name": "b", "scopes": ["read"], "expiresInDays": 1 }))).json["token"].as_str().unwrap().to_string();
    assert_eq!(tool(&s, &bob_token, "list_collections", json!({}))["structuredContent"]["collections"], json!([]));
    assert_eq!(tool(&s, &bob_token, "get_request", json!({ "collection": "Shop API", "request": "List orders" }))["isError"], true);
}
