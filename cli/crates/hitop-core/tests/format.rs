//! Collection folder format: golden files and a round-trip property test.
//!
//! Regenerate the golden folder after an intended format change with
//! `UPDATE_GOLDEN=1 cargo test -p hitop-core --test format`, then review the diff.
use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};

use hitop_core::format::{self, Folder};
use hitop_core::model::{ApiKeyLocation, Auth, Collection, Environment, FormField, Graphql, Grpc, Header, Param, Request, WebSocket, WsMessage};
use proptest::prelude::*;

fn golden_dir() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/golden/every-field")
}

fn tree(dir: &Path) -> BTreeMap<String, String> {
    fn walk(base: &Path, dir: &Path, out: &mut BTreeMap<String, String>) {
        for entry in fs::read_dir(dir).unwrap() {
            let path = entry.unwrap().path();
            if path.is_dir() {
                walk(base, &path, out);
            } else {
                let rel = path.strip_prefix(base).unwrap().to_string_lossy().replace('\\', "/");
                out.insert(rel, fs::read_to_string(&path).unwrap());
            }
        }
    }
    let mut out = BTreeMap::new();
    walk(dir, dir, &mut out);
    out
}

fn golden_model() -> Folder {
    let dir = golden_dir();
    let collection: Collection = serde_json::from_str(&fs::read_to_string(dir.join("collection.json")).unwrap()).unwrap();
    let environments: Vec<Environment> = serde_json::from_str(&fs::read_to_string(dir.join("environments.json")).unwrap()).unwrap();
    let globals: Environment = serde_json::from_str(&fs::read_to_string(dir.join("globals.json")).unwrap()).unwrap();
    Folder { collection, environments, globals: Some(globals) }
}

/// The one change a round trip makes on purpose: blank header rows (the
/// editor's empty placeholder) are not written.
fn normalize(mut f: Folder) -> Folder {
    for r in &mut f.collection.requests {
        r.headers.retain(|h| !(h.enabled && h.name.is_empty() && h.value.is_empty() && h.extra.is_empty()));
    }
    f.environments.sort_by(|a, b| a.id.cmp(&b.id));
    f
}

#[test]
fn golden_json_to_folder() {
    let model = golden_model();
    let out = tempfile::tempdir().unwrap();
    format::write_folder(out.path(), &model).unwrap();
    let expected_dir = golden_dir().join("folder");
    if std::env::var_os("UPDATE_GOLDEN").is_some() {
        let _ = fs::remove_dir_all(&expected_dir);
        format::write_folder(&expected_dir, &model).unwrap();
    }
    let (got, want) = (tree(out.path()), tree(&expected_dir));
    assert_eq!(got.keys().collect::<Vec<_>>(), want.keys().collect::<Vec<_>>());
    for (path, content) in &want {
        assert_eq!(&got[path], content, "{path} differs from the golden file");
    }
}

#[test]
fn golden_folder_to_json() {
    let read = format::read_folder(&golden_dir().join("folder")).unwrap();
    assert_eq!(normalize(read), normalize(golden_model()));
}

#[test]
fn golden_covers_every_field() {
    let files = tree(&golden_dir().join("folder"));
    let all: String = files.values().cloned().collect();
    for key in [
        "format:", "name:", "id:", "workspace:", "variables:", "auth:", "order:", "params:", "headers:", "enabled: false", "body:",
        "type: json", "type: xml", "type: text", "type: graphql", "type: form", "type: multipart", "type: file",
        "content: |", "fields:", "scripts:", "pre:", "post: |", "examples:", "type: bearer", "type: basic",
        "type: apikey", "in: query", "type: none", "secret: true", "pinned: true", "color: teal", "region: eu",
        "variables: |", "operationName:", "kind: websocket", "subprotocols:", "messages:",
        "kind: grpc", "service:", "message: |", "protos:", "importPaths:", "plaintext: true",
    ] {
        assert!(all.contains(key), "no golden file has `{key}`");
    }
    assert!(files.contains_key("requests/orders/create-order.yaml"));
    assert!(files.contains_key("environments/production.yaml"));
    assert!(!all.contains("s3cr3t"), "secret values never reach collection files");
}

// --- property test: JSON model → folder → JSON model is the identity ---------------

