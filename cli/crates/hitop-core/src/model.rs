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
    fn string_ids_from_old_web_app_data_load() {
        let json = r#"[{"id":"col-legacy","name":"Billing API","requests":[{"id":"r1","name":"r","url":"","method":"GET","headers":[],"body":""},{"id":"42","name":"s","url":"","method":"GET","headers":[],"body":""},{"id":1.793470569736e15,"name":"t","url":"","method":"GET","headers":[],"body":""}]}]"#;
        let cols: Vec<Collection> = serde_json::from_str(json).unwrap();
        let again: Vec<Collection> = serde_json::from_str(json).unwrap();
        assert_eq!(cols[0].id, again[0].id, "the same string always gives the same id");
        assert_eq!(cols[0].id, crate::format::names::derived_id("legacy-id/col-legacy"));
        assert!(cols[0].id < 1 << 53, "JavaScript keeps it exact");
        assert_ne!(cols[0].requests[0].id, cols[0].id);
        assert_eq!(cols[0].requests[1].id, 42);
        assert_eq!(cols[0].requests[2].id, 1_793_470_569_736_000);
        let back = serde_json::to_value(&cols).unwrap();
        assert!(back[0]["id"].is_u64());
        for bad in [r#"{"id":-1,"name":"c","requests":[]}"#, r#"{"id":1.5,"name":"c","requests":[]}"#, r#"{"id":null,"name":"c","requests":[]}"#] {
            assert!(serde_json::from_str::<Collection>(bad).is_err(), "{bad}");
        }
    }

    #[test]
    fn history_keeps_what_the_web_app_adds() {
        let json = r#"{"id":1,"timestamp":1793470570537,"url":"https://x/","method":"GET","status_code":200,"success":true,"collection_name":null,"request_name":null,"duration_ms":12,"headers":[{"name":"A","value":"1"}],"body":"","isCollectionRun":false}"#;
        let item: HistoryItem = serde_json::from_str(json).unwrap();
        assert_eq!(item.extra["headers"][0]["name"], "A");
        let back = serde_json::to_value(&item).unwrap();
        assert_eq!(back, serde_json::from_str::<serde_json::Value>(json).unwrap());
        // What the CLI writes still reads.
        let cli = r#"{"id":1,"timestamp":1,"url":"u","method":"GET","status_code":200,"success":true,"collection_name":null,"request_name":null,"duration_ms":1}"#;
        assert!(serde_json::from_str::<HistoryItem>(cli).unwrap().extra.is_empty());
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
    #[serde(deserialize_with = "de_id")]
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
    #[serde(deserialize_with = "de_id")]
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
    /// A GraphQL request's variables and operation name (the query is
    /// `body`, `bodyType` is `graphql`). Kept when the body type changes, as
    /// the form fields are; sent only for a GraphQL body.
    #[serde(default, deserialize_with = "de_graphql", skip_serializing_if = "Option::is_none")]
    pub graphql: Option<Graphql>,
    /// What kind of request this is: None for HTTP, `websocket` for a
    /// WebSocket connection (see `websocket`). Kept as a string so a kind
    /// added later never makes stored collections unreadable; `"http"` reads
    /// as None.
    #[serde(default, deserialize_with = "de_kind", skip_serializing_if = "Option::is_none")]
    pub kind: Option<String>,
    /// A WebSocket request's subprotocols and saved messages (its URL,
    /// headers and auth are the request's own).
    #[serde(default, deserialize_with = "de_websocket", skip_serializing_if = "Option::is_none")]
    pub websocket: Option<WebSocket>,
    /// A gRPC request's service, method, message and schema source (its
    /// URL is the server address; headers and auth are sent as metadata).
    #[serde(default, deserialize_with = "de_grpc", skip_serializing_if = "Option::is_none")]
    pub grpc: Option<Grpc>,
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

/// Ids are numbers, but older web app data has strings (`"col-legacy"`).
/// A string of digits reads as that number; any other string as a stable
/// number derived from it (FNV-1a 64 of `legacy-id/<string>`, low 53 bits,
/// so JavaScript keeps it exact). Saving writes the number.
fn de_id<'de, D: serde::Deserializer<'de>>(d: D) -> Result<u64, D::Error> {
    use serde::de::Error;
    match serde_json::Value::deserialize(d)? {
        serde_json::Value::Number(n) => match (n.as_u64(), n.as_f64()) {
            (Some(v), _) => Ok(v),
            (None, Some(f)) if f >= 0.0 && f.fract() == 0.0 && f < 18446744073709551616.0 => Ok(f as u64),
            _ => Err(D::Error::custom(format!("invalid id {n}: ids are whole numbers from 0"))),
        },
        serde_json::Value::String(s) => Ok(s.parse::<u64>().unwrap_or_else(|_| crate::format::names::derived_id(&format!("legacy-id/{s}")))),
        other => Err(D::Error::custom(format!("invalid id {other}: expected a number or a string"))),
    }
}

fn de_body_mode<'de, D: serde::Deserializer<'de>>(d: D) -> Result<Option<String>, D::Error> {
    let mode: Option<String> = Option::deserialize(d)?;
    Ok(mode.filter(|m| m != "raw" && !m.is_empty()))
}

/// `graphql: {}` (nothing in it) reads as no `graphql`.
fn de_graphql<'de, D: serde::Deserializer<'de>>(d: D) -> Result<Option<Graphql>, D::Error> {
    let g: Option<Graphql> = Option::deserialize(d)?;
    Ok(g.filter(|g| !g.is_empty()))
}

fn de_kind<'de, D: serde::Deserializer<'de>>(d: D) -> Result<Option<String>, D::Error> {
    let kind: Option<String> = Option::deserialize(d)?;
    Ok(kind.filter(|k| k != "http" && !k.is_empty()))
}

/// `websocket: {}` (nothing in it) reads as no `websocket`.
fn de_websocket<'de, D: serde::Deserializer<'de>>(d: D) -> Result<Option<WebSocket>, D::Error> {
    let w: Option<WebSocket> = Option::deserialize(d)?;
    Ok(w.filter(|w| !w.is_empty()))
}

/// `grpc: {}` (nothing in it) reads as no `grpc`.
fn de_grpc<'de, D: serde::Deserializer<'de>>(d: D) -> Result<Option<Grpc>, D::Error> {
    let g: Option<Grpc> = Option::deserialize(d)?;
    Ok(g.filter(|g| !g.is_empty()))
}

/// The `kind` of a gRPC request.
pub const GRPC: &str = "grpc";

/// What a gRPC request carries besides its address, metadata and auth.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct Grpc {
    /// The service's full name (`package.Service`).
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub service: String,
    /// The method's name in the service.
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub method: String,
    /// The request message as JSON text (proto3 JSON mapping); may contain
    /// `${variables}`.
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub message: String,
    /// `.proto` files the schema comes from, relative to the collection
    /// folder; none means server reflection.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub protos: Vec<String>,
    /// Directories imports in `protos` are looked up in, relative to the
    /// collection folder (each file's own directory is always searched).
    #[serde(rename = "importPaths", default, skip_serializing_if = "Vec::is_empty")]
    pub import_paths: Vec<String>,
    /// HTTP/2 without TLS for a `host:port` address (`http://` addresses
    /// are always plaintext, `https://` never).
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub plaintext: bool,
    #[serde(flatten)]
    pub extra: Extra,
}

