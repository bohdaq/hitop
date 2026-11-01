//! What `hitop sync` keeps on this computer, under `<config>/hitop/sync/`:
//!
//! - `account.json`: server, email, account id, this device's key id and name (no secrets);
//! - the session token and the device's X25519 secret key: in the OS keychain (service `hitop`,
//!   account `sync/<server host>`), or in the file `HITOP_SYNC_KEY_FILE` names (mode 0600) where
//!   there is no keychain; never anywhere else;
//! - `pins.json`: device keys confirmed by fingerprint (key id → public key), trust on first use;
//! - `links.json`: collection folder → workspace;
//! - `state/<workspace>.json`: [`hitop_core::sync::State`] of each linked folder.
//!
//! A runner (CI, cron) uses `HITOP_SYNC_RUNNER` instead: one value holding a runner credential
//! (`hitop sync runner create` prints it), read-only and for one workspace.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use anyhow::{bail, Context, Result};
use base64::engine::general_purpose::URL_SAFE_NO_PAD;
use base64::Engine;
use hitop_sync_crypto::DeviceKeyPair;
use serde::{Deserialize, Serialize};

pub const DEFAULT_SERVER: &str = "https://sync.hitop.rws8.tech";
pub const RUNNER_ENV: &str = "HITOP_SYNC_RUNNER";

fn dir() -> Result<PathBuf> {
    let d = dirs::config_dir().context("cannot determine the config directory")?.join("hitop").join("sync");
    std::fs::create_dir_all(&d)?;
    Ok(d)
}

fn read_json<T: for<'de> Deserialize<'de> + Default>(path: &Path) -> Result<T> {
    match std::fs::read_to_string(path) {
        Ok(text) => serde_json::from_str(&text).with_context(|| path.display().to_string()),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(T::default()),
        Err(e) => Err(e).with_context(|| path.display().to_string()),
    }
}

fn write_json<T: Serialize>(path: &Path, value: &T) -> Result<()> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let tmp = path.with_extension("tmp");
    std::fs::write(&tmp, serde_json::to_string_pretty(value)? + "\n")?;
    std::fs::rename(&tmp, path)?;
    Ok(())
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Account {
    pub server: String,
    pub email: String,
    pub user_id: String,
    pub key_id: String,
    #[serde(default)]
    pub device_name: String,
}

impl Account {
    pub fn load() -> Result<Option<Account>> {
        let a: Account = read_json(&dir()?.join("account.json"))?;
        Ok((!a.server.is_empty()).then_some(a))
    }

    pub fn save(&self) -> Result<()> {
        write_json(&dir()?.join("account.json"), self)
    }

    pub fn forget() -> Result<()> {
        let p = dir()?.join("account.json");
        if p.exists() {
            std::fs::remove_file(p)?;
        }
        Ok(())
    }
}

/// The session and the device secret key, kept together.
#[derive(Serialize, Deserialize)]
pub struct Secret {
    pub session: String,
    /// The X25519 secret key, base64url.
    pub secret: String,
}

impl Secret {
    pub fn device(&self) -> Result<DeviceKeyPair> {
        let bytes = URL_SAFE_NO_PAD.decode(&self.secret).context("device key")?;
        DeviceKeyPair::from_secret_bytes(&bytes).map_err(|e| anyhow::anyhow!("device key: {e}"))
    }
}

fn keychain_account(server: &str) -> String {
    let host = url::Url::parse(server).ok().and_then(|u| u.host_str().map(|h| match u.port() {
        Some(p) => format!("{h}:{p}"),
        None => h.to_string(),
    }));
    format!("sync/{}", host.unwrap_or_else(|| server.to_string()))
}

fn key_file() -> Option<PathBuf> {
    std::env::var_os("HITOP_SYNC_KEY_FILE").filter(|v| !v.is_empty()).map(PathBuf::from)
}

pub fn save_secret(server: &str, secret: &Secret) -> Result<()> {
    let text = serde_json::to_string(secret)?;
    if let Some(path) = key_file() {
        write_private(&path, &text)?;
        return Ok(());
    }
    crate::commands::secret::store()
        .set(&keychain_account(server), &text)
        .map_err(|e| anyhow::anyhow!("can't keep the sync key in the keychain ({e}); set HITOP_SYNC_KEY_FILE to a file path to keep it there instead"))
}