fn text() -> impl Strategy<Value = String> {
    prop_oneof![
        4 => "[a-zA-Z0-9 _./${}-]{0,12}",
        2 => any::<String>(),
        1 => Just(String::new()),
        1 => "( |\t)?[a-z]{0,4}(\n|\r\n)( *)[a-z#:-]{0,6}\n?".prop_map(String::from),
        1 => prop::sample::select(vec!["true", "null", "~", "123", "0x1F", "- x", "a: b", "#c", "'q'", "\"", "{}", "[1]", "yes", "1e3"]).prop_map(String::from),
    ]
}

fn name() -> impl Strategy<Value = String> {
    prop_oneof![3 => "[A-Za-z][A-Za-z0-9 ]{0,10}", 1 => any::<String>()]
}

fn key() -> impl Strategy<Value = String> {
    "[a-z][a-zA-Z0-9_]{0,8}"
}

fn json_value() -> impl Strategy<Value = serde_json::Value> {
    let leaf = prop_oneof![
        Just(serde_json::Value::Null),
        any::<bool>().prop_map(serde_json::Value::from),
        any::<i64>().prop_map(serde_json::Value::from),
        text().prop_map(serde_json::Value::from),
    ];
    leaf.prop_recursive(2, 8, 4, |inner| {
        prop_oneof![
            prop::collection::vec(inner.clone(), 0..3).prop_map(serde_json::Value::from),
            prop::collection::btree_map("x[a-z]{0,5}", inner, 0..3).prop_map(|m| serde_json::Value::Object(m.into_iter().collect())),
        ]
    })
}

/// Unknown keys: prefixed so they never hit a key the format knows.
fn extra() -> impl Strategy<Value = serde_json::Map<String, serde_json::Value>> {
    prop::collection::btree_map("x[a-zA-Z0-9]{0,6}", json_value(), 0..2).prop_map(|m| m.into_iter().collect())
}

fn auth() -> impl Strategy<Value = Option<Auth>> {
    prop_oneof![
        Just(None),
        Just(Some(Auth::None)),
        text().prop_map(|token| Some(Auth::Bearer { token })),
        (text(), text()).prop_map(|(username, password)| Some(Auth::Basic { username, password })),
        (text(), text(), any::<bool>()).prop_map(|(key, value, q)| Some(Auth::ApiKey {
            key,
            value,
            location: if q { ApiKeyLocation::Query } else { ApiKeyLocation::Header }
        })),
    ]
}

fn header() -> impl Strategy<Value = Header> {
    (text(), text(), any::<bool>(), extra()).prop_map(|(name, value, enabled, extra)| Header { name, value, enabled, extra })
}

fn url() -> impl Strategy<Value = String> {
    prop_oneof![
        text(),
        (text(), prop::collection::vec((key(), text()), 0..4), prop::option::of("[a-z]{0,4}")).prop_map(|(base, q, hash)| {
            let mut u = base;
            if !q.is_empty() {
                u.push('?');
                u.push_str(&q.iter().map(|(k, v)| format!("{k}={v}")).collect::<Vec<_>>().join("&"));
            }
            if let Some(h) = hash {
                u.push('#');
                u.push_str(&h);
            }
            u
        }),
    ]
}

