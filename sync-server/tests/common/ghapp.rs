#![allow(dead_code)]
//! A stand-in for GitHub as the HITOP Cloud App sees it (gitsync.rs), on 127.0.0.1: the OAuth
//! code exchange and `GET /user/installations`, installation tokens (the App JWT is checked with
//! the test App's public key), `GET /installation/repositories`, `GET /repos/:o/:r` and the Git
//! Data API (refs, commits, recursive trees, blobs; new blobs, trees with `base_tree`, commits and
//! a fast-forward-only ref update). Repositories are kept in memory: commits point at flat trees
//! (full path → blob id), blob ids are Git's own, so the server's "skip unchanged blobs" works.

use std::collections::{BTreeMap, HashMap};
use std::io::{BufRead, BufReader, Read, Write};
use std::net::TcpListener;
use std::sync::{Arc, Mutex};

use base64::engine::general_purpose::STANDARD;
use base64::Engine;
use hitop_sync_server::gitsync::GitConfig;
use serde_json::{json, Value};

pub const APP_ID: &str = "4242";
pub const SLUG: &str = "hitop-cloud-test";
pub const CLIENT_ID: &str = "Iv1.app";
pub const CLIENT_SECRET: &str = "app-secret";
pub const WEBHOOK_SECRET: &str = "whsec-git";
pub const INSTALLATION: i64 = 7;
pub const PEM: &[u8] = include_bytes!("../fixtures/github-app/test-app.pem");
const PUBLIC_PEM: &[u8] = include_bytes!("../fixtures/github-app/test-app.pub.pem");

fn sha1(bytes: &[u8]) -> String {
    ring::digest::digest(&ring::digest::SHA1_FOR_LEGACY_USE_ONLY, bytes).as_ref().iter().map(|b| format!("{b:02x}")).collect()
}

pub fn blob_id(text: &[u8]) -> String {
    let mut v = format!("blob {}\0", text.len()).into_bytes();
    v.extend_from_slice(text);
    sha1(&v)
}

#[derive(Clone, Debug)]
pub struct Commit {
    pub tree: String,
    pub parents: Vec<String>,
    pub message: String,
    pub author: Value,
}

#[derive(Default)]
pub struct Inner {
    pub blobs: HashMap<String, Vec<u8>>,
    /// tree id → full path → blob id.
    pub trees: HashMap<String, BTreeMap<String, String>>,
    pub commits: HashMap<String, Commit>,
    /// "owner/name" → branch → commit.
    pub refs: HashMap<String, BTreeMap<String, String>>,
    /// The OAuth code GitHub accepts, and the installations `/user/installations` lists for it.
    pub installations: Vec<Value>,
    /// The next ref update is refused this many times (someone pushed first).
    pub refuse_updates: u32,
    pub token_requests: u32,
    pub ref_updates: u32,
    next_id: u64,
}

#[derive(Clone)]
pub struct FakeGithub {
    pub url: String,
    pub inner: Arc<Mutex<Inner>>,
}

impl FakeGithub {
    pub fn start() -> FakeGithub {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let url = format!("http://127.0.0.1:{}", listener.local_addr().unwrap().port());
        let gh = FakeGithub { url, inner: Arc::default() };
        let me = gh.clone();
        std::thread::spawn(move || {
            for stream in listener.incoming() {
                let Ok(mut stream) = stream else { continue };
                let mut reader = BufReader::new(stream.try_clone().unwrap());
                let mut line = String::new();
                if reader.read_line(&mut line).is_err() {
                    continue;
                }
                let mut parts = line.split_whitespace();
                let method = parts.next().unwrap_or("").to_string();
                let path = parts.next().unwrap_or("").to_string();
                let mut headers = Vec::new();
                loop {
                    let mut h = String::new();
                    if reader.read_line(&mut h).unwrap_or(0) == 0 || h == "\r\n" {
                        break;
                    }
                    headers.push(h.trim().to_string());
                }
                let len = headers.iter().find_map(|h| h.to_lowercase().strip_prefix("content-length:").map(|v| v.trim().parse::<usize>().unwrap_or(0))).unwrap_or(0);
                let mut body = vec![0; len];
                let _ = reader.read_exact(&mut body);
                let (status, answer) = me.answer(&method, &path, &headers, &body);
                let text = answer.to_string();
                let _ = write!(stream, "HTTP/1.1 {status} X\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{text}", text.len());
            }
        });
        gh.inner.lock().unwrap().installations = vec![
            json!({ "id": INSTALLATION, "app_id": APP_ID.parse::<i64>().unwrap(), "account": { "login": "ada" } }),
            json!({ "id": 99, "app_id": 1, "account": { "login": "other-app" } }),
        ];
        gh
    }

