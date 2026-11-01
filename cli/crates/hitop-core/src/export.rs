use anyhow::Result;
use serde_json::{json, Value};

use crate::model::Collection;

pub fn to_hitop(collections: &[Collection]) -> Result<String> {
    Ok(serde_json::to_string_pretty(collections)?)
}

pub fn from_hitop(json: &str) -> Result<Vec<Collection>> {
    // Accept either an array or a single collection
    let v: Value = serde_json::from_str(json)?;
    if v.is_array() {
        Ok(serde_json::from_value(v)?)
    } else {
        Ok(vec![serde_json::from_value(v)?])
    }
}

pub fn to_postman(collection: &Collection) -> Result<String> {
    let items: Vec<Value> = collection
        .requests
        .iter()
        .map(|r| {
            let headers: Vec<Value> = r
                .headers
                .iter()
                .filter(|h| !h.name.is_empty())
                .map(|h| json!({ "key": h.name, "value": h.value }))
                .collect();

            let mut events = vec![];
            if !r.pre_request_script.is_empty() {
                events.push(json!({ "listen": "prerequest", "script": { "type": "text/javascript", "exec": r.pre_request_script.lines().collect::<Vec<_>>() } }));
            }
            if !r.post_request_script.is_empty() {
                events.push(json!({ "listen": "test", "script": { "type": "text/javascript", "exec": r.post_request_script.lines().collect::<Vec<_>>() } }));
            }
            let mut request = json!({
                "method": r.method,
                "header": headers,
                "url": { "raw": r.url },
                "body": if let Some(op) = r.graphql_operation() {
                    json!({ "mode": "graphql", "graphql": { "query": op.query, "variables": op.variables } })
                } else if r.body.is_empty() { json!(null) } else {
                    json!({ "mode": "raw", "raw": r.body })
                }
            });
            if let Some(auth) = crate::auth::to_postman(r.auth.as_ref()) {
                request["auth"] = auth;
            }
            json!({ "name": r.name, "request": request, "event": events })
        })
        .collect();

    let variables: Vec<Value> = collection
        .variables
        .iter()
        .map(|(k, v)| json!({ "key": k, "value": v }))
        .collect();

    let out = json!({
        "info": {
            "name": collection.name,
            "schema": "https://schema.getpostman.com/json/collection/v2.1.0/collection.json"
        },
        "item": items,
        "variable": variables
    });

    Ok(serde_json::to_string_pretty(&out)?)
}

