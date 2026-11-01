#![cfg(feature = "native")]
//! The cookie jar: cookies that responses set, sent back with later requests
//! to the same site. Same rules and the same JSON shape as the web app's jar
//! (`webapp/src/features.js` `parseSetCookie`, `mergeCookies`,
//! `cookieHeader`), so the desktop shows and edits it on the Cookies page.
//!
//! A jar is kept per workspace (the keychain's workspace id: `global` for
//! global storage, a folder's `workspace`) in
//! `<config>/hitop/cookies/<workspace>.json`.
//!
//! Only the final response's `Set-Cookie` lines are stored: cookies set on a
//! redirect that reqwest follows are not seen (the web app's proxy works the
//! same way).
use std::fs;
use std::path::PathBuf;
use std::time::{SystemTime, UNIX_EPOCH};

use anyhow::{Context, Result};
use reqwest::Url;
use serde::{Deserialize, Serialize};

use crate::model::Extra;

/// One cookie, as the web app stores it.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Cookie {
    pub name: String,
    pub value: String,
    /// Lower case, without a leading dot.
    pub domain: String,
    /// Set without a `Domain` attribute: sent to that host only.
    #[serde(default)]
    pub host_only: bool,
    #[serde(default = "root")]
    pub path: String,
    /// Milliseconds since 1970; `None` for a session cookie (kept until cleared).
    #[serde(default)]
    pub expires: Option<i64>,
    #[serde(default)]
    pub secure: bool,
    #[serde(flatten)]
    pub extra: Extra,
}

fn root() -> String {
    "/".into()
}

/// Milliseconds since 1970.
pub fn now_ms() -> i64 {
    SystemTime::now().duration_since(UNIX_EPOCH).map(|d| d.as_millis() as i64).unwrap_or(0)
}

/// A list of cookies; serialized as a plain JSON array.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(transparent)]
pub struct Jar {
    pub cookies: Vec<Cookie>,
}

fn live(c: &Cookie, now: i64) -> bool {
    c.expires.is_none_or(|e| e > now)
}

/// One `Set-Cookie` line as a jar entry, or `None` when it can't be stored for
/// this URL (another site's domain, no `name=`). The flag is true when the
/// line deletes the cookie (an expiry in the past).
pub fn parse_set_cookie(line: &str, url: &Url, now: i64) -> Option<(Cookie, bool)> {
    let host = url.host_str()?.to_ascii_lowercase();
    let mut parts = line.split(';').map(str::trim);
    let pair = parts.next()?;
    let eq = pair.find('=')?;
    if eq == 0 {
        return None;
    }
    let req_path = url.path();
    let path = match req_path.rfind('/') {
        Some(i) if i > 0 => req_path[..i].to_string(),
        _ => "/".into(),
    };
    let mut c = Cookie { name: pair[..eq].trim().into(), value: pair[eq + 1..].trim().into(), domain: host.clone(), host_only: true, path, expires: None, secure: false, extra: Extra::new() };
    let mut max_age: Option<i64> = None;
    for attr in parts {
        let (key, val) = match attr.find('=') {
            Some(at) => (attr[..at].trim().to_ascii_lowercase(), attr[at + 1..].trim()),
            None => (attr.trim().to_ascii_lowercase(), ""),
        };
        match key.as_str() {
            "domain" if !val.is_empty() => {
                let d = val.trim_start_matches('.').to_ascii_lowercase();
                if host != d && !host.ends_with(&format!(".{d}")) {
                    return None; // another site's cookie
                }
                c.domain = d;
                c.host_only = false;
            }
            "path" if val.starts_with('/') => c.path = val.into(),
            "expires" => {
                if let Some(t) = parse_http_date(val) {
                    c.expires = Some(t);
                }
            }
            "max-age" => {
                if let Ok(n) = val.parse::<i64>() {
                    max_age = Some(n);
                }
            }
            "secure" => c.secure = true,
            _ => {}
        }
    }
    if let Some(n) = max_age {
        c.expires = Some(now.saturating_add(n.saturating_mul(1000)));
    }
    let deleted = c.expires.is_some_and(|e| e <= now);
    Some((c, deleted))
}

