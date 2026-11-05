//! Git sync of the personal library (gitsync.rs) against a fake GitHub (common/ghapp.rs):
//! connecting through the App (only this App's installations, only for the person who connected),
//! linking an empty folder (the collection is committed, nothing else in the repository changes),
//! every save pushed, a colleague's push merged into the library (webhook signature checked), a
//! request changed on both sides asking (and nothing pushed meanwhile) until it is resolved
//! (keep both, keep mine), a branch that moves during a push, a folder that already holds a
//! collection winning the first sync, unlinking, the App removed, and Git sync off.

mod common;

use common::ghapp::{self, FakeGithub};
use common::*;
use serde_json::{json, Value};

const REPO: &str = "ada/apis";

fn server(gh: &FakeGithub, name: &str) -> Server {
    Server::with(name, |c| {
        c.git = Some(gh.config());
        c.cors_origins.push("https://app.test.example".into());
    })
}

fn shop(urls: &[(&str, &str)]) -> Value {
    let requests: Vec<Value> = urls.iter().enumerate().map(|(i, (name, url))| json!({ "id": 100 + i, "name": name, "url": url, "method": "GET", "headers": [], "body": "" })).collect();
    json!({ "id": 1, "name": "Shop", "requests": requests })
}

fn save(s: &Server, t: &str, data: Value) -> i64 {
    let base = s.call("GET", "/v1/library", Some(t), None).json["items"].as_array().unwrap().iter().find(|i| i["id"] == "1").map_or(0, |i| i["version"].as_i64().unwrap());
    let r = s.call("PUT", "/v1/library/collection/1", Some(t), Some(json!({ "data": data, "baseVersion": base })));
    assert_eq!(r.status, 200, "{}", r.json);
    r.json["version"].as_i64().unwrap()
}

fn library_collection(s: &Server, t: &str) -> Value {
    s.call("GET", "/v1/library", Some(t), None).json["items"].as_array().unwrap().iter().find(|i| i["id"] == "1").cloned().unwrap()
}

fn connect(s: &Server, t: &str) {
    let r = s.call("POST", "/v1/git/github/connect", Some(t), Some(json!({ "returnTo": "https://app.test.example/app/" })));
    assert_eq!(r.status, 200, "{}", r.json);
    let url = url::Url::parse(r.json["installUrl"].as_str().unwrap()).unwrap();
    assert!(url.path().ends_with(&format!("/apps/{}/installations/new", ghapp::SLUG)));
    let st = url.query_pairs().find(|(k, _)| k == "state").unwrap().1.to_string();
    let back = s.call("GET", &format!("/v1/git/github/callback?code=good-code&installation_id={}&setup_action=install&state={st}", ghapp::INSTALLATION), None, None);
    assert_eq!((back.status, back.header("location")), (302, Some("https://app.test.example/app/#git=connected")), "{}", back.json);
    let again = s.call("GET", &format!("/v1/git/github/callback?code=good-code&state={st}"), None, None);
    assert_eq!(again.status, 400, "a state works once");
}

fn run_due(s: &Server) -> usize {
    hitop_sync_server::gitsync::run_due(s.app.state())
}

fn link(s: &Server, t: &str, path: &str) -> Reply {
    s.call("PUT", "/v1/library/collection/1/git", Some(t), Some(json!({ "installation": ghapp::INSTALLATION, "repo": REPO, "path": path })))
}

fn webhook(s: &Server, body: &Value, secret: &str) -> Reply {
    let raw = body.to_string().into_bytes();
    let key = ring::hmac::Key::new(ring::hmac::HMAC_SHA256, secret.as_bytes());
    let sig: String = ring::hmac::sign(&key, &raw).as_ref().iter().map(|b| format!("{b:02x}")).collect();
    s.raw("POST", "/v1/git/github/webhook", None, raw, &[("X-Hub-Signature-256", &format!("sha256={sig}")), ("X-GitHub-Event", "push"), ("Content-Type", "application/json")])
}

fn push_event(gh: &FakeGithub) -> Value {
    json!({ "ref": "refs/heads/main", "after": gh.inner.lock().unwrap().refs[REPO]["main"].clone(), "repository": { "full_name": REPO }, "installation": { "id": ghapp::INSTALLATION } })
}