    pub fn config(&self) -> GitConfig {
        GitConfig::for_tests(APP_ID, SLUG, PEM, CLIENT_ID, CLIENT_SECRET, WEBHOOK_SECRET, &self.url, &self.url)
    }

    /// A repository with one commit holding `files` on `main`.
    pub fn repo(&self, name: &str, files: &[(&str, &str)]) -> String {
        let mut inner = self.inner.lock().unwrap();
        let tree: BTreeMap<String, String> = files
            .iter()
            .map(|(p, t)| {
                let id = blob_id(t.as_bytes());
                inner.blobs.insert(id.clone(), t.as_bytes().to_vec());
                (p.to_string(), id)
            })
            .collect();
        let tree_id = put_tree(&mut inner, tree);
        let c = put_commit(&mut inner, Commit { tree: tree_id, parents: vec![], message: "initial".into(), author: json!({ "name": "github" }) });
        inner.refs.entry(name.to_string()).or_default().insert("main".into(), c.clone());
        c
    }

    /// A repository without any commit (just created on GitHub).
    pub fn empty_repo(&self, name: &str) {
        self.inner.lock().unwrap().refs.entry(name.to_string()).or_default();
    }

    /// Someone else's commit on `main` of `repo`: `files` set (None deletes).
    pub fn push(&self, repo: &str, files: &[(&str, Option<&str>)]) -> String {
        let mut inner = self.inner.lock().unwrap();
        let head = inner.refs[repo]["main"].clone();
        let mut tree = inner.trees[&inner.commits[&head].tree].clone();
        for (p, t) in files {
            match t {
                Some(t) => {
                    let id = blob_id(t.as_bytes());
                    inner.blobs.insert(id.clone(), t.as_bytes().to_vec());
                    tree.insert(p.to_string(), id);
                }
                None => {
                    tree.remove(*p);
                }
            }
        }
        let tree_id = put_tree(&mut inner, tree);
        let c = put_commit(&mut inner, Commit { tree: tree_id, parents: vec![head], message: "a colleague's change".into(), author: json!({ "name": "bob" }) });
        inner.refs.get_mut(repo).unwrap().insert("main".into(), c.clone());
        c
    }

    /// The files on `main` of `repo` now.
    pub fn files(&self, repo: &str) -> BTreeMap<String, String> {
        let inner = self.inner.lock().unwrap();
        let head = &inner.refs[repo]["main"];
        inner.trees[&inner.commits[head].tree].iter().map(|(p, id)| (p.clone(), String::from_utf8(inner.blobs[id].clone()).unwrap())).collect()
    }

    pub fn head(&self, repo: &str) -> Commit {
        let inner = self.inner.lock().unwrap();
        inner.commits[&inner.refs[repo]["main"]].clone()
    }

    pub fn commit_count(&self, repo: &str) -> usize {
        let inner = self.inner.lock().unwrap();
        let mut n = 0;
        let mut at = Some(inner.refs[repo]["main"].clone());
        while let Some(c) = at {
            n += 1;
            at = inner.commits[&c].parents.first().cloned();
        }
        n
    }

