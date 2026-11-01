//! Form bodies: `application/x-www-form-urlencoded` and `multipart/form-data`,
//! built the way the web app builds them (`webapp/src/features.js`
//! `urlEncodedBody`, `multipartBody`).
use std::collections::HashMap;

use crate::interpolation::interpolate;
use crate::model::{FormField, Request};

/// One part of a multipart body.
#[derive(Debug, Clone, PartialEq)]
pub enum Part {
    Text { name: String, value: String },
    File { name: String, filename: String, content_type: String, data: Vec<u8> },
}

/// A body and its `Content-Type`.
#[derive(Debug, Clone, PartialEq)]
pub struct Built {
    pub body: Vec<u8>,
    pub content_type: String,
}

/// JavaScript's `encodeURIComponent`.
pub fn encode_component(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for b in s.bytes() {
        if b.is_ascii_alphanumeric() || b"-_.!~*'()".contains(&b) {
            out.push(b as char);
        } else {
            out.push_str(&format!("%{b:02X}"));
        }
    }
    out
}

/// `key=value&...`, both sides percent-encoded.
pub fn urlencoded(pairs: &[(String, String)]) -> Built {
    let body = pairs.iter().map(|(k, v)| format!("{}={}", encode_component(k), encode_component(v))).collect::<Vec<_>>().join("&");
    Built { body: body.into_bytes(), content_type: "application/x-www-form-urlencoded".into() }
}

fn quote(s: &str) -> String {
    s.replace('"', "%22").replace(['\r', '\n'], " ")
}

fn contains(haystack: &[u8], needle: &[u8]) -> bool {
    haystack.windows(needle.len()).any(|w| w == needle)
}

/// A multipart body with a random boundary that no part contains.
pub fn multipart(parts: &[Part]) -> Built {
    let mut boundary;
    loop {
        boundary = format!("----hitop{}", crate::secrets::new_workspace_id());
        let clash = parts.iter().any(|p| match p {
            Part::Text { value, .. } => value.contains(&boundary),
            Part::File { data, .. } => contains(data, boundary.as_bytes()),
        });
        if !clash {
            break;
        }
    }
    let mut body = Vec::new();
    for p in parts {
        match p {
            Part::Text { name, value } => {
                body.extend_from_slice(format!("--{boundary}\r\nContent-Disposition: form-data; name=\"{}\"\r\n\r\n{value}\r\n", quote(name)).as_bytes());
            }
            Part::File { name, filename, content_type, data } => {
                body.extend_from_slice(
                    format!("--{boundary}\r\nContent-Disposition: form-data; name=\"{}\"; filename=\"{}\"\r\nContent-Type: {content_type}\r\n\r\n", quote(name), quote(filename)).as_bytes(),
                );
                body.extend_from_slice(data);
                body.extend_from_slice(b"\r\n");
            }
        }
    }
    body.extend_from_slice(format!("--{boundary}--\r\n").as_bytes());
    Built { body, content_type: format!("multipart/form-data; boundary={boundary}") }
}

/// The form body of a saved request (`bodyMode` `urlencoded` or
/// `multipart`), with variables filled in; `None` for a text body. File rows
/// carry no file in a collection (the web app keeps files only while the page
/// is open), so they go out as empty parts named after `fileName`, as the web
/// app sends them after a reload.
pub fn request_body(request: &Request, variables: &HashMap<String, String>) -> anyhow::Result<Option<Built>> {
    let mode = match request.body_mode.as_deref() {
        Some(m @ ("urlencoded" | "multipart")) => m,
        _ => return Ok(None),
    };
    let rows: Vec<&FormField> = request.form_fields.iter().filter(|f| f.enabled && !f.key.is_empty()).collect();
    if mode == "urlencoded" {
        let pairs = rows.iter().map(|f| Ok((interpolate(&f.key, variables)?, interpolate(&f.value, variables)?))).collect::<anyhow::Result<Vec<_>>>()?;
        return Ok(Some(urlencoded(&pairs)));
    }
    let parts = rows
        .iter()
        .map(|f| {
            let name = interpolate(&f.key, variables)?;
            Ok(if f.kind == "file" {
                let filename = f.extra.get("fileName").and_then(|v| v.as_str()).unwrap_or_default().to_string();
                Part::File { name, filename, content_type: "application/octet-stream".into(), data: vec![] }
            } else {
                Part::Text { name, value: interpolate(&f.value, variables)? }
            })
        })
        .collect::<anyhow::Result<Vec<_>>>()?;
    Ok(Some(multipart(&parts)))
}

