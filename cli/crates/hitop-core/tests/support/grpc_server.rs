//! A local gRPC server for hitop's tests (hitop-core's tests/grpc.rs and the
//! CLI's tests/grpc.rs, which includes this file by path): the
//! `hitop.test.Greeter` service of tests/fixtures/grpc/greeter.proto on
//! dynamic messages, with server reflection v1, v1alpha or none.
#![allow(dead_code)]

use std::convert::Infallible;
use std::net::SocketAddr;
use std::path::PathBuf;
use std::task::{Context, Poll};

use hitop_core::grpc::{load_protos, DynamicCodec};
use prost_reflect::{DescriptorPool, DynamicMessage, MessageDescriptor};
use serde_json::{json, Value};
use tonic::codegen::{http, Body, BoxFuture, Service, StdError};
use tonic::metadata::MetadataValue;
use tonic::server::NamedService;
use tonic::{Request, Response, Status};

/// tests/fixtures/grpc in hitop-core (greeter.proto, include/common.proto).
pub fn fixtures() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../hitop-core/tests/fixtures/grpc")
}

/// The schema the server serves.
pub fn pool() -> DescriptorPool {
    let dir = fixtures();
    load_protos(&[dir.join("greeter.proto")], &[dir.join("include")]).expect("the test protos compile")
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Reflect {
    V1,
    V1Alpha,
    None,
}

#[derive(Clone)]
struct Greeter {
    pool: DescriptorPool,
}

impl NamedService for Greeter {
    const NAME: &'static str = "hitop.test.Greeter";
}

fn reply(desc: MessageDescriptor, v: Value) -> DynamicMessage {
    DynamicMessage::deserialize(desc, v).expect("a valid reply")
}

fn text(msg: &DynamicMessage, field: &str) -> String {
    msg.get_field_by_name(field).and_then(|v| v.as_str().map(String::from)).unwrap_or_default()
}

fn meta(req: &Request<DynamicMessage>, name: &str) -> String {
    req.metadata().get(name).and_then(|v| v.to_str().ok()).unwrap_or_default().to_string()
}

impl<B> Service<http::Request<B>> for Greeter
where
    B: Body + Send + 'static,
    B::Error: Into<StdError> + Send + 'static,
{
    type Response = http::Response<tonic::body::Body>;
    type Error = Infallible;
    type Future = BoxFuture<Self::Response, Self::Error>;

    fn poll_ready(&mut self, _cx: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
        Poll::Ready(Ok(()))
    }

    fn call(&mut self, req: http::Request<B>) -> Self::Future {
        let pool = self.pool.clone();
        Box::pin(async move {
            let name = req.uri().path().rsplit('/').next().unwrap_or_default().to_string();
            let service = pool.get_service_by_name(Greeter::NAME).unwrap();
            let Some(method) = service.methods().find(|m| m.name() == name) else {
                return Ok(Status::unimplemented(format!("no method {name}")).into_http());
            };
            let out = method.output();
            let mut grpc = tonic::server::Grpc::new(DynamicCodec::new(method.input()));
            Ok(match name.as_str() {
                "SayHello" => {
                    let svc = tower::service_fn(move |r: Request<DynamicMessage>| {
                        let out = out.clone();
                        async move {
                            let msg = r.get_ref();
                            let body = json!({
                                "message": format!("Hello, {}", text(msg, "name")),
                                "tenant": meta(&r, "x-tenant"),
                                "authorization": meta(&r, "authorization"),
                                "at": "2026-11-01T10:00:00Z",
                                "count": "9007199254740993",
                            });
                            let mut resp = Response::new(reply(out, body));
                            resp.metadata_mut().insert("x-served-by", MetadataValue::from_static("hitop-test"));
                            resp.metadata_mut().insert_bin("x-raw-bin", MetadataValue::from_bytes(b"\x00\x01"));
                            Ok::<_, Status>(resp)
                        }
                    });
                    grpc.unary(svc, req).await
                }
                "StreamHellos" => {
                    let svc = tower::service_fn(move |r: Request<DynamicMessage>| {
                        let out = out.clone();
                        async move {
                            let msg = r.into_inner();
                            let times = msg.get_field_by_name("times").and_then(|v| v.as_i32()).unwrap_or(0);
                            let who = text(&msg, "name");
                            let items: Vec<Result<DynamicMessage, Status>> =
                                (0..times).map(|i| Ok(reply(out.clone(), json!({ "message": format!("Hello #{i}, {who}"), "index": i })))).collect();
                            Ok::<_, Status>(Response::new(futures_util::stream::iter(items)))
                        }
                    });
                    grpc.server_streaming(svc, req).await
                }
                "Fail" => {
                    let svc = tower::service_fn(move |r: Request<DynamicMessage>| async move {
                        let mut md = tonic::metadata::MetadataMap::new();
                        md.insert("x-reason", MetadataValue::from_static("test"));
                        Err::<Response<DynamicMessage>, _>(Status::with_metadata(tonic::Code::NotFound, format!("no user named {}", text(r.get_ref(), "name")), md))
                    });
                    grpc.unary(svc, req).await
                }
                other => Status::unimplemented(format!("{other} is not served")).into_http(),
            })
        })
    }
}

/// Starts the server on 127.0.0.1 in a background thread (its own tokio
/// runtime, so sync and async tests can use it) and returns its address.
pub fn start(reflect: Reflect) -> SocketAddr {
    let (tx, rx) = std::sync::mpsc::channel();
    std::thread::spawn(move || {
        let rt = tokio::runtime::Builder::new_multi_thread().worker_threads(2).enable_all().build().unwrap();
        rt.block_on(async move {
            let pool = pool();
            let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
            tx.send(listener.local_addr().unwrap()).unwrap();
            let encoded: &'static [u8] = Box::leak(pool.encode_to_vec().into_boxed_slice());
            let builder = || tonic_reflection::server::Builder::configure().register_encoded_file_descriptor_set(encoded);
            let mut router = tonic::transport::Server::builder().add_service(Greeter { pool: pool.clone() });
            router = match reflect {
                Reflect::V1 => router.add_service(builder().build_v1().unwrap()),
                Reflect::V1Alpha => router.add_service(builder().build_v1alpha().unwrap()),
                Reflect::None => router,
            };
            router.serve_with_incoming(tonic::transport::server::TcpIncoming::from(listener)).await.unwrap();
        });
    });
    rx.recv().unwrap()
}
