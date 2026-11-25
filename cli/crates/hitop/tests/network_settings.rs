//! The desktop's network settings in this process (they are global state, so one test): blocked
//! while unusable, the proxy used for requests and not for the app's own (`plain`) ones.
use std::io::{Read, Write};
use std::net::TcpListener;

use hitop_core::http::{blocked, send_with, Options};
use hitop_core::network_settings::NetworkSettings;

fn serve(answer: &'static str) -> String {
    let l = TcpListener::bind("127.0.0.1:0").unwrap();
    let url = format!("http://127.0.0.1:{}", l.local_addr().unwrap().port());
    std::thread::spawn(move || {
        for s in l.incoming() {
            let mut s = s.unwrap();
            let _ = s.read(&mut [0u8; 4096]);
            let _ = write!(s, "HTTP/1.1 200 OK\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{answer}", answer.len());
        }
    });
    url
}

#[test]
fn blocked_until_usable_and_plain_is_untouched() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("network.json");
    let (direct, proxy) = (serve("direct"), serve("proxied"));
    let get = |plain: bool| send_with("GET", &direct, &[], vec![], Options { plain, ..Default::default() }).map(|r| String::from_utf8(r.body).unwrap());
    NetworkSettings { ca_file: String::new(), proxy: proxy.clone(), ..Default::default() }.save(&path).unwrap();
    assert_eq!(get(false).unwrap(), "proxied");
    assert_eq!(get(true).unwrap(), "direct");

    // A start with a certificate file that is gone: nothing goes out around the proxy.
    std::fs::write(&path, format!(r#"{{"caFile":"/nowhere.pem","proxy":"{proxy}"}}"#)).unwrap();
    NetworkSettings::start(&path);
    assert!(blocked().unwrap().contains("/nowhere.pem"));
    assert!(get(false).unwrap_err().to_string().contains("/nowhere.pem"));
    assert_eq!(get(true).unwrap(), "direct");
    let rt = tokio::runtime::Runtime::new().unwrap();
    let ws = hitop_core::websocket::Target { url: direct.replace("http", "ws"), ..Default::default() };
    assert!(rt.block_on(hitop_core::websocket::connect(&ws, None)).err().unwrap().to_string().contains("/nowhere.pem"));

    NetworkSettings { proxy, ..Default::default() }.save(&path).unwrap();
    assert_eq!(get(false).unwrap(), "proxied");
    NetworkSettings::default().save(&path).unwrap();
    assert_eq!((get(false).unwrap().as_str(), std::fs::read_to_string(&path).unwrap().as_str()), ("direct", "{}"));
}
