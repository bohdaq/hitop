//! hitop-server: serves the hitop.rws8.tech site from its working directory
//! and runs the web app's requests for it (`POST /api/proxy`), so browser
//! CORS rules don't apply.
//!
//! Runs behind the front rws, which terminates TLS and rate limits /api/*.
//! Settings (environment):
//! - `HITOP_ALLOWED_ORIGINS`: comma separated Origins allowed to call the proxy and the bridges
//!   (default: the web app, the Chrome Web Store extension and `moz-extension://*`, see origins.rs)
//! - `HITOP_DENY_IPS`: extra addresses to refuse, e.g. the host's own public IP
//! - `HITOP_TELEMETRY_DIR`, `HITOP_STATS_TOKEN`: opt-in usage pings (`POST /api/ping`,
//!   `GET /api/stats`), see telemetry.rs
//! - rws's own settings (`RWS_CONFIG_IP`, `RWS_CONFIG_PORT`, ...) for the listener

mod grpcbridge;
mod guard;
mod legacy;
mod origins;
mod proxy;
mod telemetry;
mod wsbridge;

use rust_web_server::prelude::*;
use rust_web_server::state::AppWithState;

use proxy::Proxy;
use telemetry::Telemetry;

#[derive(Clone)]
pub struct State {
    pub proxy: Proxy,
    pub telemetry: Telemetry,
    pub ws: wsbridge::Bridge,
    pub grpc: grpcbridge::GrpcBridge,
}

pub fn app(state: State) -> AppWithState<State> {
    App::with_state(state)
        .post("/api/proxy", |request, _, _, state| state.proxy.handle(request))
        .post("/api/proxy/stream", |request, _, _, state| state.proxy.stream(request))
        // WebSocket requests with handshake headers (wsbridge.rs).
        .get("/api/ws", |request, _, _, state| state.ws.handle(request))
        // gRPC calls with server reflection (grpcbridge.rs).
        .post("/api/grpc/list", |request, _, _, state| state.grpc.list(request))
        .post("/api/grpc/describe", |request, _, _, state| state.grpc.describe(request))
        .post("/api/grpc/call", |request, _, _, state| state.grpc.call(request))
        .post("/api/ping", |request, _, _, state| state.telemetry.ping(request))
        .get("/api/stats", |request, _, _, state| state.telemetry.stats(request))
        // The guides moved from /terms/<name>.html to /guides/<name>/ (legacy.rs).
        .get("/terms", |request, _, _, _| legacy::redirect(request))
        .get("/terms/*rest", |request, _, _, _| legacy::redirect(request))
}

fn main() {
    // The blocking client owns a runtime of its own: build it before tokio's.
    let state = State { proxy: Proxy::from_env(), telemetry: Telemetry::from_env(), ws: wsbridge::Bridge::from_env(), grpc: grpcbridge::GrpcBridge::from_env() };
    let runtime = tokio::runtime::Builder::new_multi_thread().enable_all().build().expect("tokio runtime");
    runtime.block_on(async {
        let (listener, pool) = Server::setup().expect("server setup");
        Server::run_tls(listener, pool, app(state)).await;
    });
}

#[cfg(test)]
mod tests {
    use super::*;
    use rust_web_server::test_client::TestClient;

    #[test]
    fn the_stream_endpoint_wants_the_web_apps_origin() {
        let client = TestClient::new(app(State { proxy: Proxy::from_env(), telemetry: Telemetry::from_env(), ws: wsbridge::Bridge::from_env(), grpc: grpcbridge::GrpcBridge::from_env() }));
        let res = client.post("/api/proxy/stream").body_json(r#"{"method":"GET","url":"https://example.com/"}"#).send();
        assert_eq!(res.status(), 403);
        let res = client.post("/api/proxy/stream").header("Origin", "https://hitop.rws8.tech").body_json(r#"{"method":"GET","url":"http://127.0.0.1/"}"#).send();
        assert_eq!(res.status(), 403, "a private address is refused as by /api/proxy");
    }

    #[test]
    fn old_guide_urls_redirect() {
        let client = TestClient::new(app(State { proxy: Proxy::from_env(), telemetry: Telemetry::from_env(), ws: wsbridge::Bridge::from_env(), grpc: grpcbridge::GrpcBridge::from_env() }));
        for (path, location) in [("/terms", "/guides/"), ("/terms/", "/guides/"), ("/terms/postman-alternative.html", "/guides/postman-alternative/")] {
            let res = client.get(path).send();
            assert_eq!(res.status(), 301, "{path}");
            assert_eq!(res.header("Location"), Some(location), "{path}");
        }
    }
}