    fn answer(&self, method: &str, path: &str, headers: &[String], body: &[u8]) -> (u16, Value) {
        let mut inner = self.inner.lock().unwrap();
        let auth = headers.iter().find_map(|h| h.strip_prefix("Authorization: Bearer ").or_else(|| h.strip_prefix("authorization: Bearer "))).unwrap_or("").to_string();
        let body_json: Value = serde_json::from_slice(body).unwrap_or(Value::Null);
        let (route, _q) = path.split_once('?').unwrap_or((path, ""));
        let segs: Vec<&str> = route.trim_start_matches('/').split('/').collect();
        let installation_token = format!("ghs_installation_{INSTALLATION}");
        match (method, segs.as_slice()) {
            ("POST", ["login", "oauth", "access_token"]) => {
                let form: HashMap<String, String> = url::form_urlencoded::parse(body).into_owned().collect();
                if form.get("client_id").map(String::as_str) != Some(CLIENT_ID) || form.get("client_secret").map(String::as_str) != Some(CLIENT_SECRET) {
                    return (200, json!({ "error": "incorrect_client_credentials" }));
                }
                if form.get("code").map(String::as_str) != Some("good-code") {
                    return (200, json!({ "error": "bad_verification_code" }));
                }
                (200, json!({ "access_token": "ghu_user", "token_type": "bearer" }))
            }
            ("GET", ["user", "installations"]) if auth == "ghu_user" => (200, json!({ "total_count": inner.installations.len(), "installations": inner.installations })),
            ("POST", ["app", "installations", id, "access_tokens"]) => {
                inner.token_requests += 1;
                let key = jsonwebtoken::DecodingKey::from_rsa_pem(PUBLIC_PEM).unwrap();
                let mut v = jsonwebtoken::Validation::new(jsonwebtoken::Algorithm::RS256);
                v.set_required_spec_claims(&["exp", "iss"]);
                v.set_issuer(&[APP_ID]);
                if jsonwebtoken::decode::<Value>(&auth, &key, &v).is_err() {
                    return (401, json!({ "message": "A JSON web token could not be decoded" }));
                }
                if *id != INSTALLATION.to_string() {
                    return (404, json!({ "message": "Not Found" }));
                }
                (201, json!({ "token": installation_token, "expires_at": "2099-01-01T00:00:00Z" }))
            }
            _ if auth != installation_token => (401, json!({ "message": "Bad credentials" })),
            ("GET", ["installation", "repositories"]) => {
                let repos: Vec<Value> = inner.refs.keys().map(|r| json!({ "full_name": r, "default_branch": "main", "private": true })).collect();
                (200, json!({ "total_count": repos.len(), "repositories": repos }))
            }
            ("GET", ["repos", o, r]) => {
                let full = format!("{o}/{r}");
                if inner.refs.contains_key(&full) {
                    (200, json!({ "full_name": full, "default_branch": "main", "private": true }))
                } else {
                    (404, json!({ "message": "Not Found" }))
                }
            }
            (_, ["repos", o, r, "git", rest @ ..]) => {
                let full = format!("{o}/{r}");
                if !inner.refs.contains_key(&full) {
                    return (404, json!({ "message": "Not Found" }));
                }
                // GitHub's Git Data API on a repository without commits.
                if inner.refs[&full].is_empty() {
                    return (409, json!({ "message": "Git Repository is empty." }));
                }
                git_data(&mut inner, method, &full, rest, &body_json)
            }
            // The Contents API: one file, one commit (here only what first_commit uses).
            ("PUT", ["repos", o, r, "contents", path @ ..]) => {
                let full = format!("{o}/{r}");
                let Some(branches) = inner.refs.get(&full).cloned() else {
                    return (404, json!({ "message": "Not Found" }));
                };
                let branch = body_json["branch"].as_str().unwrap_or("main").to_string();
                let text = STANDARD.decode(body_json["content"].as_str().unwrap_or("")).unwrap_or_default();
                let id = blob_id(&text);
                inner.blobs.insert(id.clone(), text);
                let parent = branches.get(&branch).cloned();
                let mut tree = parent.as_ref().map(|c| inner.trees[&inner.commits[c].tree].clone()).unwrap_or_default();
                tree.insert(path.join("/"), id);
                let tree_id = put_tree(&mut inner, tree);
                let message = body_json["message"].as_str().unwrap_or("").to_string();
                let c = put_commit(&mut inner, Commit { tree: tree_id, parents: parent.into_iter().collect(), message, author: json!({ "name": "hitop" }) });
                inner.refs.get_mut(&full).unwrap().insert(branch, c.clone());
                (201, json!({ "commit": { "sha": c } }))
            }
            _ => (404, json!({ "message": "Not Found" })),
        }
    }
}

