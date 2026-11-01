//! Opt-in usage pings from the CLI and the free desktop builds (never the web app or App Store
//! builds): `POST /api/ping` with `{"id","surface","version","os"}`, at most once a week per
//! install. `GET /api/stats` (Bearer `HITOP_STATS_TOKEN`) returns the counts.
//!
//! What is kept: daily counts per surface, per surface and version, and per surface and OS.
//! What is not: IP addresses, headers, or install ids. An id is only hashed with a salt that
//! lives for the current week, to count each install once a week; the salt and the hashes are
//! dropped when the week changes, so installs can't be followed from one week to the next.
//!
//! Settings (environment): `HITOP_TELEMETRY_DIR` (where `telemetry.json` lives; unset keeps
//! counts in memory only) and `HITOP_STATS_TOKEN` (unset turns `/api/stats` off).

use std::collections::{BTreeMap, HashSet};
use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use std::time::{SystemTime, UNIX_EPOCH};

use rust_web_server::prelude::*;
use rust_web_server::response::StatusCodeReasonPhrase;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

const MAX_BODY: usize = 1024;
/// Days of counts kept.
const KEEP_DAYS: usize = 400;
/// Bound on installs remembered per week; past it pings are accepted but not counted.
const MAX_SEEN: usize = 1_000_000;
const SURFACES: &[&str] = &["cli", "desktop"];
const OSES: &[&str] = &["macos", "linux", "windows", "other"];

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Ping {
    id: String,
    surface: String,
    version: String,
    os: String,
}

#[derive(Default, Serialize, Deserialize)]
struct Store {
    /// Week number (Monday based) the salt and `seen` belong to.
    week: i64,
    salt: String,
    /// Salted hashes of the ids counted this week.
    seen: HashSet<String>,
    /// Day (YYYY-MM-DD) → counter name → count.
    days: BTreeMap<String, BTreeMap<String, u64>>,
}

#[derive(Clone)]
pub struct Telemetry {
    store: Arc<Mutex<Store>>,
    file: Option<PathBuf>,
    token: Option<String>,
}

fn error(status: &'static StatusCodeReasonPhrase, message: &str) -> Response {
    Response::json(status, serde_json::json!({ "error": message }).to_string().into_bytes())
}

fn days_since_epoch(now: SystemTime) -> i64 {
    (now.duration_since(UNIX_EPOCH).map(|d| d.as_secs()).unwrap_or(0) / 86_400) as i64
}

/// 1970-01-01 was a Thursday: shifting by 3 makes weeks start on Monday.
fn week_of(day: i64) -> i64 {
    (day + 3).div_euclid(7)
}

/// Days since 1970-01-01 → "YYYY-MM-DD" (Howard Hinnant's civil_from_days).
fn date_of(day: i64) -> String {
    let z = day + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    let y = yoe + era * 400 + i64::from(m <= 2);
    format!("{y:04}-{m:02}-{d:02}")
}

fn random_salt() -> String {
    let mut bytes = [0u8; 32];
    getrandom::fill(&mut bytes).expect("random salt");
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}

fn valid(ping: &Ping) -> Result<(), &'static str> {
    let token = |s: &str, max: usize, extra: &str| {
        !s.is_empty() && s.len() <= max && s.chars().all(|c| c.is_ascii_alphanumeric() || extra.contains(c))
    };
    if !token(&ping.id, 64, "-") || ping.id.len() < 16 {
        return Err("id must be 16 to 64 letters, digits or dashes");
    }
    if !SURFACES.contains(&ping.surface.as_str()) {
        return Err("surface must be cli or desktop");
    }
    if !token(&ping.version, 32, ".-+") {
        return Err("version must be a version number");
    }
    if !OSES.contains(&ping.os.as_str()) {
        return Err("os must be macos, linux, windows or other");
    }
    Ok(())
}

impl Telemetry {
    pub fn from_env() -> Self {
        let file = std::env::var("HITOP_TELEMETRY_DIR").ok().filter(|s| !s.is_empty()).map(|d| PathBuf::from(d).join("telemetry.json"));
        let token = std::env::var("HITOP_STATS_TOKEN").ok().filter(|s| !s.is_empty());
        Self::new(file, token)
    }

