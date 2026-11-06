//! gRPC requests: a request with `kind: grpc` calls a method on a gRPC
//! server. Its URL is the server address, its enabled headers and auth are
//! sent as metadata, and `grpc` holds the service, method, the request
//! message as JSON text and where the schema comes from (`.proto` files, or
//! server reflection when none are given). [`target`] fills in
//! `${variables}` (no native dependencies, so hitop-wasm builds it too); the
//! client (feature `grpc`: [`connect`], [`schema`], [`call`]) runs on tonic
//! over rustls with dynamic messages from prost-reflect, for the CLI's
//! `hitop grpc` and the desktop app. `.proto` files are parsed at run time by
//! protox, so no `protoc` is needed.
use std::collections::HashMap;
use std::path::{Path, PathBuf};

use anyhow::{bail, Result};

use crate::{
    auth,
    interpolation::interpolate,
    model::{Auth, Request, GRPC},
};

impl Request {
    /// True when this request makes a gRPC call.
    pub fn is_grpc(&self) -> bool {
        self.kind.as_deref() == Some(GRPC)
    }
}

/// A call to make, everything filled in.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Target {
    /// `host:port`, `http://host:port` or `https://host:port`.
    pub address: String,
    /// No TLS for a `host:port` address.
    pub plaintext: bool,
    /// Sent with the call, in order (names as given; HTTP/2 lowercases them).
    pub metadata: Vec<(String, String)>,
    /// `package.Service`.
    pub service: String,
    pub method: String,
    /// The request message, JSON text (empty means `{}`).
    pub message: String,
    /// `.proto` files; none means server reflection.
    pub protos: Vec<PathBuf>,
    /// Where imports are looked up, besides each file's own directory.
    pub import_paths: Vec<PathBuf>,
    /// Connect to this address instead of resolving the host (which is still used for TLS and
    /// `:authority`): hitop-server's gRPC bridge connects only to an address it has checked.
    pub connect_to: Option<std::net::SocketAddr>,
}

/// The URI tonic connects to and whether it uses TLS: `http://` is
/// plaintext, `https://` TLS, and a bare `host:port` TLS unless `plaintext`.
pub fn endpoint(address: &str, plaintext: bool) -> Result<(String, bool)> {
    let address = address.trim().trim_end_matches('/');
    let lower = address.to_ascii_lowercase();
    if lower.starts_with("http://") {
        return Ok((address.to_string(), false));
    }
    if lower.starts_with("https://") {
        return Ok((address.to_string(), true));
    }
    if address.is_empty() {
        bail!("no gRPC server address: give host:port");
    }
    if address.contains("://") {
        bail!("'{address}' is not a gRPC address: use host:port, http://host:port or https://host:port");
    }
    Ok((format!("{}://{address}", if plaintext { "http" } else { "https" }), !plaintext))
}

/// The call a gRPC request makes: `${variables}` filled in (address, enabled
/// headers, auth, service, method, message) and the auth applied as
/// metadata, the collection's `default_auth` when the request has none of
/// its own. Relative `.proto` paths and import paths are taken from `base`
/// (the collection folder) when given.
pub fn target(request: &Request, default_auth: Option<&Auth>, variables: &HashMap<String, String>, base: Option<&Path>) -> Result<Target> {
    let fill = |s: &str| interpolate(s, variables);
    let address = fill(&request.url)?;
    let metadata = request
        .headers
        .iter()
        .filter(|h| h.enabled && !h.name.is_empty())
        .map(|h| Ok((fill(&h.name)?, fill(&h.value)?)))
        .collect::<Result<Vec<_>>>()?;
    // An API key `in: query` changes only the URL, which gRPC doesn't have.
    let (_, metadata) = auth::apply(request.auth.as_ref().or(default_auth), String::new(), metadata, &fill)?;
    let g = request.grpc.clone().unwrap_or_default();
    let path = |p: &String| -> Result<PathBuf> {
        let p = PathBuf::from(fill(p)?);
        Ok(match base {
            Some(b) if p.is_relative() => b.join(p),
            _ => p,
        })
    };
    Ok(Target {
        address,
        plaintext: g.plaintext,
        metadata,
        service: fill(&g.service)?,
        method: fill(&g.method)?,
        message: fill(&g.message)?,
        protos: g.protos.iter().map(path).collect::<Result<_>>()?,
        connect_to: None,
        import_paths: g.import_paths.iter().map(path).collect::<Result<_>>()?,
    })
}

/// `package.Service/Method`, `package.Service.Method` or `/package.Service/Method`
/// as (service, method).
pub fn split_method(name: &str) -> Option<(String, String)> {
    let name = name.trim().trim_start_matches('/');
    let (service, method) = name.rsplit_once('/').or_else(|| name.rsplit_once('.'))?;
    (!service.is_empty() && !method.is_empty()).then(|| (service.to_string(), method.to_string()))
}

