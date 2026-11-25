//! `--cacert`, `--cert`/`--key`, `--proxy` and `--insecure` against local servers (certificates made
//! with openssl for the test).
use std::io::{Read, Write};
use std::net::TcpListener;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};
use std::sync::{Arc, Mutex};

use rustls::pki_types::pem::PemObject;
use rustls::pki_types::{CertificateDer, PrivateKeyDer};

fn openssl(dir: &Path, args: &[&str]) {
    let out = Command::new("openssl").current_dir(dir).args(args).output().expect("openssl");
    assert!(out.status.success(), "{}", String::from_utf8_lossy(&out.stderr));
}

struct Env {
    dir: PathBuf,
}

impl Env {
    fn new(name: &str) -> Env {
        let dir = std::env::temp_dir().join(format!("hitop-tls-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let ec = ["-pkeyopt", "ec_paramgen_curve:prime256v1", "-nodes"];
        std::fs::write(dir.join("ext"), "subjectAltName=IP:127.0.0.1,DNS:localhost\nbasicConstraints=CA:FALSE\n").unwrap();
        openssl(&dir, &[&["req", "-x509", "-newkey", "ec", "-keyout", "ca.key", "-out", "ca.pem", "-days", "2", "-subj", "/CN=Test CA"], &ec[..]].concat());
        for who in ["server", "client"] {
            let (key, csr, pem) = (format!("{who}.key"), format!("{who}.csr"), format!("{who}.pem"));
            openssl(&dir, &[&["req", "-newkey", "ec", "-keyout", &key, "-out", &csr, "-subj", "/CN=test"], &ec[..]].concat());
            openssl(&dir, &["x509", "-req", "-in", &csr, "-CA", "ca.pem", "-CAkey", "ca.key", "-CAcreateserial", "-out", &pem, "-days", "2", "-extfile", "ext"]);
        }
        Env { dir }
    }

    fn file(&self, n: &str) -> String {
        self.dir.join(n).to_string_lossy().into_owned()
    }

    fn hitop(&self, args: &[&str]) -> Output {
        Command::new(env!("CARGO_BIN_EXE_hitop"))
            .args(args)
            .env("HOME", &self.dir)
            .env("XDG_CONFIG_HOME", self.dir.join("config"))
            .env("HITOP_KEYCHAIN", "off")
            .env("NO_COLOR", "1")
            .output()
            .unwrap()
    }

    /// A TLS server; with `mtls` it wants a client certificate of the test CA.
    fn serve(&self, mtls: bool) -> String {
        let provider = Arc::new(rustls::crypto::ring::default_provider());
        let chain = vec![CertificateDer::from_pem_file(self.dir.join("server.pem")).unwrap()];
        let key = PrivateKeyDer::from_pem_file(self.dir.join("server.key")).unwrap();
        let b = rustls::ServerConfig::builder_with_provider(provider.clone()).with_safe_default_protocol_versions().unwrap();
        let config = if mtls {
            let mut roots = rustls::RootCertStore::empty();
            roots.add(CertificateDer::from_pem_file(self.dir.join("ca.pem")).unwrap()).unwrap();
            b.with_client_cert_verifier(rustls::server::WebPkiClientVerifier::builder_with_provider(Arc::new(roots), provider).build().unwrap())
        } else {
            b.with_no_client_auth()
        };
        let config = Arc::new(config.with_single_cert(chain, key).unwrap());
        let l = TcpListener::bind("127.0.0.1:0").unwrap();
        let url = format!("https://127.0.0.1:{}/", l.local_addr().unwrap().port());
        std::thread::spawn(move || {
            for s in l.incoming() {
                let (mut s, config) = (s.unwrap(), config.clone());
                std::thread::spawn(move || {
                    let mut c = rustls::ServerConnection::new(config).unwrap();
                    let mut t = rustls::Stream::new(&mut c, &mut s);
                    if t.read(&mut [0u8; 4096]).is_ok() {
                        let _ = t.write_all(b"HTTP/1.1 200 OK\r\nContent-Length: 6\r\nConnection: close\r\n\r\nsecure");
                        t.conn.send_close_notify();
                        let _ = t.flush();
                    }
                });
            }
        });
        url
    }
}

impl Drop for Env {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.dir);
    }
}