/// Headers and body as they go out.
pub type Outgoing = (Vec<(String, String)>, Vec<u8>);

/// The headers and body a saved request goes out with after its pre-request
/// script: a form body (with its `Content-Type`, unless a header sets one)
/// when the request has one and the script didn't replace the body, else the
/// prepared body.
pub fn outgoing(request: &Request, headers: &[(String, String)], body: &str, variables: &HashMap<String, String>) -> anyhow::Result<Outgoing> {
    let mut headers = headers.to_vec();
    let unchanged = interpolate(&request.body, variables).is_ok_and(|text| text == body);
    match request_body(request, variables)? {
        Some(built) if unchanged => {
            if !headers.iter().any(|(k, v)| k.eq_ignore_ascii_case("content-type") && !v.is_empty()) {
                headers.push(("Content-Type".into(), built.content_type));
            }
            Ok((headers, built.body))
        }
        _ => Ok((headers, body.as_bytes().to_vec())),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn urlencoded_matches_encode_uri_component() {
        let b = urlencoded(&[("a b".into(), "x&y=z/é".into()), ("k".into(), "(ok)!*'~".into())]);
        assert_eq!(String::from_utf8(b.body).unwrap(), "a%20b=x%26y%3Dz%2F%C3%A9&k=(ok)!*'~");
        assert_eq!(b.content_type, "application/x-www-form-urlencoded");
    }

    #[test]
    fn multipart_layout() {
        let b = multipart(&[
            Part::Text { name: "note".into(), value: "hi".into() },
            Part::File { name: "fi\"le".into(), filename: "a.bin".into(), content_type: "image/png".into(), data: vec![0, 255] },
        ]);
        let boundary = b.content_type.strip_prefix("multipart/form-data; boundary=").unwrap().to_string();
        let mut want = format!("--{boundary}\r\nContent-Disposition: form-data; name=\"note\"\r\n\r\nhi\r\n--{boundary}\r\nContent-Disposition: form-data; name=\"fi%22le\"; filename=\"a.bin\"\r\nContent-Type: image/png\r\n\r\n").into_bytes();
        want.extend_from_slice(&[0, 255]);
        want.extend_from_slice(format!("\r\n--{boundary}--\r\n").as_bytes());
        assert_eq!(b.body, want);
    }

    #[test]
    fn saved_form_bodies() {
        let vars: HashMap<String, String> = [("n".to_string(), "Ada".to_string())].into();
        let field = |key: &str, value: &str, kind: &str, enabled: bool| FormField { key: key.into(), value: value.into(), kind: kind.into(), enabled, ..Default::default() };
        let mut r = Request { body_mode: Some("urlencoded".into()), form_fields: vec![field("name", "${n}", "text", true), field("off", "x", "text", false), field("", "x", "text", true)], ..Default::default() };
        assert_eq!(request_body(&r, &vars).unwrap().unwrap().body, b"name=Ada");
        r.body_mode = Some("multipart".into());
        let text = String::from_utf8(request_body(&r, &vars).unwrap().unwrap().body).unwrap();
        assert!(text.contains("name=\"name\"\r\n\r\nAda\r\n") && !text.contains("off"), "{text}");
        r.body_mode = None;
        assert!(request_body(&r, &vars).unwrap().is_none());
    }
}
