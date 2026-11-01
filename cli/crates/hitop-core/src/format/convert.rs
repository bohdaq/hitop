//! The JSON model ⇄ YAML documents, one request or environment at a time.
use anyhow::{anyhow, bail, Result};
use serde_json::Value;

use super::doc::{BodyDoc, EnvironmentDoc, RequestDoc, RowDoc, ScriptsDoc};
use super::names;
use crate::model::{Environment, Extra, FormField, Graphql, Header, Param, Request};
use crate::params::{append_query, parse_query, with_query};

/// Keys the JSON model or the YAML files give a meaning to. An unknown key
/// carried from one side to the other must not be one of them, or it
/// would be read back as something else.
const REQUEST_KEYS: &[&str] = &[
    "id", "name", "url", "method", "headers", "body", "preRequestScript", "postRequestScript", "auth", "bodyMode",
    "formFields", "disabledParams", "bodyType", "examples", "file", "params", "scripts", "graphql",
];
const ROW_KEYS: &[&str] = &["key", "name", "value", "type", "enabled"];
pub const COLLECTION_KEYS: &[&str] = &["id", "name", "workspace", "requests", "variables", "auth", "format", "order"];
const ENVIRONMENT_KEYS: &[&str] = &["id", "name", "variables", "secrets"];

pub fn check_extra(extra: &Extra, reserved: &[&str], what: &str) -> Result<()> {
    if let Some(k) = extra.keys().find(|k| reserved.contains(&k.as_str())) {
        bail!("{what}: the key '{k}' can't be carried over (it means something else on the other side)");
    }
    Ok(())
}

/// The body type hitop infers from the body and the Content-Type header.
pub fn infer_body_type(headers: &[Header], body: &str) -> &'static str {
    if body.is_empty() {
        return "none";
    }
    let ct = headers
        .iter()
        .find(|h| h.enabled && h.name.eq_ignore_ascii_case("content-type"))
        .map(|h| h.value.to_ascii_lowercase())
        .unwrap_or_default();
    if ct.contains("json") {
        return "json";
    }
    if ct.contains("xml") {
        return "xml";
    }
    if !ct.is_empty() {
        return "text";
    }
    let t = body.trim_start();
    if (t.starts_with('{') || t.starts_with('[')) && serde_json::from_str::<Value>(body).is_ok() {
        "json"
    } else if t.starts_with('<') {
        "xml"
    } else {
        "text"
    }
}

const RAW_TYPES: &[&str] = &["none", "json", "text", "xml", "graphql"];

fn field_to_row(f: &FormField) -> Result<RowDoc> {
    check_extra(&f.extra, ROW_KEYS, "form field")?;
    Ok(RowDoc {
        name: f.key.clone(),
        value: f.value.clone(),
        kind: if f.kind == "text" { None } else { Some(f.kind.clone()) },
        enabled: f.enabled,
        extra: f.extra.clone(),
    })
}

fn row_to_field(r: RowDoc) -> Result<FormField> {
    check_extra(&r.extra, ROW_KEYS, "form field")?;
    Ok(FormField { key: r.name, value: r.value, kind: r.kind.unwrap_or_else(|| "text".into()), enabled: r.enabled, extra: r.extra })
}

/// A request as the document for its file. `file` is where it goes (the
/// derived id is left out).
pub fn request_to_doc(r: &Request, file: &str) -> Result<RequestDoc> {
    check_extra(&r.extra, REQUEST_KEYS, &format!("request '{}'", r.name))?;

    // The query goes to `params` only when the URL can be rebuilt from it
    // exactly; otherwise the URL is kept as typed.
    let on = parse_query(&r.url);
    let stripped = with_query(&r.url, &[]);
    let split = !on.is_empty() && append_query(&stripped, &on) == r.url;
    let (url, mut params) = if split {
        (stripped, on.into_iter().map(|(k, v)| RowDoc { name: k, value: v, kind: None, enabled: true, extra: Extra::new() }).collect())
    } else {
        (r.url.clone(), vec![])
    };
    for p in &r.disabled_params {
        check_extra(&p.extra, ROW_KEYS, "query parameter")?;
        params.push(RowDoc { name: p.key.clone(), value: p.value.clone(), kind: None, enabled: false, extra: p.extra.clone() });
    }

    // Blank rows are the editor's empty placeholder: not written.
    let headers = r
        .headers
        .iter()
        .filter(|h| !(h.enabled && h.name.is_empty() && h.value.is_empty() && h.extra.is_empty()))
        .map(|h| {
            check_extra(&h.extra, ROW_KEYS, "header")?;
            Ok(RowDoc { name: h.name.clone(), value: h.value.clone(), kind: None, enabled: h.enabled, extra: h.extra.clone() })
        })
        .collect::<Result<_>>()?;

    if let Some(t) = &r.body_type {
        if !RAW_TYPES.contains(&t.as_str()) || r.body_mode.is_some() {
            bail!("request '{}': body type '{t}' needs a raw body ({})", r.name, RAW_TYPES.join(", "));
        }
    }
    if let Some(m) = &r.body_mode {
        if RAW_TYPES.contains(&m.as_str()) || m == "form" {
            bail!("request '{}': unsupported body mode '{m}'", r.name);
        }
    }
    let kind = match r.body_mode.as_deref() {
        None => r.body_type.clone().unwrap_or_else(|| infer_body_type(&r.headers, &r.body).to_string()),
        Some("urlencoded") => "form".into(),
        Some(other) => other.to_string(),
    };
    let fields = r.form_fields.iter().map(field_to_row).collect::<Result<Vec<_>>>()?;
    let graphql = r.graphql.clone().unwrap_or_default();
    if let Some(k) = graphql.extra.keys().next() {
        bail!("request '{}': graphql: the key '{k}' has no place in a collection folder", r.name);
    }
    let body = if kind == "none" && r.body.is_empty() && fields.is_empty() && graphql.is_empty() {
        None
    } else {
        Some(BodyDoc {
            kind,
            content: (!r.body.is_empty()).then(|| r.body.clone()),
            variables: (!graphql.variables.is_empty()).then_some(graphql.variables),
            operation_name: (!graphql.operation_name.is_empty()).then_some(graphql.operation_name),
            fields,
        })
    };

    let scripts = if r.pre_request_script.is_empty() && r.post_request_script.is_empty() {
        None
    } else {
        Some(ScriptsDoc { pre: r.pre_request_script.clone(), post: r.post_request_script.clone() })
    };

    Ok(RequestDoc {
        name: Some(r.name.clone()),
        method: r.method.clone(),
        url,
        params,
        headers,
        auth: r.auth.clone(),
        body,
        scripts,
        examples: r.examples.clone(),
        id: (r.id != names::request_id(file)).then_some(r.id),
        extra: r.extra.clone(),
    })
}