impl Grpc {
    pub fn is_empty(&self) -> bool {
        self.service.is_empty()
            && self.method.is_empty()
            && self.message.is_empty()
            && self.protos.is_empty()
            && self.import_paths.is_empty()
            && !self.plaintext
            && self.extra.is_empty()
    }
}

/// The `kind` of a WebSocket request.
pub const WEBSOCKET: &str = "websocket";

/// What a WebSocket request carries besides its URL, headers and auth.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct WebSocket {
    /// Offered in `Sec-WebSocket-Protocol`, in order.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub subprotocols: Vec<String>,
    /// Messages saved to send, each with a name.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub messages: Vec<WsMessage>,
    #[serde(flatten)]
    pub extra: Extra,
}

impl WebSocket {
    pub fn is_empty(&self) -> bool {
        self.subprotocols.is_empty() && self.messages.is_empty() && self.extra.is_empty()
    }
}

/// A saved WebSocket message: text, `${variables}` filled in when sent.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct WsMessage {
    #[serde(default)]
    pub name: String,
    #[serde(default)]
    pub content: String,
    #[serde(flatten)]
    pub extra: Extra,
}

/// What a GraphQL request sends besides its query (see `graphql.rs`).
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct Graphql {
    /// JSON text of an object, as typed; may contain `${variables}`.
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub variables: String,
    #[serde(rename = "operationName", default, skip_serializing_if = "String::is_empty")]
    pub operation_name: String,
    #[serde(flatten)]
    pub extra: Extra,
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
    /// OAuth 2.0: a token from `token_url` (client credentials, or the password grant), sent as
    /// `Authorization: Bearer` (oauth2.rs). The token lives in memory for its lifetime.
    #[serde(rename = "oauth2")]
    OAuth2(Box<OAuth2>),
    /// HTTP Digest: answered after the server's `401` challenge (digest_auth.rs).
    Digest {
        #[serde(default)]
        username: String,
        #[serde(default)]
        password: String,
    },
    /// AWS Signature Version 4: the request signed as sent (aws_sigv4.rs).
    #[serde(rename = "awsv4")]
    AwsV4(Box<AwsV4>),
}

