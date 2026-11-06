//! Mock servers (mocks.rs): a library collection answers at /mock/<id>/... from its saved
//! examples; HTML goes out as text; the plan gate; turning a mock off.

mod common;

use common::*;
use serde_json::json;

fn shop(s: &Server, token: &str) {
    let r = s.call("PUT", "/v1/library/collection/1", Some(token), Some(json!({ "baseVersion": 0, "data": {
        "id": 1, "name": "Shop API",
        "requests": [
            { "id": 11, "name": "Get order", "method": "GET", "url": "${baseUrl}/orders/${id}", "headers": [], "body": "",
              "examples": [{ "name": "paid", "status": 200, "headers": { "content-type": "application/json" }, "body": "{\"id\":7,\"status\":\"paid\"}" },
                           { "name": "missing", "status": 404, "headers": { "content-type": "application/json" }, "body": "{\"error\":\"no order\"}" }] },
            { "id": 12, "name": "Create order", "method": "POST", "url": "https://api.shop.test/orders", "headers": [], "body": "",
              "examples": [{ "name": "created", "status": 201, "headers": { "content-type": "application/json" }, "body": "{\"id\":8}" }] },
            { "id": 13, "name": "Page", "method": "GET", "url": "${baseUrl}/page", "headers": [], "body": "",
              "examples": [{ "name": "html", "status": 200, "headers": { "content-type": "text/html" }, "body": "<script>alert(1)</script>" }] },
            { "id": 14, "name": "No example", "method": "GET", "url": "${baseUrl}/health", "headers": [], "body": "" }
        ]
    } })));
    assert_eq!(r.status, 200, "{}", r.json);
}

#[test]
fn a_collection_answers_from_its_examples() {
    let s = Server::new("mocks");
    let ada = s.sign_in("ada@example.com");
    shop(&s, &ada);
    assert_eq!(s.call("GET", "/v1/library/collection/1/mock", Some(&ada), None).status, 404, "off at first");
    let on = s.call("PUT", "/v1/library/collection/1/mock", Some(&ada), None);
    assert_eq!(on.status, 201, "{}", on.json);
    let url = on.json["url"].as_str().unwrap().to_string();
    let base = &url[url.find("/mock/").unwrap()..];
    assert_eq!(base.len(), "/mock/".len() + 24);
    assert_eq!(s.call("PUT", "/v1/library/collection/1/mock", Some(&ada), None).json["url"], json!(url), "the same URL again");

    let got = s.call("GET", &format!("{base}/orders/7"), None, None);
    assert_eq!((got.status, got.json["status"].as_str()), (200, Some("paid")), "{}", got.json);
    assert_eq!(got.header("Access-Control-Allow-Origin"), Some("*"));
    let missing = s.call("GET", &format!("{base}/orders/7?example=missing"), None, None);
    assert_eq!(missing.status, 404);
    assert_eq!(s.call("POST", &format!("{base}/orders"), None, Some(json!({}))).status, 201);
    assert_eq!(s.call("DELETE", &format!("{base}/orders/7"), None, None).status, 405, "the path is known, the method isn't");
    assert_eq!(s.call("GET", &format!("{base}/nowhere"), None, None).status, 404);
    assert_eq!(s.call("GET", &format!("{base}/health"), None, None).status, 501, "no saved example");
    let page = s.call("GET", &format!("{base}/page"), None, None);
    assert!(page.content_type.as_deref().unwrap_or("").starts_with("text/plain"), "never HTML: {:?}", page.content_type);
    assert_eq!(s.call("GET", "/mock/0123456789abcdef01234567/orders/7", None, None).status, 404, "an unknown id");

    // Only the owner turns it off; then the URL stops answering.
    let bob = s.sign_in("bob@example.com");
    s.call("DELETE", "/v1/library/collection/1/mock", Some(&bob), None);
    assert_eq!(s.call("GET", &format!("{base}/orders/7"), None, None).status, 200, "Bob can't turn off Ada's mock");
    assert_eq!(s.call("DELETE", "/v1/library/collection/1/mock", Some(&ada), None).status, 200);
    assert_eq!(s.call("GET", &format!("{base}/orders/7"), None, None).status, 404);
}

#[test]
fn mocks_need_a_paid_plan_when_plans_are_on() {
    let s = Server::with("mocks-plans", |c| c.plan_limits = true);
    let ada = s.sign_in("ada@example.com");
    shop(&s, &ada);
    let r = s.call("PUT", "/v1/library/collection/1/mock", Some(&ada), None);
    assert_eq!((r.status, r.json["error"].as_str()), (402, Some("limit")), "{}", r.json);
}
