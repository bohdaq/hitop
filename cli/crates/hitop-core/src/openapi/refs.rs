//! Parsing a spec (JSON or YAML) and resolving local `$ref`s.
use anyhow::{anyhow, bail, Result};
use serde_json::{Map, Value};

/// A spec's text as JSON. YAML map keys that aren't strings (response
/// codes written `200:`) become strings; merge keys (`<<`) are applied.
pub fn parse(text: &str) -> Result<Value> {
    let text = text.trim_start_matches('\u{feff}');
    if text.trim_start().starts_with('{') {
        if let Ok(v) = serde_json::from_str::<Value>(text) {
            return Ok(v);
        }
    }
    let mut y: yaml_serde::Value = yaml_serde::from_str(text).map_err(|e| anyhow!("not JSON or YAML: {e}"))?;
    y.apply_merge().map_err(|e| anyhow!("YAML merge keys: {e}"))?;
    Ok(from_yaml(y))
}

fn from_yaml(v: yaml_serde::Value) -> Value {
    use yaml_serde::Value as Y;
    match v {
        Y::Null => Value::Null,
        Y::Bool(b) => Value::Bool(b),
        Y::Number(n) => {
            if let Some(i) = n.as_i64() {
                Value::from(i)
            } else if let Some(u) = n.as_u64() {
                Value::from(u)
            } else {
                n.as_f64().and_then(serde_json::Number::from_f64).map(Value::Number).unwrap_or(Value::Null)
            }
        }
        Y::String(s) => Value::String(s),
        Y::Sequence(s) => Value::Array(s.into_iter().map(from_yaml).collect()),
        Y::Mapping(m) => {
            let mut out = Map::new();
            for (k, v) in m {
                out.insert(key_text(k), from_yaml(v));
            }
            Value::Object(out)
        }
        Y::Tagged(t) => from_yaml(t.value),
    }
}

fn key_text(k: yaml_serde::Value) -> String {
    use yaml_serde::Value as Y;
    match k {
        Y::String(s) => s,
        Y::Null => "null".into(),
        Y::Bool(b) => b.to_string(),
        Y::Number(n) => n.to_string(),
        Y::Tagged(t) => key_text(t.value),
        other => yaml_serde::to_string(&other).unwrap_or_default().trim().to_string(),
    }
}

/// Why a `$ref` couldn't be followed.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RefError {
    /// Points into another file or URL: not supported.
    External(String),
    /// Points at nothing in this document.
    Missing(String),
    /// Refs that only point at each other.
    Loop(String),
}

impl std::fmt::Display for RefError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            RefError::External(r) => write!(f, "external $ref {r} (only refs inside the spec are followed)"),
            RefError::Missing(r) => write!(f, "$ref {r} points at nothing"),
            RefError::Loop(r) => write!(f, "$ref {r} loops back to itself"),
        }
    }
}

/// Follows local `$ref`s (`#/components/schemas/Pet`, any JSON pointer).
pub struct Resolver<'a> {
    pub root: &'a Value,
}

impl<'a> Resolver<'a> {
    pub fn new(root: &'a Value) -> Self {
        Resolver { root }
    }

    /// The value at a local JSON pointer (`#/a/b~1c`).
    pub fn pointer(&self, r: &str) -> Result<&'a Value, RefError> {
        let Some(ptr) = r.strip_prefix('#') else { return Err(RefError::External(r.to_string())) };
        let ptr = percent_decode(ptr);
        if ptr.is_empty() {
            return Ok(self.root);
        }
        if !ptr.starts_with('/') {
            return Err(RefError::Missing(r.to_string()));
        }
        let mut at = self.root;
        for seg in ptr[1..].split('/') {
            let seg = seg.replace("~1", "/").replace("~0", "~");
            at = match at {
                Value::Object(m) => m.get(&seg),
                Value::Array(a) => seg.parse::<usize>().ok().and_then(|i| a.get(i)),
                _ => None,
            }
            .ok_or_else(|| RefError::Missing(r.to_string()))?;
        }
        Ok(at)
    }

    /// `v`, or what its `$ref` chain ends at.
    pub fn get(&self, v: &'a Value) -> Result<&'a Value, RefError> {
        let mut at = v;
        let mut seen: Vec<&str> = vec![];
        while let Some(r) = at.get("$ref").and_then(Value::as_str) {
            if seen.contains(&r) || seen.len() > 64 {
                return Err(RefError::Loop(r.to_string()));
            }
            seen.push(r);
            at = self.pointer(r)?;
        }
        Ok(at)
    }
}

fn percent_decode(s: &str) -> String {
    let bytes = s.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        let hex = |b: u8| (b as char).to_digit(16);
        if bytes[i] == b'%' && i + 2 < bytes.len() {
            if let (Some(h), Some(l)) = (hex(bytes[i + 1]), hex(bytes[i + 2])) {
                out.push((h * 16 + l) as u8);
                i += 3;
                continue;
            }
        }
        out.push(bytes[i]);
        i += 1;
    }
    String::from_utf8_lossy(&out).into_owned()
}

/// Refuses what isn't OpenAPI 3.x; returns the version.
pub fn check_version(spec: &Value) -> Result<String> {
    if let Some(v) = spec.get("swagger") {
        bail!("this is a Swagger {} spec; hitop imports OpenAPI 3.0 and 3.1 (convert it first, for example with swagger2openapi)", super::scalar(v));
    }
    let Some(v) = spec.get("openapi").map(super::scalar) else {
        bail!("not an OpenAPI spec (no `openapi` version at the top)");
    };
    if !v.starts_with("3.") {
        bail!("OpenAPI {v} isn't supported; hitop imports 3.0 and 3.1");
    }
    Ok(v)
}
