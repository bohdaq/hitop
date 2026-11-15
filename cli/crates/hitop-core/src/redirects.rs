#![cfg(feature = "native")]
//! Redirects, followed one hop at a time so every hop can be shown and its cookies kept.
//!
//! reqwest's own redirect policy hides the hops: a `302` with `Set-Cookie` (a login) loses its
//! cookie and the user sees only the last answer. The CLI, the desktop and hitop-server's proxy
//! all send with redirects off and walk them with a [`Follower`], so the three behave the same:
//!
//! - each hop's status, URL, `Location` and `Set-Cookie` lines are kept ([`Hop`]);
//! - cookies a hop sets go out with the next hops (a jar for this exchange, plus the caller's);
//! - `301`/`302` turn a `POST` into a `GET`, `303` turns anything but `HEAD` into a `GET` (the
//!   body and its `Content-*` headers go); `307`/`308` keep the method and the body;
//! - moving to another host drops `Authorization`, `Cookie` and `Proxy-Authorization` as
//!   browsers do (cookies for the new host still come from the jar);
//! - at most [`MAX_REDIRECTS`] hops.
use reqwest::Url;

use crate::cookies::{self, Jar};

pub const MAX_REDIRECTS: usize = 10;

pub use crate::model::Hop;

/// The next request of an exchange.
#[derive(Debug, Clone)]
pub struct Step {
    pub method: String,
    pub url: Url,
    pub headers: Vec<(String, String)>,
    pub body: Vec<u8>,
}

pub struct Follower {
    step: Step,
    first_host: String,
    /// Cookies the hops set (and, for the CLI and desktop, the caller's jar).
    jar: Jar,
    /// The request had its own `Cookie` header (sent as is to the first host).
    own_cookie: Option<String>,
    pub hops: Vec<Hop>,
    follow: bool,
}

fn host(url: &Url) -> String {
    format!("{}:{}", url.host_str().unwrap_or_default().to_ascii_lowercase(), url.port_or_known_default().unwrap_or(0))
}

const CROSS_HOST_DROPPED: &[&str] = &["authorization", "cookie", "proxy-authorization"];

impl Follower {
    /// `jar` holds the cookies to send (the caller's jar, or an empty one); `follow` false stops at
    /// the first answer, redirect or not.
    pub fn new(method: &str, url: Url, headers: Vec<(String, String)>, body: Vec<u8>, jar: Jar, follow: bool) -> Self {
        let own_cookie = headers.iter().find(|(k, v)| k.eq_ignore_ascii_case("cookie") && !v.is_empty()).map(|(_, v)| v.clone());
        let headers = headers.into_iter().filter(|(k, _)| !k.eq_ignore_ascii_case("cookie")).collect();
        Follower { first_host: host(&url), step: Step { method: method.to_string(), url, headers, body }, jar, own_cookie, hops: vec![], follow }
    }

    /// The request to send now, with its `Cookie` header.
    pub fn request(&self) -> Step {
        let mut step = self.step.clone();
        let same = host(&step.url) == self.first_host;
        let from_jar = self.jar.header(&step.url, cookies::now_ms());
        let cookie = match (same.then_some(self.own_cookie.as_deref()).flatten(), from_jar.is_empty()) {
            // The request's own Cookie header wins over the jar, as it always has.
            (Some(own), true) => Some(own.to_string()),
            (Some(own), false) if self.hops.is_empty() => Some(own.to_string()),
            (Some(own), false) => Some(format!("{own}; {from_jar}")),
            (None, false) => Some(from_jar),
            (None, true) => None,
        };
        if let Some(c) = cookie {
            step.headers.push(("Cookie".into(), c));
        }
        step
    }

    /// The jar with every cookie the hops set.
    pub fn jar(&self) -> &Jar {
        &self.jar
    }