/// AWS credentials, region and service for SigV4; values may contain `${variables}`.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AwsV4 {
    #[serde(default)]
    pub access_key: String,
    #[serde(default)]
    pub secret_key: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub session_token: String,
    #[serde(default)]
    pub region: String,
    #[serde(default)]
    pub service: String,
}

/// An OAuth 2.0 token request (RFC 6749 §4.3, §4.4); values may contain `${variables}`.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct OAuth2 {
    /// `client_credentials` (default), `password` or `authorization_code` (PKCE; a person signs in
    /// in a browser).
    #[serde(default = "client_credentials")]
    pub grant: String,
    #[serde(default)]
    pub token_url: String,
    #[serde(default)]
    pub client_id: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub client_secret: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub scope: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub username: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub password: String,
    /// Where the client id and secret go: `basic` (an Authorization header, default) or `body`.
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub client_auth: String,
    /// `authorization_code`: the provider's sign-in page.
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub auth_url: String,
    /// `authorization_code`: where the provider sends the browser back (default: the CLI's
    /// loopback address, the web app's callback page).
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub redirect_uri: String,
}

fn client_credentials() -> String {
    "client_credentials".into()
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

/// The id and name of the globals scope: the variables below every other
/// scope (globals < collection < environment), kept as an [`Environment`]
/// (`globals.json` in global storage, `globals.yaml` in a collection folder).
pub const GLOBALS_ID: &str = "globals";

impl Environment {
    /// An empty globals scope.
    pub fn globals() -> Environment {
        Environment { id: GLOBALS_ID.into(), name: GLOBALS_ID.into(), ..Default::default() }
    }
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
    /// What the desktop's web app keeps besides (headers, body, scripts).
    #[serde(flatten)]
    pub extra: Extra,
}

#[derive(Debug, Clone)]
pub struct RequestResult {
    pub status: u16,
    pub headers: HashMap<String, String>,
    pub body: String,
    pub duration_ms: u64,
    /// Where the time went, when the client could tell (src/timing.rs).
    pub timings: Option<Timings>,
    /// The redirects followed on the way (src/redirects.rs).
    pub redirects: Vec<Hop>,
}

/// One redirect answer on the way to the final response.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Hop {
    pub status: u16,
    /// The URL that answered with the redirect.
    pub url: String,
    /// Where it pointed, resolved against `url`.
    pub location: String,
    /// Its `Set-Cookie` lines as sent.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub set_cookies: Vec<String>,
}