#[test]
fn connect_link_push_pull_conflict_and_resolve() {
    let gh = FakeGithub::start();
    gh.repo(REPO, &[("README.md", "# APIs\n")]);
    let s = server(&gh, "git-flow");
    let ada = s.sign_in("ada@example.com");
    assert_eq!(s.call("GET", "/v1/git", Some(&ada), None).json["installations"].as_array().unwrap().len(), 0);
    connect(&s, &ada);
    let info = s.call("GET", "/v1/git", Some(&ada), None).json;
    assert_eq!(info["installations"], json!([{ "id": ghapp::INSTALLATION, "account": "ada" }]), "only this App's installation: {info}");
    let repos = s.call("GET", &format!("/v1/git/installations/{}/repos", ghapp::INSTALLATION), Some(&ada), None).json;
    assert_eq!(repos["repositories"][0]["fullName"], json!(REPO));

    // Bob can't use Ada's installation, nor link a collection he doesn't have.
    let bob = s.sign_in("bob@example.com");
    assert_eq!(s.call("GET", &format!("/v1/git/installations/{}/repos", ghapp::INSTALLATION), Some(&bob), None).status, 404);

    // Link an empty folder: the collection is committed there; the README stays.
    save(&s, &ada, shop(&[("List orders", "https://shop.test/orders"), ("Get order", "https://shop.test/orders/1")]));
    assert_eq!(s.call("PUT", "/v1/library/collection/1/git", Some(&bob), Some(json!({ "installation": ghapp::INSTALLATION, "repo": REPO }))).status, 404);
    let l = link(&s, &ada, "shop");
    assert_eq!((l.status, l.json["state"].as_str()), (200, Some("ok")), "{}", l.json);
    let files = gh.files(REPO);
    assert!(files.contains_key("shop/hitop.yaml") && files.contains_key("README.md"), "{:?}", files.keys());
    let requests: Vec<&String> = files.keys().filter(|p| p.starts_with("shop/requests/")).collect();
    assert_eq!(requests.len(), 2, "{requests:?}");
    assert!(files.values().any(|t| t.contains("https://shop.test/orders/1")));
    let head = gh.head(REPO);
    assert_eq!(head.author["email"], json!("ada@example.com"));
    assert!(head.message.starts_with("hitop: add"), "{}", head.message);
    assert_eq!(library_collection(&s, &ada)["version"], json!(1), "the first push doesn't rewrite the library");
    assert_eq!(s.call("GET", "/v1/library", Some(&ada), None).json["git"]["1"]["repo"], json!(REPO));

    // A save is pushed (debounce 0 in tests: the worker's next step).
    save(&s, &ada, shop(&[("List orders", "https://shop.test/v2/orders"), ("Get order", "https://shop.test/orders/1")]));
    assert_eq!(run_due(&s), 1);
    assert_eq!(run_due(&s), 0, "nothing due after the push");
    assert!(gh.files(REPO).values().any(|t| t.contains("https://shop.test/v2/orders")));
    assert!(gh.head(REPO).message.contains("update"), "{}", gh.head(REPO).message);
    let commits = gh.commit_count(REPO);

    // A colleague changes the other request on GitHub: the webhook brings it into the library.
    let get_path = gh.files(REPO).into_iter().find(|(_, t)| t.contains("orders/1")).unwrap().0;
    let theirs_text = gh.files(REPO)[&get_path].replace("https://shop.test/orders/1", "https://shop.test/orders/{id}");
    gh.push(REPO, &[(&get_path, Some(&theirs_text))]);
    assert_eq!(webhook(&s, &push_event(&gh), "wrong-secret").status, 401);
    let w = webhook(&s, &push_event(&gh), ghapp::WEBHOOK_SECRET);
    assert_eq!((w.status, w.json["links"].as_i64()), (200, Some(1)), "{}", w.json);
    assert_eq!(run_due(&s), 1);
    let lib = library_collection(&s, &ada);
    let urls: Vec<&str> = lib["data"]["requests"].as_array().unwrap().iter().map(|r| r["url"].as_str().unwrap()).collect();
    assert!(urls.contains(&"https://shop.test/orders/{id}") && urls.contains(&"https://shop.test/v2/orders"), "{urls:?}");
    let ids: Vec<i64> = lib["data"]["requests"].as_array().unwrap().iter().map(|r| r["id"].as_i64().unwrap()).collect();
    assert_eq!(ids, vec![100, 101], "request ids survive the round trip");
    assert_eq!(gh.commit_count(REPO), commits + 1, "pulling made no commit of its own");

    // Both change "List orders": a conflict, nothing pushed while it's open.
    let list_path = gh.files(REPO).into_iter().find(|(_, t)| t.contains("v2/orders")).unwrap().0;
    gh.push(REPO, &[(&list_path, Some(&gh.files(REPO)[&list_path].replace("v2/orders", "v3/orders")))]);
    let mut mine = lib["data"].clone();
    mine["requests"][0]["url"] = json!("https://shop.test/v4/orders");
    save(&s, &ada, mine);
    let before = gh.commit_count(REPO);
    run_due(&s);
    let l = s.call("GET", "/v1/library/collection/1/git", Some(&ada), None).json;
    assert_eq!(l["state"], json!("conflict"), "{l}");
    assert_eq!(l["conflicts"].as_array().unwrap().len(), 1);
    assert_eq!(l["conflicts"][0]["name"], json!("List orders"));
    assert!(l["conflicts"][0]["mine"].as_str().unwrap().contains("v4") && l["conflicts"][0]["theirs"].as_str().unwrap().contains("v3"));
    assert_eq!(gh.commit_count(REPO), before, "nothing pushed during a conflict");
    // Another save during the conflict pushes nothing either.
    save(&s, &ada, library_collection(&s, &ada)["data"].clone());
    assert_eq!(run_due(&s), 0);

    // Keep both: GitHub's stays, mine is added as "List orders (mine)", in the library and on GitHub.
    let r = s.call("POST", "/v1/library/collection/1/git/resolve", Some(&ada), Some(json!({ "choices": { l["conflicts"][0]["path"].as_str().unwrap(): "both" } })));
    assert_eq!((r.status, r.json["state"].as_str()), (200, Some("ok")), "{}", r.json);
    let names: Vec<String> = library_collection(&s, &ada)["data"]["requests"].as_array().unwrap().iter().map(|r| r["name"].as_str().unwrap().to_string()).collect();
    assert!(names.contains(&"List orders".into()) && names.contains(&"List orders (mine)".into()), "{names:?}");
    let on_github = gh.files(REPO);
    assert!(on_github.values().any(|t| t.contains("v3/orders")) && on_github.values().any(|t| t.contains("v4/orders") && t.contains("(mine)")), "{:?}", on_github.keys());
    assert!(gh.commit_count(REPO) > before);

    // A bad choice is refused.
    assert_eq!(s.call("POST", "/v1/library/collection/1/git/resolve", Some(&ada), Some(json!({ "choices": { "x": "maybe" } }))).status, 400);
}

