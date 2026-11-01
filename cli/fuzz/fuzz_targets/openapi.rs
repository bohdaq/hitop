//! The OpenAPI 3.0/3.1 importer (JSON or YAML, local `$ref`s, schema examples) and
//! sync against its own import, on untrusted specs.
#![no_main]

use hitop_core::openapi::{self, Options};
use libfuzzer_sys::fuzz_target;

fuzz_target!(|data: &[u8]| {
    // The first byte picks the options, the rest is the spec.
    let Some((&flags, rest)) = data.split_first() else { return };
    let Ok(text) = std::str::from_utf8(rest) else { return };
    let opts = Options { flat: flags & 1 != 0, inline_auth: flags & 2 != 0, source: (flags & 4 != 0).then(|| "https://api.example.test/v1/spec.json".to_string()) };
    let _ = openapi::detect(text);
    if let Ok((collection, _)) = openapi::import(text, &opts) {
        let _ = openapi::sync(&collection, text, &opts, flags & 8 != 0);
    }
});
