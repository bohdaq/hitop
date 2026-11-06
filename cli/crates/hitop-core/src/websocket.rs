//! WebSocket requests: a request with `kind: websocket` opens a connection
//! instead of sending an HTTP request. Its URL, headers and auth go into the
//! opening handshake; `websocket` holds the subprotocols offered and saved
//! messages. [`target`] and [`message`] fill in `${variables}` (no native
//! dependencies, so hitop-wasm builds them too); the client ([`connect`],
//! feature `websocket`) runs on tokio-tungstenite over rustls, for the CLI's
//! `hitop ws` and the desktop app.
use std::collections::HashMap;

use anyhow::{anyhow, bail, Result};

use crate::{
    auth,
    interpolation::interpolate,
    model::{Auth, Request, WEBSOCKET},
};

impl Request {
    /// True when this request opens a WebSocket connection.
    pub fn is_websocket(&self) -> bool {
        self.kind.as_deref() == Some(WEBSOCKET)
    }
}

/// A connection to open, everything filled in.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Target {
    /// A `ws://` or `wss://` URL.
    pub url: String,
    /// Sent with the handshake, in order.
    pub headers: Vec<(String, String)>,
    /// Offered in `Sec-WebSocket-Protocol`, in order.
    pub subprotocols: Vec<String>,
    /// Connect to this address instead of resolving the URL's host (the host is still used for
    /// TLS and `Host`): hitop-server's bridge connects only to an address it has checked.
    pub connect_to: Option<std::net::SocketAddr>,
}

/// `ws://` and `wss://` URLs as given; `http://` and `https://` become
/// `ws://` and `wss://`. Anything else is an error.
pub fn ws_url(url: &str) -> Result<String> {
    let lower = url.to_ascii_lowercase();
    for (from, to) in [("ws://", "ws://"), ("wss://", "wss://"), ("http://", "ws://"), ("https://", "wss://")] {
        if lower.starts_with(from) {
            return Ok(format!("{to}{}", &url[from.len()..]));
        }
    }
    bail!("'{url}' is not a WebSocket URL: use ws:// or wss://")
}

/// The connection a WebSocket request opens: `${variables}` filled in
/// (URL, enabled headers, auth, subprotocols) and the auth applied, the
/// collection's `default_auth` when the request has none of its own.
pub fn target(request: &Request, default_auth: Option<&Auth>, variables: &HashMap<String, String>) -> Result<Target> {
    let fill = |s: &str| interpolate(s, variables);
    let url = ws_url(&fill(&request.url)?)?;
    let headers = request
        .headers
        .iter()
        .filter(|h| h.enabled && !h.name.is_empty())
        .map(|h| Ok((fill(&h.name)?, fill(&h.value)?)))
        .collect::<Result<Vec<_>>>()?;
    let (url, headers) = auth::apply(request.auth.as_ref().or(default_auth), url, headers, &fill)?;
    let subprotocols = match &request.websocket {
        Some(w) => w.subprotocols.iter().map(|p| fill(p)).filter(|p| !matches!(p, Ok(p) if p.is_empty())).collect::<Result<_>>()?,
        None => vec![],
    };
    Ok(Target { url, headers, subprotocols, connect_to: None })
}

/// The text of the saved message named `name`, `${variables}` filled in.
pub fn message(request: &Request, name: &str, variables: &HashMap<String, String>) -> Result<String> {
    let saved = request.websocket.as_ref().map(|w| w.messages.as_slice()).unwrap_or_default();
    let m = saved.iter().find(|m| m.name == name).ok_or_else(|| {
        let names: Vec<&str> = saved.iter().map(|m| m.name.as_str()).collect();
        if names.is_empty() {
            anyhow!("request '{}' has no saved messages", request.name)
        } else {
            anyhow!("request '{}' has no message '{name}' (saved: {})", request.name, names.join(", "))
        }
    })?;
    interpolate(&m.content, variables)
}

#[cfg(feature = "websocket")]
pub use client::*;

#[cfg(feature = "websocket")]
mod client {
    use std::sync::Arc;
    use std::time::Duration;

