//! Query parameters as rows, without decoding or validating anything, so
//! `${variables}` and half-typed URLs survive a round trip unchanged. Same
//! rules as the web app's `parseQuery` / `withQuery`.

/// The URL's query as `(key, value)` pairs, in order.
pub fn parse_query(url: &str) -> Vec<(String, String)> {
    let Some(q) = url.find('?') else { return vec![] };
    let mut query = &url[q + 1..];
    if let Some(hash) = query.find('#') {
        query = &query[..hash];
    }
    if query.is_empty() {
        return vec![];
    }
    query
        .split('&')
        .map(|part| match part.find('=') {
            Some(eq) => (part[..eq].to_string(), part[eq + 1..].to_string()),
            None => (part.to_string(), String::new()),
        })
        .collect()
}

fn encode_special(s: &str, chars: &[char]) -> String {
    let mut out = String::with_capacity(s.len());
    for c in s.chars() {
        if c.is_whitespace() || chars.contains(&c) {
            for b in c.to_string().bytes() {
                out.push_str(&format!("%{b:02X}"));
            }
        } else {
            out.push(c);
        }
    }
    out
}

/// The URL with its query replaced by `rows`; the fragment is kept. Spaces
/// and characters that would end a key or value are encoded, the rest is
/// left as typed.
pub fn with_query(url: &str, rows: &[(String, String)]) -> String {
    let (no_hash, hash) = match url.find('#') {
        Some(i) => (&url[..i], &url[i..]),
        None => (url, ""),
    };
    let base = no_hash.split('?').next().unwrap_or("");
    let query: Vec<String> = rows
        .iter()
        .filter(|(k, v)| !k.is_empty() || !v.is_empty())
        .map(|(k, v)| {
            let key = encode_special(k, &['&', '#', '=']);
            if v.is_empty() { key } else { format!("{key}={}", encode_special(v, &['&', '#'])) }
        })
        .collect();
    if query.is_empty() {
        format!("{base}{hash}")
    } else {
        format!("{base}?{}{hash}", query.join("&"))
    }
}

/// Appends `rows` to the URL's existing query.
pub fn append_query(url: &str, rows: &[(String, String)]) -> String {
    let mut all = parse_query(url);
    all.extend(rows.iter().cloned());
    with_query(url, &all)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn rows(pairs: &[(&str, &str)]) -> Vec<(String, String)> {
        pairs.iter().map(|(k, v)| (k.to_string(), v.to_string())).collect()
    }

    #[test]
    fn parses_and_rebuilds_without_touching_variables() {
        let url = "${base}/posts?userId=${id}&flag&q=a%20b#top";
        let parsed = parse_query(url);
        assert_eq!(parsed, rows(&[("userId", "${id}"), ("flag", ""), ("q", "a%20b")]));
        assert_eq!(with_query(url, &parsed), url);
    }

    #[test]
    fn encodes_only_what_would_break_the_query() {
        let out = with_query("https://x.dev/s", &rows(&[("q", "a b&c"), ("k=1", "v")]));
        assert_eq!(out, "https://x.dev/s?q=a%20b%26c&k%3D1=v");
    }

    #[test]
    fn empty_rows_remove_the_query() {
        assert_eq!(with_query("https://x.dev/s?a=1#f", &[]), "https://x.dev/s#f");
        assert!(parse_query("https://x.dev/s").is_empty());
    }

    #[test]
    fn append_keeps_existing_parameters() {
        assert_eq!(append_query("https://x.dev/?a=1", &rows(&[("b", "2")])), "https://x.dev/?a=1&b=2");
    }
}
