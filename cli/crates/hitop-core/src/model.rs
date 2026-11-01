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
                headers: vec![Header::new("Accept", "application/json")],
                ..Default::default()
            }],
            variables: [("host".to_string(), "https://example.com".to_string())].into(),
            ..Default::default()
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
            pre_request_script: "setUrl('x')".to_string(),
            post_request_script: "setContext('k','v')".to_string(),
            ..Default::default()
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
    fn existing_json_serializes_unchanged() {
        let json = r#"{"id":1,"name":"Default","requests":[{"id":2,"name":"r","url":"https://x","method":"GET","headers":[{"name":"","value":""}],"body":"","preRequestScript":"","postRequestScript":""}],"variables":{"a":"1"}}"#;
        let col: Collection = serde_json::from_str(json).unwrap();
        let back: serde_json::Value = serde_json::to_value(&col).unwrap();
        assert_eq!(back, serde_json::from_str::<serde_json::Value>(json).unwrap());
    }

    #[test]
    fn web_app_fields_and_unknown_keys_survive_a_save() {
        let json = r#"{"id":1,"name":"c","color":"teal","requests":[{"id":2,"name":"r","url":"u","method":"POST","headers":[{"name":"X","value":"1","enabled":false}],"body":"",
            "bodyMode":"multipart","formFields":[{"id":"f1","key":"file","value":"","type":"file","enabled":false,"fileName":"a.png"}],
            "disabledParams":[{"key":"debug","value":"1"}],"pinned":true}]}"#;
        let col: Collection = serde_json::from_str(json).unwrap();
        let r = &col.requests[0];
        assert_eq!(r.body_mode.as_deref(), Some("multipart"));
        assert!(!r.headers[0].enabled);
        assert_eq!(r.form_fields[0].extra["fileName"], "a.png");
        assert_eq!(r.disabled_params[0].key, "debug");
        assert_eq!(r.extra["pinned"], true);
        let back = serde_json::to_value(&col).unwrap();
        assert_eq!(back["color"], "teal");
        assert_eq!(back["requests"][0]["formFields"][0]["id"], "f1");
        assert_eq!(back["requests"][0]["headers"][0]["enabled"], false);
    }

    #[test]
    fn raw_body_mode_reads_as_none() {
        let json = r#"{"id":1,"name":"r","url":"","method":"GET","headers":[],"body":"","bodyMode":"raw"}"#;
        let req: Request = serde_json::from_str(json).unwrap();
        assert_eq!(req.body_mode, None);
        assert!(!serde_json::to_string(&req).unwrap().contains("bodyMode"));
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

/// Fields a JSON object carries that hitop-core doesn't model. Kept so a
/// save never drops what another surface (or a newer version) wrote.
pub type Extra = serde_json::Map<String, serde_json::Value>;

fn is_true(b: &bool) -> bool {
    *b
}

fn yes() -> bool {
    true
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct Collection {
    pub id: u64,
    pub name: String,
    pub requests: Vec<Request>,
    #[serde(default)]
    pub variables: HashMap<String, String>,
    /// The id that keys this collection's secrets in the OS keychain
    /// (`workspace` in a folder's `hitop.yaml`; see `secrets::workspace_id`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub workspace: Option<String>,
    /// Default auth for requests without their own `auth`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub auth: Option<Auth>,
    #[serde(flatten)]
    pub extra: Extra,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
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
    /// The web app's body mode: `multipart`, `urlencoded`, or None for a raw
    /// body (`"raw"` in JSON reads as None). Kept as a string so a mode added
    /// later never makes stored collections unreadable.
    #[serde(rename = "bodyMode", default, deserialize_with = "de_body_mode", skip_serializing_if = "Option::is_none")]
    pub body_mode: Option<String>,
    /// Form rows for the `multipart` and `urlencoded` body modes.
    #[serde(rename = "formFields", default, skip_serializing_if = "Vec::is_empty")]
    pub form_fields: Vec<FormField>,
    /// Query parameters switched off in the editor (the ones that are on
    /// live in `url`).
    #[serde(rename = "disabledParams", default, skip_serializing_if = "Vec::is_empty")]
    pub disabled_params: Vec<Param>,
    /// The body type a collection folder gives, kept only when it differs
    /// from what hitop infers from the body and headers (json, xml, text,
    /// graphql, ...).
    #[serde(rename = "bodyType", default, skip_serializing_if = "Option::is_none")]
    pub body_type: Option<String>,
    /// Saved responses.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub examples: Vec<serde_json::Value>,
    /// Where the request lives in a collection folder, relative to
    /// `requests/` and without `.yaml` (`orders/create-order`). None means
    /// the default: the request name as a file name at the top level.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub file: Option<String>,
    #[serde(flatten)]
    pub extra: Extra,
}

fn de_body_mode<'de, D: serde::Deserializer<'de>>(d: D) -> Result<Option<String>, D::Error> {
    let mode: Option<String> = Option::deserialize(d)?;
    Ok(mode.filter(|m| m != "raw" && !m.is_empty()))
}

/// A query parameter row.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct Param {
    #[serde(default)]
    pub key: String,
    #[serde(default)]
    pub value: String,
    #[serde(flatten)]
    pub extra: Extra,
}

/// A form body row: `kind` is `text` or `file`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct FormField {
    #[serde(default)]
    pub key: String,
    #[serde(default)]
    pub value: String,
    #[serde(rename = "type", default = "text_kind")]
    pub kind: String,
    #[serde(default = "yes", skip_serializing_if = "is_true")]
    pub enabled: bool,
    #[serde(flatten)]
    pub extra: Extra,
}

fn text_kind() -> String {
    "text".into()
}

impl Default for FormField {
    fn default() -> Self {
        FormField { key: String::new(), value: String::new(), kind: text_kind(), enabled: true, extra: Extra::new() }
    }
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
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct Environment {
    pub id: String,
    pub name: String,
    #[serde(default)]
    pub variables: HashMap<String, String>,
    /// Names of secret variables. Their values never go into collection
    /// files or exports; `secrets` resolves them at run time (env vars,
    /// `--secret`, the OS keychain, a prompt). The web app keeps their values
    /// in `variables` (its localStorage); the CLI keeps them in the keychain.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub secrets: Vec<String>,
    #[serde(flatten)]
    pub extra: Extra,
}

/// One `assert.*` call from a post-request script.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct TestResult {
    pub name: String,
    pub pass: bool,
    #[serde(default)]
    pub detail: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Header {
    pub name: String,
    pub value: String,
    /// False for a header row switched off: kept, not sent.
    #[serde(default = "yes", skip_serializing_if = "is_true")]
    pub enabled: bool,
    #[serde(flatten)]
    pub extra: Extra,
}

impl Header {
    pub fn new(name: impl Into<String>, value: impl Into<String>) -> Self {
        Header { name: name.into(), value: value.into(), enabled: true, extra: Extra::new() }
    }
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