    use anyhow::{anyhow, Context, Result};
    use futures_util::{SinkExt, StreamExt};
    use tokio::sync::mpsc;
    use tokio_tungstenite::tungstenite::{
        self,
        client::IntoClientRequest,
        http::{HeaderName, HeaderValue},
        protocol::{frame::coding::CloseCode, CloseFrame},
        Message,
    };
    use tokio_tungstenite::Connector;

    use super::Target;

    /// A message's payload.
    #[derive(Debug, Clone, PartialEq, Eq)]
    pub enum Data {
        Text(String),
        Binary(Vec<u8>),
    }

    /// What happens on an open connection.
    #[derive(Debug, Clone, PartialEq, Eq)]
    pub enum Event {
        /// A message from the server.
        Message(Data),
        /// The connection is closed (by either side); the last event.
        Closed { code: Option<u16>, reason: String },
        /// The connection failed; the last event.
        Error(String),
    }

    /// The server's answer to the handshake.
    #[derive(Debug, Clone, PartialEq, Eq)]
    pub struct Opened {
        pub status: u16,
        /// The subprotocol the server picked.
        pub protocol: Option<String>,
        pub headers: Vec<(String, String)>,
    }

    enum Command {
        Send(Data),
        Close,
    }

    /// Sends on a connection; cheap to clone (the desktop keeps one per
    /// connection while events go elsewhere).
    #[derive(Debug, Clone)]
    pub struct Sender(mpsc::UnboundedSender<Command>);