    /// Reads one answer. `Ok(true)`: it was a redirect to follow, send [`Self::request`] again.
    /// `Ok(false)`: this is the final answer. An error past `MAX_REDIRECTS` or for a `Location`
    /// that isn't a URL.
    pub fn answer(&mut self, status: u16, location: Option<&str>, set_cookies: Vec<String>) -> Result<bool, String> {
        let now = cookies::now_ms();
        self.jar.merge(set_cookies.iter().map(String::as_str), &self.step.url, now);
        let redirect = matches!(status, 301 | 302 | 303 | 307 | 308);
        let Some(location) = location.filter(|_| redirect && self.follow) else {
            return Ok(false);
        };
        if self.hops.len() >= MAX_REDIRECTS {
            return Err(format!("stopped after {MAX_REDIRECTS} redirects"));
        }
        let next = self.step.url.join(location.trim()).map_err(|e| format!("the redirect to {location:?} isn't a URL: {e}"))?;
        self.hops.push(Hop { status, url: self.step.url.to_string(), location: next.to_string(), set_cookies });
        let method = self.step.method.to_ascii_uppercase();
        let to_get = match status {
            303 => method != "HEAD",
            301 | 302 => method == "POST",
            _ => false,
        };
        if to_get {
            self.step.method = "GET".into();
            self.step.body.clear();
            self.step.headers.retain(|(k, _)| !k.to_ascii_lowercase().starts_with("content-"));
        }
        if host(&next) != host(&self.step.url) {
            self.step.headers.retain(|(k, _)| !CROSS_HOST_DROPPED.contains(&k.to_ascii_lowercase().as_str()));
        }
        self.step.url = next;
        Ok(true)
    }
}

/// The `Set-Cookie` lines of a reqwest response.
pub fn set_cookie_lines(headers: &reqwest::header::HeaderMap) -> Vec<String> {
    headers.get_all(reqwest::header::SET_COOKIE).iter().filter_map(|v| v.to_str().ok()).map(str::to_string).collect()
}

/// The `Location` header of a reqwest response.
pub fn location(headers: &reqwest::header::HeaderMap) -> Option<String> {
    headers.get(reqwest::header::LOCATION).and_then(|v| v.to_str().ok()).map(str::to_string)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn follower(method: &str, url: &str, headers: &[(&str, &str)]) -> Follower {
        let headers = headers.iter().map(|(k, v)| (k.to_string(), v.to_string())).collect();
        Follower::new(method, Url::parse(url).unwrap(), headers, b"body".to_vec(), Jar::default(), true)
    }

    fn cookie(step: &Step) -> Option<&str> {
        step.headers.iter().find(|(k, _)| k == "Cookie").map(|(_, v)| v.as_str())
    }

    #[test]
    fn a_cookie_set_on_a_redirect_goes_to_the_next_hop() {
        let mut f = follower("GET", "https://a.test/login", &[]);
        assert!(f.answer(302, Some("/home"), vec!["sid=1; Path=/".into()]).unwrap());
        let next = f.request();
        assert_eq!(next.url.as_str(), "https://a.test/home");
        assert_eq!(cookie(&next), Some("sid=1"));
        assert!(!f.answer(200, None, vec![]).unwrap());
        assert_eq!(f.hops.len(), 1);
        assert_eq!(f.hops[0].set_cookies, ["sid=1; Path=/"]);
        assert_eq!(f.jar().cookies.len(), 1);
    }

    #[test]
    fn post_becomes_get_on_302_and_303_but_not_307() {
        let mut f = follower("POST", "https://a.test/x", &[("Content-Type", "application/json")]);
        assert!(f.answer(303, Some("https://a.test/y"), vec![]).unwrap());
        let s = f.request();
        assert_eq!(s.method, "GET");
        assert!(s.body.is_empty() && !s.headers.iter().any(|(k, _)| k == "Content-Type"));
        let mut f = follower("POST", "https://a.test/x", &[]);
        assert!(f.answer(307, Some("/z"), vec![]).unwrap());
        assert_eq!((f.request().method.as_str(), f.request().body.as_slice()), ("POST", &b"body"[..]));
    }

    #[test]
    fn credentials_stay_on_the_first_host() {
        let mut f = follower("GET", "https://a.test/", &[("Authorization", "Bearer t"), ("Cookie", "own=1")]);
        assert_eq!(cookie(&f.request()), Some("own=1"));
        assert!(f.answer(301, Some("https://b.test/"), vec![]).unwrap());
        let s = f.request();
        assert!(!s.headers.iter().any(|(k, _)| k.eq_ignore_ascii_case("authorization")));
        assert_eq!(cookie(&s), None);
    }

    #[test]
    fn not_following_returns_the_redirect_itself_and_loops_stop() {
        let mut f = Follower::new("GET", Url::parse("https://a.test/").unwrap(), vec![], vec![], Jar::default(), false);
        assert!(!f.answer(302, Some("/b"), vec![]).unwrap());
        assert!(f.hops.is_empty());
        let mut f = follower("GET", "https://a.test/", &[]);
        for _ in 0..MAX_REDIRECTS {
            assert!(f.answer(302, Some("/again"), vec![]).unwrap());
        }
        assert!(f.answer(302, Some("/again"), vec![]).is_err());
    }
}