/// The name of a gRPC status code (`NOT_FOUND`), as grpcurl and the gRPC docs write it.
pub fn code_name(code: i32) -> &'static str {
    match code {
        0 => "OK",
        1 => "CANCELLED",
        2 => "UNKNOWN",
        3 => "INVALID_ARGUMENT",
        4 => "DEADLINE_EXCEEDED",
        5 => "NOT_FOUND",
        6 => "ALREADY_EXISTS",
        7 => "PERMISSION_DENIED",
        8 => "RESOURCE_EXHAUSTED",
        9 => "FAILED_PRECONDITION",
        10 => "ABORTED",
        11 => "OUT_OF_RANGE",
        12 => "UNIMPLEMENTED",
        13 => "INTERNAL",
        14 => "UNAVAILABLE",
        15 => "DATA_LOSS",
        16 => "UNAUTHENTICATED",
        _ => "UNKNOWN",
    }
}

#[cfg(feature = "grpc")]
pub use client::*;

#[cfg(feature = "grpc")]
mod client {
    use std::collections::{BTreeMap, HashSet};
    use std::path::{Path, PathBuf};
    use std::time::Duration;

    use anyhow::{anyhow, bail, Context, Result};
    use base64::Engine;
    use prost::Message as _;
    use prost_reflect::{
        Cardinality, DynamicMessage, EnumDescriptor, FieldDescriptor, Kind, MessageDescriptor, MethodDescriptor, SerializeOptions, ServiceDescriptor,
    };
    use serde_json::{json, Map, Value};
    use tokio::sync::mpsc;
    use tonic::codec::{Codec, DecodeBuf, Decoder, EncodeBuf, Encoder};
    use tonic::metadata::{AsciiMetadataKey, BinaryMetadataKey, BinaryMetadataValue, KeyAndValueRef, MetadataMap, MetadataValue};
    use tonic::transport::{ClientTlsConfig, Endpoint};

    use super::{code_name, endpoint, Target};

    /// The schema type, for callers that keep one (the desktop between calls).
    pub use prost_reflect::DescriptorPool;
    /// A connection to a server, from [`connect`].
    pub use tonic::transport::Channel;

    const B64: base64::engine::GeneralPurpose = base64::engine::general_purpose::STANDARD;

    // ── Codec ────────────────────────────────────────────────────────────

    /// A tonic codec for dynamic messages: encodes any [`DynamicMessage`],
    /// decodes messages of one type. Public so test servers can use it too.
    #[derive(Debug, Clone)]
    pub struct DynamicCodec(MessageDescriptor);

    impl DynamicCodec {
        /// Decodes messages of type `decode` (the response type for a client).
        pub fn new(decode: MessageDescriptor) -> Self {
            Self(decode)
        }
    }

    #[derive(Debug)]
    pub struct DynamicEncoder;

    #[derive(Debug)]
    pub struct DynamicDecoder(MessageDescriptor);

    impl Codec for DynamicCodec {
        type Encode = DynamicMessage;
        type Decode = DynamicMessage;
        type Encoder = DynamicEncoder;
        type Decoder = DynamicDecoder;

        fn encoder(&mut self) -> Self::Encoder {
            DynamicEncoder
        }

        fn decoder(&mut self) -> Self::Decoder {
            DynamicDecoder(self.0.clone())
        }
    }

    impl Encoder for DynamicEncoder {
        type Item = DynamicMessage;
        type Error = tonic::Status;

        fn encode(&mut self, item: DynamicMessage, dst: &mut EncodeBuf<'_>) -> Result<(), tonic::Status> {
            item.encode(dst).map_err(|e| tonic::Status::internal(format!("couldn't encode the message: {e}")))
        }
    }

    impl Decoder for DynamicDecoder {
        type Item = DynamicMessage;
        type Error = tonic::Status;

        fn decode(&mut self, src: &mut DecodeBuf<'_>) -> Result<Option<DynamicMessage>, tonic::Status> {
            DynamicMessage::decode(self.0.clone(), src).map(Some).map_err(|e| tonic::Status::internal(format!("couldn't decode the message: {e}")))
        }
    }

    // ── Connection ───────────────────────────────────────────────────────

    /// Connects to the target's server (must run inside a tokio runtime),
    /// within `timeout`. TLS uses the webpki roots, as reqwest does.
    pub async fn connect(target: &Target, timeout: Option<Duration>) -> Result<Channel> {
        let (uri, tls) = endpoint(&target.address, target.plaintext)?;
        let parsed: tonic::codegen::http::Uri = uri.parse().with_context(|| format!("bad gRPC address '{}'", target.address))?;
        let host = parsed.host().unwrap_or_default().trim_matches(['[', ']']).to_string();
        let dial = match target.connect_to {
            Some(addr) => format!("{}://{addr}", if tls { "https" } else { "http" }),
            None => uri.clone(),
        };
        let mut ep = Endpoint::from_shared(dial).with_context(|| format!("bad gRPC address '{}'", target.address))?;
        if target.connect_to.is_some() {
            ep = ep.origin(parsed.clone());
        }
        if tls {
            ep = ep.tls_config(ClientTlsConfig::new().with_webpki_roots().domain_name(host)).context("TLS setup")?;
        }
        if let Some(t) = timeout {
            ep = ep.connect_timeout(t);
        }
        ep.connect().await.map_err(|e| anyhow!("couldn't connect to {uri}: {}", error_chain(&e)))
    }

