//! `setNetwork {roots?: [base64 DER], proxy?, noProxy?}`: certificate authorities and a proxy the
//! person turned on in the app's settings; without both, hitop-core's defaults (webpki roots only).

use base64::Engine;
use hitop_core::http::{set_network, Network};
use serde_json::Value;

use crate::Args;

pub(crate) fn set(a: &Args) -> Result<Value, String> {
    let b64: Vec<String> = a.opt_json("roots")?.unwrap_or_default();
    let roots = b64.iter().map(|r| base64::engine::general_purpose::STANDARD.decode(r).map_err(|e| e.to_string())).collect::<Result<Vec<_>, _>>()?;
    // A certificate this TLS library can't read is left out; only when none is left is it an error.
    let given = roots.len();
    let mut problem = String::new();
    let roots: Vec<Vec<u8>> = roots
        .into_iter()
        .filter(|r| {
            let one = Network { roots: vec![r.clone()], ..Default::default() };
            let ok = one.apply(reqwest::blocking::Client::builder().use_rustls_tls()).and_then(|b| Ok(b.build()?));
            if let Err(e) = &ok {
                problem = format!("{e:#}");
            }
            ok.is_ok()
        })
        .collect();
    if given > 0 && roots.is_empty() {
        return Err(format!("no usable certificate: {problem}"));
    }
    let n = Network { roots, proxy: a.opt_json("proxy")?, no_proxy: a.opt_json("noProxy")?.unwrap_or_default() };
    let n = (n != Network::default()).then_some(n);
    set_network(n).map_err(|e| format!("{e:#}"))?;
    Ok(Value::Bool(true))
}