/// The phases of one request, in milliseconds. `dns + connect + wait + download == total`.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Timings {
    /// Looking up the host name (0 for an IP literal or a reused connection).
    pub dns_ms: u64,
    /// Opening the TCP connection and, for https, the TLS handshake.
    pub connect_ms: u64,
    /// From the connection being ready (or the start, when reused) to the response headers:
    /// sending the request and the server's think time.
    pub wait_ms: u64,
    /// Reading the response body.
    pub download_ms: u64,
    pub total_ms: u64,
    /// The request went over a connection that was already open.
    pub reused: bool,
}



/// Auth and headers a collection or a folder gives the requests under it (hitop.yaml `headers`
/// and `folderDefaults`, the web app's collection page). A request without its own `auth`
/// inherits the nearest one (its folder's, a parent folder's, then the collection's); a
/// request's `{type: none}` sends no auth. Inherited headers go first; a request header of the
/// same name replaces them.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct Defaults {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub auth: Option<Auth>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub headers: Vec<Header>,
}

/// A path segment value percent-encoded, `${...}` left for interpolation.
fn encode_segment(v: &str) -> String {
    let mut out = String::new();
    for c in v.chars() {
        if c.is_ascii_alphanumeric() || "-._~${}".contains(c) {
            out.push(c);
        } else {
            let mut buf = [0u8; 4];
            for b in c.encode_utf8(&mut buf).bytes() {
                out.push_str(&format!("%{b:02X}"));
            }
        }
    }
    out
}

/// Per-request sending options (`settings` in the JSON model and in a request file).
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RequestSettings {
    /// False: answer with the redirect itself instead of following it. Absent means true.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub follow_redirects: Option<bool>,
    /// A time limit for this request in milliseconds (0 or absent: the run's or none).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub timeout_ms: Option<u64>,
}

impl Request {
    /// The request's `settings` (absent or unreadable: the defaults).
    pub fn settings(&self) -> RequestSettings {
        self.extra.get("settings").and_then(|v| serde_json::from_value(v.clone()).ok()).unwrap_or_default()
    }

    /// `:name` path segments filled in from `pathVariables` ([{key, value}]); a name with no value
    /// stays as it is. Values are percent-encoded, `${variables}` in them kept for interpolation.
    pub fn url_with_path_variables(&self) -> String {
        let rows: Vec<(String, String)> = self
            .extra
            .get("pathVariables")
            .and_then(|v| v.as_array())
            .map(|a| {
                a.iter()
                    .filter_map(|r| Some((r.get("key")?.as_str()?.to_string(), r.get("value").and_then(|v| v.as_str()).unwrap_or("").to_string())))
                    .filter(|(k, v)| !k.is_empty() && !v.is_empty())
                    .collect()
            })
            .unwrap_or_default();
        if rows.is_empty() {
            return self.url.clone();
        }
        let cut = self.url.find(['?', '#']).unwrap_or(self.url.len());
        let (head, tail) = self.url.split_at(cut);
        let mut out = String::new();
        let mut rest = head;
        while let Some(at) = rest.find("/:") {
            out.push_str(&rest[..at + 1]);
            let after = &rest[at + 2..];
            let len = after.find(|c: char| !(c.is_ascii_alphanumeric() || c == '_' || c == '-')).unwrap_or(after.len());
            let name = &after[..len];
            let ends = after[len..].is_empty() || after[len..].starts_with('/');
            match rows.iter().find(|(k, _)| k == name).filter(|_| ends && !name.is_empty()) {
                Some((_, v)) => out.push_str(&encode_segment(v)),
                None => {
                    out.push(':');
                    out.push_str(name);
                }
            }
            rest = &after[len..];
        }
        out.push_str(rest);
        out.push_str(tail);
        out
    }

    /// The folder the request lives in (`""` at the top), from `file`.
    pub fn folder(&self) -> &str {
        self.file.as_deref().and_then(|f| f.rsplit_once('/')).map_or("", |(dir, _)| dir)
    }
}

impl Collection {
    /// The collection's own headers (`headers`).
    pub fn headers(&self) -> Vec<Header> {
        self.extra.get("headers").and_then(|v| serde_json::from_value(v.clone()).ok()).unwrap_or_default()
    }

