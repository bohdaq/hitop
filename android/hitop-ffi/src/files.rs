//! Files of multipart rows. The app keeps what the person picked in its private storage and hands
//! over, per send, `files: {token: {path, name, type, request}}`; a row names its file by the token
//! (`fileToken`). Only that map is read, only for the request it was picked for: a collection
//! (imported, synced, shared) can't name a path.

use std::cell::RefCell;
use std::collections::HashMap;

use hitop_core::form::FileData;
use hitop_core::model::{FormField, Request};
use serde::Deserialize;

use crate::Args;

#[derive(Debug, Clone, Deserialize)]
pub(crate) struct Entry {
    path: String,
    name: String,
    #[serde(rename = "type")]
    content_type: String,
    request: String,
}

pub(crate) type Map = HashMap<String, Entry>;

thread_local! {
    static FILES: RefCell<Map> = RefCell::new(HashMap::new());
}

fn source(request: &Request, field: &FormField) -> anyhow::Result<Option<FileData>> {
    let Some(token) = field.extra.get("fileToken").and_then(|v| v.as_str()) else { return Ok(None) };
    let entry = FILES.with(|f| f.borrow().get(token).filter(|e| e.request == request.id.to_string()).cloned());
    let Some(e) = entry else {
        let name = field.extra.get("fileName").and_then(|v| v.as_str()).unwrap_or("the file");
        anyhow::bail!("{name} isn't on this phone: pick it again for the field {:?}", field.key);
    };
    let data = std::fs::read(&e.path).map_err(|err| anyhow::anyhow!("{}: {err}", e.name))?;
    Ok(Some(FileData { filename: e.name, content_type: e.content_type, data }))
}

pub(crate) fn map(a: &Args) -> Result<Map, String> {
    Ok(a.opt_json("files")?.unwrap_or_default())
}

/// This thread's files until it is dropped.
pub(crate) struct Scope;

impl Scope {
    pub(crate) fn new(m: Map) -> Scope {
        hitop_core::form::install_file_source(source);
        FILES.with(|f| *f.borrow_mut() = m);
        Scope
    }
    pub(crate) fn of(a: &Args) -> Result<Scope, String> {
        Ok(Scope::new(map(a)?))
    }
}

impl Drop for Scope {
    fn drop(&mut self) {
        FILES.with(|f| f.borrow_mut().clear());
    }
}

#[cfg(test)]
mod tests {
    use crate::call_json;
    use serde_json::{json, Value};
    use std::io::{Read, Write};

    fn send(request: &str) -> (Value, Vec<u8>) {
        let dir = std::env::temp_dir().join(format!("hitop-ffi-files-{}-{request}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("f");
        std::fs::write(&path, [0u8, 255, 10, 13, 10, 200]).unwrap();
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let port = listener.local_addr().unwrap().port();
        let (tx, rx) = std::sync::mpsc::channel();
        std::thread::spawn(move || {
            let Ok((mut s, _)) = listener.accept() else { return };
            let mut all = vec![];
            let mut buf = [0u8; 4096];
            // The closing boundary ends the request.
            while !all.ends_with(b"--\r\n") {
                let n = s.read(&mut buf).unwrap();
                if n == 0 {
                    break;
                }
                all.extend_from_slice(&buf[..n]);
            }
            s.write_all(b"HTTP/1.1 200 OK\r\nContent-Length: 0\r\nConnection: close\r\n\r\n").unwrap();
            let _ = tx.send(all);
        });
        let c = json!({ "id": 1, "name": "c", "requests": [{ "id": 7, "name": "up", "method": "POST", "url": format!("http://127.0.0.1:{port}/"), "headers": [], "body": "",
            "bodyMode": "multipart", "formFields": [{ "key": "doc", "value": "", "type": "file", "fileName": "a.bin", "fileToken": "tok" }, { "key": "t", "value": "v", "type": "text" }] }] });
        let args = json!({ "collection": c.to_string(), "request": 0, "files": { "tok": { "path": path, "name": "a.bin", "type": "image/png", "request": request } } });
        let out: Value = serde_json::from_str(&call_json("send", &args.to_string())).unwrap();
        let wire = if out["ok"].get("error").is_some() { vec![] } else { rx.recv().unwrap() };
        std::fs::remove_dir_all(&dir).unwrap();
        (out, wire)
    }

    #[test]
    fn a_picked_file_goes_out_byte_for_byte() {
        let (out, wire) = send("7");
        assert_eq!(out["ok"]["response"]["status"], 200, "{out}");
        let needle = b"name=\"doc\"; filename=\"a.bin\"\r\nContent-Type: image/png\r\n\r\n\x00\xff\n\r\n\xc8\r\n";
        assert!(wire.windows(needle.len()).any(|w| w == needle), "{}", String::from_utf8_lossy(&wire));
    }

    #[test]
    fn a_file_picked_for_another_request_is_not_sent() {
        let (out, _) = send("8");
        assert!(out["ok"]["error"].as_str().unwrap().contains("isn't on this phone"), "{out}");
    }
}
