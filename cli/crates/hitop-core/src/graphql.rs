//! GraphQL bodies, sent the GraphQL-over-HTTP way: a JSON POST of
//! `{"query", "variables", "operationName"}`. A request is a GraphQL request
//! when its `bodyType` is `graphql` (and it has no form body); `body` is the
//! query and `graphql` the variables (JSON text) and operation name. No
//! native dependencies: hitop-wasm builds this too.
use anyhow::{anyhow, Result};
use serde_json::Value;

use crate::model::{Graphql, Request};

/// The `bodyType` of a GraphQL request.
pub const BODY_TYPE: &str = "graphql";

/// The `Content-Type` a GraphQL request goes out with, unless a header sets one.
pub const CONTENT_TYPE: &str = "application/json";

/// The query, variables and operation name of a GraphQL request, as saved
/// (`${variables}` not yet filled in).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Operation<'a> {
    pub query: &'a str,
    /// JSON text of an object; empty means no variables.
    pub variables: &'a str,
    /// Empty means none.
    pub operation_name: &'a str,
}

impl Request {
    /// True when this request is sent as GraphQL.
    pub fn is_graphql(&self) -> bool {
        self.body_type.as_deref() == Some(BODY_TYPE) && self.body_mode.is_none()
    }

    /// The GraphQL operation this request sends, or None for any other body.
    pub fn graphql_operation(&self) -> Option<Operation<'_>> {
        self.is_graphql().then(|| {
            let g = self.graphql.as_ref();
            Operation {
                query: &self.body,
                variables: g.map(|g| g.variables.as_str()).unwrap_or_default(),
                operation_name: g.map(|g| g.operation_name.as_str()).unwrap_or_default(),
            }
        })
    }
}

impl Graphql {
    /// True when there is nothing to keep (`graphql` is then left out).
    pub fn is_empty(&self) -> bool {
        self.variables.is_empty() && self.operation_name.is_empty() && self.extra.is_empty()
    }
}

/// The variables text as JSON: empty or blank text is no variables (None);
/// otherwise it must be a JSON object or `null`.
pub fn parse_variables(text: &str) -> Result<Option<Value>> {
    if text.trim().is_empty() {
        return Ok(None);
    }
    let v: Value = serde_json::from_str(text).map_err(|e| anyhow!("GraphQL variables are not valid JSON: {e}"))?;
    match v {
        Value::Object(_) => Ok(Some(v)),
        Value::Null => Ok(None),
        other => Err(anyhow!("GraphQL variables must be a JSON object, got {}", kind(&other))),
    }
}

fn kind(v: &Value) -> &'static str {
    match v {
        Value::Null => "null",
        Value::Bool(_) => "a boolean",
        Value::Number(_) => "a number",
        Value::String(_) => "a string",
        Value::Array(_) => "an array",
        Value::Object(_) => "an object",
    }
}

/// The JSON body for an operation whose `${variables}` are already filled
/// in: `{"query":...,"variables":...,"operationName":...}`, in that order,
/// with `variables` and `operationName` left out when empty. The variables
/// go in as typed (checked to be a JSON object), so their key order stays.
pub fn request_body(query: &str, variables: &str, operation_name: &str) -> Result<String> {
    let mut out = format!("{{\"query\":{}", Value::String(query.to_string()));
    if parse_variables(variables)?.is_some() {
        out.push_str(",\"variables\":");
        out.push_str(variables.trim());
    }
    if !operation_name.trim().is_empty() {
        out.push_str(&format!(",\"operationName\":{}", Value::String(operation_name.trim().to_string())));
    }
    out.push('}');
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn body_shapes() {
        assert_eq!(request_body("{ me { id } }", "", "").unwrap(), r#"{"query":"{ me { id } }"}"#);
        assert_eq!(request_body("query Q($id: ID!) { u(id: $id) { n } }", " {\"z\": \"7\", \"a\": 1} ", " Q ").unwrap(), r#"{"query":"query Q($id: ID!) { u(id: $id) { n } }","variables":{"z": "7", "a": 1},"operationName":"Q"}"#);
        assert_eq!(request_body("q", "null", "").unwrap(), r#"{"query":"q"}"#);
        // The query is JSON-encoded: quotes and new lines survive.
        let b: Value = serde_json::from_str(&request_body("{ a(s: \"x\\\"y\")\n}", "", "").unwrap()).unwrap();
        assert_eq!(b["query"], "{ a(s: \"x\\\"y\")\n}");
    }

    #[test]
    fn bad_variables() {
        assert!(request_body("q", "{id: 1}", "").unwrap_err().to_string().starts_with("GraphQL variables are not valid JSON"));
        assert_eq!(request_body("q", "[1]", "").unwrap_err().to_string(), "GraphQL variables must be a JSON object, got an array");
    }

    #[test]
    fn which_requests_are_graphql() {
        let mut r = Request { body: "{ me }".into(), body_type: Some("graphql".into()), ..Default::default() };
        assert_eq!(r.graphql_operation(), Some(Operation { query: "{ me }", variables: "", operation_name: "" }));
        r.graphql = Some(Graphql { variables: "{}".into(), operation_name: "Me".into(), ..Default::default() });
        assert_eq!(r.graphql_operation().unwrap().operation_name, "Me");
        r.body_mode = Some("multipart".into());
        assert!(r.graphql_operation().is_none());
        r.body_mode = None;
        r.body_type = Some("json".into());
        assert!(!r.is_graphql());
    }
}