#[test]
fn a_moving_branch_an_existing_folder_unlink_and_the_app_removed() {
    let gh = FakeGithub::start();
    gh.repo(REPO, &[("hitop.yaml", "format: 1\nname: From GitHub\n"), ("requests/ping.yaml", "name: Ping\nmethod: GET\nurl: https://gh.test/ping\n")]);
    let s = server(&gh, "git-edge");
    let ada = s.sign_in("ada@example.com");
    connect(&s, &ada);
    save(&s, &ada, shop(&[("Local only", "https://local.test/")]));
    // A folder that already holds a collection wins the first sync.
    let l = link(&s, &ada, "");
    assert_eq!(l.json["state"], json!("ok"), "{}", l.json);
    let lib = library_collection(&s, &ada);
    assert_eq!(lib["data"]["name"], json!("From GitHub"));
    assert_eq!(lib["data"]["requests"][0]["url"], json!("https://gh.test/ping"));
    assert_eq!(lib["data"]["id"], json!(1), "the library keeps its id");

    // The branch moves while we push: the push is retried on top of the new head.
    let mut data = lib["data"].clone();
    data["requests"][0]["url"] = json!("https://gh.test/ping2");
    save(&s, &ada, data);
    gh.inner.lock().unwrap().refuse_updates = 1;
    run_due(&s);
    assert!(gh.files(REPO)["requests/ping.yaml"].contains("ping2"));
    assert_eq!(s.call("GET", "/v1/library/collection/1/git", Some(&ada), None).json["state"], json!("ok"));

    // Unlink: the repository keeps its files; saves push nothing.
    let files = gh.files(REPO);
    assert_eq!(s.call("DELETE", "/v1/library/collection/1/git", Some(&ada), None).status, 200);
    assert_eq!(s.call("GET", "/v1/library/collection/1/git", Some(&ada), None).status, 404);
    save(&s, &ada, shop(&[("After unlink", "https://x.test/")]));
    assert_eq!(run_due(&s), 0);
    assert_eq!(gh.files(REPO), files);

    // The App removed from GitHub: links show as disconnected, the installation is gone.
    link(&s, &ada, "");
    let body = json!({ "action": "deleted", "installation": { "id": ghapp::INSTALLATION } });
    let raw = body.to_string().into_bytes();
    let key = ring::hmac::Key::new(ring::hmac::HMAC_SHA256, ghapp::WEBHOOK_SECRET.as_bytes());
    let sig: String = ring::hmac::sign(&key, &raw).as_ref().iter().map(|b| format!("{b:02x}")).collect();
    let r = s.raw("POST", "/v1/git/github/webhook", None, raw, &[("X-Hub-Signature-256", &format!("sha256={sig}")), ("X-GitHub-Event", "installation")]);
    assert_eq!(r.status, 200, "{}", r.json);
    assert_eq!(s.call("GET", "/v1/library/collection/1/git", Some(&ada), None).json["state"], json!("disconnected"));
    assert_eq!(s.call("GET", "/v1/git", Some(&ada), None).json["installations"], json!([]));
    // Deleting the collection drops its link.
    let v = library_collection(&s, &ada)["version"].as_i64().unwrap();
    assert_eq!(s.call("DELETE", &format!("/v1/library/collection/1?baseVersion={v}"), Some(&ada), None).status, 200);
    assert_eq!(s.call("GET", "/v1/library/collection/1/git", Some(&ada), None).status, 404);
}

