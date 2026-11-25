//! Certificate authorities the person added in the app's settings, for hitop-core's own connections
//! (gRPC, OAuth token requests), beside the built-in ones.
//!
//! - `certificates {data}` (base64url, a PEM or DER file) → each certificate's DER, checked;
//! - `networkRoots {roots: [DER]}`: the authorities from now on (none: the defaults).
use hitop_core::http::{pem_certificates, set_network, Network};
use serde_json::{json, Value};

use crate::{b64, unb64, Args, R};

fn set(roots: Vec<Vec<u8>>) -> Result<(), String> {
    let n = Network { roots, ..Default::default() };
    set_network((n != Network::default()).then_some(n)).map_err(|e| format!("{e:#}"))
}

pub(crate) fn call(op: &str, a: &Args) -> R {
    match op {
        "certificates" => {
            let data = a.bytes("data")?;
            let is_pem = data.windows(10).any(|w| w == b"-----BEGIN");
            let found = if is_pem { pem_certificates(&data).map_err(|e| format!("{e:#}"))? } else { vec![data] };
            for der in &found {
                hitop_core::http::check_root(der).map_err(|e| format!("not a usable certificate: {e:#}"))?;
            }
            Ok(json!(found.iter().map(|d| b64(d)).collect::<Vec<_>>()))
        }
        _ => {
            let roots: Vec<String> = a.json("roots")?;
            set(roots.iter().map(|r| unb64("roots", r)).collect::<Result<_, _>>()?)?;
            Ok(Value::Bool(true))
        }
    }
}
