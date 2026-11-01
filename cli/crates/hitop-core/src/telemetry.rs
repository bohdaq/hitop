#![cfg(feature = "telemetry")]
//! The opt-in usage ping (feature `telemetry`: on for the CLI and the GitHub
//! Release desktop builds; Mac App Store builds, iOS and the web app don't
//! contain this module).
//!
//! Off by default. When it is on, at most once a week:
//! `POST https://hitop.rws8.tech/api/ping` with exactly
//! `{"id","surface","version","os"}`: a random install id made on this
//! device, `cli` or `desktop`, the version, and the OS family. Nothing else.
//! The server side is `server/src/telemetry.rs`; the privacy page
//! (`site/src/pages/privacy.js`) describes both.
//!
//! Consent is per surface: `hitop telemetry on` turns on the CLI only, the
//! desktop app asks for itself. Settings: `<config>/hitop/telemetry.json`
//! (`{"cli": {...}, "desktop": {...}}`). Turning it off forgets the id.
use std::collections::BTreeMap;
use std::fs;
use std::path::PathBuf;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};

/// Where pings go; `HITOP_TELEMETRY_URL` overrides it (tests).
pub const ENDPOINT: &str = "https://hitop.rws8.tech/api/ping";
/// The least time between two pings.
pub const WEEK: u64 = 7 * 86_400;
const TIMEOUT: Duration = Duration::from_secs(3);

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Surface {
    Cli,
    Desktop,
}

impl Surface {
    pub fn as_str(self) -> &'static str {
        match self {
            Surface::Cli => "cli",
            Surface::Desktop => "desktop",
        }
    }
}

/// The OS family as the server takes it.
pub fn os() -> &'static str {
    match std::env::consts::OS {
        "macos" => "macos",
        "linux" => "linux",
        "windows" => "windows",
        _ => "other",
    }
}

pub fn endpoint() -> String {
    std::env::var("HITOP_TELEMETRY_URL").ok().filter(|s| !s.is_empty()).unwrap_or_else(|| ENDPOINT.into())
}

/// What one surface has stored.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct State {
    #[serde(default)]
    pub enabled: bool,
    /// The desktop asked its first-run question (or the setting was changed).
    #[serde(default)]
    pub asked: bool,
    /// The install id, only while enabled.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub id: Option<String>,
    /// When the last ping was attempted, seconds since 1970.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub last_ping: Option<u64>,
}

/// The ping's body, exactly what is sent.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Ping {
    pub id: String,
    pub surface: &'static str,
    pub version: String,
    pub os: &'static str,
}

/// What [`Telemetry::maybe_ping`] did.
#[derive(Debug, Clone, PartialEq)]
pub enum Outcome {
    Off,
    NotDue,
    /// Sent; `counted` is the server's answer.
    Sent { counted: bool },
    /// Tried and failed (offline, server error); not tried again this week.
    Failed(String),
}

/// One surface's setting in a settings file.
pub struct Telemetry {
    path: PathBuf,
    surface: Surface,
}

pub fn now() -> u64 {
    SystemTime::now().duration_since(UNIX_EPOCH).map(|d| d.as_secs()).unwrap_or(0)
}

impl Telemetry {
    /// The settings in hitop's config directory.
    pub fn new(surface: Surface) -> Result<Self> {
        let base = dirs::config_dir().context("cannot determine config directory")?;
        Ok(Self::at(base.join("hitop").join("telemetry.json"), surface))
    }

    pub fn at(path: PathBuf, surface: Surface) -> Self {
        Telemetry { path, surface }
    }

    pub fn path(&self) -> &std::path::Path {
        &self.path
    }

    fn load_all(&self) -> BTreeMap<String, serde_json::Value> {
        fs::read_to_string(&self.path).ok().and_then(|t| serde_json::from_str(&t).ok()).unwrap_or_default()
    }

    pub fn state(&self) -> State {
        self.load_all().get(self.surface.as_str()).and_then(|v| serde_json::from_value(v.clone()).ok()).unwrap_or_default()
    }

    fn save(&self, state: &State) -> Result<()> {
        let mut all = self.load_all();
        all.insert(self.surface.as_str().into(), serde_json::to_value(state)?);
        if let Some(dir) = self.path.parent() {
            fs::create_dir_all(dir)?;
        }
        fs::write(&self.path, serde_json::to_string_pretty(&all)?).with_context(|| format!("writing {}", self.path.display()))
    }