    impl std::fmt::Debug for Command {
        fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
            f.write_str(match self {
                Command::Send(_) => "Send",
                Command::Close => "Close",
            })
        }
    }

    impl Sender {
        /// Queues a message; an error once the connection is closed.
        pub fn send(&self, data: Data) -> Result<()> {
            self.0.send(Command::Send(data)).map_err(|_| anyhow!("the connection is closed"))
        }

        /// Starts the closing handshake (code 1000); `Event::Closed` follows.
        pub fn close(&self) {
            let _ = self.0.send(Command::Close);
        }
    }

    /// An open connection. Dropping every [`Sender`] closes it.
    #[derive(Debug)]
    pub struct Connection {
        pub opened: Opened,
        pub sender: Sender,
        pub events: mpsc::UnboundedReceiver<Event>,
    }

    /// How long a closing handshake may take before the socket is dropped.
    const CLOSE_WAIT: Duration = Duration::from_secs(5);

    fn tls() -> Result<Arc<rustls::ClientConfig>> {
        let mut roots = rustls::RootCertStore::empty();
        roots.extend(webpki_roots::TLS_SERVER_ROOTS.iter().cloned());
        let provider = Arc::new(rustls::crypto::ring::default_provider());
        let config = rustls::ClientConfig::builder_with_provider(provider)
            .with_safe_default_protocol_versions()
            .context("TLS setup")?
            .with_root_certificates(roots)
            .with_no_client_auth();
        Ok(Arc::new(config))
    }

    fn handshake_error(e: tungstenite::Error) -> anyhow::Error {
        match e {
            tungstenite::Error::Http(resp) => {
                let status = resp.status();
                let body = resp.body().as_deref().map(String::from_utf8_lossy).unwrap_or_default();
                let reason = status.canonical_reason().map(|r| format!(" {r}")).unwrap_or_default();
                let body = body.trim();
                if body.is_empty() {
                    anyhow!("the server answered {}{reason} instead of switching to WebSocket", status.as_u16())
                } else {
                    anyhow!("the server answered {}{reason} instead of switching to WebSocket: {}", status.as_u16(), body.chars().take(200).collect::<String>())
                }
            }
            other => anyhow!(other),
        }
    }

    /// Opens the connection (must run inside a tokio runtime): the handshake
    /// within `timeout`, then a task that reads and writes until either side
    /// closes. Events arrive on `Connection::events`.
    pub async fn connect(target: &Target, timeout: Option<Duration>) -> Result<Connection> {
        let mut request = target.url.as_str().into_client_request().with_context(|| format!("bad WebSocket URL '{}'", target.url))?;
        let headers = request.headers_mut();
        for (name, value) in &target.headers {
            let name = HeaderName::from_bytes(name.as_bytes()).with_context(|| format!("bad header name '{name}'"))?;
            let value = HeaderValue::from_str(value).with_context(|| format!("bad value for header '{name}'"))?;
            headers.append(name, value);
        }
        if !target.subprotocols.is_empty() && !headers.contains_key("sec-websocket-protocol") {
            let joined = target.subprotocols.join(", ");
            headers.insert("sec-websocket-protocol", HeaderValue::from_str(&joined).with_context(|| format!("bad subprotocols '{joined}'"))?);
        }
        let connector = target.url.to_ascii_lowercase().starts_with("wss://").then(|| tls().map(Connector::Rustls)).transpose()?;
        let to = target.connect_to;
        let handshake = async move {
            match to {
                Some(addr) => {
                    let tcp = tokio::net::TcpStream::connect(addr).await.map_err(tungstenite::Error::Io)?;
                    tokio_tungstenite::client_async_tls_with_config(request, tcp, None, connector).await
                }
                None => tokio_tungstenite::connect_async_tls_with_config(request, None, false, connector).await,
            }
        };
        let (stream, response) = match timeout {
            Some(t) => tokio::time::timeout(t, handshake).await.map_err(|_| anyhow!("no WebSocket handshake within {} ms", t.as_millis()))?,
            None => handshake.await,
        }
        .map_err(handshake_error)
        .with_context(|| format!("couldn't connect to {}", target.url))?;

        let opened = Opened {
            status: response.status().as_u16(),
            protocol: response.headers().get("sec-websocket-protocol").and_then(|v| v.to_str().ok()).map(String::from),
            headers: response.headers().iter().map(|(k, v)| (k.to_string(), String::from_utf8_lossy(v.as_bytes()).into_owned())).collect(),
        };
        let (commands, mut inbox) = mpsc::unbounded_channel::<Command>();
        let (events_tx, events) = mpsc::unbounded_channel();
        tokio::spawn(async move {
            // The loop borrows both channel ends, so they drop in a fixed
            // order after it: the command inbox first (a send after the
            // close is an error), then the events (a reader sees `None`).
            // Left to the task's captures, the order was a race.
            run_socket(stream, &mut inbox, &events_tx).await;
            drop(inbox);
            drop(events_tx);
        });
        Ok(Connection { opened, sender: Sender(commands), events })
    }

    async fn run_socket(
        stream: tokio_tungstenite::WebSocketStream<tokio_tungstenite::MaybeTlsStream<tokio::net::TcpStream>>,
        inbox: &mut mpsc::UnboundedReceiver<Command>,
        events_tx: &mpsc::UnboundedSender<Event>,
    ) {
        {
            let (mut sink, mut source) = stream.split();
            let emit = |e: Event| {
                let _ = events_tx.send(e);
            };
            let mut closing = false;
            loop {
                tokio::select! {
                    cmd = inbox.recv(), if !closing => match cmd {
                        Some(Command::Send(data)) => {
                            let msg = match data {
                                Data::Text(t) => Message::text(t),
                                Data::Binary(b) => Message::binary(b),
                            };
                            if let Err(e) = sink.send(msg).await {
                                emit(Event::Error(format!("couldn't send: {e}")));
                                return;
                            }
                        }
                        // Close asked for, or every sender dropped.
                        Some(Command::Close) | None => {
                            closing = true;
                            let frame = CloseFrame { code: CloseCode::Normal, reason: "".into() };
                            if sink.send(Message::Close(Some(frame))).await.is_err() {
                                emit(Event::Closed { code: Some(1000), reason: String::new() });
                                return;
                            }
                        }
                    },
                    msg = source.next() => match msg {
                        Some(Ok(Message::Text(t))) => emit(Event::Message(Data::Text(t.as_str().to_string()))),
                        Some(Ok(Message::Binary(b))) => emit(Event::Message(Data::Binary(b.to_vec()))),
                        Some(Ok(Message::Close(frame))) => {
                            let (code, reason) = frame.map(|f| (Some(u16::from(f.code)), f.reason.as_str().to_string())).unwrap_or((None, String::new()));
                            // tungstenite answers a server's close itself; let it go out.
                            let _ = tokio::time::timeout(CLOSE_WAIT, sink.flush()).await;
                            emit(Event::Closed { code, reason });
                            return;
                        }
                        Some(Ok(_)) => {} // ping, pong (answered by tungstenite), raw frames
                        Some(Err(tungstenite::Error::ConnectionClosed | tungstenite::Error::AlreadyClosed)) | None => {
                            emit(Event::Closed { code: None, reason: String::new() });
                            return;
                        }
                        Some(Err(e)) => {
                            emit(Event::Error(e.to_string()));
                            return;
                        }
                    },
                    _ = tokio::time::sleep(CLOSE_WAIT), if closing => {
                        emit(Event::Closed { code: Some(1000), reason: "no close answer from the server".into() });
                        return;
                    }
                }
            }
        }
    }

    #[cfg(test)]
    #[allow(clippy::result_large_err)] // tungstenite's handshake callback type
    mod tests {
        use super::super::{message, target};
        use super::*;
        use crate::model::{ApiKeyLocation, Auth, Header, Request, WebSocket, WsMessage};
        use std::collections::HashMap;
        use std::sync::Mutex;
        use tokio_tungstenite::tungstenite::handshake::server::{Request as HsRequest, Response as HsResponse};

        /// An echo server on a free port: echoes text and binary, closes
        /// with 4000 "bye" on the text `close`, and records each
        /// handshake's headers. Picks the first subprotocol offered.
        async fn echo_server() -> (String, Arc<Mutex<Vec<Vec<(String, String)>>>>) {
            let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
            let addr = listener.local_addr().unwrap();
            let seen = Arc::new(Mutex::new(Vec::new()));
            let log = seen.clone();
            tokio::spawn(async move {
                while let Ok((tcp, _)) = listener.accept().await {
                    let log = log.clone();
                    tokio::spawn(async move {
                        let callback = |req: &HsRequest, mut resp: HsResponse| {
                            let headers = req.headers().iter().map(|(k, v)| (k.to_string(), v.to_str().unwrap_or("").to_string())).collect();
                            log.lock().unwrap().push(headers);
                            if let Some(p) = req.headers().get("sec-websocket-protocol").and_then(|v| v.to_str().ok()) {
                                let first = p.split(',').next().unwrap().trim().to_string();
                                resp.headers_mut().insert("sec-websocket-protocol", HeaderValue::from_str(&first).unwrap());
                            }
                            Ok(resp)
                        };
                        let Ok(mut ws) = tokio_tungstenite::accept_hdr_async(tcp, callback).await else { return };
                        while let Some(Ok(msg)) = ws.next().await {
                            match msg {
                                Message::Text(t) if t.as_str() == "close" => {
                                    let _ = ws.close(Some(CloseFrame { code: CloseCode::from(4000), reason: "bye".into() })).await;
                                }
                                Message::Text(_) | Message::Binary(_) => {
                                    let _ = ws.send(msg).await;
                                }
                                _ => {}
                            }
                        }
                    });
                }
            });
            (format!("ws://{addr}"), seen)
        }

        fn socket_request(url: &str) -> Request {
            Request {
                name: "Echo".into(),
                url: url.into(),
                method: "GET".into(),
                headers: vec![Header::new("X-Who", "${who}")],
                kind: Some("websocket".into()),
                websocket: Some(WebSocket {
                    subprotocols: vec!["${proto}".into(), "chat.v1".into()],
                    messages: vec![WsMessage { name: "hello".into(), content: "hello ${who}".into(), ..Default::default() }],
                    ..Default::default()
                }),
                ..Default::default()
            }
        }

        #[tokio::test(flavor = "multi_thread")]
        async fn echoes_text_and_binary_with_headers_auth_and_subprotocols() {
            let (base, seen) = echo_server().await;
            let vars: HashMap<String, String> = [("who".to_string(), "ada".to_string()), ("proto".to_string(), "chat.v2".to_string()), ("tok".to_string(), "t0k".to_string())].into();
            let req = socket_request(&base.replace("ws://", "http://"));
            let t = target(&req, Some(&Auth::Bearer { token: "${tok}".into() }), &vars).unwrap();
            assert!(t.url.starts_with("ws://127.0.0.1:"), "{}", t.url);
            let mut conn = connect(&t, Some(Duration::from_secs(5))).await.unwrap();
            assert_eq!((conn.opened.status, conn.opened.protocol.as_deref()), (101, Some("chat.v2")));
            let headers = seen.lock().unwrap()[0].clone();
            let get = |n: &str| headers.iter().find(|(k, _)| k == n).map(|(_, v)| v.clone());
            assert_eq!(get("x-who").as_deref(), Some("ada"));
            assert_eq!(get("authorization").as_deref(), Some("Bearer t0k"));
            assert_eq!(get("sec-websocket-protocol").as_deref(), Some("chat.v2, chat.v1"));

            conn.sender.send(Data::Text(message(&req, "hello", &vars).unwrap())).unwrap();
            assert_eq!(conn.events.recv().await, Some(Event::Message(Data::Text("hello ada".into()))));
            conn.sender.send(Data::Binary(vec![0, 159, 255])).unwrap();
            assert_eq!(conn.events.recv().await, Some(Event::Message(Data::Binary(vec![0, 159, 255]))));
            // The server closes: code and reason come through, then nothing.
            conn.sender.send(Data::Text("close".into())).unwrap();
            assert_eq!(conn.events.recv().await, Some(Event::Closed { code: Some(4000), reason: "bye".into() }));
            assert_eq!(conn.events.recv().await, None);
            assert!(conn.sender.send(Data::Text("late".into())).is_err());
        }

        #[tokio::test(flavor = "multi_thread")]
        async fn client_close_and_refused_connections() {
            let (base, _) = echo_server().await;
            let req = Request { url: base, kind: Some("websocket".into()), ..Default::default() };
            let mut conn = connect(&target(&req, None, &HashMap::new()).unwrap(), None).await.unwrap();
            assert_eq!(conn.opened.protocol, None);
            conn.sender.close();
            assert_eq!(conn.events.recv().await, Some(Event::Closed { code: Some(1000), reason: String::new() }));

            // Port 9 refuses the connection.
            let refused = Target { url: "ws://127.0.0.1:9/".into(), ..Default::default() };
            let err = connect(&refused, Some(Duration::from_secs(5))).await.unwrap_err();
            assert!(format!("{err:#}").contains("couldn't connect to ws://127.0.0.1:9/"), "{err:#}");
        }

        #[test]
        fn targets_fill_in_variables_and_auth() {
            let mut req = socket_request("${base}/feed?x=1");
            req.auth = Some(Auth::ApiKey { key: "key".into(), value: "${tok}".into(), location: ApiKeyLocation::Query });
            let vars: HashMap<String, String> = [("base".to_string(), "https://h".to_string()), ("who".to_string(), "b".to_string()), ("proto".to_string(), "".to_string()), ("tok".to_string(), "k 1".to_string())].into();
            let t = target(&req, None, &vars).unwrap();
            assert_eq!(t.url, "wss://h/feed?x=1&key=k%201");
            assert_eq!(t.headers, vec![("X-Who".to_string(), "b".to_string())]);
            // A subprotocol that fills in empty isn't offered.
            assert_eq!(t.subprotocols, ["chat.v1"]);
            assert!(target(&Request { url: "ftp://x".into(), ..Default::default() }, None, &vars).unwrap_err().to_string().contains("not a WebSocket URL"));
            assert_eq!(target(&req, None, &HashMap::new()).unwrap_err().to_string(), "variable 'base' is not defined");
            assert!(message(&req, "nope", &vars).unwrap_err().to_string().contains("saved: hello"));
            assert!(req.is_websocket() && !Request::default().is_websocket());
        }
    }
}
