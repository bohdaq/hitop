use anyhow::Result;
use serde_json::{json, Value};

use crate::model::{Collection, Header, Request};

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

            json!({
                "name": r.name,
                "request": {
                    "method": r.method,
                    "header": headers,
                    "url": { "raw": r.url },
                    "body": if r.body.is_empty() { json!(null) } else {
                        json!({ "mode": "raw", "raw": r.body })
                    }
                }
            })
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

pub fn from_postman(json: &str) -> Result<Collection> {
    let v: Value = serde_json::from_str(json)?;

    let name = v["info"]["name"].as_str().unwrap_or("Imported").to_string();

    let requests: Vec<Request> = v["item"]
        .as_array()
        .unwrap_or(&vec![])
        .iter()
        .filter_map(|item| {
            let req = &item["request"];
            let method = req["method"].as_str().unwrap_or("GET").to_string();
            let url = req["url"]["raw"].as_str()
                .or_else(|| req["url"].as_str())
                .unwrap_or("")
                .to_string();
            let req_name = item["name"].as_str().unwrap_or("Request").to_string();
            let headers: Vec<Header> = req["header"]
                .as_array()
                .unwrap_or(&vec![])
                .iter()
                .map(|h| Header {
                    name: h["key"].as_str().unwrap_or("").to_string(),
                    value: h["value"].as_str().unwrap_or("").to_string(),
                })
                .collect();
            let body = req["body"]["raw"].as_str().unwrap_or("").to_string();

            Some(Request {
                id: generate_id(),
                name: req_name,
                url,
                method,
                headers,
                body,
                pre_request_script: String::new(),
                post_request_script: String::new(),
            })
        })
        .collect();

    let variables = v["variable"]
        .as_array()
        .unwrap_or(&vec![])
        .iter()
        .filter_map(|var| {
            let k = var["key"].as_str()?.to_string();
            let v = var["value"].as_str().unwrap_or("").to_string();
            Some((k, v))
        })
        .collect();

    Ok(Collection {
        id: generate_id(),
        name,
        requests,
        variables,
    })
}

fn generate_id() -> u64 {
    use std::time::{SystemTime, UNIX_EPOCH};
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as u64
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::Header;

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
                    headers: vec![Header { name: "Accept".to_string(), value: "application/json".to_string() }],
                    body: String::new(),
                    pre_request_script: String::new(),
                    post_request_script: String::new(),
                },
                Request {
                    id: 11,
                    name: "Create todo".to_string(),
                    url: "https://example.com/todos".to_string(),
                    method: "POST".to_string(),
                    headers: vec![],
                    body: r#"{"title":"test"}"#.to_string(),
                    pre_request_script: String::new(),
                    post_request_script: String::new(),
                },
            ],
            variables: [("host".to_string(), "https://example.com".to_string())].into(),
        }
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
