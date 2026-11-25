//! The desktop app's network settings (`<config>/hitop/network.json`): extra certificate
//! authorities, a client certificate and a proxy for requests. Paths only: no certificate or key is
//! copied, and there is no setting that skips verification.
use std::path::{Path, PathBuf};

use anyhow::{bail, Context, Result};
use serde::{Deserialize, Serialize};

use crate::http::{block_network, pem_certificates, set_network, Network};

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct NetworkSettings {
    /// A PEM file of certificate authorities to trust beside the built-in ones.
    #[serde(skip_serializing_if = "String::is_empty")]
    pub ca_file: String,
    /// A PEM client certificate (it may hold its key too).
    #[serde(skip_serializing_if = "String::is_empty")]
    pub cert_file: String,
    #[serde(skip_serializing_if = "String::is_empty")]
    pub key_file: String,
    /// `http://host:port`, for HTTP requests.
    #[serde(skip_serializing_if = "String::is_empty")]
    pub proxy: String,
    /// Hosts that don't use the proxy, comma separated.
    #[serde(skip_serializing_if = "String::is_empty")]
    pub no_proxy: String,
}

impl NetworkSettings {
    pub fn path() -> Result<PathBuf> {
        Ok(crate::storage::config_dir()?.join("network.json"))
    }

    pub fn load(path: &Path) -> Result<Self> {
        match std::fs::read_to_string(path) {
            Ok(t) => serde_json::from_str(&t).with_context(|| format!("{} can't be read", path.display())),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(Self::default()),
            Err(e) => Err(e).with_context(|| format!("{} can't be read", path.display())),
        }
    }

    pub fn trimmed(&self) -> Self {
        let t = |s: &str| s.trim().to_string();
        NetworkSettings { ca_file: t(&self.ca_file), cert_file: t(&self.cert_file), key_file: t(&self.key_file), proxy: t(&self.proxy), no_proxy: t(&self.no_proxy) }
    }

    /// The settings with their files read.
    pub fn network(&self) -> Result<Network> {
        let read = |p: &str| std::fs::read(p).with_context(|| format!("{p} can't be read"));
        let mut n = Network { no_proxy: self.no_proxy.clone(), ..Default::default() };
        if !self.proxy.is_empty() {
            let u = reqwest::Url::parse(&self.proxy).ok().filter(|u| matches!(u.scheme(), "http" | "https") && u.has_host());
            let Some(u) = u else { bail!("the proxy must be like http://host:port") };
            if !u.username().is_empty() || u.password().is_some() {
                bail!("the proxy address can't hold a password: it would be stored in a file");
            }
            n.proxy = Some(self.proxy.clone());
        }
        if !self.ca_file.is_empty() {
            n.roots = pem_certificates(&read(&self.ca_file)?).with_context(|| self.ca_file.clone())?;
        }
        if !self.cert_file.is_empty() {
            let mut pem = read(&self.cert_file)?;
            if !self.key_file.is_empty() {
                pem.push(b'\n');
                pem.extend(read(&self.key_file)?);
            }
            n.identity = Some(pem);
        } else if !self.key_file.is_empty() {
            bail!("a key needs its certificate");
        }
        Ok(n)
    }

    /// Uses these settings for requests from now on.
    pub fn apply(&self) -> Result<()> {
        let n = self.network()?;
        set_network((n != Network::default()).then_some(n))
    }

    /// Uses the settings saved at `path`. When they can't be used, requests answer with the reason
    /// (never going out around a proxy) until [`save`](Self::save) succeeds.
    pub fn start(path: &Path) {
        if let Err(e) = Self::load(path).and_then(|s| s.apply()) {
            block_network(format!("the network settings can't be used, see Settings: {e:#}"));
        }
    }

    /// Checks and uses the settings, then writes them; nothing changes on an error.
    pub fn save(&self, path: &Path) -> Result<()> {
        let s = self.trimmed();
        let before = crate::http::network();
        s.apply()?;
        if let Err(e) = crate::storage::write_json::<Self>(path, &serde_json::to_string_pretty(&s)?) {
            let _ = set_network(before);
            return Err(e);
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn checked_before_saving() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("network.json");
        assert_eq!(NetworkSettings::load(&path).unwrap(), NetworkSettings::default());
        let bad = |s: NetworkSettings, why: &str| {
            let e = format!("{:#}", s.save(&path).unwrap_err());
            assert!(e.contains(why), "{e}");
            assert!(!path.exists());
        };
        bad(NetworkSettings { proxy: "http://ann:pw@p:1".into(), ..Default::default() }, "password");
        bad(NetworkSettings { proxy: "ftp://p".into(), ..Default::default() }, "http://host:port");
        bad(NetworkSettings { ca_file: "/nowhere.pem".into(), ..Default::default() }, "/nowhere.pem");
        bad(NetworkSettings { key_file: "k".into(), ..Default::default() }, "certificate");
        std::fs::write(&path, "{nope").unwrap();
        assert!(NetworkSettings::load(&path).is_err());
    }
}