    pub fn new(file: Option<PathBuf>, token: Option<String>) -> Self {
        let store = file
            .as_ref()
            .and_then(|f| std::fs::read(f).ok())
            .and_then(|bytes| serde_json::from_slice(&bytes).ok())
            .unwrap_or_default();
        Telemetry { store: Arc::new(Mutex::new(store)), file, token }
    }

    pub fn ping(&self, request: &Request) -> Response {
        self.ping_at(request, SystemTime::now())
    }

    fn ping_at(&self, request: &Request, now: SystemTime) -> Response {
        let s = STATUS_CODE_REASON_PHRASE;
        // Pings come from the CLI and the desktop app, never from web pages.
        if request.header("origin").is_some() {
            return error(s.n403_forbidden, "pings don't come from web pages");
        }
        if request.body.len() > MAX_BODY {
            return error(s.n413_payload_too_large, "the ping is too large");
        }
        let ping: Ping = match serde_json::from_slice(&request.body) {
            Ok(p) => p,
            Err(_) => return error(s.n400_bad_request, "expected {\"id\",\"surface\",\"version\",\"os\"}"),
        };
        if let Err(message) = valid(&ping) {
            return error(s.n400_bad_request, message);
        }

        let day = days_since_epoch(now);
        let mut store = self.store.lock().unwrap_or_else(|e| e.into_inner());
        if store.week != week_of(day) || store.salt.is_empty() {
            store.week = week_of(day);
            store.salt = random_salt();
            store.seen.clear();
        }
        let hash = format!("{:x}", Sha256::digest(format!("{}:{}", store.salt, ping.id)));
        let counted = !store.seen.contains(&hash) && store.seen.len() < MAX_SEEN;
        if counted {
            store.seen.insert(hash);
            let counters = store.days.entry(date_of(day)).or_default();
            for name in [
                format!("surface:{}", ping.surface),
                format!("version:{} {}", ping.surface, ping.version),
                format!("os:{} {}", ping.surface, ping.os),
            ] {
                *counters.entry(name).or_default() += 1;
            }
            while store.days.len() > KEEP_DAYS {
                let oldest = store.days.keys().next().cloned().unwrap_or_default();
                store.days.remove(&oldest);
            }
            self.save(&store);
        }
        Response::json(s.n200_ok, serde_json::json!({ "counted": counted }).to_string().into_bytes())
    }

    fn save(&self, store: &Store) {
        let Some(file) = &self.file else { return };
        let tmp = file.with_extension("json.tmp");
        if let Ok(bytes) = serde_json::to_vec(store) {
            if std::fs::write(&tmp, bytes).is_ok() {
                let _ = std::fs::rename(&tmp, file);
            }
        }
    }

