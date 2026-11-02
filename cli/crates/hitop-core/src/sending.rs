//! What every surface does to a request on its way out, beyond the
//! collection: a Content-Type for a raw body that has none, and a scheme for
//! a URL typed without one. Pure (no native deps): the CLI, MCP and snippets
//! use it here, the web app and desktop through hitop-wasm, and HitopKit
//! follows the same rules. Test vectors: `spec/vectors/send.json`.
//!
//! The rules were the web app's first (1.1.0); they are written to match its
//! JavaScript exactly, so whitespace and character classes are JavaScript's,
//! not Rust's Unicode ones.

/// The methods a raw body gets a Content-Type with (the web app sends a body
/// only with these).
pub const BODY_METHODS: [&str; 3] = ["POST", "PUT", "PATCH"];

/// JavaScript's `String.prototype.trim` whitespace: WhiteSpace and
/// LineTerminator, so U+FEFF is in and U+0085 is out (Rust's
/// `char::is_whitespace` has it the other way round).
pub fn is_js_whitespace(c: char) -> bool {
    matches!(
        c,
        '\u{9}'..='\u{d}' | ' ' | '\u{a0}' | '\u{1680}' | '\u{2000}'..='\u{200a}' | '\u{2028}' | '\u{2029}' | '\u{202f}' | '\u{205f}' | '\u{3000}' | '\u{feff}'
    )
}

/// The Content-Type a raw body gets when the request sets none, as Postman,
/// Insomnia and Bruno do: JSON, XML or HTML by its first character, else
/// plain text. An empty or blank body gets none.
pub fn raw_content_type(body: &str) -> Option<&'static str> {
    let text = body.trim_start_matches(is_js_whitespace);
    let first = text.chars().next()?;
    if first == '{' || first == '[' {
        return Some("application/json");
    }
    let starts = |p: &str| text.len() >= p.len() && text.as_bytes()[..p.len()].eq_ignore_ascii_case(p.as_bytes());
    if starts("<!doctype html") || starts("<html") {
        return Some("text/html");
    }
    if first == '<' {
        return Some("application/xml");
    }
    Some("text/plain")
}

/// Adds the raw body's Content-Type to `headers` (the request as sent, after
/// its pre-request script) when the method sends a body and no Content-Type
/// header has a value. Never written back to the collection.
pub fn add_content_type(method: &str, headers: &mut Vec<(String, String)>, body: &str) {
    if !BODY_METHODS.iter().any(|m| m.eq_ignore_ascii_case(method.trim())) {
        return;
    }
    if headers.iter().any(|(k, v)| k.eq_ignore_ascii_case("content-type") && !v.is_empty()) {
        return;
    }
    if let Some(t) = raw_content_type(body) {
        headers.push(("Content-Type".into(), t.into()));
    }
}

fn is_word(b: u8) -> bool {
    b.is_ascii_alphanumeric() || b == b'_' || b == b'-'
}

/// `[\w-]+(\.[\w-]+)*` (ASCII) at the start of `s`: the length matched.
fn host_len(s: &[u8]) -> usize {
    let mut i = 0;
    loop {
        let start = i;
        while i < s.len() && is_word(s[i]) {
            i += 1;
        }
        if i == start {
            // An empty label: back off the dot that led to it.
            return start.saturating_sub(1);
        }
        if i < s.len() && s[i] == b'.' {
            i += 1;
        } else {
            return i;
        }
    }
}

fn digits(s: &[u8]) -> usize {
    s.iter().take_while(|b| b.is_ascii_digit()).count()
}

/// `^[a-z][a-z0-9+.-]*:` (any case) and not `host:port`.
fn has_scheme(s: &[u8]) -> bool {
    if !s.first().is_some_and(u8::is_ascii_alphabetic) {
        return false;
    }
    let n = 1 + s[1..].iter().take_while(|b| b.is_ascii_alphanumeric() || matches!(b, b'+' | b'.' | b'-')).count();
    if s.get(n) != Some(&b':') {
        return false;
    }
    // `host:8080` is a host and a port, not a scheme.
    let h = host_len(s);
    !(h > 0 && s.get(h) == Some(&b':') && s.get(h + 1).is_some_and(u8::is_ascii_digit))
}