fn put_tree(inner: &mut Inner, tree: BTreeMap<String, String>) -> String {
    let id = sha1(serde_json::to_string(&tree).unwrap().as_bytes());
    inner.trees.insert(id.clone(), tree);
    id
}

fn put_commit(inner: &mut Inner, c: Commit) -> String {
    inner.next_id += 1;
    let id = sha1(format!("{}{:?}{}{}", c.tree, c.parents, c.message, inner.next_id).as_bytes());
    inner.commits.insert(id.clone(), c);
    id
}

fn is_ancestor(inner: &Inner, of: &str, commit: &str) -> bool {
    let mut stack = vec![commit.to_string()];
    while let Some(c) = stack.pop() {
        if c == of {
            return true;
        }
        if let Some(x) = inner.commits.get(&c) {
            stack.extend(x.parents.iter().cloned());
        }
    }
    false
}

fn git_data(inner: &mut Inner, method: &str, repo: &str, rest: &[&str], body: &Value) -> (u16, Value) {
    match (method, rest) {
        ("GET", ["ref", "heads", branch @ ..]) => match inner.refs[repo].get(&branch.join("/")) {
            Some(c) => (200, json!({ "ref": format!("refs/heads/{}", branch.join("/")), "object": { "sha": c, "type": "commit" } })),
            None => (404, json!({ "message": "Not Found" })),
        },
        ("GET", ["commits", id]) => match inner.commits.get(*id) {
            Some(c) => (200, json!({ "sha": id, "tree": { "sha": c.tree }, "parents": c.parents.iter().map(|p| json!({ "sha": p })).collect::<Vec<_>>() })),
            None => (404, json!({ "message": "Not Found" })),
        },
        ("GET", ["trees", id]) => match inner.trees.get(*id) {
            Some(t) => (200, json!({ "sha": id, "truncated": false, "tree": t.iter().map(|(p, b)| json!({ "path": p, "type": "blob", "mode": "100644", "sha": b })).collect::<Vec<_>>() })),
            None => (404, json!({ "message": "Not Found" })),
        },
        ("GET", ["blobs", id]) => match inner.blobs.get(*id) {
            Some(b) => (200, json!({ "sha": id, "encoding": "base64", "content": STANDARD.encode(b) })),
            None => (404, json!({ "message": "Not Found" })),
        },
        ("POST", ["blobs"]) => {
            let text = body["content"].as_str().unwrap_or("").as_bytes().to_vec();
            let id = blob_id(&text);
            inner.blobs.insert(id.clone(), text);
            (201, json!({ "sha": id }))
        }
        ("POST", ["trees"]) => {
            let mut tree = body["base_tree"].as_str().and_then(|b| inner.trees.get(b)).cloned().unwrap_or_default();
            for e in body["tree"].as_array().into_iter().flatten() {
                let p = e["path"].as_str().unwrap().to_string();
                match e["sha"].as_str() {
                    Some(s) => {
                        tree.insert(p, s.to_string());
                    }
                    None => {
                        tree.remove(&p);
                    }
                }
            }
            (201, json!({ "sha": put_tree(inner, tree) }))
        }
        ("POST", ["commits"]) => {
            let c = Commit {
                tree: body["tree"].as_str().unwrap_or("").to_string(),
                parents: body["parents"].as_array().into_iter().flatten().filter_map(|p| p.as_str().map(String::from)).collect(),
                message: body["message"].as_str().unwrap_or("").to_string(),
                author: body["author"].clone(),
            };
            (201, json!({ "sha": put_commit(inner, c) }))
        }
        ("PATCH", ["refs", "heads", branch @ ..]) => {
            let branch = branch.join("/");
            let new = body["sha"].as_str().unwrap_or("").to_string();
            let current = inner.refs[repo].get(&branch).cloned().unwrap_or_default();
            if inner.refuse_updates > 0 {
                inner.refuse_updates -= 1;
                return (422, json!({ "message": "Update is not a fast forward" }));
            }
            if !is_ancestor(inner, &current, &new) {
                return (422, json!({ "message": "Update is not a fast forward" }));
            }
            inner.ref_updates += 1;
            inner.refs.get_mut(repo).unwrap().insert(branch, new.clone());
            (200, json!({ "object": { "sha": new } }))
        }
        _ => (404, json!({ "message": "Not Found" })),
    }
}