    pub fn stats(&self, request: &Request) -> Response {
        let s = STATUS_CODE_REASON_PHRASE;
        let Some(token) = &self.token else {
            return error(s.n404_not_found, "not found");
        };
        let given = request
            .header("authorization")
            .and_then(|h| h.value.strip_prefix("Bearer ").map(str::to_string))
            .unwrap_or_default();
        // Same time whatever the mismatch position.
        let same = given.len() == token.len() && given.bytes().zip(token.bytes()).fold(0u8, |acc, (a, b)| acc | (a ^ b)) == 0;
        if !same {
            return error(s.n401_unauthorized, "a valid stats token is required");
        }
        let store = self.store.lock().unwrap_or_else(|e| e.into_inner());
        let body = serde_json::json!({ "installsThisWeek": store.seen.len(), "days": store.days });
        Response::json(s.n200_ok, body.to_string().into_bytes())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rust_web_server::header::Header;
    use std::time::Duration;

    const ID: &str = "0123456789abcdef-install";

    fn request(body: &str, headers: &[(&str, &str)]) -> Request {
        let mut r = Request { method: "POST".into(), request_uri: "/api/ping".into(), http_version: "HTTP/1.1".into(), headers: vec![], body: body.as_bytes().to_vec().into() };
        for (name, value) in headers {
            r.headers.push(Header { name: name.to_string().into(), value: value.to_string().into() });
        }
        r
    }

    fn ping(id: &str) -> String {
        format!(r#"{{"id":"{id}","surface":"cli","version":"0.3.0","os":"linux"}}"#)
    }

    fn day(n: i64) -> SystemTime {
        UNIX_EPOCH + Duration::from_secs(n as u64 * 86_400 + 3600)
    }

    fn counted(r: &Response) -> bool {
        let v: serde_json::Value = serde_json::from_slice(&r.content_range_list[0].body).unwrap();
        v["counted"].as_bool().unwrap()
    }

    #[test]
    fn dates_and_weeks() {
        assert_eq!(date_of(0), "1970-01-01");
        assert_eq!(date_of(20_393), "2025-11-01");
        assert_eq!(date_of(11_016), "2000-02-29");
        // 2026-11-02 is a Monday: a new week starts there.
        let monday = 20_759;
        assert_eq!(date_of(monday), "2026-11-02");
        assert_eq!(week_of(monday - 1) + 1, week_of(monday));
        assert_eq!(week_of(monday), week_of(monday + 6));
    }

    #[test]
    fn counts_an_install_once_a_week() {
        let t = Telemetry::new(None, None);
        let monday = 20_759;
        assert!(counted(&t.ping_at(&request(&ping(ID), &[]), day(monday))));
        assert!(!counted(&t.ping_at(&request(&ping(ID), &[]), day(monday + 3))));
        assert!(counted(&t.ping_at(&request(&ping("another-install-0001"), &[]), day(monday + 3))));
        // Next week the same install counts again, under a new salt.
        let salt = t.store.lock().unwrap().salt.clone();
        assert!(counted(&t.ping_at(&request(&ping(ID), &[]), day(monday + 7))));
        let store = t.store.lock().unwrap();
        assert_ne!(store.salt, salt);
        assert_eq!(store.seen.len(), 1);
        assert_eq!(store.days["2026-11-02"]["surface:cli"], 1);
        assert_eq!(store.days["2026-11-05"]["version:cli 0.3.0"], 1);
        assert_eq!(store.days["2026-11-09"]["os:cli linux"], 1);
    }

    #[test]
    fn stores_no_ip_header_or_id() {
        let dir = std::env::temp_dir().join(format!("hitop-telemetry-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let file = dir.join("telemetry.json");
        let t = Telemetry::new(Some(file.clone()), None);
        let headers = [("x-forwarded-for", "203.0.113.7"), ("x-real-ip", "203.0.113.7"), ("user-agent", "hitop/0.3.0")];
        assert!(counted(&t.ping(&request(&ping(ID), &headers))));
        let saved = std::fs::read_to_string(&file).unwrap();
        assert!(!saved.contains("203.0.113"), "{saved}");
        assert!(!saved.contains(ID), "{saved}");
        assert!(!saved.contains("hitop/0.3.0"), "{saved}");
        // Reloaded from disk, the install is still known this week.
        let again = Telemetry::new(Some(file), None);
        assert!(!counted(&again.ping(&request(&ping(ID), &[]))));
        std::fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn rejects_bad_pings() {
        let t = Telemetry::new(None, None);
        let status = |body: &str, headers: &[(&str, &str)]| t.ping(&request(body, headers)).status_code;
        assert_eq!(status(&ping(ID), &[("origin", "https://example.com")]), 403);
        assert_eq!(status("{}", &[]), 400);
        assert_eq!(status(&ping("short"), &[]), 400);
        assert_eq!(status(&ping(ID).replace("cli", "web"), &[]), 400);
        assert_eq!(status(&ping(ID).replace("linux", "beos"), &[]), 400);
        assert_eq!(status(&ping(ID).replace("}", r#","ip":"1.2.3.4"}"#), &[]), 400);
        assert_eq!(status(&"x".repeat(2000), &[]), 413);
    }

    #[test]
    fn stats_need_the_token() {
        let off = Telemetry::new(None, None);
        assert_eq!(off.stats(&request("", &[])).status_code, 404);
        let t = Telemetry::new(None, Some("s3cret-token".into()));
        t.ping(&request(&ping(ID), &[]));
        assert_eq!(t.stats(&request("", &[])).status_code, 401);
        assert_eq!(t.stats(&request("", &[("authorization", "Bearer wrong-token1")])).status_code, 401);
        let r = t.stats(&request("", &[("authorization", "Bearer s3cret-token")]));
        assert_eq!(r.status_code, 200);
        let v: serde_json::Value = serde_json::from_slice(&r.content_range_list[0].body).unwrap();
        assert_eq!(v["installsThisWeek"], 1);
    }
}
