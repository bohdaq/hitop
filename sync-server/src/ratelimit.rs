//! Request rate limits in the server (HITOP Cloud C2 abuse limits), on top of the front proxy's
//! per-IP route limits (`deploy/rws-routes.toml`: 20 a minute on `/v1/auth/*`, 600 a minute on
//! the rest). The proxy only sees addresses; these limits also key on the session, so one account
//! can't use many addresses, and they hold on a self-hosted server without that proxy.
//!
//! Fixed windows in memory (one process per database, db.rs), per class:
//!
//! | class    | requests                                         | key             | default        |
//! | -------- | ------------------------------------------------ | --------------- | -------------- |
//! | `auth`   | `POST /v1/auth/*` (sign-in mails, codes, passkeys)| client IP       | 60 a minute    |
//! | `write`  | every other POST, PUT, DELETE                    | session token   | 600 a minute   |
//! | `write`  | the same                                          | client IP       | 1200 a minute  |
//! | `create` | new workspaces, orgs, tokens, device keys, runners, invites, recovery | session token | 200 an hour |
//!
//! Wrong passwords back off per address on top of these (password.rs `Backoff`: after 5, 30 s
//! doubling to 15 min, for addresses with and without an account alike).
//!
//! Reads are never limited here. Over a limit: 429 `{error: "rate_limited", limit, retryAfter}`
//! and `Retry-After`. Settings: `HITOP_SYNC_RATE_LIMITS=off` turns them all off;
//! `HITOP_SYNC_RATE_AUTH`, `HITOP_SYNC_RATE_WRITE`, `HITOP_SYNC_RATE_WRITE_IP` (per minute),
//! `HITOP_SYNC_RATE_CREATE` (per hour); 0 turns one off.

use std::collections::HashMap;
use std::sync::Mutex;

/// The limits (requests per window; 0 = no limit).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RateLimits {
    pub auth_per_minute: u32,
    pub write_per_minute: u32,
    pub write_ip_per_minute: u32,
    pub create_per_hour: u32,
}

impl RateLimits {
    pub fn defaults() -> Self {
        RateLimits { auth_per_minute: 60, write_per_minute: 600, write_ip_per_minute: 1200, create_per_hour: 200 }
    }

    pub fn off() -> Self {
        RateLimits { auth_per_minute: 0, write_per_minute: 0, write_ip_per_minute: 0, create_per_hour: 0 }
    }
}

/// What a request counts against.
#[derive(Debug, PartialEq, Eq)]
pub enum Class {
    Auth,
    Write { create: bool },
    Read,
}

/// The class of `method path` (path without the query).
pub fn classify(method: &str, path: &str) -> Class {
    if method == "GET" || method == "HEAD" || method == "OPTIONS" {
        return Class::Read;
    }
    if path.starts_with("/v1/auth/") {
        return Class::Auth;
    }
    let parts: Vec<&str> = path.trim_matches('/').split('/').collect();
    let create = method == "POST"
        && matches!(
            parts.as_slice(),
            ["v1", "workspaces"]
                | ["v1", "orgs"]
                | ["v1", "orgs", _, "workspaces"]
                | ["v1", "orgs", _, "members"]
                | ["v1", "me", "tokens"]
                | ["v1", "me", "keys"]
                | ["v1", "workspaces", _, "runners"]
                | ["v1", "workspaces", _, "invites"]
        )
        || (method == "PUT" && parts.as_slice() == ["v1", "me", "recovery"]);
    Class::Write { create }
}

/// A refusal: which limit, and seconds until the window ends.
#[derive(Debug, PartialEq, Eq)]
pub struct Refused {
    pub limit: &'static str,
    pub retry_after: i64,
}

