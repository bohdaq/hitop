//! The YAML documents of a collection folder. Field order here is the key
//! order in the files; unknown keys land in `extra` and are written back
//! after the known ones.
use std::collections::BTreeMap;

use serde::{Deserialize, Deserializer, Serialize};
use serde_json::Value;

use crate::model::{Auth, Extra};

pub const FORMAT_VERSION: u32 = 1;

fn is_true(b: &bool) -> bool {
    *b
}

fn yes() -> bool {
    true
}

/// A string written as any YAML scalar (`port: 8080` reads as "8080").
fn stringish<'de, D: Deserializer<'de>>(d: D) -> Result<String, D::Error> {
    scalar_string(Value::deserialize(d)?).map_err(serde::de::Error::custom)
}

pub fn scalar_string(v: Value) -> Result<String, String> {
    match v {
        Value::String(s) => Ok(s),
        Value::Null => Ok(String::new()),
        Value::Bool(b) => Ok(b.to_string()),
        Value::Number(n) => Ok(n.to_string()),
        other => Err(format!("expected a string, found {other}")),
    }
}

fn strings<'de, D: Deserializer<'de>>(d: D) -> Result<Vec<String>, D::Error> {
    let raw: Vec<Value> = Vec::deserialize(d)?;
    raw.into_iter().map(scalar_string).collect::<Result<_, _>>().map_err(serde::de::Error::custom)
}

fn stringish_map<'de, D: Deserializer<'de>>(d: D) -> Result<BTreeMap<String, String>, D::Error> {
    let raw: BTreeMap<String, Value> = BTreeMap::deserialize(d)?;
    raw.into_iter()
        .map(|(k, v)| scalar_string(v).map(|v| (k, v)))
        .collect::<Result<_, _>>()
        .map_err(serde::de::Error::custom)
}

/// `hitop.yaml`.
#[derive(Debug, Serialize, Deserialize)]
pub struct CollectionDoc {
    #[serde(default)]
    pub format: Option<u32>,
    #[serde(default)]
    pub name: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub id: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub workspace: Option<String>,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty", deserialize_with = "stringish_map")]
    pub variables: BTreeMap<String, String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub auth: Option<Auth>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub order: Vec<String>,
    #[serde(flatten)]
    pub extra: Extra,
}

/// One file under `requests/`.
#[derive(Debug, Serialize, Deserialize)]
pub struct RequestDoc {
    #[serde(default)]
    pub name: Option<String>,
    /// `websocket` or `grpc`; left out for HTTP.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub kind: Option<String>,
    #[serde(default = "get")]
    pub method: String,
    #[serde(default)]
    pub url: String,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub params: Vec<RowDoc>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub headers: Vec<RowDoc>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub auth: Option<Auth>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub body: Option<BodyDoc>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub websocket: Option<WebSocketDoc>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub grpc: Option<GrpcDoc>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub scripts: Option<ScriptsDoc>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub examples: Vec<Value>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub id: Option<u64>,
    #[serde(flatten)]
    pub extra: Extra,
}

fn get() -> String {
    "GET".into()
}

/// A header, query parameter or form field row.
#[derive(Debug, Serialize, Deserialize)]
pub struct RowDoc {
    #[serde(default, deserialize_with = "stringish")]
    pub name: String,
    #[serde(default, deserialize_with = "stringish")]
    pub value: String,
    /// Form fields only: `file` (absent means text).
    #[serde(rename = "type", default, skip_serializing_if = "Option::is_none")]
    pub kind: Option<String>,
    #[serde(default = "yes", skip_serializing_if = "is_true")]
    pub enabled: bool,
    #[serde(flatten)]
    pub extra: Extra,
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct BodyDoc {
    #[serde(rename = "type", default = "none")]
    pub kind: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub content: Option<String>,
    /// GraphQL variables: JSON text (a block scalar), not a YAML map.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub variables: Option<String>,
    #[serde(rename = "operationName", default, skip_serializing_if = "Option::is_none")]
    pub operation_name: Option<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub fields: Vec<RowDoc>,
}

/// A WebSocket request's `websocket` section.
#[derive(Debug, Default, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct WebSocketDoc {
    #[serde(default, skip_serializing_if = "Vec::is_empty", deserialize_with = "strings")]
    pub subprotocols: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub messages: Vec<MessageDoc>,
}

/// A gRPC request's `grpc` section.
#[derive(Debug, Default, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct GrpcDoc {
    #[serde(default, deserialize_with = "stringish", skip_serializing_if = "String::is_empty")]
    pub service: String,
    #[serde(default, deserialize_with = "stringish", skip_serializing_if = "String::is_empty")]
    pub method: String,
    /// JSON text (a block scalar), not a YAML map, so `${variables}` work anywhere.
    #[serde(default, deserialize_with = "stringish", skip_serializing_if = "String::is_empty")]
    pub message: String,
    #[serde(default, skip_serializing_if = "Vec::is_empty", deserialize_with = "strings")]
    pub protos: Vec<String>,
    #[serde(rename = "importPaths", default, skip_serializing_if = "Vec::is_empty", deserialize_with = "strings")]
    pub import_paths: Vec<String>,
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub plaintext: bool,
}

/// A saved WebSocket message row.
#[derive(Debug, Serialize, Deserialize)]
pub struct MessageDoc {
    #[serde(default, deserialize_with = "stringish")]
    pub name: String,
    #[serde(default, deserialize_with = "stringish", skip_serializing_if = "String::is_empty")]
    pub content: String,
    #[serde(flatten)]
    pub extra: Extra,
}

fn none() -> String {
    "none".into()
}

#[derive(Debug, Default, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ScriptsDoc {
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub pre: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub post: String,
}

/// One file under `environments/`. Variables map to a value, or to
/// `{secret: true}` for a secret that has no value in the file.
#[derive(Debug, Serialize, Deserialize)]
pub struct EnvironmentDoc {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub id: Option<String>,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub variables: BTreeMap<String, Value>,
    #[serde(flatten)]
    pub extra: Extra,
}