pub fn load_secret(server: &str) -> Result<Secret> {
    let text = if let Some(path) = key_file() {
        std::fs::read_to_string(&path).with_context(|| format!("{} (HITOP_SYNC_KEY_FILE)", path.display()))?
    } else {
        match crate::commands::secret::store().get(&keychain_account(server)) {
            Ok(Some(t)) => t,
            Ok(None) => bail!("this device isn't signed in to {server}: run `hitop sync login`"),
            Err(e) => bail!("can't read the sync key from the keychain ({e}); HITOP_SYNC_KEY_FILE names a file to use instead"),
        }
    };
    serde_json::from_str(&text).context("the stored sync key is damaged: run `hitop sync login` again")
}

pub fn delete_secret(server: &str) -> Result<()> {
    if let Some(path) = key_file() {
        if path.exists() {
            std::fs::remove_file(path)?;
        }
        return Ok(());
    }
    let _ = crate::commands::secret::store().delete(&keychain_account(server));
    Ok(())
}

fn write_private(path: &Path, text: &str) -> Result<()> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    #[cfg(unix)]
    {
        use std::io::Write;
        use std::os::unix::fs::OpenOptionsExt;
        let mut f = std::fs::OpenOptions::new().write(true).create(true).truncate(true).mode(0o600).open(path)?;
        f.write_all(text.as_bytes())?;
    }
    #[cfg(not(unix))]
    std::fs::write(path, text)?;
    Ok(())
}

/// Device keys confirmed by fingerprint: key id → public key (base64url).
pub type Pins = BTreeMap<String, String>;

pub fn load_pins() -> Result<Pins> {
    read_json(&dir()?.join("pins.json"))
}

pub fn save_pins(pins: &Pins) -> Result<()> {
    write_json(&dir()?.join("pins.json"), pins)
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Link {
    pub workspace: String,
    pub server: String,
}

fn canonical(folder: &Path) -> Result<String> {
    Ok(std::fs::canonicalize(folder).with_context(|| folder.display().to_string())?.display().to_string())
}

pub fn link_of(folder: &Path) -> Result<Option<Link>> {
    let links: BTreeMap<String, Link> = read_json(&dir()?.join("links.json"))?;
    Ok(links.get(&canonical(folder)?).cloned())
}

pub fn save_link(folder: &Path, link: Option<Link>) -> Result<()> {
    let path = dir()?.join("links.json");
    let mut links: BTreeMap<String, Link> = read_json(&path)?;
    let key = canonical(folder)?;
    match link {
        Some(l) => links.insert(key, l),
        None => links.remove(&key),
    };
    write_json(&path, &links)
}

pub fn load_state(workspace: &str, folder: &Path) -> Result<hitop_core::sync::State> {
    read_json(&dir()?.join("state").join(format!("{}.json", state_name(workspace, folder)?)))
}

pub fn save_state(workspace: &str, folder: &Path, state: &hitop_core::sync::State) -> Result<()> {
    write_json(&dir()?.join("state").join(format!("{}.json", state_name(workspace, folder)?)), state)
}

/// One state per (workspace, folder): the same workspace can be checked out twice.
fn state_name(workspace: &str, folder: &Path) -> Result<String> {
    let key = canonical(folder)?;
    let h = hitop_core::format::names::derived_id(&key);
    Ok(format!("{workspace}-{h:016x}"))
}

/// A runner credential: everything a CI job needs to pull one workspace, read-only.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RunnerCredential {
    pub v: u32,
    pub server: String,
    pub workspace: String,
    pub token: String,
    pub key_id: String,
    /// The runner's X25519 secret key, base64url.
    pub secret: String,
}

impl RunnerCredential {
    pub fn encode(&self) -> String {
        format!("hitop-runner-1.{}", URL_SAFE_NO_PAD.encode(serde_json::to_vec(self).unwrap_or_default()))
    }

    pub fn decode(text: &str) -> Result<RunnerCredential> {
        let body = text.trim().strip_prefix("hitop-runner-1.").context("HITOP_SYNC_RUNNER is not a runner credential (hitop-runner-1.…)")?;
        let bytes = URL_SAFE_NO_PAD.decode(body).context("HITOP_SYNC_RUNNER: bad encoding")?;
        serde_json::from_slice(&bytes).context("HITOP_SYNC_RUNNER: bad credential")
    }

    pub fn from_env() -> Result<Option<RunnerCredential>> {
        match std::env::var(RUNNER_ENV) {
            Ok(v) if !v.trim().is_empty() => Ok(Some(Self::decode(&v)?)),
            _ => Ok(None),
        }
    }
}
