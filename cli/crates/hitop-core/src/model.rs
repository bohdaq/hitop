use std::collections::HashMap;
use serde::{Deserialize, Serialize};

#[cfg(test)]
mod tests {
    use super::*;

    fn sample_collection() -> Collection {
        Collection {
            id: 1,
            name: "Test".to_string(),
            requests: vec![Request {
                id: 42,
                name: "Get todo".to_string(),
                url: "https://example.com/todos/1".to_string(),
                method: "GET".to_string(),
                headers: vec![Header { name: "Accept".to_string(), value: "application/json".to_string() }],
                body: String::new(),
                pre_request_script: String::new(),
                post_request_script: String::new(),
            }],
            variables: [("host".to_string(), "https://example.com".to_string())].into(),
        }
    }

    #[test]
    fn collection_serializes_and_deserializes() {
        let col = sample_collection();
        let json = serde_json::to_string(&col).unwrap();
        let restored: Collection = serde_json::from_str(&json).unwrap();
        assert_eq!(restored.id, 1);
        assert_eq!(restored.name, "Test");
        assert_eq!(restored.requests.len(), 1);
        assert_eq!(restored.requests[0].method, "GET");
        assert_eq!(restored.variables["host"], "https://example.com");
    }

    #[test]
    fn script_fields_use_camel_case_json_keys() {
        let req = Request {
            id: 1,
            name: "r".to_string(),
            url: String::new(),
            method: "GET".to_string(),
            headers: vec![],
            body: String::new(),
            pre_request_script: "setUrl('x')".to_string(),
            post_request_script: "setContext('k','v')".to_string(),
        };
        let json = serde_json::to_string(&req).unwrap();
        assert!(json.contains("preRequestScript"));
        assert!(json.contains("postRequestScript"));
    }

    #[test]
    fn missing_script_fields_default_to_empty_string() {
        let json = r#"{"id":1,"name":"r","url":"","method":"GET","headers":[],"body":""}"#;
        let req: Request = serde_json::from_str(json).unwrap();
        assert_eq!(req.pre_request_script, "");
        assert_eq!(req.post_request_script, "");
    }

    #[test]
    fn collection_array_roundtrip() {
        let cols = vec![sample_collection()];
        let json = serde_json::to_string(&cols).unwrap();
        let restored: Vec<Collection> = serde_json::from_str(&json).unwrap();
        assert_eq!(restored.len(), 1);
        assert_eq!(restored[0].name, "Test");
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Collection {
    pub id: u64,
    pub name: String,
    pub requests: Vec<Request>,
    #[serde(default)]
    pub variables: HashMap<String, String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Request {
    pub id: u64,
    pub name: String,
    pub url: String,
    pub method: String,
    pub headers: Vec<Header>,
    pub body: String,
    #[serde(rename = "preRequestScript", default)]
    pub pre_request_script: String,
    #[serde(rename = "postRequestScript", default)]
    pub post_request_script: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Header {
    pub name: String,
    pub value: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HistoryItem {
    pub id: u64,
    pub timestamp: u64,
    pub url: String,
    pub method: String,
    pub status_code: u16,
    pub success: bool,
    pub collection_name: Option<String>,
    pub request_name: Option<String>,
    pub duration_ms: u64,
}

#[derive(Debug, Clone)]
pub struct RequestResult {
    pub status: u16,
    pub headers: HashMap<String, String>,
    pub body: String,
    pub duration_ms: u64,
}