/// A Postman collection as a HITOP collection; [`crate::postman`] has the
/// details and the report of what couldn't be carried over.
pub fn from_postman(json: &str) -> Result<Collection> {
    Ok(crate::postman::import_collection(json)?.0)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::{Header, Request};

    fn sample_collection() -> Collection {
        Collection {
            id: 1,
            name: "My API".to_string(),
            requests: vec![
                Request {
                    id: 10,
                    name: "Get todos".to_string(),
                    url: "https://example.com/todos".to_string(),
                    method: "GET".to_string(),
                    headers: vec![Header::new("Accept", "application/json")],
                    ..Default::default()
                },
                Request {
                    id: 11,
                    name: "Create todo".to_string(),
                    url: "https://example.com/todos".to_string(),
                    method: "POST".to_string(),
                    headers: vec![],
                    body: r#"{"title":"test"}"#.to_string(),
                    ..Default::default()
                },
            ],
            variables: [("host".to_string(), "https://example.com".to_string())].into(),
            ..Default::default()
        }
    }

    #[test]
    fn postman_keeps_scripts_and_auth() {
        let mut col = sample_collection();
        col.requests[0].post_request_script = "assert.status(200);\nsetContext('a', 1);".into();
        col.requests[0].auth = Some(crate::model::Auth::Bearer { token: "${t}".into() });
        let back = from_postman(&to_postman(&col).unwrap()).unwrap();
        assert_eq!(back.requests[0].post_request_script, col.requests[0].post_request_script);
        assert_eq!(back.requests[0].auth, col.requests[0].auth);
        assert_eq!(back.requests[1].auth, None);
    }

    // --- hitop format ---

    #[test]
    fn to_hitop_produces_valid_json_array() {
        let col = sample_collection();
        let json = to_hitop(std::slice::from_ref(&col)).unwrap();
        let parsed: serde_json::Value = serde_json::from_str(&json).unwrap();
        assert!(parsed.is_array());
        assert_eq!(parsed[0]["name"], "My API");
    }

    #[test]
    fn from_hitop_parses_array() {
        let col = sample_collection();
        let json = to_hitop(std::slice::from_ref(&col)).unwrap();
        let restored = from_hitop(&json).unwrap();
        assert_eq!(restored.len(), 1);
        assert_eq!(restored[0].name, "My API");
        assert_eq!(restored[0].requests.len(), 2);
        assert_eq!(restored[0].variables["host"], "https://example.com");
    }

    #[test]
    fn from_hitop_parses_single_object() {
        let col = sample_collection();
        let json = serde_json::to_string_pretty(&col).unwrap();
        let restored = from_hitop(&json).unwrap();
        assert_eq!(restored.len(), 1);
        assert_eq!(restored[0].name, "My API");
    }

    #[test]
    fn hitop_roundtrip_preserves_all_fields() {
        let col = sample_collection();
        let json = to_hitop(std::slice::from_ref(&col)).unwrap();
        let restored = from_hitop(&json).unwrap();
        let r = &restored[0].requests[1];
        assert_eq!(r.name, "Create todo");
        assert_eq!(r.method, "POST");
        assert_eq!(r.body, r#"{"title":"test"}"#);
    }

    #[test]
    fn from_hitop_invalid_json_returns_error() {
        assert!(from_hitop("not json").is_err());
    }

    // --- postman format ---

    #[test]
    fn to_postman_has_correct_schema_field() {
        let col = sample_collection();
        let json = to_postman(&col).unwrap();
        let parsed: serde_json::Value = serde_json::from_str(&json).unwrap();
        assert!(parsed["info"]["schema"].as_str().unwrap().contains("getpostman.com"));
    }

    #[test]
    fn to_postman_maps_collection_name() {
        let col = sample_collection();
        let json = to_postman(&col).unwrap();
        let parsed: serde_json::Value = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed["info"]["name"], "My API");
    }

    #[test]
    fn to_postman_maps_requests_to_items() {
        let col = sample_collection();
        let json = to_postman(&col).unwrap();
        let parsed: serde_json::Value = serde_json::from_str(&json).unwrap();
        let items = parsed["item"].as_array().unwrap();
        assert_eq!(items.len(), 2);
        assert_eq!(items[0]["name"], "Get todos");
        assert_eq!(items[0]["request"]["method"], "GET");
        assert_eq!(items[0]["request"]["url"]["raw"], "https://example.com/todos");
    }

    #[test]
    fn to_postman_maps_headers() {
        let col = sample_collection();
        let json = to_postman(&col).unwrap();
        let parsed: serde_json::Value = serde_json::from_str(&json).unwrap();
        let headers = &parsed["item"][0]["request"]["header"];
        assert_eq!(headers[0]["key"], "Accept");
        assert_eq!(headers[0]["value"], "application/json");
    }

    #[test]
    fn to_postman_body_null_when_empty() {
        let col = sample_collection();
        let json = to_postman(&col).unwrap();
        let parsed: serde_json::Value = serde_json::from_str(&json).unwrap();
        assert!(parsed["item"][0]["request"]["body"].is_null());
    }

    #[test]
    fn to_postman_body_raw_when_present() {
        let col = sample_collection();
        let json = to_postman(&col).unwrap();
        let parsed: serde_json::Value = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed["item"][1]["request"]["body"]["raw"], r#"{"title":"test"}"#);
    }

    #[test]
    fn to_postman_maps_variables() {
        let col = sample_collection();
        let json = to_postman(&col).unwrap();
        let parsed: serde_json::Value = serde_json::from_str(&json).unwrap();
        let vars = parsed["variable"].as_array().unwrap();
        assert_eq!(vars.len(), 1);
        assert_eq!(vars[0]["key"], "host");
        assert_eq!(vars[0]["value"], "https://example.com");
    }

    #[test]
    fn from_postman_parses_collection_name() {
        let col = sample_collection();
        let json = to_postman(&col).unwrap();
        let restored = from_postman(&json).unwrap();
        assert_eq!(restored.name, "My API");
    }

    #[test]
    fn from_postman_parses_requests() {
        let col = sample_collection();
        let json = to_postman(&col).unwrap();
        let restored = from_postman(&json).unwrap();
        assert_eq!(restored.requests.len(), 2);
        assert_eq!(restored.requests[0].name, "Get todos");
        assert_eq!(restored.requests[0].method, "GET");
        assert_eq!(restored.requests[0].url, "https://example.com/todos");
    }

    #[test]
    fn from_postman_parses_variables() {
        let col = sample_collection();
        let json = to_postman(&col).unwrap();
        let restored = from_postman(&json).unwrap();
        assert_eq!(restored.variables["host"], "https://example.com");
    }

    #[test]
    fn postman_roundtrip_preserves_method_and_url() {
        let col = sample_collection();
        let json = to_postman(&col).unwrap();
        let restored = from_postman(&json).unwrap();
        assert_eq!(restored.requests[1].method, "POST");
        assert_eq!(restored.requests[1].url, "https://example.com/todos");
    }
}