/// `localhost`, `127.d.d.d`, `0.0.0.0` or `[::1]`, then `(:\d+)?` and
/// `[/?#]` or the end.
fn is_local(s: &[u8]) -> bool {
    let host = if s.len() >= 9 && s[..9].eq_ignore_ascii_case(b"localhost") {
        9
    } else if s.starts_with(b"0.0.0.0") {
        7
    } else if s.starts_with(b"[::1]") {
        5
    } else if s.starts_with(b"127.") {
        let mut i = 4;
        let mut ok = true;
        for part in 0..3 {
            let d = digits(&s[i..]);
            if d == 0 {
                ok = false;
                break;
            }
            i += d;
            if part < 2 {
                if s.get(i) != Some(&b'.') {
                    ok = false;
                    break;
                }
                i += 1;
            }
        }
        if !ok {
            return false;
        }
        i
    } else {
        return false;
    };
    let mut i = host;
    if s.get(i) == Some(&b':') && digits(&s[i + 1..]) > 0 {
        i += 1 + digits(&s[i + 1..]);
    }
    // `(:\d+)?` may also match nothing: then the ':' itself must be the end test.
    let end = |i: usize| i == s.len() || matches!(s[i], b'/' | b'?' | b'#');
    end(i) || end(host)
}

/// `^[\w-]+(\.[\w-]+)*(:\d+)?([/?#]\S*)?$` and a dot or a port.
fn is_host_like(text: &str) -> bool {
    let s = text.as_bytes();
    let h = host_len(s);
    if h == 0 {
        return false;
    }
    let mut i = h;
    let port = s.get(i) == Some(&b':') && digits(&s[i + 1..]) > 0;
    if port {
        i += 1 + digits(&s[i + 1..]);
    }
    let rest = &text[i..];
    let shape = rest.is_empty() || (rest.starts_with(['/', '?', '#']) && !rest.chars().any(is_js_whitespace));
    // `[\w-]+\.[\w-]` or `[\w-]+:\d`.
    let first = s.iter().take_while(|b| is_word(**b)).count();
    let dotted = s.get(first) == Some(&b'.') && s.get(first + 1).is_some_and(|b| is_word(*b));
    let ported = s.get(first) == Some(&b':') && s.get(first + 1).is_some_and(u8::is_ascii_digit);
    shape && (dotted || ported)
}

/// A URL as typed, with a scheme: `api.example.com/users` → `https://...`,
/// `localhost:3000` → `http://...` (as curl does for local hosts). Anything
/// else that isn't a URL is an error, so nothing is sent relative to
/// something else.
pub fn with_scheme(url: &str) -> anyhow::Result<String> {
    let text = url.trim_matches(is_js_whitespace);
    let s = text.as_bytes();
    if has_scheme(s) {
        return Ok(text.to_string());
    }
    if is_local(s) {
        return Ok(format!("http://{text}"));
    }
    if is_host_like(text) {
        return Ok(format!("https://{text}"));
    }
    anyhow::bail!("\"{text}\" isn't a URL. Enter a full URL, such as https://api.example.com/users")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn content_type_by_first_character() {
        assert_eq!(raw_content_type(" {\"a\": 1}"), Some("application/json"));
        assert_eq!(raw_content_type("<!DOCTYPE html>"), Some("text/html"));
        assert_eq!(raw_content_type("<a/>"), Some("application/xml"));
        assert_eq!(raw_content_type("hi"), Some("text/plain"));
        assert_eq!(raw_content_type("\u{feff}"), None);
        // A multi-byte first character is not cut in half.
        assert_eq!(raw_content_type("é<html"), Some("text/plain"));
    }

    #[test]
    fn scheme_rules() {
        assert_eq!(with_scheme("api.example.com/x").unwrap(), "https://api.example.com/x");
        assert_eq!(with_scheme("localhost:3000").unwrap(), "http://localhost:3000");
        assert_eq!(with_scheme("a..b").unwrap_err().to_string(), "\"a..b\" isn't a URL. Enter a full URL, such as https://api.example.com/users");
        // As the web app's regular expressions had it: a scheme, and a host with a port.
        assert_eq!(with_scheme("localhost:").unwrap(), "localhost:");
        assert_eq!(with_scheme("127.0.0:1").unwrap(), "https://127.0.0:1");
    }
}