impl Jar {
    /// Applies the `Set-Cookie` lines of one response to `url` (the final URL).
    /// Expired cookies are dropped.
    pub fn merge<'a>(&mut self, lines: impl IntoIterator<Item = &'a str>, url: &Url, now: i64) {
        self.cookies.retain(|c| live(c, now));
        for line in lines {
            let Some((cookie, deleted)) = parse_set_cookie(line, url, now) else { continue };
            self.cookies.retain(|c| !(c.name == cookie.name && c.domain == cookie.domain && c.path == cookie.path));
            if !deleted {
                self.cookies.push(cookie);
            }
        }
    }

    /// The `Cookie` header value for `url` (empty when no cookie applies).
    pub fn header(&self, url: &Url, now: i64) -> String {
        let host = url.host_str().unwrap_or_default().to_ascii_lowercase();
        let path = if url.path().is_empty() { "/" } else { url.path() };
        let mut matching: Vec<&Cookie> = self
            .cookies
            .iter()
            .filter(|c| live(c, now))
            .filter(|c| if c.host_only { host == c.domain } else { host == c.domain || host.ends_with(&format!(".{}", c.domain)) })
            .filter(|c| {
                let prefix = if c.path.ends_with('/') { c.path.clone() } else { format!("{}/", c.path) };
                path == c.path || path.starts_with(&prefix) || c.path == "/"
            })
            .filter(|c| !c.secure || url.scheme() == "https")
            .collect();
        // Longest path first; stable, so equal paths keep the order they were set in.
        matching.sort_by_key(|c| std::cmp::Reverse(c.path.len()));
        matching.iter().map(|c| format!("{}={}", c.name, c.value)).collect::<Vec<_>>().join("; ")
    }

    /// The jar of `workspace`, empty when none is stored.
    pub fn load(workspace: &str) -> Result<Jar> {
        let path = jar_path(workspace)?;
        if !path.exists() {
            return Ok(Jar::default());
        }
        let text = fs::read_to_string(&path).with_context(|| format!("reading {}", path.display()))?;
        serde_json::from_str(&text).with_context(|| format!("reading {}", path.display()))
    }

    /// Writes the jar of `workspace`; an empty jar removes the file.
    pub fn save(&self, workspace: &str) -> Result<()> {
        let path = jar_path(workspace)?;
        if self.cookies.is_empty() {
            if path.exists() {
                fs::remove_file(&path)?;
            }
            return Ok(());
        }
        fs::create_dir_all(path.parent().expect("jar path has a parent"))?;
        fs::write(&path, serde_json::to_string_pretty(self)?)?;
        Ok(())
    }
}

/// `<config>/hitop/cookies/<workspace>.json`; characters other than letters,
/// digits, `-` and `_` in the workspace id become `_`.
pub fn jar_path(workspace: &str) -> Result<PathBuf> {
    let safe: String = workspace.chars().map(|c| if c.is_ascii_alphanumeric() || c == '-' || c == '_' { c } else { '_' }).collect();
    let base = dirs::config_dir().context("cannot determine config directory")?;
    Ok(base.join("hitop").join("cookies").join(format!("{safe}.json")))
}

/// An HTTP date (`Wed, 21 Oct 2015 07:28:00 GMT`, also with dashes as in
/// `21-Oct-2015` or a two-digit year) in milliseconds since 1970.
pub fn parse_http_date(s: &str) -> Option<i64> {
    let s = s.split_once(',').map(|(_, rest)| rest).unwrap_or(s);
    let tokens: Vec<&str> = s.split([' ', '-']).filter(|t| !t.is_empty()).collect();
    let (mut day, mut month, mut year, mut time) = (None, None, None, None);
    const MONTHS: [&str; 12] = ["jan", "feb", "mar", "apr", "may", "jun", "jul", "aug", "sep", "oct", "nov", "dec"];
    for t in tokens {
        let lower = t.to_ascii_lowercase();
        if t.contains(':') {
            let hms: Vec<i64> = t.split(':').map(|p| p.parse().ok()).collect::<Option<_>>()?;
            if hms.len() != 3 {
                return None;
            }
            time = Some((hms[0], hms[1], hms[2]));
        } else if let Some(m) = MONTHS.iter().position(|m| lower.starts_with(m)) {
            month = Some(m as i64 + 1);
        } else if let Ok(n) = t.parse::<i64>() {
            if day.is_none() && t.len() <= 2 && year.is_none() {
                day = Some(n);
            } else {
                year = Some(if n < 70 { 2000 + n } else if n < 100 { 1900 + n } else { n });
            }
        }
    }
    let (d, m, y, (hh, mm, ss)) = (day?, month?, year?, time?);
    if !(1..=31).contains(&d) || hh > 23 || mm > 59 || ss > 60 {
        return None;
    }
    let days = days_from_civil(y, m, d);
    Some(((days * 86_400) + hh * 3600 + mm * 60 + ss) * 1000)
}

