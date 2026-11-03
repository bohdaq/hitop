#![allow(dead_code)]
//! A DNS-over-HTTPS resolver for tests (the JSON API, `?name=&type=TXT`), answering TXT records
//! the test sets. Domain verification (orgsso.rs) asks it.

use std::collections::HashMap;
use std::io::{BufRead, BufReader, Write};
use std::net::TcpListener;
use std::sync::{Arc, Mutex};

use serde_json::json;

#[derive(Clone)]
pub struct MockDns {
    pub url: String,
    pub records: Arc<Mutex<HashMap<String, Vec<String>>>>,
    pub queries: Arc<Mutex<Vec<String>>>,
}

impl MockDns {
    pub fn start() -> MockDns {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let url = format!(
            "http://127.0.0.1:{}/dns-query",
            listener.local_addr().unwrap().port()
        );
        let dns = MockDns {
            url,
            records: Arc::default(),
            queries: Arc::default(),
        };
        let me = dns.clone();
        std::thread::spawn(move || {
            for stream in listener.incoming() {
                let Ok(mut stream) = stream else { continue };
                let mut reader = BufReader::new(stream.try_clone().unwrap());
                let mut line = String::new();
                if reader.read_line(&mut line).is_err() {
                    continue;
                }
                loop {
                    let mut h = String::new();
                    if reader.read_line(&mut h).unwrap_or(0) == 0 || h == "\r\n" {
                        break;
                    }
                }
                let path = line.split_whitespace().nth(1).unwrap_or("").to_string();
                let q: HashMap<String, String> = url::Url::parse(&format!("http://x{path}"))
                    .unwrap()
                    .query_pairs()
                    .into_owned()
                    .collect();
                let name = q.get("name").cloned().unwrap_or_default();
                me.queries.lock().unwrap().push(name.clone());
                let answers: Vec<_> = me.records.lock().unwrap().get(&name).cloned().unwrap_or_default().iter().map(|t| json!({ "name": name, "type": 16, "TTL": 60, "data": format!("\"{t}\"") })).collect();
                let body = if answers.is_empty() {
                    json!({ "Status": 3 })
                } else {
                    json!({ "Status": 0, "Answer": answers })
                }
                .to_string();
                let _ = write!(stream, "HTTP/1.1 200 OK\r\nContent-Type: application/dns-json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}", body.len());
            }
        });
        dns
    }

    pub fn set(&self, name: &str, txt: &[&str]) {
        self.records.lock().unwrap().insert(
            name.to_string(),
            txt.iter().map(|t| t.to_string()).collect(),
        );
    }
}
