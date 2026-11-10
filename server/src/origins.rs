//! Which page origins may use the proxy and the bridges (`HITOP_ALLOWED_ORIGINS`).
//!
//! An entry is an exact Origin (`https://hitop.rws8.tech`, the Chrome Web Store build
//! `chrome-extension://mmbpagencfjfgeicmeobbmeocdpeokdk`) or `moz-extension://*`: Firefox gives
//! each installed extension a random per-install UUID origin, so a fixed value can't name ours.
//! That entry accepts `moz-extension://<uuid>` only (the exact UUID shape, nothing else).
//!
//! Why the Origin check exists, and what the Firefox wildcard gives up: browsers set Origin and
//! pages can't forge it, so the check keeps *other websites* from using the proxy through their
//! visitors' browsers. It was never authentication: curl can send any Origin, and the front rws's
//! per-IP rate limit is what bounds that. `moz-extension://*` lets any Firefox extension the
//! person installed call the proxy too. Such an extension already holds its own network
//! permissions, so it gains nothing it couldn't do itself or with curl; the address guard
//! (guard.rs), body limits and the rate limit apply to it like to everyone else.

use std::sync::Arc;

/// The default when `HITOP_ALLOWED_ORIGINS` is unset: the web app and both store extensions.
pub const DEFAULT: &str = "https://hitop.rws8.tech,chrome-extension://mmbpagencfjfgeicmeobbmeocdpeokdk,moz-extension://*";

const FIREFOX_ANY: &str = "moz-extension://*";

#[derive(Clone, Debug)]
pub struct Origins {
    exact: Arc<Vec<String>>,
    any_firefox: bool,
}

impl Origins {
    pub fn new(list: Vec<String>) -> Self {
        let any_firefox = list.iter().any(|o| o == FIREFOX_ANY);
        Origins { exact: Arc::new(list.into_iter().filter(|o| o != FIREFOX_ANY).collect()), any_firefox }
    }

    /// `HITOP_ALLOWED_ORIGINS` (comma separated), or [`DEFAULT`].
    pub fn from_env() -> Self {
        let raw = std::env::var("HITOP_ALLOWED_ORIGINS").unwrap_or_else(|_| DEFAULT.into());
        Origins::new(raw.split(',').map(|s| s.trim().to_string()).filter(|s| !s.is_empty()).collect())
    }

    pub fn allows(&self, origin: &str) -> bool {
        if self.exact.iter().any(|o| o == origin) {
            return true;
        }
        self.any_firefox && origin.strip_prefix("moz-extension://").is_some_and(is_uuid)
    }
}

impl From<Vec<String>> for Origins {
    fn from(list: Vec<String>) -> Self {
        Origins::new(list)
    }
}

/// `xxxxxxxx-xxxx-xxxx-xxxx-xxxxxxxxxxxx`, lower or upper case hex.
fn is_uuid(s: &str) -> bool {
    let groups: Vec<&str> = s.split('-').collect();
    groups.len() == 5 && groups.iter().zip([8, 4, 4, 4, 12]).all(|(g, n)| g.len() == n && g.bytes().all(|b| b.is_ascii_hexdigit()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn exact_entries_and_the_firefox_pattern() {
        let o = Origins::new(DEFAULT.split(',').map(String::from).collect());
        assert!(o.allows("https://hitop.rws8.tech"));
        assert!(o.allows("chrome-extension://mmbpagencfjfgeicmeobbmeocdpeokdk"));
        assert!(o.allows("moz-extension://8f3c9a2e-1b4d-4c6e-9f00-1234567890ab"));
        for bad in [
            "",
            "null",
            "https://evil.example",
            "https://hitop.rws8.tech.evil.example",
            "chrome-extension://aaaabbbbccccddddeeeeffffgggghhhh",
            "moz-extension://*",
            "moz-extension://not-a-uuid",
            "moz-extension://8f3c9a2e-1b4d-4c6e-9f00-1234567890ab.evil.example",
            "moz-extension://8f3c9a2e-1b4d-4c6e-9f00-1234567890ab/",
            "http://moz-extension://8f3c9a2e-1b4d-4c6e-9f00-1234567890ab",
        ] {
            assert!(!o.allows(bad), "{bad:?} must be refused");
        }
    }

    #[test]
    fn without_the_pattern_firefox_is_refused() {
        let o = Origins::new(vec!["https://hitop.rws8.tech".into()]);
        assert!(!o.allows("moz-extension://8f3c9a2e-1b4d-4c6e-9f00-1234567890ab"));
    }
}
