//! An example value from a JSON Schema (OpenAPI 3.0 schema objects and 3.1's
//! JSON Schema 2020-12): `example`, `examples`, `const`, `default` and
//! `enum` win; otherwise a value of the right shape. Recursive schemas stop
//! where a `$ref` comes back into itself.
use serde_json::{Map, Value};

use super::refs::Resolver;

/// Which side the example is for: requests leave `readOnly` properties out,
/// responses `writeOnly` ones.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Side {
    Request,
    Response,
}

/// Deeper than this, nested objects are left out.
const MAX_DEPTH: usize = 12;
/// At most this many values in one example (big specs nest wide).
const MAX_NODES: usize = 4000;

pub fn example(r: &Resolver, schema: &Value, side: Side) -> Value {
    let mut g = Gen { r, side, stack: vec![], nodes: 0 };
    g.value(schema, 0).unwrap_or(Value::Null)
}

struct Gen<'a, 'b> {
    r: &'b Resolver<'a>,
    side: Side,
    stack: Vec<String>,
    nodes: usize,
}

impl<'a> Gen<'a, '_> {
    fn value(&mut self, schema: &'a Value, depth: usize) -> Option<Value> {
        if depth > MAX_DEPTH || self.nodes > MAX_NODES {
            return None;
        }
        self.nodes += 1;
        if let Some(r) = schema.get("$ref").and_then(Value::as_str) {
            if self.stack.iter().any(|s| s == r) {
                return None;
            }
            let target = self.r.pointer(r).ok()?;
            self.stack.push(r.to_string());
            // 3.1 allows keywords beside $ref: an own example still wins.
            let out = match own_example(schema) {
                Some(v) => Some(v),
                None => self.value(target, depth),
            };
            self.stack.pop();
            return out;
        }
        if let Some(v) = own_example(schema) {
            return Some(v);
        }
        if let Some(all) = schema.get("allOf").and_then(Value::as_array) {
            let mut merged: Option<Value> = None;
            for part in all {
                let Some(v) = self.value(part, depth) else { continue };
                merged = Some(match (merged, v) {
                    (Some(Value::Object(mut a)), Value::Object(b)) => {
                        a.extend(b);
                        Value::Object(a)
                    }
                    (None, v) => v,
                    (Some(a), _) => a,
                });
            }
            // Properties beside allOf join in.
            if let (Some(Value::Object(mut m)), Some(own)) = (merged.clone(), self.object(schema, depth).filter(|o| !o.is_empty())) {
                m.extend(own);
                return Some(Value::Object(m));
            }
            if merged.is_some() {
                return merged;
            }
        }
        for key in ["oneOf", "anyOf"] {
            if let Some(list) = schema.get(key).and_then(Value::as_array) {
                // The first choice that isn't null.
                for choice in list {
                    if is_null_type(self.r, choice) {
                        continue;
                    }
                    if let Some(v) = self.value(choice, depth) {
                        return Some(v);
                    }
                }
            }
        }
        match schema_type(schema).as_deref() {
            Some("object") => self.object(schema, depth).map(Value::Object),
            Some("array") => {
                let item = schema.get("items").or_else(|| schema.get("prefixItems").and_then(|p| p.get(0)));
                Some(Value::Array(item.and_then(|i| self.value(i, depth + 1)).into_iter().collect()))
            }
            Some("string") => Some(Value::String(string_for(schema))),
            Some("integer") => Some(number_for(schema, true)),
            Some("number") => Some(number_for(schema, false)),
            Some("boolean") => Some(Value::Bool(true)),
            Some("null") => Some(Value::Null),
            _ => None,
        }
    }

    fn object(&mut self, schema: &'a Value, depth: usize) -> Option<Map<String, Value>> {
        let props = schema.get("properties").and_then(Value::as_object);
        let mut out = Map::new();
        for (name, prop) in props.into_iter().flatten() {
            let resolved = self.r.get(prop).unwrap_or(prop);
            let skip = match self.side {
                Side::Request => "readOnly",
                Side::Response => "writeOnly",
            };
            if prop.get(skip) == Some(&Value::Bool(true)) || resolved.get(skip) == Some(&Value::Bool(true)) {
                continue;
            }
            if let Some(v) = self.value(prop, depth + 1) {
                out.insert(name.clone(), v);
            }
        }
        if props.is_none() {
            if let Some(extra) = schema.get("additionalProperties").filter(|a| a.is_object()) {
                if let Some(v) = self.value(extra, depth + 1) {
                    out.insert("key".into(), v);
                }
            }
        }
        Some(out)
    }
}

pub fn own_example(schema: &Value) -> Option<Value> {
    if let Some(v) = schema.get("example") {
        return Some(v.clone());
    }
    if let Some(v) = schema.get("examples").and_then(Value::as_array).and_then(|a| a.first()) {
        return Some(v.clone());
    }
    if let Some(v) = schema.get("const") {
        return Some(v.clone());
    }
    // An empty `default: {}` says less than the schema does (Kubernetes).
    if let Some(v) = schema.get("default").filter(|d| !matches!(d, Value::Object(m) if m.is_empty()) && !matches!(d, Value::Array(a) if a.is_empty())) {
        return Some(v.clone());
    }
    schema.get("enum").and_then(Value::as_array).and_then(|a| a.iter().find(|v| !v.is_null())).cloned()
}

/// The schema's type: 3.1's list form takes its first type other than null;
/// without one, what its keywords say.
pub fn schema_type(schema: &Value) -> Option<String> {
    match schema.get("type") {
        Some(Value::String(t)) => return Some(t.clone()),
        Some(Value::Array(list)) => {
            let names: Vec<&str> = list.iter().filter_map(Value::as_str).collect();
            return names.iter().find(|t| **t != "null").or(names.first()).map(|t| t.to_string());
        }
        _ => {}
    }
    if schema.get("properties").is_some() || schema.get("additionalProperties").is_some() {
        Some("object".into())
    } else if schema.get("items").is_some() || schema.get("prefixItems").is_some() {
        Some("array".into())
    } else {
        None
    }
}

fn is_null_type(r: &Resolver, schema: &Value) -> bool {
    let s = r.get(schema).unwrap_or(schema);
    s.get("type").and_then(Value::as_str) == Some("null")
}

fn string_for(schema: &Value) -> String {
    match schema.get("format").and_then(Value::as_str).unwrap_or("") {
        "date-time" => "2026-01-01T00:00:00Z",
        "date" => "2026-01-01",
        "time" => "00:00:00",
        "email" => "user@example.com",
        "uuid" => "00000000-0000-0000-0000-000000000000",
        "uri" | "url" | "iri" => "https://example.com",
        "hostname" => "example.com",
        "ipv4" => "192.0.2.1",
        "ipv6" => "2001:db8::1",
        "byte" | "binary" | "password" => "",
        _ => "string",
    }
    .to_string()
}

fn number_for(schema: &Value, integer: bool) -> Value {
    let min = schema.get("minimum").and_then(Value::as_f64).or_else(|| schema.get("exclusiveMinimum").and_then(Value::as_f64).map(|m| m + 1.0));
    let n = min.unwrap_or(0.0);
    if integer || n.fract() == 0.0 {
        Value::from(n as i64)
    } else {
        serde_json::Number::from_f64(n).map(Value::Number).unwrap_or(Value::from(0))
    }
}
