//! Private certificate authorities and the proxy (its own process: the settings are global).
use std::io::{Read, Write};
use std::net::TcpListener;
use std::process::Command;
use std::sync::{Arc, Mutex};

use base64::Engine;
use serde_json::{json, Value};

fn call(op: &str, a: Value) -> Value {
    serde_json::from_str(&hitop_ffi::call_json(op, &a.to_string())).unwrap()
}

fn get(url: &str) -> Value {
    let c = json!({ "id": 1, "name": "c", "requests": [{ "id": 2, "name": "r", "method": "GET", "url": url, "headers": [], "body": "" }] });
    call("send", json!({ "collection": c.to_string(), "request": 0 }))["ok"].clone()
}

fn openssl(dir: &std::path::Path, args: &[&str]) {
    let out = Command::new("openssl").current_dir(dir).args(args).output().expect("openssl");
    assert!(out.status.success(), "{}", String::from_utf8_lossy(&out.stderr));
}

const OK: &[u8] = b"HTTP/1.1 200 OK\r\nContent-Length: 2\r\nConnection: close\r\n\r\nok";

#[test]
fn a_private_ca_and_a_proxy_only_count_when_turned_on() {
    let dir = std::env::temp_dir().join(format!("hitop-ffi-net-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(dir.join("ext"), "subjectAltName=IP:127.0.0.1\nbasicConstraints=CA:FALSE\n").unwrap();
    openssl(&dir, &["req", "-x509", "-newkey", "ec", "-pkeyopt", "ec_paramgen_curve:prime256v1", "-nodes", "-keyout", "ca.key", "-out", "ca.pem", "-days", "2", "-subj", "/CN=Test CA"]);
    openssl(&dir, &["req", "-newkey", "ec", "-pkeyopt", "ec_paramgen_curve:prime256v1", "-nodes", "-keyout", "s.key", "-out", "s.csr", "-subj", "/CN=127.0.0.1"]);
    openssl(&dir, &["x509", "-req", "-in", "s.csr", "-CA", "ca.pem", "-CAkey", "ca.key", "-CAcreateserial", "-out", "s.pem", "-days", "2", "-extfile", "ext"]);
    openssl(&dir, &["x509", "-in", "ca.pem", "-outform", "DER", "-out", "ca.der"]);
    openssl(&dir, &["x509", "-in", "s.pem", "-outform", "DER", "-out", "s.der"]);
    openssl(&dir, &["pkcs8", "-topk8", "-nocrypt", "-in", "s.key", "-outform", "DER", "-out", "s.p8"]);
    let read = |n: &str| std::fs::read(dir.join(n)).unwrap();
    let key = rustls::pki_types::PrivatePkcs8KeyDer::from(read("s.p8"));
    let config = rustls::ServerConfig::builder_with_provider(Arc::new(rustls::crypto::ring::default_provider()))
        .with_safe_default_protocol_versions()
        .unwrap()
        .with_no_client_auth()
        .with_single_cert(vec![read("s.der").into()], key.into())
        .unwrap();
    let config = Arc::new(config);
    let l = TcpListener::bind("127.0.0.1:0").unwrap();
    let tls = format!("https://127.0.0.1:{}/", l.local_addr().unwrap().port());
    std::thread::spawn(move || {
        for s in l.incoming() {
            let (mut s, config) = (s.unwrap(), config.clone());
            std::thread::spawn(move || {
                let mut c = rustls::ServerConnection::new(config).unwrap();
                let mut t = rustls::Stream::new(&mut c, &mut s);
                let mut buf = [0u8; 4096];
                if t.read(&mut buf).is_ok() {
                    let _ = t.write_all(OK);
                    t.conn.send_close_notify();
                    let _ = t.flush();
                }
            });
        }
    });
    let ca = base64::engine::general_purpose::STANDARD.encode(read("ca.der"));
    assert!(get(&tls).get("error").is_some());
    call("setNetwork", json!({ "roots": [ca] }));
    assert_eq!(get(&tls)["response"]["status"], 200);
    // A bad certificate changes nothing.
    assert!(call("setNetwork", json!({ "roots": ["AAAA"] })).get("error").is_some());
    assert_eq!(get(&tls)["response"]["status"], 200);
    call("setNetwork", json!({}));
    assert!(get(&tls).get("error").is_some());

    let l = TcpListener::bind("127.0.0.1:0").unwrap();
    let proxy = format!("http://127.0.0.1:{}", l.local_addr().unwrap().port());
    let seen = Arc::new(Mutex::new(String::new()));
    let s2 = seen.clone();
    std::thread::spawn(move || {
        for s in l.incoming() {
            let mut s = s.unwrap();
            let mut buf = [0u8; 4096];
            let n = s.read(&mut buf).unwrap();
            s2.lock().unwrap().push_str(&String::from_utf8_lossy(&buf[..n]));
            let _ = s.write_all(OK);
        }
    });
    call("setNetwork", json!({ "proxy": proxy, "noProxy": "direct.invalid" }));
    assert_eq!(get("http://api.invalid/x")["response"]["status"], 200);
    assert!(seen.lock().unwrap().starts_with("GET http://api.invalid/x "));
    assert!(get("http://direct.invalid/x").get("error").is_some());
    call("setNetwork", json!({}));
    assert!(get("http://api.invalid/x").get("error").is_some());
    std::fs::remove_dir_all(&dir).unwrap();
}
