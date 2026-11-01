//! `hitop-sync-server`: listens where rws's settings say (`RWS_CONFIG_IP`, `RWS_CONFIG_PORT`,
//! default 127.0.0.1:7881 here) behind a TLS-terminating proxy. Settings: config.rs, mail.rs.

use hitop_sync_server::{Config, Db, Mailer, State, SyncApp};
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
    let app = SyncApp::new(State::new(db, mailer, config).unwrap_or_else(|e| fail(e)));
    let runtime = tokio::runtime::Builder::new_multi_thread().enable_all().build().expect("tokio runtime");
    runtime.block_on(async {
        let (listener, pool) = Server::setup().expect("server setup");
        Server::run_tls(listener, pool, app).await;
    });
}