    /// Folder defaults by folder path (`folderDefaults`).
    pub fn folder_defaults(&self) -> std::collections::BTreeMap<String, Defaults> {
        self.extra.get("folderDefaults").and_then(|v| serde_json::from_value(v.clone()).ok()).unwrap_or_default()
    }

    /// `request` as it is sent: auth inherited from its folders or the collection when it has
    /// none, inherited headers in front of its own.
    pub fn effective(&self, request: &Request) -> Request {
        let folders = self.folder_defaults();
        // Outermost first: the collection, then each folder down to the request's.
        let mut layers: Vec<Defaults> = vec![Defaults { auth: self.auth.clone(), headers: self.headers() }];
        let folder = request.folder();
        if !folder.is_empty() {
            let parts: Vec<&str> = folder.split('/').collect();
            for i in 1..=parts.len() {
                if let Some(d) = folders.get(&parts[..i].join("/")) {
                    layers.push(d.clone());
                }
            }
        }
        let mut out = request.clone();
        out.url = request.url_with_path_variables();
        if out.auth.is_none() {
            out.auth = layers.iter().rev().find_map(|d| d.auth.clone());
        }
        let own: std::collections::HashSet<String> = request.headers.iter().filter(|h| h.enabled && !h.name.is_empty()).map(|h| h.name.to_ascii_lowercase()).collect();
        let mut inherited: Vec<Header> = vec![];
        for layer in &layers {
            for h in layer.headers.iter().filter(|h| h.enabled && !h.name.is_empty()) {
                inherited.retain(|x| !x.name.eq_ignore_ascii_case(&h.name));
                inherited.push(h.clone());
            }
        }
        inherited.retain(|h| !own.contains(&h.name.to_ascii_lowercase()));
        inherited.extend(request.headers.iter().cloned());
        out.headers = inherited;
        out
    }
}

#[cfg(test)]
mod defaults_tests {
    use super::*;

    #[test]
    fn requests_inherit_the_nearest_auth_and_headers() {
        let collection: Collection = serde_json::from_value(serde_json::json!({
            "id": 1, "name": "c",
            "auth": {"type": "bearer", "token": "col"},
            "headers": [{"name": "X-App", "value": "1"}, {"name": "Accept", "value": "*/*"}],
            "folderDefaults": {"orders": {"auth": {"type": "basic", "username": "u", "password": "p"}, "headers": [{"name": "X-App", "value": "2"}]}},
            "requests": [
                {"id": 1, "name": "a", "url": "u", "method": "GET", "headers": [], "body": ""},
                {"id": 2, "name": "b", "url": "u", "method": "GET", "headers": [{"name": "accept", "value": "json"}], "body": "", "file": "orders/new/b"},
                {"id": 3, "name": "c", "url": "u", "method": "GET", "headers": [], "body": "", "file": "orders/c", "auth": {"type": "none"},
                 "settings": {"followRedirects": false, "timeoutMs": 500}}
            ]
        }))
        .unwrap();
        let a = collection.effective(&collection.requests[0]);
        assert_eq!(a.auth, Some(Auth::Bearer { token: "col".into() }));
        assert_eq!(a.headers.iter().map(|h| (h.name.as_str(), h.value.as_str())).collect::<Vec<_>>(), [("X-App", "1"), ("Accept", "*/*")]);
        let b = collection.effective(&collection.requests[1]);
        assert!(matches!(b.auth, Some(Auth::Basic { .. })), "the folder's auth, from a parent folder");
        assert_eq!(b.headers.iter().map(|h| (h.name.as_str(), h.value.as_str())).collect::<Vec<_>>(), [("X-App", "2"), ("accept", "json")]);
        let c = &collection.requests[2];
        assert_eq!(collection.effective(c).auth, Some(Auth::None), "a request's none wins");
        assert_eq!(c.settings(), RequestSettings { follow_redirects: Some(false), timeout_ms: Some(500) });
        assert_eq!(collection.requests[0].settings(), RequestSettings::default());
    }
}
