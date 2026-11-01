//! hitop-server: serves the hitop.rws8.tech site from its working directory
//! and runs the web app's requests for it (`POST /api/proxy`), so browser
//! CORS rules don't apply.
//!
//! Runs behind the front rws, which terminates TLS and rate limits /api/*.
//! Settings (environment):
//! - `HITOP_ALLOWED_ORIGINS`: comma separated Origins allowed to call the
//!   proxy (default `https://hitop.rws8.tech`)
//! - `HITOP_DENY_IPS`: extra addresses to refuse, e.g. the host's own public IP
//! - `HITOP_TELEMETRY_DIR`, `HITOP_STATS_TOKEN`: opt-in usage pings (`POST /api/ping`,
//!   `GET /api/stats`), see telemetry.rs
//! - rws's own settings (`RWS_CONFIG_IP`, `RWS_CONFIG_PORT`, ...) for the listener

mod guard;
mod proxy;
mod telemetry;

use rust_web_server::prelude::*;
use rust_web_server::state::AppWithState;

use proxy::Proxy;
use telemetry::Telemetry;

#[derive(Clone)]
pub struct State {
    pub proxy: Proxy,
    pub telemetry: Telemetry,
}

pub fn app(state: State) -> AppWithState<State> {
    App::with_state(state)
        .post("/api/proxy", |request, _, _, state| state.proxy.handle(request))
        .post("/api/ping", |request, _, _, state| state.telemetry.ping(request))
        .get("/api/stats", |request, _, _, state| state.telemetry.stats(request))
}

fn main() {
    // The blocking client owns a runtime of its own: build it before tokio's.
    let state = State { proxy: Proxy::from_env(), telemetry: Telemetry::from_env() };
    let runtime = tokio::runtime::Builder::new_multi_thread().enable_all().build().expect("tokio runtime");
    runtime.block_on(async {
        let (listener, pool) = Server::setup().expect("server setup");
        Server::run_tls(listener, pool, app(state)).await;
    });
}
