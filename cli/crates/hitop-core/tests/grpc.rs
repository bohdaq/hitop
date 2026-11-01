//! hitop-core's gRPC client against a local tonic server
//! (tests/support/grpc_server.rs): reflection v1 and v1alpha, .proto files,
//! unary and server-streaming calls, metadata, an error status, refusals.
#![cfg(feature = "grpc")]

#[path = "support/grpc_server.rs"]
mod grpc_server;

use std::collections::HashMap;
use std::time::Duration;

use grpc_server::{fixtures, start, Reflect};
use hitop_core::grpc::{self, Event, Source, Target};
use hitop_core::model::{Auth, Grpc, Header, Request};
use serde_json::json;

const T: Option<Duration> = Some(Duration::from_secs(10));

fn target(addr: std::net::SocketAddr, method: &str, message: &str) -> Target {
    Target {
        address: addr.to_string(),
        plaintext: true,
        metadata: vec![("X-Tenant".into(), "acme".into())],
        service: "hitop.test.Greeter".into(),
        method: method.into(),
        message: message.into(),
        ..Default::default()
    }
}

async fn events(t: &Target) -> Vec<Event> {
    let channel = grpc::connect(t, T).await.unwrap();
    let (pool, _) = grpc::schema(t, &channel, T).await.unwrap();
    let mut rx = grpc::call(channel, &pool, t, T).await.unwrap();
    let mut out = vec![];
    while let Some(e) = rx.recv().await {
        out.push(e);
    }
    out
}

fn status(events: &[Event]) -> &grpc::Status {
    match events.last() {
        Some(Event::Status(s)) => s,
        other => panic!("no status at the end: {other:?}"),
    }
}

#[tokio::test]
async fn reflection_v1_lists_and_describes() {
    let addr = start(Reflect::V1);
    let t = target(addr, "", "");
    let channel = grpc::connect(&t, T).await.unwrap();
    let (pool, source) = grpc::schema(&t, &channel, T).await.unwrap();
    assert_eq!(source, Source::Reflection(grpc::Reflection::V1));
    let services = grpc::services(&pool);
    let names: Vec<&str> = services.iter().map(|s| s.name.as_str()).collect();
    assert!(names.contains(&"hitop.test.Greeter"), "{names:?}");
    let greeter = services.iter().find(|s| s.name == "hitop.test.Greeter").unwrap();
    let stream = greeter.methods.iter().find(|m| m.name == "StreamHellos").unwrap();
    assert!(stream.server_streaming && !stream.client_streaming);
    assert_eq!(stream.input, "hitop.test.HelloRequest");

    let svc = grpc::describe(&pool, "hitop.test.Greeter").unwrap();
    assert!(svc.contains("rpc StreamHellos(hitop.test.HelloRequest) returns (stream hitop.test.HelloReply);"), "{svc}");
    let msg = grpc::describe(&pool, "hitop.test.HelloRequest").unwrap();
    for line in ["repeated string labels = 5;", "map<string, string> attributes = 6;", "hitop.common.Tag tag = 4;", "oneof contact {", "hitop.test.Mood mood = 3;"] {
        assert!(msg.contains(line), "{line} in {msg}");
    }
    assert!(grpc::describe(&pool, "hitop.test.Greeter/SayHello").unwrap().contains("rpc SayHello("));
    assert!(grpc::describe(&pool, "hitop.test.Mood").unwrap().contains("HAPPY = 1;"));
    assert!(grpc::describe(&pool, "nope.Nothing").is_err());

    // The template is every field at a sample value, and parses back.
    let m = grpc::find_method(&pool, "Greeter", "SayHello").unwrap();
    let tpl = grpc::template(&m.input());
    assert_eq!(tpl, json!({"name": "", "times": 0, "mood": "MOOD_UNSPECIFIED", "tag": {"key": ""}, "labels": [""], "attributes": {}, "email": ""}));
    grpc::request_message(&m, &tpl.to_string()).unwrap();
    let reply = grpc::template(&m.output());
    assert_eq!(reply["at"], json!("1970-01-01T00:00:00Z"));
    assert_eq!(reply["count"], json!("0"));
}

