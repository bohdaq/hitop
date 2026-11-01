//! The collection folder reader (spec/collection-format.md) on untrusted files.
//!
//! Input: `\0`-separated chunks. The first is `hitop.yaml`; every other one is
//! `<path inside the folder>\n<file text>`. Reading must never panic; a folder
//! that reads is written back and read again, which must also succeed.
#![no_main]

use hitop_core::format::files::{read_files, to_files, Files};
use libfuzzer_sys::fuzz_target;

fuzz_target!(|data: &[u8]| {
    let Ok(text) = std::str::from_utf8(data) else { return };
    let mut chunks = text.split('\0');
    let mut files = Files::new();
    files.insert("hitop.yaml".into(), chunks.next().unwrap_or_default().to_string());
    for chunk in chunks {
        let (path, body) = chunk.split_once('\n').unwrap_or((chunk, ""));
        files.insert(path.to_string(), body.to_string());
    }
    let Ok(folder) = read_files(&files, "fuzz") else { return };
    let written = to_files(&folder).expect("a folder that reads can be written");
    read_files(&written, "fuzz").expect("a written folder reads back");
});