fn ok(o: &Output) -> bool {
    o.status.success() && String::from_utf8_lossy(&o.stdout).contains("secure")
}

#[test]
fn a_private_ca_is_trusted_only_when_named() {
    let e = Env::new("ca");
    let url = e.serve(false);
    assert!(!ok(&e.hitop(&["send", &url])));
    let o = e.hitop(&["send", &url, "--cacert", &e.file("ca.pem")]);
    assert!(ok(&o), "{}{}", String::from_utf8_lossy(&o.stdout), String::from_utf8_lossy(&o.stderr));
    // Not a certificate: an error that names the file.
    let o = e.hitop(&["send", &url, "--cacert", &e.file("ext")]);
    assert!(!o.status.success() && String::from_utf8_lossy(&o.stderr).contains("ext"));
}

#[test]
fn insecure_warns_and_is_for_one_command() {
    let e = Env::new("insecure");
    let url = e.serve(false);
    let o = e.hitop(&["send", &url, "-k"]);
    assert!(ok(&o), "{}", String::from_utf8_lossy(&o.stderr));
    assert!(String::from_utf8_lossy(&o.stderr).contains("accepts any certificate"));
    assert!(!ok(&e.hitop(&["send", &url])));
    let o = e.hitop(&["grpc", &url.replace("https://", "").replace('/', ""), "list", "-k"]);
    assert!(String::from_utf8_lossy(&o.stderr).contains("can't skip certificate verification"), "{}", String::from_utf8_lossy(&o.stderr));
}

#[test]
fn a_client_certificate_is_shown_to_a_server_that_asks() {
    let e = Env::new("mtls");
    let url = e.serve(true);
    let ca = e.file("ca.pem");
    assert!(!ok(&e.hitop(&["send", &url, "--cacert", &ca])));
    let o = e.hitop(&["send", &url, "--cacert", &ca, "--cert", &e.file("client.pem"), "--key", &e.file("client.key")]);
    assert!(ok(&o), "{}{}", String::from_utf8_lossy(&o.stdout), String::from_utf8_lossy(&o.stderr));
    // One file with both, also for a collection run.
    let both = [std::fs::read(e.file("client.pem")).unwrap(), std::fs::read(e.file("client.key")).unwrap()].concat();
    std::fs::write(e.file("both.pem"), both).unwrap();
    let folder = e.file("api");
    assert!(e.hitop(&["init", &folder]).status.success());
    for f in std::fs::read_dir(format!("{folder}/requests")).unwrap() {
        std::fs::remove_file(f.unwrap().path()).unwrap();
    }
    std::fs::write(format!("{folder}/requests/a.yaml"), format!("name: A\nmethod: GET\nurl: {url}\n")).unwrap();
    let run = |more: &[&str]| e.hitop(&[&["run", &folder, "--cacert", &ca], more].concat()).status.code();
    assert_eq!(run(&[]), Some(1));
    assert_eq!(run(&["--cert", &e.file("both.pem")]), Some(0));
    assert_eq!(run(&["--cert", &e.file("nowhere.pem")]), Some(2));
    // Nothing of it is in the folder.
    let text: String = walk(Path::new(&folder));
    assert!(!text.contains("cacert") && !text.contains("client.pem") && !text.contains("insecure"));
}

fn walk(dir: &Path) -> String {
    std::fs::read_dir(dir).unwrap().map(|f| f.unwrap().path()).map(|p| if p.is_dir() { walk(&p) } else { std::fs::read_to_string(&p).unwrap_or_default() }).collect()
}

#[test]
fn a_proxy_gets_the_requests_but_not_those_of_noproxy() {
    let e = Env::new("proxy");
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
            let _ = s.write_all(b"HTTP/1.1 200 OK\r\nContent-Length: 6\r\nConnection: close\r\n\r\nsecure");
        }
    });
    assert!(ok(&e.hitop(&["send", "http://api.invalid/x", "--proxy", &proxy])));
    assert!(seen.lock().unwrap().starts_with("GET http://api.invalid/x "));
    assert!(!ok(&e.hitop(&["send", "http://direct.invalid/x", "--proxy", &proxy, "--noproxy", "direct.invalid"])));
}
