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
                auth: None,
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
            auth: None,
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
    fn auth_uses_the_web_app_json() {
        let json = r#"{"id":1,"name":"r","url":"","method":"GET","headers":[],"body":"","auth":{"type":"apikey","key":"X-Key","value":"${k}","in":"query"}}"#;
        let req: Request = serde_json::from_str(json).unwrap();
        assert_eq!(
            req.auth,
            Some(Auth::ApiKey { key: "X-Key".into(), value: "${k}".into(), location: ApiKeyLocation::Query })
        );
        let back = serde_json::to_value(&req).unwrap();
        assert_eq!(back["auth"]["in"], "query");
        let none: Request = serde_json::from_str(r#"{"id":1,"name":"r","url":"","method":"GET","headers":[],"body":""}"#).unwrap();
        assert!(!serde_json::to_string(&none).unwrap().contains("auth"));
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
    /// Auth preset, applied when the request is sent. Same JSON as the web
    /// app's `auth` (`{"type": "bearer", "token": "..."}` and so on).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub auth: Option<Auth>,
}

/// Auth presets: a Bearer token, Basic credentials or an API key sent as a
/// header or query parameter. Values may contain `${variables}`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "lowercase")]
pub enum Auth {
    None,
    Bearer {
        #[serde(default)]
        token: String,
    },
    Basic {
        #[serde(default)]
        username: String,
        #[serde(default)]
        password: String,
    },
    #[serde(rename = "apikey")]
    ApiKey {
        #[serde(default)]
        key: String,
        #[serde(default)]
        value: String,
        #[serde(default, rename = "in")]
        location: ApiKeyLocation,
    },
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ApiKeyLocation {
    #[default]
    Header,
    Query,
}

/// A named set of variables (Dev, Staging, Prod). The active one overrides
/// collection variables with the same name.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Environment {
    pub id: String,
    pub name: String,
    #[serde(default)]
    pub variables: HashMap<String, String>,
}

/// One `assert.*` call from a post-request script.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct TestResult {
    pub name: String,
    pub pass: bool,
    #[serde(default)]
    pub detail: String,
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
