//! `--cacert`, `--cert`/`--key`, `--proxy`/`--noproxy` and `--insecure` of the commands that send.
//! They hold for one command and are never stored in a collection.
use std::path::{Path, PathBuf};

use anyhow::{Context, Result};
use clap::Args;
use hitop_core::http::{pem_certificates, set_network, Network};

#[derive(Args, Clone, Default)]
pub struct NetworkFlags {
    /// Also trust the certificate authorities in this PEM file (repeatable)
    #[arg(long, value_name = "FILE", help_heading = "Network")]
    cacert: Vec<PathBuf>,

    /// Client certificate (PEM; may hold the key too)
    #[arg(long, value_name = "FILE", help_heading = "Network")]
    cert: Option<PathBuf>,

    /// The client certificate's key (PEM), when --cert doesn't hold it
    #[arg(long, value_name = "FILE", requires = "cert", help_heading = "Network")]
    key: Option<PathBuf>,

    /// Proxy for HTTP requests (http://host:port; WebSocket and gRPC connect directly)
    #[arg(long, value_name = "URL", help_heading = "Network")]
    proxy: Option<String>,

    /// Hosts that don't use --proxy (comma separated)
    #[arg(long, value_name = "HOSTS", requires = "proxy", help_heading = "Network")]
    noproxy: Option<String>,

    /// Accept any server certificate: anyone on the way can read and change the traffic
    #[arg(short = 'k', long, help_heading = "Network")]
    insecure: bool,
}

fn read(path: &Path) -> Result<Vec<u8>> {
    std::fs::read(path).with_context(|| format!("reading {}", path.display()))
}

impl NetworkFlags {
    /// Sets the network for this process. Call before the first request.
    pub fn apply(&self) -> Result<()> {
        let mut n = Network { proxy: self.proxy.clone(), no_proxy: self.noproxy.clone().unwrap_or_default(), insecure: self.insecure, ..Default::default() };
        for p in &self.cacert {
            n.roots.extend(pem_certificates(&read(p)?).with_context(|| format!("--cacert {}", p.display()))?);
        }
        if let Some(cert) = &self.cert {
            let mut pem = read(cert)?;
            if let Some(key) = &self.key {
                pem.push(b'\n');
                pem.extend(read(key)?);
            }
            n.identity = Some(pem);
        }
        if n == Network::default() {
            return Ok(());
        }
        if n.insecure {
            eprintln!("warning: --insecure accepts any certificate; anyone on the way can read and change the traffic");
        }
        set_network(Some(n))
    }
}