/// (limit name, key) → (window start, requests in it).
type Windows = HashMap<(&'static str, String), (i64, u32)>;

pub struct Limiter {
    limits: RateLimits,
    windows: Mutex<Windows>,
}

impl Limiter {
    pub fn new(limits: RateLimits) -> Self {
        Limiter { limits, windows: Mutex::new(HashMap::new()) }
    }

    /// Counts the request (`now` in seconds); `Err` when it is over a limit. `session` is the
    /// bearer token's hash, when there is one.
    pub fn check(&self, class: &Class, ip: &str, session: Option<&str>, now: i64) -> Result<(), Refused> {
        let mut checks: Vec<(&'static str, String, u32, i64)> = Vec::new();
        let l = &self.limits;
        match class {
            Class::Read => return Ok(()),
            Class::Auth => checks.push(("auth", ip.to_string(), l.auth_per_minute, 60)),
            Class::Write { create } => {
                checks.push(("write_ip", ip.to_string(), l.write_ip_per_minute, 60));
                if let Some(s) = session {
                    checks.push(("write", s.to_string(), l.write_per_minute, 60));
                    if *create {
                        checks.push(("create", s.to_string(), l.create_per_hour, 3600));
                    }
                }
            }
        }
        let mut windows = self.windows.lock().unwrap_or_else(|e| e.into_inner());
        if windows.len() > 100_000 {
            windows.retain(|(name, _), (start, _)| now - *start < if *name == "create" { 3600 } else { 60 });
        }
        // Refuse before counting anything, so a refused request doesn't use up another limit.
        for (name, key, max, secs) in &checks {
            if *max == 0 {
                continue;
            }
            if let Some((start, count)) = windows.get(&(*name, key.clone())) {
                if now - start < *secs && count >= max {
                    return Err(Refused { limit: name, retry_after: (start + secs - now).max(1) });
                }
            }
        }
        for (name, key, max, secs) in checks {
            if max == 0 {
                continue;
            }
            let w = windows.entry((name, key)).or_insert((now, 0));
            if now - w.0 >= secs {
                *w = (now, 0);
            }
            w.1 += 1;
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn classes() {
        assert_eq!(classify("GET", "/v1/workspaces"), Class::Read);
        assert_eq!(classify("POST", "/v1/auth/email"), Class::Auth);
        assert_eq!(classify("POST", "/v1/workspaces"), Class::Write { create: true });
        assert_eq!(classify("POST", "/v1/orgs/o1/workspaces"), Class::Write { create: true });
        assert_eq!(classify("PUT", "/v1/workspaces/w/blobs/b"), Class::Write { create: false });
        assert_eq!(classify("PUT", "/v1/me/recovery"), Class::Write { create: true });
        assert_eq!(classify("DELETE", "/v1/workspaces/w/members/u"), Class::Write { create: false });
    }

    #[test]
    fn limits_hold_at_the_boundary_and_reset_with_the_window() {
        let l = Limiter::new(RateLimits { auth_per_minute: 3, write_per_minute: 2, write_ip_per_minute: 5, create_per_hour: 1 });
        for _ in 0..3 {
            assert!(l.check(&Class::Auth, "1.2.3.4", None, 100).is_ok());
        }
        assert_eq!(l.check(&Class::Auth, "1.2.3.4", None, 120), Err(Refused { limit: "auth", retry_after: 40 }));
        assert!(l.check(&Class::Auth, "5.6.7.8", None, 120).is_ok(), "another address");
        assert!(l.check(&Class::Auth, "1.2.3.4", None, 160).is_ok(), "next window");

        let w = Class::Write { create: false };
        assert!(l.check(&w, "1.1.1.1", Some("s1"), 0).is_ok());
        assert!(l.check(&w, "2.2.2.2", Some("s1"), 0).is_ok());
        assert_eq!(l.check(&w, "3.3.3.3", Some("s1"), 1).unwrap_err().limit, "write", "per session across addresses");
        assert!(l.check(&w, "3.3.3.3", Some("s2"), 1).is_ok());

        let c = Class::Write { create: true };
        assert!(l.check(&c, "9.9.9.9", Some("s3"), 0).is_ok());
        assert_eq!(l.check(&c, "9.9.9.9", Some("s3"), 120).unwrap_err().limit, "create");
        assert!(l.check(&c, "9.9.9.9", Some("s3"), 3600).is_ok());
    }

    #[test]
    fn zero_is_no_limit() {
        let l = Limiter::new(RateLimits::off());
        for _ in 0..10_000 {
            assert!(l.check(&Class::Auth, "1.2.3.4", None, 0).is_ok());
        }
    }
}