#[test]
fn off_without_an_app_and_bad_input() {
    let s = Server::new("git-off");
    let t = s.sign_in("ada@example.com");
    assert_eq!(s.call("GET", "/v1/git", Some(&t), None).json, json!({ "enabled": false }));
    let r = s.call("POST", "/v1/git/github/connect", Some(&t), Some(json!({ "returnTo": "https://app.test.example/" })));
    assert_eq!((r.status, r.json["error"].as_str()), (404, Some("git_off")));

    let gh = FakeGithub::start();
    gh.repo(REPO, &[("README.md", "x")]);
    let s = server(&gh, "git-bad");
    let t = s.sign_in("ada@example.com");
    assert_eq!(s.call("POST", "/v1/git/github/connect", Some(&t), Some(json!({ "returnTo": "https://evil.example/" }))).status, 400, "returnTo must be an allowed origin");
    connect(&s, &t);
    save(&s, &t, shop(&[("A", "https://a.test/")]));
    for (path, why) in [("../up", "no .."), (".git", "no hidden folders")] {
        assert_eq!(link(&s, &t, path).status, 400, "{why}");
    }
    let r = s.call("PUT", "/v1/library/collection/1/git", Some(&t), Some(json!({ "installation": ghapp::INSTALLATION, "repo": "ada/missing" })));
    assert_eq!((r.status, r.json["error"].as_str()), (404, Some("repo_not_found")), "{}", r.json);
    let r = s.call("PUT", "/v1/library/collection/1/git", Some(&t), Some(json!({ "installation": 99, "repo": REPO })));
    assert_eq!(r.status, 404, "another App's installation isn't recorded: {}", r.json);
    // A code GitHub refuses sends the browser back with an error.
    let st = url::Url::parse(s.call("POST", "/v1/git/github/connect", Some(&t), Some(json!({ "returnTo": "https://app.test.example/app/" }))).json["authorizeUrl"].as_str().unwrap())
        .unwrap()
        .query_pairs()
        .find(|(k, _)| k == "state")
        .unwrap()
        .1
        .to_string();
    let back = s.call("GET", &format!("/v1/git/github/callback?code=bad&state={st}"), None, None);
    assert_eq!(back.header("location"), Some("https://app.test.example/app/#git=error&reason=github"));
}

#[test]
fn an_empty_repository_gets_a_first_commit() {
    // A repository just created on GitHub has no commit, and the Git Data API answers 409 until
    // it has one: the server adds a README through the Contents API, then commits the collection.
    let gh = FakeGithub::start();
    gh.empty_repo(REPO);
    let s = server(&gh, "git-empty");
    let ada = s.sign_in("ada@example.com");
    connect(&s, &ada);
    save(&s, &ada, shop(&[("List orders", "https://shop.test/orders")]));
    let l = link(&s, &ada, "shop");
    assert_eq!((l.status, l.json["state"].as_str()), (200, Some("ok")), "{}", l.json);
    let files = gh.files(REPO);
    assert!(files["README.md"].contains("HITOP"), "{files:?}");
    assert!(files.contains_key("shop/hitop.yaml") && files.contains_key("shop/requests/list-orders.yaml"), "{:?}", files.keys());
    let head = gh.head(REPO);
    assert!(head.message.starts_with("hitop: add"), "{}", head.message);
    assert_eq!(head.parents.len(), 1, "on top of the README commit");
}