/// Days since 1970-01-01 (Howard Hinnant's days_from_civil).
fn days_from_civil(y: i64, m: i64, d: i64) -> i64 {
    let y = if m <= 2 { y - 1 } else { y };
    let era = y.div_euclid(400);
    let yoe = y - era * 400;
    let mp = (m + 9) % 12;
    let doy = (153 * mp + 2) / 5 + d - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    era * 146_097 + doe - 719_468
}

#[cfg(test)]
mod tests {
    use super::*;

    fn url(s: &str) -> Url {
        Url::parse(s).unwrap()
    }

    // The same cases as webapp/tests/unit.mjs, so both jars agree.
    #[test]
    fn same_rules_as_the_web_app() {
        let now = parse_http_date("Sat, 31 Oct 2026 12:00:00 GMT").unwrap();
        let mut jar = Jar::default();
        jar.merge(
            ["sid=abc; Path=/; HttpOnly", "pref=dark; Domain=example.com; Path=/; Max-Age=3600", "evil=1; Domain=other.com", "old=x; Expires=Thu, 01 Jan 2026 00:00:00 GMT"],
            &url("https://api.example.com/v1/login"),
            now,
        );
        let mut names: Vec<_> = jar.cookies.iter().map(|c| c.name.as_str()).collect();
        names.sort();
        assert_eq!(names, ["pref", "sid"]);
        assert_eq!(jar.header(&url("https://api.example.com/v1/me"), now), "sid=abc; pref=dark");
        let mut one = Jar::default();
        one.merge(["a=1"], &url("https://x.dev/v1/login"), now);
        assert_eq!(one.cookies[0].path, "/v1", "default path is the request directory");
        assert_eq!(jar.header(&url("https://www.example.com/"), now), "pref=dark", "domain cookie reaches subdomains, host-only doesn't");
        assert_eq!(jar.header(&url("https://example.org/"), now), "");
        jar.merge(["sid=; Path=/; Max-Age=0"], &url("https://api.example.com/v1/logout"), now);
        assert_eq!(jar.header(&url("https://api.example.com/"), now), "pref=dark", "Max-Age=0 deletes");
        assert_eq!(jar.header(&url("https://www.example.com/"), now + 3_601_000), "", "expired cookies aren't sent");
    }

    #[test]
    fn path_secure_expiry_and_order() {
        let now = 1_700_000_000_000;
        let mut jar = Jar::default();
        jar.merge(["a=1; Path=/api; Secure", "b=2; Path=/api/v2", "c=3; Max-Age=60", "d=4; Expires=Wed, 21 Oct 2015 07:28:00 GMT"], &url("https://h.test/x"), now);
        assert_eq!(jar.cookies.iter().map(|c| c.name.as_str()).collect::<Vec<_>>(), ["a", "b", "c"], "an expiry in the past deletes");
        assert_eq!(jar.header(&url("https://h.test/api/v2/x"), now), "b=2; a=1; c=3");
        assert_eq!(jar.header(&url("http://h.test/api/v2/x"), now), "b=2; c=3", "secure only over https");
        assert_eq!(jar.header(&url("https://h.test/apix"), now), "c=3");
        assert_eq!(jar.header(&url("https://h.test/"), now + 61_000), "", "max-age ran out");
        assert_eq!(jar.cookies[2].expires, Some(now + 60_000));
    }

    #[test]
    fn http_dates() {
        assert_eq!(parse_http_date("Wed, 21 Oct 2015 07:28:00 GMT"), Some(1_445_412_480_000));
        assert_eq!(parse_http_date("Wednesday, 21-Oct-15 07:28:00 GMT"), Some(1_445_412_480_000));
        assert_eq!(parse_http_date("Thu, 01 Jan 1970 00:00:00 GMT"), Some(0));
        assert_eq!(parse_http_date("soon"), None);
    }

    #[test]
    fn reads_the_web_apps_shape() {
        let web = r#"[{"name":"sid","value":"abc","domain":"example.com","hostOnly":true,"path":"/","expires":null,"secure":false}]"#;
        let jar: Jar = serde_json::from_str(web).unwrap();
        assert_eq!(jar.header(&url("http://example.com/a"), 0), "sid=abc");
        assert_eq!(serde_json::to_value(&jar).unwrap(), serde_json::from_str::<serde_json::Value>(web).unwrap());
    }
}