/// A request file's document as a request. `file` is its path under
/// `requests/` without the extension; the caller decides whether the
/// model keeps it.
pub fn doc_to_request(d: RequestDoc, file: &str) -> Result<Request> {
    let what = format!("requests/{file}");
    check_extra(&d.extra, REQUEST_KEYS, &what)?;
    let mut on = vec![];
    let mut disabled_params = vec![];
    for p in d.params {
        check_extra(&p.extra, ROW_KEYS, &what)?;
        if p.enabled {
            if let Some(k) = p.extra.keys().next() {
                bail!("{what}: query parameter '{}': extra keys like '{k}' are only kept on disabled parameters (enabled ones live in the URL)", p.name);
            }
            on.push((p.name, p.value));
        } else {
            disabled_params.push(Param { key: p.name, value: p.value, extra: p.extra });
        }
    }
    let url = if on.is_empty() { d.url } else { append_query(&d.url, &on) };
    let headers: Vec<Header> = d
        .headers
        .into_iter()
        .map(|h| {
            check_extra(&h.extra, ROW_KEYS, &what)?;
            Ok(Header { name: h.name, value: h.value, enabled: h.enabled, extra: h.extra })
        })
        .collect::<Result<_>>()?;

    let (mut body, mut body_mode, mut body_type, mut form_fields, mut graphql) = (String::new(), None, None, vec![], None);
    if let Some(b) = d.body {
        body = b.content.unwrap_or_default();
        let g = Graphql { variables: b.variables.unwrap_or_default(), operation_name: b.operation_name.unwrap_or_default(), extra: Extra::new() };
        graphql = (!g.is_empty()).then_some(g);
        form_fields = b.fields.into_iter().map(row_to_field).collect::<Result<_>>()?;
        if RAW_TYPES.contains(&b.kind.as_str()) {
            if b.kind != infer_body_type(&headers, &body) {
                body_type = Some(b.kind);
            }
        } else if b.kind == "form" {
            body_mode = Some("urlencoded".to_string());
        } else {
            body_mode = Some(b.kind);
        }
    }
    let scripts = d.scripts.unwrap_or_default();
    let name = d.name.unwrap_or_else(|| file.rsplit('/').next().unwrap_or(file).to_string());
    Ok(Request {
        id: d.id.unwrap_or_else(|| names::request_id(file)),
        name,
        url,
        method: d.method,
        headers,
        body,
        pre_request_script: scripts.pre,
        post_request_script: scripts.post,
        auth: d.auth,
        body_mode,
        form_fields,
        disabled_params,
        body_type,
        graphql,
        examples: d.examples,
        file: Some(file.to_string()),
        extra: d.extra,
    })
}

/// An environment as the document for `environments/<stem>.yaml`. Secret
/// variables are written as `{secret: true}`, never with their value.
pub fn environment_to_doc(e: &Environment, stem: &str) -> Result<EnvironmentDoc> {
    check_extra(&e.extra, ENVIRONMENT_KEYS, &format!("environment '{}'", e.name))?;
    let mut variables: std::collections::BTreeMap<String, Value> = e
        .variables
        .iter()
        .filter(|(k, _)| !e.secrets.contains(k))
        .map(|(k, v)| (k.clone(), Value::String(v.clone())))
        .collect();
    for s in &e.secrets {
        variables.insert(s.clone(), serde_json::json!({ "secret": true }));
    }
    Ok(EnvironmentDoc {
        name: (e.name != stem).then(|| e.name.clone()),
        id: (e.id != stem).then(|| e.id.clone()),
        variables,
        extra: e.extra.clone(),
    })
}

pub fn doc_to_environment(d: EnvironmentDoc, stem: &str) -> Result<Environment> {
    let what = format!("environments/{stem}");
    check_extra(&d.extra, ENVIRONMENT_KEYS, &what)?;
    let mut variables = std::collections::HashMap::new();
    let mut secrets = vec![];
    for (k, v) in d.variables {
        match v {
            Value::Object(o) => {
                if o.get("secret") == Some(&Value::Bool(true)) && o.len() == 1 {
                    secrets.push(k);
                } else {
                    bail!("{what}: variable '{k}' must be a value or {{secret: true}}");
                }
            }
            v => {
                let s = super::doc::scalar_string(v).map_err(|e| anyhow!("{what}: variable '{k}': {e}"))?;
                variables.insert(k, s);
            }
        }
    }
    Ok(Environment {
        id: d.id.unwrap_or_else(|| stem.to_string()),
        name: d.name.unwrap_or_else(|| stem.to_string()),
        variables,
        secrets,
        extra: d.extra,
    })
}