    fn error_chain(e: &(dyn std::error::Error + 'static)) -> String {
        let mut out = e.to_string();
        let mut source = e.source();
        while let Some(s) = source {
            let s_text = s.to_string();
            if !out.contains(&s_text) {
                out.push_str(": ");
                out.push_str(&s_text);
            }
            source = s.source();
        }
        out
    }

    fn metadata_map(pairs: &[(String, String)]) -> Result<MetadataMap> {
        let mut map = MetadataMap::new();
        for (name, value) in pairs {
            let lower = name.trim().to_ascii_lowercase();
            if lower.ends_with("-bin") {
                let key = BinaryMetadataKey::from_bytes(lower.as_bytes()).with_context(|| format!("bad metadata name '{name}'"))?;
                let bytes = B64.decode(value.trim()).or_else(|_| base64::engine::general_purpose::STANDARD_NO_PAD.decode(value.trim())).with_context(|| format!("metadata '{name}' ends in -bin, so its value must be base64"))?;
                map.append_bin(key, BinaryMetadataValue::from_bytes(&bytes));
            } else {
                let key = AsciiMetadataKey::from_bytes(lower.as_bytes()).with_context(|| format!("bad metadata name '{name}'"))?;
                let value = MetadataValue::try_from(value.as_str()).with_context(|| format!("bad value for metadata '{name}'"))?;
                map.append(key, value);
            }
        }
        Ok(map)
    }

    /// Metadata as (name, value) pairs; `-bin` values as base64.
    pub fn metadata_pairs(map: &MetadataMap) -> Vec<(String, String)> {
        map.iter()
            .map(|kv| match kv {
                KeyAndValueRef::Ascii(k, v) => (k.to_string(), v.to_str().map(String::from).unwrap_or_else(|_| String::from_utf8_lossy(v.as_encoded_bytes()).into_owned())),
                KeyAndValueRef::Binary(k, v) => (k.to_string(), v.to_bytes().map(|b| B64.encode(b)).unwrap_or_else(|_| String::from_utf8_lossy(v.as_encoded_bytes()).into_owned())),
            })
            .collect()
    }

    // ── Schema ───────────────────────────────────────────────────────────

    /// The schema from `.proto` files, parsed here (no `protoc`). Imports are
    /// looked up in each file's own directory, then `import_paths`; the
    /// well-known `google/protobuf/*.proto` files are built in.
    pub fn load_protos(protos: &[PathBuf], import_paths: &[PathBuf]) -> Result<DescriptorPool> {
        if protos.is_empty() {
            bail!("no .proto files given");
        }
        let mut includes: Vec<PathBuf> = vec![];
        for p in protos {
            if !p.is_file() {
                bail!("no .proto file at {}", p.display());
            }
            let dir = p.parent().map(Path::to_path_buf).unwrap_or_default();
            let dir = if dir.as_os_str().is_empty() { PathBuf::from(".") } else { dir };
            if !includes.contains(&dir) {
                includes.push(dir);
            }
        }
        for p in import_paths {
            if !includes.contains(p) {
                includes.push(p.clone());
            }
        }
        let mut compiler = protox::Compiler::new(&includes).map_err(|e| anyhow!("{}", protox_error(&e)))?;
        compiler.include_imports(true);
        for p in protos {
            compiler.open_file(p).map_err(|e| anyhow!("{}", protox_error(&e)))?;
        }
        Ok(compiler.descriptor_pool())
    }

    fn protox_error(e: &protox::Error) -> String {
        let mut text = e.to_string();
        if let Some(file) = e.file() {
            if !text.contains(file) {
                text = format!("{file}: {text}");
            }
        }
        text
    }

    /// Which reflection API answered.
    #[derive(Debug, Clone, Copy, PartialEq, Eq)]
    pub enum Reflection {
        V1,
        V1Alpha,
    }

    /// The schema from the server's reflection service: `grpc.reflection.v1`,
    /// then `v1alpha` when the server doesn't have v1. Every service the
    /// server lists, with the files they need.
    pub async fn reflect(channel: Channel, metadata: &[(String, String)], timeout: Option<Duration>) -> Result<(DescriptorPool, Reflection)> {
        match reflect_v1(channel.clone(), metadata, timeout).await {
            Ok(pool) => Ok((pool, Reflection::V1)),
            Err(ReflectError::Status(s)) if s.code() == tonic::Code::Unimplemented => match reflect_v1alpha(channel, metadata, timeout).await {
                Ok(pool) => Ok((pool, Reflection::V1Alpha)),
                Err(ReflectError::Status(s)) if s.code() == tonic::Code::Unimplemented => {
                    bail!("the server has no reflection service (grpc.reflection.v1 or v1alpha): give its .proto files")
                }
                Err(e) => Err(e.into_anyhow()),
            },
            Err(e) => Err(e.into_anyhow()),
        }
    }

    enum ReflectError {
        Status(tonic::Status),
        Other(anyhow::Error),
    }

    impl ReflectError {
        fn into_anyhow(self) -> anyhow::Error {
            match self {
                ReflectError::Status(s) => anyhow!("server reflection failed: {} {}", code_name(s.code() as i32), s.message()),
                ReflectError::Other(e) => e.context("server reflection failed"),
            }
        }
    }

    impl From<tonic::Status> for ReflectError {
        fn from(s: tonic::Status) -> Self {
            ReflectError::Status(s)
        }
    }

    impl From<anyhow::Error> for ReflectError {
        fn from(e: anyhow::Error) -> Self {
            ReflectError::Other(e)
        }
    }

    /// What the v1 and v1alpha reflection exchanges have in common.
    enum Ask {
        List,
        Symbol(String),
        File(String),
    }

    enum Answer {
        Services(Vec<String>),
        Files(Vec<Vec<u8>>),
        Error(i32, String),
    }

    /// The reflection conversation, the same for both versions: list the
    /// services, ask for each one's file, then for every missing dependency.
    trait Exchange {
        fn ask(&mut self, ask: Ask) -> impl std::future::Future<Output = Result<Answer, ReflectError>> + Send;
    }

    async fn converse(ex: &mut impl Exchange) -> Result<DescriptorPool, ReflectError> {
        let services = match ex.ask(Ask::List).await? {
            Answer::Services(s) => s,
            Answer::Error(code, msg) => return Err(anyhow!("listing services: {} {msg}", code_name(code)).into()),
            Answer::Files(_) => return Err(anyhow!("unexpected answer to list services").into()),
        };
        let mut files: BTreeMap<String, prost_types::FileDescriptorProto> = BTreeMap::new();
        let add = |files: &mut BTreeMap<String, prost_types::FileDescriptorProto>, raw: Vec<Vec<u8>>| -> Result<(), ReflectError> {
            for bytes in raw {
                let f = prost_types::FileDescriptorProto::decode(bytes.as_slice()).map_err(|e| anyhow!("bad file descriptor: {e}"))?;
                files.entry(f.name().to_string()).or_insert(f);
            }
            Ok(())
        };
        for s in &services {
            match ex.ask(Ask::Symbol(s.clone())).await? {
                Answer::Files(raw) => add(&mut files, raw)?,
                Answer::Error(code, msg) => return Err(anyhow!("the file of {s}: {} {msg}", code_name(code)).into()),
                Answer::Services(_) => return Err(anyhow!("unexpected answer for {s}").into()),
            }
        }
        // Servers may leave out dependencies they think the client has.
        let mut asked = HashSet::new();
        loop {
            let missing: Vec<String> = files.values().flat_map(|f| f.dependency.iter()).filter(|d| !files.contains_key(*d) && !asked.contains(*d)).cloned().collect();
            if missing.is_empty() {
                break;
            }
            for name in missing {
                asked.insert(name.clone());
                match ex.ask(Ask::File(name.clone())).await? {
                    Answer::Files(raw) => add(&mut files, raw)?,
                    Answer::Error(code, msg) => return Err(anyhow!("the file {name}: {} {msg}", code_name(code)).into()),
                    Answer::Services(_) => return Err(anyhow!("unexpected answer for {name}").into()),
                }
            }
        }
        let mut pool = DescriptorPool::new();
        pool.add_file_descriptor_protos(files.into_values()).map_err(|e| anyhow!("the server's descriptors don't fit together: {e}"))?;
        Ok(pool)
    }

    macro_rules! reflect_version {
        ($name:ident, $pb:path) => {
            async fn $name(channel: Channel, metadata: &[(String, String)], timeout: Option<Duration>) -> Result<DescriptorPool, ReflectError> {
                use $pb as pb;
                use pb::server_reflection_request::MessageRequest;
                use pb::server_reflection_response::MessageResponse;

                struct Ex {
                    tx: mpsc::UnboundedSender<pb::ServerReflectionRequest>,
                    incoming: tonic::Streaming<pb::ServerReflectionResponse>,
                    /// The first question (list services) went out with the call.
                    first: bool,
                }

                impl Exchange for Ex {
                    async fn ask(&mut self, ask: Ask) -> Result<Answer, ReflectError> {
                        if !std::mem::take(&mut self.first) {
                            let m = match ask {
                                Ask::List => MessageRequest::ListServices(String::new()),
                                Ask::Symbol(s) => MessageRequest::FileContainingSymbol(s),
                                Ask::File(f) => MessageRequest::FileByFilename(f),
                            };
                            self.tx.send(pb::ServerReflectionRequest { host: String::new(), message_request: Some(m) }).map_err(|_| ReflectError::Other(anyhow!("the reflection stream closed")))?;
                        }
                        let resp = self.incoming.message().await?.ok_or_else(|| ReflectError::Other(anyhow!("the server closed the reflection stream")))?;
                        Ok(match resp.message_response {
                            Some(MessageResponse::ListServicesResponse(l)) => Answer::Services(l.service.into_iter().map(|s| s.name).collect()),
                            Some(MessageResponse::FileDescriptorResponse(f)) => Answer::Files(f.file_descriptor_proto),
                            Some(MessageResponse::ErrorResponse(e)) => Answer::Error(e.error_code, e.error_message),
                            _ => Answer::Error(2, "unexpected reflection answer".into()),
                        })
                    }
                }

                let (tx, rx) = mpsc::unbounded_channel::<pb::ServerReflectionRequest>();
                let outgoing = futures_util::stream::unfold(rx, |mut rx| async move { rx.recv().await.map(|m| (m, rx)) });
                let mut request = tonic::Request::new(outgoing);
                *request.metadata_mut() = metadata_map(metadata)?;
                if let Some(t) = timeout {
                    request.set_timeout(t);
                }
                tx.send(pb::ServerReflectionRequest { host: String::new(), message_request: Some(MessageRequest::ListServices(String::new())) })
                    .map_err(|_| ReflectError::Other(anyhow!("the reflection stream closed")))?;
                let mut client = pb::server_reflection_client::ServerReflectionClient::new(channel);
                let incoming = client.server_reflection_info(request).await?.into_inner();
                converse(&mut Ex { tx, incoming, first: true }).await
            }
        };
    }

    reflect_version!(reflect_v1, tonic_reflection::pb::v1);
    reflect_version!(reflect_v1alpha, tonic_reflection::pb::v1alpha);

    /// Where a schema came from, for messages.
    #[derive(Debug, Clone, PartialEq, Eq)]
    pub enum Source {
        Protos,
        Reflection(Reflection),
    }

    /// The target's schema: its `.proto` files, or server reflection over
    /// `channel` when it has none.
    pub async fn schema(target: &Target, channel: &Channel, timeout: Option<Duration>) -> Result<(DescriptorPool, Source)> {
        if target.protos.is_empty() {
            let (pool, version) = reflect(channel.clone(), &target.metadata, timeout).await?;
            Ok((pool, Source::Reflection(version)))
        } else {
            Ok((load_protos(&target.protos, &target.import_paths)?, Source::Protos))
        }
    }

    // ── Listing and describing ───────────────────────────────────────────

    /// A method, as the CLI's `list` and the desktop's pickers show it.
    #[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
    #[serde(rename_all = "camelCase")]
    pub struct MethodInfo {
        pub name: String,
        /// `package.Service.Method`.
        pub full_name: String,
        pub input: String,
        pub output: String,
        pub client_streaming: bool,
        pub server_streaming: bool,
    }

    #[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
    #[serde(rename_all = "camelCase")]
    pub struct ServiceInfo {
        /// `package.Service`.
        pub name: String,
        pub methods: Vec<MethodInfo>,
    }

    fn method_info(m: &MethodDescriptor) -> MethodInfo {
        MethodInfo {
            name: m.name().to_string(),
            full_name: m.full_name().to_string(),
            input: m.input().full_name().to_string(),
            output: m.output().full_name().to_string(),
            client_streaming: m.is_client_streaming(),
            server_streaming: m.is_server_streaming(),
        }
    }

    /// Every service in the schema, sorted by name.
    pub fn services(pool: &DescriptorPool) -> Vec<ServiceInfo> {
        let mut out: Vec<ServiceInfo> = pool.services().map(|s| ServiceInfo { name: s.full_name().to_string(), methods: s.methods().map(|m| method_info(&m)).collect() }).collect();
        out.sort_by(|a, b| a.name.cmp(&b.name));
        out
    }

    /// The service named `name` (full name, or a unique short name).
    pub fn find_service(pool: &DescriptorPool, name: &str) -> Result<ServiceDescriptor> {
        let name = name.trim_start_matches('.');
        if let Some(s) = pool.get_service_by_name(name) {
            return Ok(s);
        }
        let short: Vec<ServiceDescriptor> = pool.services().filter(|s| s.name() == name).collect();
        match short.len() {
            1 => Ok(short.into_iter().next().unwrap()),
            0 => {
                let names: Vec<String> = pool.services().map(|s| s.full_name().to_string()).collect();
                bail!("no service '{name}' (the schema has: {})", if names.is_empty() { "none".into() } else { names.join(", ") })
            }
            _ => bail!("'{name}' names more than one service: give the full name"),
        }
    }

    /// The method `method` of service `service`.
    pub fn find_method(pool: &DescriptorPool, service: &str, method: &str) -> Result<MethodDescriptor> {
        let s = find_service(pool, service)?;
        let found = s.methods().find(|m| m.name() == method);
        found.ok_or_else(|| {
            let names: Vec<String> = s.methods().map(|m| m.name().to_string()).collect();
            anyhow!("service {} has no method '{method}' (it has: {})", s.full_name(), names.join(", "))
        })
    }

    fn type_name(f: &FieldDescriptor) -> String {
        match f.kind() {
            Kind::Message(m) => m.full_name().to_string(),
            Kind::Enum(e) => e.full_name().to_string(),
            k => format!("{k:?}").to_ascii_lowercase(),
        }
    }

    fn field_line(f: &FieldDescriptor) -> String {
        if f.is_map() {
            if let Kind::Message(entry) = f.kind() {
                let (k, v) = (entry.map_entry_key_field(), entry.map_entry_value_field());
                return format!("map<{}, {}> {} = {};", type_name(&k), type_name(&v), f.name(), f.number());
            }
        }
        let label = match f.cardinality() {
            Cardinality::Repeated => "repeated ",
            Cardinality::Optional if f.supports_presence() && f.containing_oneof().is_none() && !matches!(f.kind(), Kind::Message(_)) => "optional ",
            Cardinality::Required => "required ",
            _ => "",
        };
        format!("{label}{} {} = {};", type_name(f), f.name(), f.number())
    }

    fn describe_message(m: &MessageDescriptor) -> String {
        let mut out = format!("message {} {{\n", m.full_name());
        let mut oneofs_done = HashSet::new();
        for f in m.fields() {
            match f.containing_oneof().filter(|o| !o.is_synthetic()) {
                Some(o) => {
                    if oneofs_done.insert(o.name().to_string()) {
                        out.push_str(&format!("  oneof {} {{\n", o.name()));
                        for of in o.fields() {
                            out.push_str(&format!("    {}\n", field_line(&of)));
                        }
                        out.push_str("  }\n");
                    }
                }
                None => out.push_str(&format!("  {}\n", field_line(&f))),
            }
        }
        out.push('}');
        out
    }

    fn describe_enum(e: &EnumDescriptor) -> String {
        let mut out = format!("enum {} {{\n", e.full_name());
        for v in e.values() {
            out.push_str(&format!("  {} = {};\n", v.name(), v.number()));
        }
        out.push('}');
        out
    }

    fn rpc_line(m: &MethodDescriptor) -> String {
        let stream = |yes: bool| if yes { "stream " } else { "" };
        format!("rpc {}({}{}) returns ({}{});", m.name(), stream(m.is_client_streaming()), m.input().full_name(), stream(m.is_server_streaming()), m.output().full_name())
    }

    /// A symbol (service, method, message or enum) written as in a `.proto`
    /// file, with full type names.
    pub fn describe(pool: &DescriptorPool, symbol: &str) -> Result<String> {
        let symbol = symbol.trim().trim_start_matches('.');
        if let Some(s) = pool.get_service_by_name(symbol) {
            let mut out = format!("service {} {{\n", s.full_name());
            for m in s.methods() {
                out.push_str(&format!("  {}\n", rpc_line(&m)));
            }
            out.push('}');
            return Ok(out);
        }
        if let Some(m) = pool.get_message_by_name(symbol) {
            return Ok(describe_message(&m));
        }
        if let Some(e) = pool.get_enum_by_name(symbol) {
            return Ok(describe_enum(&e));
        }
        if let Some((service, method)) = super::split_method(symbol) {
            if let Ok(m) = find_method(pool, &service, &method) {
                return Ok(format!("{}\n{}", m.full_name(), rpc_line(&m)));
            }
        }
        if let Ok(s) = find_service(pool, symbol) {
            return describe(pool, s.full_name());
        }
        bail!("no service, method, message or enum '{symbol}' in the schema")
    }

    // ── Message templates ────────────────────────────────────────────────

    /// A JSON message with every field of `message` at a sample value (zero
    /// values, the first enum value, nested messages filled in up to a few
    /// levels, one element in repeated fields, the first field of a oneof),
    /// to edit before sending.
    pub fn template(message: &MessageDescriptor) -> Value {
        template_at(message, 0)
    }

    fn template_at(m: &MessageDescriptor, depth: usize) -> Value {
        if m.full_name().starts_with("google.protobuf.") {
            // Well-known types have their own JSON forms (Timestamp is a string, ...).
            let default = DynamicMessage::new(m.clone());
            let opts = SerializeOptions::new().skip_default_fields(false);
            return default.serialize_with_options(serde_json::value::Serializer, &opts).unwrap_or(Value::Null);
        }
        let mut out = Map::new();
        let mut oneofs = HashSet::new();
        for f in m.fields() {
            if let Some(o) = f.containing_oneof().filter(|o| !o.is_synthetic()) {
                if !oneofs.insert(o.full_name().to_string()) {
                    continue;
                }
            }
            if let Some(v) = field_template(&f, depth) {
                out.insert(f.json_name().to_string(), v);
            }
        }
        Value::Object(out)
    }

    fn scalar_template(kind: &Kind, depth: usize) -> Option<Value> {
        Some(match kind {
            Kind::Double | Kind::Float => json!(0.0),
            Kind::Int32 | Kind::Sint32 | Kind::Sfixed32 | Kind::Uint32 | Kind::Fixed32 => json!(0),
            // 64-bit integers are strings in the JSON mapping.
            Kind::Int64 | Kind::Sint64 | Kind::Sfixed64 | Kind::Uint64 | Kind::Fixed64 => json!("0"),
            Kind::Bool => json!(false),
            Kind::String => json!(""),
            Kind::Bytes => json!(""),
            Kind::Enum(e) => json!(e.values().next().map(|v| v.name().to_string()).unwrap_or_default()),
            Kind::Message(m) if depth >= 4 => {
                let _ = m;
                return None;
            }
            Kind::Message(m) => template_at(m, depth + 1),
        })
    }

    fn field_template(f: &FieldDescriptor, depth: usize) -> Option<Value> {
        if f.is_map() {
            return Some(json!({}));
        }
        let one = scalar_template(&f.kind(), depth)?;
        Some(if f.is_list() { Value::Array(vec![one]) } else { one })
    }

    // ── Calling ──────────────────────────────────────────────────────────

    /// What a call produces, in order; `Status` is the last event.
    #[derive(Debug, Clone, PartialEq)]
    pub enum Event {
        /// The response headers (initial metadata).
        Headers(Vec<(String, String)>),
        /// A response message, in the JSON mapping.
        Message(Value),
        /// How the call ended, with the trailers.
        Status(Status),
    }

    /// A call's final status.
    #[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
    pub struct Status {
        pub code: i32,
        /// `OK`, `NOT_FOUND`, ...
        pub name: String,
        pub message: String,
        pub trailers: Vec<(String, String)>,
    }

    impl Status {
        fn ok(trailers: Vec<(String, String)>) -> Self {
            Status { code: 0, name: "OK".into(), message: String::new(), trailers }
        }

        fn of(s: &tonic::Status) -> Self {
            let code = s.code() as i32;
            // tonic puts the status's headers and trailers together; leave
            // out what is the status itself.
            let trailers = metadata_pairs(s.metadata()).into_iter().filter(|(k, _)| !matches!(k.as_str(), "grpc-status" | "grpc-message" | "grpc-status-details-bin" | "content-type")).collect();
            Status { code, name: code_name(code).into(), message: s.message().to_string(), trailers }
        }

        pub fn is_ok(&self) -> bool {
            self.code == 0
        }
    }

    /// The request message for `method` from JSON text (empty means `{}`).
    pub fn request_message(method: &MethodDescriptor, json: &str) -> Result<DynamicMessage> {
        let text = if json.trim().is_empty() { "{}" } else { json };
        let mut de = serde_json::Deserializer::from_str(text);
        let msg = DynamicMessage::deserialize(method.input(), &mut de).map_err(|e| anyhow!("the message isn't a valid {}: {e}", method.input().full_name()))?;
        de.end().map_err(|e| anyhow!("the message isn't a valid {}: {e}", method.input().full_name()))?;
        Ok(msg)
    }

    /// A message in the JSON mapping, every field shown (defaults too).
    pub fn to_json(msg: &DynamicMessage) -> Value {
        let opts = SerializeOptions::new().skip_default_fields(false);
        msg.serialize_with_options(serde_json::value::Serializer, &opts).unwrap_or_else(|e| json!({ "error": e.to_string() }))
    }

    /// Calls the target's method (unary or server streaming) with its
    /// message and metadata, `timeout` as the call's deadline. Problems found
    /// before anything is sent (no such method, a client-streaming method,
    /// bad JSON, bad metadata) are errors; everything after arrives as
    /// events, ending with the status (a failed connection is `UNAVAILABLE`).
    pub async fn call(channel: Channel, pool: &DescriptorPool, target: &Target, timeout: Option<Duration>) -> Result<mpsc::UnboundedReceiver<Event>> {
        if target.service.is_empty() || target.method.is_empty() {
            bail!("no gRPC method: give the service and method (package.Service/Method)");
        }
        let method = find_method(pool, &target.service, &target.method)?;
        if method.is_client_streaming() {
            bail!(
                "{} is a {} method: hitop calls unary and server-streaming methods only",
                method.full_name(),
                if method.is_server_streaming() { "bidirectional streaming" } else { "client-streaming" }
            );
        }
        let message = request_message(&method, &target.message)?;
        let mut request = tonic::Request::new(message);
        *request.metadata_mut() = metadata_map(&target.metadata)?;
        if let Some(t) = timeout {
            request.set_timeout(t);
        }
        let path = tonic::codegen::http::uri::PathAndQuery::try_from(format!("/{}/{}", method.parent_service().full_name(), method.name())).context("bad method path")?;
        let codec = DynamicCodec::new(method.output());
        let (tx, rx) = mpsc::unbounded_channel();
        tokio::spawn(async move {
            let emit = |e: Event| {
                let _ = tx.send(e);
            };
            let mut grpc = tonic::client::Grpc::new(channel);
            if let Err(e) = grpc.ready().await {
                emit(Event::Status(Status { code: 14, name: code_name(14).into(), message: error_chain(&*Into::<Box<dyn std::error::Error + Send + Sync>>::into(e)), trailers: vec![] }));
                return;
            }
            // Unary and server-streaming calls look the same on the wire;
            // reading a unary answer as a stream gives its trailers too.
            let response = match grpc.server_streaming(request, path, codec).await {
                Ok(r) => r,
                Err(s) => {
                    emit(Event::Status(Status::of(&s)));
                    return;
                }
            };
            emit(Event::Headers(metadata_pairs(response.metadata()).into_iter().filter(|(k, _)| k != "content-type").collect()));
            let mut stream = response.into_inner();
            loop {
                match stream.message().await {
                    Ok(Some(m)) => emit(Event::Message(to_json(&m))),
                    Ok(None) => break,
                    Err(s) => {
                        emit(Event::Status(Status::of(&s)));
                        return;
                    }
                }
            }
            let trailers = match stream.trailers().await {
                Ok(Some(t)) => metadata_pairs(&t).into_iter().filter(|(k, _)| !k.starts_with("grpc-")).collect(),
                Ok(None) => vec![],
                Err(s) => {
                    emit(Event::Status(Status::of(&s)));
                    return;
                }
            };
            emit(Event::Status(Status::ok(trailers)));
        });
        Ok(rx)
    }

    /// The template of a method's request message, for the desktop's editor.
    pub fn method_template(pool: &DescriptorPool, service: &str, method: &str) -> Result<Value> {
        Ok(template(&find_method(pool, service, method)?.input()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::{ApiKeyLocation, Grpc, Header};

    #[test]
    fn endpoints() {
        assert_eq!(endpoint("localhost:50051", false).unwrap(), ("https://localhost:50051".into(), true));
        assert_eq!(endpoint("localhost:50051", true).unwrap(), ("http://localhost:50051".into(), false));
        assert_eq!(endpoint("http://h:1/", false).unwrap(), ("http://h:1".into(), false));
        assert_eq!(endpoint("HTTPS://h:1", true).unwrap(), ("HTTPS://h:1".into(), true));
        assert!(endpoint("ws://h:1", false).is_err());
        assert!(endpoint(" ", false).is_err());
    }

    #[test]
    fn target_fills_in_variables_and_auth() {
        let vars: HashMap<String, String> = [("host", "127.0.0.1:9"), ("tok", "s3cret"), ("who", "Ann"), ("svc", "a.B")].into_iter().map(|(k, v)| (k.into(), v.into())).collect();
        let mut r = Request {
            url: "${host}".into(),
            kind: Some("grpc".into()),
            headers: vec![Header::new("x-who", "${who}"), Header { name: "off".into(), value: "x".into(), enabled: false, extra: Default::default() }],
            grpc: Some(Grpc {
                service: "${svc}".into(),
                method: "M".into(),
                message: r#"{"name": "${who}"}"#.into(),
                protos: vec!["p/a.proto".into(), "/abs/b.proto".into()],
                import_paths: vec!["inc".into()],
                plaintext: true,
                ..Default::default()
            }),
            ..Default::default()
        };
        let collection_auth = Auth::Bearer { token: "${tok}".into() };
        let t = target(&r, Some(&collection_auth), &vars, Some(Path::new("/col"))).unwrap();
        assert!(r.is_grpc());
        assert_eq!(t.address, "127.0.0.1:9");
        assert!(t.plaintext);
        assert_eq!(t.metadata, [("x-who".to_string(), "Ann".to_string()), ("Authorization".into(), "Bearer s3cret".into())]);
        assert_eq!((t.service.as_str(), t.method.as_str(), t.message.as_str()), ("a.B", "M", r#"{"name": "Ann"}"#));
        assert_eq!(t.protos, [PathBuf::from("/col/p/a.proto"), PathBuf::from("/abs/b.proto")]);
        assert_eq!(t.import_paths, [PathBuf::from("/col/inc")]);
        // An API key in the query has nowhere to go; in a header it is metadata.
        r.auth = Some(Auth::ApiKey { key: "k".into(), value: "v".into(), location: ApiKeyLocation::Query });
        assert_eq!(target(&r, None, &vars, None).unwrap().metadata.len(), 1);
        r.auth = Some(Auth::ApiKey { key: "x-api-key".into(), value: "v".into(), location: ApiKeyLocation::Header });
        assert_eq!(target(&r, None, &vars, None).unwrap().metadata[1], ("x-api-key".into(), "v".into()));
    }

    #[test]
    fn method_names() {
        assert_eq!(split_method("a.b.Svc/Do"), Some(("a.b.Svc".into(), "Do".into())));
        assert_eq!(split_method("/a.Svc/Do"), Some(("a.Svc".into(), "Do".into())));
        assert_eq!(split_method("a.Svc.Do"), Some(("a.Svc".into(), "Do".into())));
        assert_eq!(split_method("Do"), None);
        assert_eq!(code_name(5), "NOT_FOUND");
    }
}