fn request() -> impl Strategy<Value = Request> {
    let core = (any::<u64>().prop_map(|n| n & ((1 << 53) - 1)), name(), url(), prop::sample::select(vec!["GET", "POST", "PUT", "patch", ""]));
    let rows = (
        prop::collection::vec(header(), 0..4),
        text(),
        text(),
        text(),
        auth(),
        prop::sample::select(vec![None, Some("multipart"), Some("urlencoded"), Some("binary")]),
    );
    let more = (
        prop::collection::vec(
            (text(), text(), prop::sample::select(vec!["text", "file"]), any::<bool>(), extra())
                .prop_map(|(key, value, kind, enabled, extra)| FormField { key, value, kind: kind.into(), enabled, extra }),
            0..3,
        ),
        prop::collection::vec((text(), text(), extra()).prop_map(|(key, value, extra)| Param { key, value, extra }), 0..3),
        prop::option::of(prop::sample::select(vec!["none", "json", "text", "xml", "graphql"])),
        prop::collection::vec(json_value(), 0..2),
        prop::option::of(("[a-z]{1,6}", "[a-z0-9-]{1,8}").prop_map(|(d, f)| format!("{d}/{f}"))),
        extra(),
        // GraphQL variables and operation name: kept on any body, empty means none.
        prop::option::of((text(), text()).prop_map(|(variables, operation_name)| Graphql { variables, operation_name, ..Default::default() })),
    );
    // A WebSocket request: kind and the websocket section, each on its own
    // (the section is kept on any request, as graphql is).
    let socket = (
        prop::option::of(prop::sample::select(vec!["websocket", "grpc"])),
        prop::option::of(
            (
                prop::collection::vec(text(), 0..3),
                prop::collection::vec((text(), text(), extra()).prop_map(|(name, content, extra)| WsMessage { name, content, extra }), 0..3),
            )
                .prop_map(|(subprotocols, messages)| WebSocket { subprotocols, messages, ..Default::default() }),
        ),
        // A gRPC request's section, also kept on any request.
        prop::option::of(
            (text(), text(), text(), prop::collection::vec(text(), 0..3), prop::collection::vec(text(), 0..2), any::<bool>()).prop_map(
                |(service, method, message, protos, import_paths, plaintext)| Grpc { service, method, message, protos, import_paths, plaintext, ..Default::default() },
            ),
        ),
    );
    (core, rows, more, socket).prop_map(|((id, name, url, method), (headers, body, pre, post, auth, mode), (form_fields, disabled_params, body_type, examples, file, extra, graphql), (kind, websocket, grpc))| {
        let mut r = Request {
            id,
            name,
            url,
            method: method.into(),
            headers,
            body,
            pre_request_script: pre,
            post_request_script: post,
            auth,
            body_mode: mode.map(String::from),
            form_fields,
            disabled_params,
            body_type: None,
            graphql: graphql.filter(|g| !g.is_empty()),
            kind: kind.map(String::from),
            websocket: websocket.filter(|w| !w.is_empty()),
            grpc: grpc.filter(|g| !g.is_empty()),
            examples,
            file,
            extra,
        };
        // bodyType is only kept for raw bodies, and only when it differs
        // from what hitop infers.
        if r.body_mode.is_none() {
            r.body_type = body_type.filter(|t| *t != format::infer_body_type(&r.headers, &r.body)).map(String::from);
        }
        r
    })
}

fn environment() -> impl Strategy<Value = Environment> {
    (name(), prop::option::of("[a-z0-9-]{1,10}"), prop::collection::btree_map(key(), text(), 0..4), prop::collection::btree_set(key(), 0..3), extra()).prop_map(
        |(name, id, vars, secrets, extra)| {
            let secrets: Vec<String> = secrets.into_iter().filter(|s| !vars.contains_key(s)).collect();
            Environment { id: id.unwrap_or_else(|| format!("env-{}", name.len())), name, variables: vars.into_iter().collect(), secrets, extra }
        },
    )
}

fn folder() -> impl Strategy<Value = Folder> {
    (
        any::<u64>().prop_map(|n| n & ((1 << 53) - 1)),
        name(),
        prop::collection::vec(request(), 0..6),
        prop::collection::btree_map(key(), text(), 0..4),
        auth(),
        extra(),
        prop::collection::vec(environment(), 0..3),
        prop::option::of("[0-9a-f]{1,32}"),
        prop::option::of(environment().prop_map(|e| Environment { id: "globals".into(), name: "globals".into(), ..e })),
    )
        .prop_map(|(id, name, mut requests, variables, auth, extra, mut environments, workspace, globals)| {
            // Explicit files must be unique (writing rejects duplicates).
            let mut seen = std::collections::HashSet::new();
            for r in &mut requests {
                if let Some(f) = &r.file {
                    if !seen.insert(f.clone()) {
                        r.file = None;
                    }
                }
            }
            let mut ids = std::collections::HashSet::new();
            environments.retain(|e| ids.insert(e.id.clone()));
            Folder { collection: Collection { id, name, requests, workspace, variables: variables.into_iter().collect(), auth, extra }, environments, globals }
        })
}

proptest! {
    #![proptest_config(ProptestConfig { cases: 512, ..ProptestConfig::default() })]

    #[test]
    fn json_to_folder_to_json_is_equal(model in folder()) {
        let dir = tempfile::tempdir().unwrap();
        format::write_folder(dir.path(), &model).unwrap();
        let back = format::read_folder(dir.path()).unwrap();
        prop_assert_eq!(normalize(back.clone()), normalize(model));
        // Writing what was read changes no file.
        let before = tree(dir.path());
        format::write_folder(dir.path(), &back).unwrap();
        prop_assert_eq!(tree(dir.path()), before);
        // And the JSON the model serializes to reads back the same.
        let json = serde_json::to_string(&back.collection).unwrap();
        let again: Collection = serde_json::from_str(&json).unwrap();
        prop_assert_eq!(again, back.collection);
    }
}
