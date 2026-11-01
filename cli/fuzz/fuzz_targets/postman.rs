//! The Postman importer (collections v2.0/v2.1, environments, globals) on untrusted JSON.
#![no_main]

use hitop_core::postman;
use libfuzzer_sys::fuzz_target;

fuzz_target!(|data: &[u8]| {
    let Ok(text) = std::str::from_utf8(data) else { return };
    if let Ok(v) = serde_json::from_str::<serde_json::Value>(text) {
        let _ = postman::detect(&v);
    }
    let _ = postman::import_collection(text);
    let _ = postman::import_environment(text);
});