#[tokio::test]
async fn unary_call_with_metadata() {
    let addr = start(Reflect::V1);
    let mut t = target(addr, "SayHello", r#"{"name": "Ann"}"#);
    t.metadata.push(("authorization".into(), "Bearer tok".into()));
    let ev = events(&t).await;
    assert_eq!(ev.len(), 3, "{ev:?}");
    let Event::Headers(h) = &ev[0] else { panic!("{ev:?}") };
    assert!(h.contains(&("x-served-by".into(), "hitop-test".into())), "{h:?}");
    assert!(h.contains(&("x-raw-bin".into(), "AAE=".into())), "{h:?}");
    let Event::Message(m) = &ev[1] else { panic!("{ev:?}") };
    assert_eq!(m["message"], "Hello, Ann");
    assert_eq!(m["tenant"], "acme");
    assert_eq!(m["authorization"], "Bearer tok");
    assert_eq!(m["at"], "2026-11-01T10:00:00Z");
    assert_eq!(m["count"], "9007199254740993", "64-bit integers stay exact");
    assert_eq!(m["index"], 0, "default fields are shown");
    assert!(status(&ev).is_ok());
}

#[tokio::test]
async fn server_streaming_call() {
    let addr = start(Reflect::V1);
    let ev = events(&target(addr, "StreamHellos", r#"{"name": "Bo", "times": 3}"#)).await;
    let messages: Vec<_> = ev.iter().filter_map(|e| if let Event::Message(m) = e { Some(m["message"].as_str().unwrap().to_string()) } else { None }).collect();
    assert_eq!(messages, ["Hello #0, Bo", "Hello #1, Bo", "Hello #2, Bo"]);
    assert!(status(&ev).is_ok());
    // No messages at all is still OK.
    let ev = events(&target(addr, "StreamHellos", "")).await;
    assert_eq!(ev.iter().filter(|e| matches!(e, Event::Message(_))).count(), 0);
    assert!(status(&ev).is_ok());
}

#[tokio::test]
async fn error_status_with_trailers() {
    let addr = start(Reflect::V1);
    let ev = events(&target(addr, "Fail", r#"{"name": "Zed"}"#)).await;
    let s = status(&ev);
    assert_eq!((s.code, s.name.as_str(), s.message.as_str()), (5, "NOT_FOUND", "no user named Zed"));
    assert!(s.trailers.contains(&("x-reason".into(), "test".into())), "{:?}", s.trailers);
    assert!(!s.trailers.iter().any(|(k, _)| k.starts_with("grpc-")), "{:?}", s.trailers);
}

#[tokio::test]
async fn v1alpha_fallback() {
    let addr = start(Reflect::V1Alpha);
    let t = target(addr, "SayHello", r#"{"name": "Al"}"#);
    let channel = grpc::connect(&t, T).await.unwrap();
    let (pool, source) = grpc::schema(&t, &channel, T).await.unwrap();
    assert_eq!(source, Source::Reflection(grpc::Reflection::V1Alpha));
    assert!(grpc::find_method(&pool, "hitop.test.Greeter", "SayHello").is_ok());
    let ev = events(&t).await;
    assert!(status(&ev).is_ok());
}

#[tokio::test]
async fn proto_files_without_reflection() {
    let addr = start(Reflect::None);
    let dir = fixtures();
    let mut t = target(addr, "SayHello", r#"{"name": "Pat", "tag": {"key": "k"}}"#);
    // Reflection isn't there: a clear error.
    let channel = grpc::connect(&t, T).await.unwrap();
    let err = grpc::schema(&t, &channel, T).await.unwrap_err();
    assert!(format!("{err:#}").contains("no reflection service"), "{err:#}");
    // The import isn't found without the import path.
    t.protos = vec![dir.join("greeter.proto")];
    let err = grpc::schema(&t, &channel, T).await.unwrap_err();
    assert!(format!("{err:#}").contains("common.proto"), "{err:#}");
    t.import_paths = vec![dir.join("include")];
    let (_, source) = grpc::schema(&t, &channel, T).await.unwrap();
    assert_eq!(source, Source::Protos);
    let ev = events(&t).await;
    let Some(Event::Message(m)) = ev.iter().find(|e| matches!(e, Event::Message(_))) else { panic!("{ev:?}") };
    assert_eq!(m["message"], "Hello, Pat");
    // A file that isn't there.
    t.protos = vec![dir.join("missing.proto")];
    assert!(format!("{:#}", grpc::schema(&t, &channel, T).await.unwrap_err()).contains("missing.proto"));
}

#[tokio::test]
async fn refusals_before_sending() {
    let addr = start(Reflect::V1);
    let t = target(addr, "Chat", "");
    let channel = grpc::connect(&t, T).await.unwrap();
    let (pool, _) = grpc::schema(&t, &channel, T).await.unwrap();
    let err = grpc::call(channel.clone(), &pool, &t, T).await.unwrap_err();
    assert!(err.to_string().contains("bidirectional streaming"), "{err}");
    let err = grpc::call(channel.clone(), &pool, &target(addr, "SayHello", r#"{"nam": 1}"#), T).await.unwrap_err();
    assert!(err.to_string().contains("isn't a valid hitop.test.HelloRequest"), "{err}");
    let err = grpc::call(channel.clone(), &pool, &target(addr, "Nope", "{}"), T).await.unwrap_err();
    assert!(err.to_string().contains("has no method 'Nope'"), "{err}");
    let mut bad = target(addr, "SayHello", "{}");
    bad.metadata = vec![("x-key-bin".into(), "not base64!".into())];
    assert!(grpc::call(channel, &pool, &bad, T).await.unwrap_err().to_string().contains("base64"));
}

#[tokio::test]
async fn refused_connection() {
    // A port nothing listens on.
    let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    let addr = listener.local_addr().unwrap();
    drop(listener);
    let err = grpc::connect(&target(addr, "SayHello", ""), Some(Duration::from_secs(2))).await.unwrap_err();
    assert!(err.to_string().contains("couldn't connect"), "{err}");
    // TLS to a plaintext server fails cleanly (rustls with ring, no panic).
    let addr = start(Reflect::V1);
    let mut tls = target(addr, "SayHello", "");
    tls.plaintext = false;
    let err = grpc::connect(&tls, Some(Duration::from_secs(5))).await.unwrap_err();
    assert!(err.to_string().contains("couldn't connect to https://"), "{err}");
}

#[tokio::test]
async fn saved_request_to_call() {
    let addr = start(Reflect::V1);
    let dir = fixtures();
    let request = Request {
        name: "Hello".into(),
        url: "${host}".into(),
        kind: Some("grpc".into()),
        headers: vec![Header::new("x-tenant", "${tenant}")],
        grpc: Some(Grpc {
            service: "hitop.test.Greeter".into(),
            method: "SayHello".into(),
            message: r#"{"name": "${who}"}"#.into(),
            protos: vec!["greeter.proto".into()],
            import_paths: vec!["include".into()],
            plaintext: true,
            ..Default::default()
        }),
        ..Default::default()
    };
    let vars: HashMap<String, String> = [("host", addr.to_string()), ("tenant", "t1".into()), ("who", "Kim".into()), ("tok", "abc".into())].into_iter().map(|(k, v)| (k.to_string(), v)).collect();
    let t = grpc::target(&request, Some(&Auth::Bearer { token: "${tok}".into() }), &vars, Some(&dir)).unwrap();
    let ev = events(&t).await;
    let Some(Event::Message(m)) = ev.iter().find(|e| matches!(e, Event::Message(_))) else { panic!("{ev:?}") };
    assert_eq!((m["message"].as_str(), m["tenant"].as_str(), m["authorization"].as_str()), (Some("Hello, Kim"), Some("t1"), Some("Bearer abc")));
}
