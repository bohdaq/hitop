//! `hitop-sync-server`: listens where rws's settings say (`RWS_CONFIG_IP`, `RWS_CONFIG_PORT`,
//! default 127.0.0.1:7881 here) behind a TLS-terminating proxy. Settings: config.rs, mail.rs.

use hitop_sync_server::{audit, Config, Db, Mailer, State, SyncApp};
use rust_web_server::server::Server;

fn main() {
    // Defaults for the listener unless the environment sets them.
    for (name, value) in [("RWS_CONFIG_IP", "127.0.0.1"), ("RWS_CONFIG_PORT", "7881")] {
        if std::env::var_os(name).is_none() {
            std::env::set_var(name, value);
        }
    }
    let fail = |e: String| -> ! {
        eprintln!("hitop-sync-server: {e}");
        std::process::exit(2)
    };
    let config = Config::from_env().unwrap_or_else(|e| fail(e));
    let mailer = Mailer::from_env().unwrap_or_else(|e| fail(e));
    if !mailer.enabled() {
        eprintln!("hitop-sync-server: SMTP_URL is not set, email sign-in is off");
    }
    let db = Db::open(&config.database).unwrap_or_else(|e| fail(e.to_string()));
    if config.oidc.is_some() {
        eprintln!("hitop-sync-server: SSO on (callback {})", config.public_url.join("v1/auth/sso/callback").map(|u| u.to_string()).unwrap_or_default());
    }
    let state = State::new(db, mailer, config).unwrap_or_else(|e| fail(e));
    // `hitop-sync-server audit-verify`: checks every audit chain in the database and exits.
    if std::env::args().nth(1).as_deref() == Some("audit-verify") {
        let chains = audit::verify_all(&state).unwrap_or_else(|e| fail(e.to_string()));
        let mut bad = 0;
        for (ws, v) in &chains {
            let name = if ws.is_empty() { "(accounts)" } else { ws.as_str() };
            match &v.problem {
                None => println!("{name}: ok, {} rows, head {} {}", v.checked, v.head_seq, v.head_hash),
                Some(p) => {
                    bad += 1;
                    println!("{name}: BROKEN: {p}");
                }
            }
        }
        println!("{} chains, {bad} broken", chains.len());
        std::process::exit(if bad == 0 { 0 } else { 1 });
    }
    let app = SyncApp::new(state);
    // The audit retention purge: at start, then hourly.
    let purger = app.clone();
    std::thread::spawn(move || loop {
        match audit::purge(purger.state()) {
            Ok(0) => {}
            Ok(n) => eprintln!("hitop-sync-server: purged {n} audit rows past retention"),
            Err(e) => eprintln!("hitop-sync-server: audit purge: {e}"),
        }
        std::thread::sleep(std::time::Duration::from_secs(3600));
    });
    let runtime = tokio::runtime::Builder::new_multi_thread().enable_all().build().expect("tokio runtime");
    runtime.block_on(async {
        let (listener, pool) = Server::setup().expect("server setup");
        Server::run_tls(listener, pool, app).await;
    });
}