    /// On makes an install id; off forgets it (and when the last ping was).
    pub fn set_enabled(&self, enabled: bool) -> Result<State> {
        let mut s = self.state();
        s.asked = true;
        s.enabled = enabled;
        if enabled {
            s.id.get_or_insert_with(crate::secrets::new_workspace_id);
        } else {
            s.id = None;
            s.last_ping = None;
        }
        self.save(&s)?;
        Ok(s)
    }

    /// What a ping sends now: `None` while off.
    pub fn payload(&self, version: &str) -> Option<Ping> {
        let s = self.state();
        let id = s.id.filter(|_| s.enabled)?;
        Some(Ping { id, surface: self.surface.as_str(), version: version.into(), os: os() })
    }

    /// True when on and no ping was attempted in the last week.
    pub fn due(&self, now: u64) -> bool {
        let s = self.state();
        s.enabled && s.id.is_some() && s.last_ping.is_none_or(|last| now < last || now - last >= WEEK)
    }

    /// Sends the ping when on and due. The attempt is recorded before
    /// sending, so a machine that is offline doesn't try again before a week.
    pub fn maybe_ping(&self, version: &str, now: u64) -> Outcome {
        if !self.state().enabled {
            return Outcome::Off;
        }
        if !self.due(now) {
            return Outcome::NotDue;
        }
        let Some(ping) = self.payload(version) else { return Outcome::Off };
        let mut s = self.state();
        s.last_ping = Some(now);
        if let Err(e) = self.save(&s) {
            return Outcome::Failed(format!("{e:#}"));
        }
        match send(&endpoint(), &ping) {
            Ok(counted) => Outcome::Sent { counted },
            Err(e) => Outcome::Failed(format!("{e:#}")),
        }
    }
}

fn send(url: &str, ping: &Ping) -> Result<bool> {
    let client = reqwest::blocking::Client::builder().use_rustls_tls().timeout(TIMEOUT).build()?;
    let resp = client.post(url).json(ping).send()?.error_for_status()?;
    let answer: serde_json::Value = resp.json().unwrap_or_default();
    Ok(answer.get("counted").and_then(|v| v.as_bool()).unwrap_or(false))
}

/// "YYYY-MM-DD" for seconds since 1970 (UTC).
pub fn date(secs: u64) -> String {
    let z = (secs / 86_400) as i64 + 719_468;
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

#[cfg(test)]
mod tests {
    use super::*;

    fn at(dir: &tempfile::TempDir, surface: Surface) -> Telemetry {
        Telemetry::at(dir.path().join("telemetry.json"), surface)
    }

    #[test]
    fn off_by_default_and_per_surface() {
        let dir = tempfile::tempdir().unwrap();
        let cli = at(&dir, Surface::Cli);
        let desktop = at(&dir, Surface::Desktop);
        assert_eq!(cli.state(), State::default());
        assert!(cli.payload("1.0").is_none() && !cli.due(0));
        assert_eq!(cli.maybe_ping("1.0", now()), Outcome::Off);
        assert!(!dir.path().join("telemetry.json").exists(), "nothing written while off");
        let s = cli.set_enabled(true).unwrap();
        let id = s.id.clone().unwrap();
        assert!(id.len() == 32 && id.chars().all(|c| c.is_ascii_hexdigit()), "{id}");
        assert_eq!(desktop.state(), State::default(), "the desktop's consent is its own");
        let ping = cli.payload("0.3.0").unwrap();
        assert_eq!(serde_json::to_value(&ping).unwrap(), serde_json::json!({ "id": id, "surface": "cli", "version": "0.3.0", "os": os() }));
        cli.set_enabled(false).unwrap();
        let off = cli.state();
        assert!(!off.enabled && off.asked && off.id.is_none() && off.last_ping.is_none(), "{off:?}");
    }

    #[test]
    fn at_most_once_a_week() {
        let dir = tempfile::tempdir().unwrap();
        let t = at(&dir, Surface::Desktop);
        t.set_enabled(true).unwrap();
        let mut s = t.state();
        let start = 1_800_000_000;
        s.last_ping = Some(start);
        t.save(&s).unwrap();
        assert!(!t.due(start + WEEK - 1));
        assert_eq!(t.maybe_ping("1", start + 3600), Outcome::NotDue);
        assert!(t.due(start + WEEK));
        assert!(t.due(start - 10), "a clock set back doesn't block it for good");
    }

    #[test]
    fn dates() {
        assert_eq!(date(0), "1970-01-01");
        assert_eq!(date(1_793_491_200), "2026-11-01");
    }
}
