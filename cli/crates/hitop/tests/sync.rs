//! `hitop sync` end to end: two clients with separate config directories and key files against a
//! real sync server on localhost (the sync-server binary from `HITOP_SYNC_SERVER_BIN`; skipped
//! without it locally, required when `CI` is set). Create, edit, conflict, resolve, delete; teams
//! (invite, accept, trust, add, remove with rotation); a runner credential; the audit log; and
//! `login --sso` against Keycloak when `HITOP_SYNC_TEST_KEYCLOAK` is set (sync-server/tests/keycloak.rs).

use std::io::{BufRead, BufReader};
use std::path::{Path, PathBuf};
use std::process::{Child, Stdio};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use assert_cmd::Command;
use tempfile::TempDir;

struct SyncServer {
    child: Child,
    url: String,
    log: Arc<Mutex<String>>,
    dir: TempDir,
}

impl Drop for SyncServer {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

fn free_port() -> u16 {
    std::net::TcpListener::bind("127.0.0.1:0").unwrap().local_addr().unwrap().port()
}

// The child is waited for in SyncServer's Drop, which clippy can't see.
#[allow(clippy::zombie_processes)]
fn server() -> Option<SyncServer> {
    server_with(&[])
}

#[allow(clippy::zombie_processes)]
fn server_with(envs: &[(&str, String)]) -> Option<SyncServer> {
    let Some(bin) = std::env::var_os("HITOP_SYNC_SERVER_BIN") else {
        assert!(std::env::var_os("CI").is_none(), "HITOP_SYNC_SERVER_BIN must be set on CI (cargo build in sync-server/)");
        eprintln!("HITOP_SYNC_SERVER_BIN not set: skipped");
        return None;
    };
    let dir = TempDir::new().unwrap();
    let port = free_port();
    let url = format!("http://127.0.0.1:{port}");
    let mut child = std::process::Command::new(bin)
        .env("RWS_CONFIG_IP", "127.0.0.1")
        .env("RWS_CONFIG_PORT", port.to_string())
        .env("HITOP_SYNC_PUBLIC_URL", &url)
        // Passkeys need a domain name as the relying party id (not used here).
        .env("HITOP_SYNC_RP_ID", "localhost")
        .env("HITOP_SYNC_RP_ORIGINS", format!("http://localhost:{port}"))
        .env("HITOP_SYNC_DATABASE", format!("sqlite:{}", dir.path().join("sync.db").display()))
        .env("HITOP_SYNC_MAIL", "log")
        .envs(envs.iter().map(|(k, v)| (*k, v.as_str())))
        .current_dir(dir.path())
        .stdout(Stdio::null())
        .stderr(Stdio::piped())
        .spawn()
        .expect("start the sync server");
    let log = Arc::new(Mutex::new(String::new()));
    let stderr = child.stderr.take().unwrap();
    let sink = log.clone();
    std::thread::spawn(move || {
        for line in BufReader::new(stderr).lines().map_while(Result::ok) {
            let mut l = sink.lock().unwrap();
            l.push_str(&line);
            l.push('\n');
        }
    });
    for _ in 0..100 {
        if reqwest::blocking::get(format!("{url}/healthz")).is_ok_and(|r| r.status().is_success()) {
            return Some(SyncServer { child, url, log, dir });
        }
        std::thread::sleep(Duration::from_millis(100));
    }
    let _ = child.kill();
    let _ = child.wait();
    panic!("the sync server didn't start: {}", log.lock().unwrap());
}

impl SyncServer {
    /// The code in the last mail to `email` (the server prints mails with HITOP_SYNC_MAIL=log).
    fn code_for(&self, email: &str, after: &str) -> String {
        for _ in 0..50 {
            let log = self.log.lock().unwrap().clone();
            if let Some(mail) = log.rsplit(&format!("mail to {email}:")).next().filter(|_| log.contains(&format!("mail to {email}:"))) {
                if let Some(line) = mail.lines().find(|l| l.contains(after)) {
                    return line.rsplit(['#', ' ']).next().unwrap().trim().to_string();
                }
            }
            std::thread::sleep(Duration::from_millis(50));
        }
        panic!("no mail to {email}");
    }

    fn database(&self) -> Vec<u8> {
        let mut all = Vec::new();
        for e in std::fs::read_dir(self.dir.path()).unwrap() {
            all.extend(std::fs::read(e.unwrap().path()).unwrap_or_default());
        }
        all
    }
}

/// One person on one computer: its own config directory and key file.
struct Client {
    home: TempDir,
    server: String,
}

struct Out {
    code: i32,
    stdout: String,
    stderr: String,
}

impl Client {
    fn new(server: &SyncServer) -> Client {
        Client { home: TempDir::new().unwrap(), server: server.url.clone() }
    }

    fn cmd(&self) -> Command {
        let mut c = Command::cargo_bin("hitop").unwrap();
        c.env("HOME", self.home.path())
            .env("XDG_CONFIG_HOME", self.home.path().join("config"))
            .env("HITOP_KEYCHAIN", "off")
            .env("HITOP_SYNC_KEY_FILE", self.home.path().join("sync-key.json"))
            .env("HITOP_SYNC_SERVER", &self.server)
            .env_remove("HITOP_SYNC_RUNNER")
            .env("NO_COLOR", "1")
            .current_dir(self.home.path());
        c
    }

    fn run(&self, args: &[&str]) -> Out {
        Self::out(self.cmd().args(args).output().unwrap())
    }

    fn out(o: std::process::Output) -> Out {
        Out { code: o.status.code().unwrap_or(-1), stdout: String::from_utf8_lossy(&o.stdout).into(), stderr: String::from_utf8_lossy(&o.stderr).into() }
    }

    fn ok(&self, args: &[&str]) -> String {
        let o = self.run(args);
        assert_eq!(o.code, 0, "hitop {}\nstdout:\n{}\nstderr:\n{}", args.join(" "), o.stdout, o.stderr);
        o.stdout
    }

    fn folder(&self, name: &str) -> PathBuf {
        self.home.path().join(name)
    }

    /// Signs in; returns this device's key fingerprint.
    fn login(&self, server: &SyncServer, email: &str) -> String {
        let first = self.ok(&["sync", "login", "--email", email]);
        assert!(first.contains("--code"), "{first}");
        let code = server.code_for(email, "/signin#");
        let out = self.ok(&["sync", "login", "--email", email, "--code", &code]);
        out.lines().find_map(|l| l.split("key fingerprint ").nth(1)).unwrap().trim().to_string()
    }
}

fn request(name: &str, url: &str) -> String {
    format!("name: {name}\nmethod: GET\nurl: {url}\n")
}

fn write(dir: &Path, rel: &str, text: &str) {
    let p = dir.join(rel);
    std::fs::create_dir_all(p.parent().unwrap()).unwrap();
    std::fs::write(p, text).unwrap();
}

fn read(dir: &Path, rel: &str) -> String {
    std::fs::read_to_string(dir.join(rel)).unwrap()
}

/// The synced files of a folder (what both sides must agree on).
fn snapshot(dir: &Path) -> Vec<(String, String)> {
    fn walk(root: &Path, d: &Path, out: &mut Vec<(String, String)>) {
        for e in std::fs::read_dir(d).unwrap() {
            let p = e.unwrap().path();
            if p.is_dir() {
                walk(root, &p, out);
            } else {
                let rel = p.strip_prefix(root).unwrap().to_string_lossy().replace('\\', "/");
                if rel.ends_with(".yaml") {
                    out.push((rel, std::fs::read_to_string(&p).unwrap()));
                }
            }
        }
    }
    let mut out = vec![];
    walk(dir, dir, &mut out);
    out.sort();
    out
}

#[test]
fn two_clients_sync_a_folder_end_to_end() {
    let Some(server) = server() else { return };
    let (ada, bob) = (Client::new(&server), Client::new(&server));
    let ada_fp = ada.login(&server, "ada@example.com");
    let bob_fp = bob.login(&server, "bob@example.com");
    assert_ne!(ada_fp, bob_fp);
    assert!(ada.ok(&["sync", "whoami"]).contains(&ada_fp));

    // Ada shares a collection folder.
    let a = ada.folder("shop");
    ada.ok(&["init", a.to_str().unwrap(), "--name", "Shop"]);
    write(&a, "requests/list-orders.yaml", &request("List orders", "https://internal.example.test/orders?unique=7f3a9c"));
    write(&a, "environments/prod.yaml", "variables:\n  host: api.example.test\n  token:\n    secret: true\n");
    let out = ada.ok(&["sync", "init", "-c", a.to_str().unwrap()]);
    let ws = out.split("workspace ").nth(1).unwrap().split_whitespace().next().unwrap().to_string();
    assert_eq!(ws.len(), 32);
    // A second init is refused; status is clean.
    assert_eq!(ada.run(&["sync", "init", "-c", a.to_str().unwrap()]).code, 2);
    assert!(ada.ok(&["sync", "status", "-c", a.to_str().unwrap()]).contains("up to date"));

    // Invite Bob by mail; he accepts; Ada checks his fingerprint and adds him.
    assert!(ada.ok(&["sync", "invite", "bob@example.com", "--role", "editor", "-c", a.to_str().unwrap()]).contains("mailed"));
    let code = server.code_for("bob@example.com", "hitop sync accept");
    let accepted = bob.ok(&["sync", "accept", &code]);
    assert!(accepted.contains(&bob_fp) && accepted.contains(&ws));
    let members = ada.ok(&["sync", "members", "-c", a.to_str().unwrap()]);
    assert!(members.contains("Invite accepted by bob@example.com") && members.contains(&bob_fp), "{members}");
    // Without trusting his key first, adding him is refused (no terminal here to ask).
    let refused = ada.run(&["sync", "add", "bob@example.com", "-c", a.to_str().unwrap()]);
    assert_eq!(refused.code, 2);
    assert!(refused.stderr.contains(&bob_fp), "{}", refused.stderr);
    ada.ok(&["sync", "add", "bob@example.com", "--trust", &bob_fp, "-c", a.to_str().unwrap()]);

    // Bob links an empty folder and gets everything.
    let b = bob.folder("shop");
    bob.ok(&["sync", "link", &ws, "-c", b.to_str().unwrap()]);
    assert_eq!(snapshot(&b), snapshot(&a));

    // Edit on one side, pull on the other.
    write(&b, "requests/list-orders.yaml", &request("List orders", "https://internal.example.test/v2/orders"));
    let out = bob.ok(&["sync", "push", "-c", b.to_str().unwrap()]);
    assert!(out.contains("pushed  requests/list-orders.yaml"), "{out}");
    assert!(ada.ok(&["sync", "pull", "-c", a.to_str().unwrap()]).contains("pulled  requests/list-orders.yaml"));
    assert_eq!(snapshot(&a), snapshot(&b));

    // Create on one side.
    write(&a, "requests/orders/create-order.yaml", &request("Create order", "https://internal.example.test/orders"));
    ada.ok(&["sync", "push", "-c", a.to_str().unwrap()]);
    bob.ok(&["sync", "pull", "-c", b.to_str().unwrap()]);
    assert_eq!(read(&b, "requests/orders/create-order.yaml"), request("Create order", "https://internal.example.test/orders"));

    // Both edit the same request: a conflict for the second.
    write(&a, "requests/list-orders.yaml", &request("List orders", "https://ada.example.test"));
    write(&b, "requests/list-orders.yaml", &request("List orders", "https://bob.example.test"));
    ada.ok(&["sync", "push", "-c", a.to_str().unwrap()]);
    let c = bob.run(&["sync", "push", "-c", b.to_str().unwrap()]);
    assert_eq!(c.code, 1, "{}", c.stdout);
    assert!(c.stdout.contains("CONFLICT requests/list-orders.yaml"), "{}", c.stdout);
    assert_eq!(read(&b, "requests/list-orders.yaml"), request("List orders", "https://bob.example.test"), "mine stays");
    assert_eq!(bob.run(&["sync", "status", "-c", b.to_str().unwrap()]).code, 1);
    let both = bob.ok(&["sync", "resolve", "requests/list-orders.yaml", "--show", "-c", b.to_str().unwrap()]);
    assert!(both.contains("https://ada.example.test") && both.contains("https://bob.example.test") && both.contains("v2/orders"), "{both}");
    // Take theirs.
    bob.ok(&["sync", "resolve", "requests/list-orders.yaml", "--theirs", "-c", b.to_str().unwrap()]);
    assert_eq!(read(&b, "requests/list-orders.yaml"), request("List orders", "https://ada.example.test"));
    assert!(bob.ok(&["sync", "push", "-c", b.to_str().unwrap()]).contains("Up to date"));

    // Again, keep mine.
    write(&a, "requests/list-orders.yaml", &request("List orders", "https://ada2.example.test"));
    write(&b, "requests/list-orders.yaml", &request("List orders", "https://bob2.example.test"));
    ada.ok(&["sync", "push", "-c", a.to_str().unwrap()]);
    assert_eq!(bob.run(&["sync", "push", "-c", b.to_str().unwrap()]).code, 1);
    bob.ok(&["sync", "resolve", "requests/list-orders.yaml", "--mine", "-c", b.to_str().unwrap()]);
    bob.ok(&["sync", "push", "-c", b.to_str().unwrap()]);
    ada.ok(&["sync", "pull", "-c", a.to_str().unwrap()]);
    assert_eq!(read(&a, "requests/list-orders.yaml"), request("List orders", "https://bob2.example.test"));

    // Delete on one side.
    std::fs::remove_file(b.join("requests/orders/create-order.yaml")).unwrap();
    bob.ok(&["sync", "push", "-c", b.to_str().unwrap()]);
    assert!(ada.ok(&["sync", "pull", "-c", a.to_str().unwrap()]).contains("deleted requests/orders/create-order.yaml"));
    assert!(!a.join("requests/orders/create-order.yaml").exists());
    assert_eq!(snapshot(&a), snapshot(&b));

    // The server holds no plaintext: no URL, no file name, no variable.
    let db = server.database();
    for needle in ["internal.example.test", "bob2.example.test", "list-orders", "api.example.test", "List orders"] {
        assert!(!db.windows(needle.len()).any(|w| w == needle.as_bytes()), "{needle} is in the server's database");
    }

    // A viewer can pull but not push.
    let cy = Client::new(&server);
    let cy_fp = cy.login(&server, "cy@example.com");
    ada.ok(&["sync", "invite", "cy@example.com", "--role", "viewer", "-c", a.to_str().unwrap()]);
    cy.ok(&["sync", "accept", &server.code_for("cy@example.com", "hitop sync accept")]);
    ada.ok(&["sync", "add", "cy@example.com", "--trust", &cy_fp, "-c", a.to_str().unwrap()]);
    let cf = cy.folder("shop");
    cy.ok(&["sync", "link", &ws, "-c", cf.to_str().unwrap()]);
    write(&cf, "requests/list-orders.yaml", &request("List orders", "https://cy.example.test"));
    let o = cy.run(&["sync", "push", "-c", cf.to_str().unwrap()]);
    assert_eq!(o.code, 2);
    assert!(o.stderr.contains("role"), "{}", o.stderr);

    // A runner credential pulls read-only into a fresh folder.
    let cred = ada.ok(&["sync", "runner", "create", "--name", "nightly", "-c", a.to_str().unwrap()]).trim().to_string();
    assert!(cred.starts_with("hitop-runner-1."));
    let ci = Client::new(&server);
    let r = ci.folder("checkout");
    // A CI checkout that already holds an older copy, and a request the workspace doesn't have:
    // the runner's pull makes the folder the workspace's, without conflicts.
    write(&r, "requests/list-orders.yaml", &request("List orders", "https://stale.example.test"));
    write(&r, "requests/stray.yaml", &request("Stray", "https://stray.example.test"));
    write(&r, "README.md", "not synced, left alone\n");
    let o = Client::out(ci.cmd().env("HITOP_SYNC_RUNNER", &cred).args(["sync", "pull", "-c", r.to_str().unwrap()]).output().unwrap());
    assert_eq!(o.code, 0, "{}\n{}", o.stdout, o.stderr);
    assert_eq!(snapshot(&r), snapshot(&a));
    assert!(!r.join("requests/stray.yaml").exists() && r.join("README.md").exists());
    let o = Client::out(ci.cmd().env("HITOP_SYNC_RUNNER", &cred).args(["sync", "push", "-c", r.to_str().unwrap()]).output().unwrap());
    assert_eq!(o.code, 2);
    assert!(o.stderr.contains("read-only"));

    // Ada removes Bob: the key rotates and every file is re-encrypted; Bob is out.
    let out = ada.ok(&["sync", "remove", "bob@example.com", "-c", a.to_str().unwrap()]);
    assert!(out.contains("epoch 2"), "{out}");
    assert!(out.contains(&format!("re-encrypted {} file(s)", snapshot(&a).len())), "{out}");
    let o = bob.run(&["sync", "pull", "-c", b.to_str().unwrap()]);
    assert_eq!(o.code, 2);
    assert!(o.stderr.contains("no such workspace"), "{}", o.stderr);
    // Everyone left still syncs, under the new key.
    write(&a, "requests/after.yaml", &request("After", "https://after.example.test"));
    ada.ok(&["sync", "push", "-c", a.to_str().unwrap()]);
    assert!(cy.ok(&["sync", "pull", "-c", cf.to_str().unwrap()]).contains("pulled  requests/after.yaml"));
    let o = Client::out(ci.cmd().env("HITOP_SYNC_RUNNER", &cred).args(["sync", "pull", "-c", r.to_str().unwrap()]).output().unwrap());
    assert_eq!(o.code, 0, "{}", o.stderr);
    assert!(r.join("requests/after.yaml").exists());

    // The owner reads the audit log of all that and checks its chain on this computer.
    let log = ada.ok(&["sync", "audit", "--limit", "0", "-c", a.to_str().unwrap()]);
    for event in ["workspace_created", "invite_created", "invite_accepted", "member_added", "runner_created", "member_removed", "key_rotated"] {
        assert!(log.contains(event), "{event} in\n{log}");
    }
    let rows: serde_json::Value = serde_json::from_str(&ada.ok(&["sync", "audit", "--json", "--event", "member_removed", "-c", a.to_str().unwrap()])).unwrap();
    assert_eq!(rows.as_array().unwrap().len(), 1);
    assert_eq!(rows[0]["detail"]["email"], "bob@example.com");
    assert!(ada.ok(&["sync", "audit", "--verify", "--limit", "1", "-c", a.to_str().unwrap()]).contains("audit log intact"));
    // Twice: the second run also checks the head the first one saw.
    assert!(ada.ok(&["sync", "audit", "--verify", "--limit", "1", "-c", a.to_str().unwrap()]).contains("audit log intact"));
    let mine = ada.ok(&["sync", "audit", "--mine"]);
    assert!(mine.contains("signin") && mine.contains("device_key_added"), "{mine}");
    // Only owners: a viewer is refused, a removed member sees nothing.
    let o = cy.run(&["sync", "audit", "-c", cf.to_str().unwrap()]);
    assert_eq!(o.code, 2);
    assert!(o.stderr.contains("owner"), "{}", o.stderr);
    assert_eq!(bob.run(&["sync", "audit", "-c", b.to_str().unwrap()]).code, 2);
    // This server has no SSO.
    let o = Client::new(&server).run(&["sync", "login", "--sso", "--server", &server.url]);
    assert_eq!(o.code, 2);
    assert!(o.stderr.contains("no single sign-on"), "{}", o.stderr);
}

#[test]
fn sync_commands_explain_what_is_missing() {
    let home = TempDir::new().unwrap();
    let cmd = |args: &[&str]| {
        let o = Command::cargo_bin("hitop")
            .unwrap()
            .env("HOME", home.path())
            .env("XDG_CONFIG_HOME", home.path().join("config"))
            .env("HITOP_KEYCHAIN", "off")
            .env_remove("HITOP_SYNC_RUNNER")
            .args(args)
            .output()
            .unwrap();
        (o.status.code().unwrap(), String::from_utf8_lossy(&o.stderr).to_string())
    };
    let (code, err) = cmd(&["sync", "status", "-c", "."]);
    assert_eq!(code, 2);
    assert!(err.contains("hitop sync login"), "{err}");
    let (code, err) = cmd(&["sync", "login", "--email", "a@example.com", "--server", "http://sync.example.com"]);
    assert_eq!(code, 2);
    assert!(err.contains("https"), "{err}");
    // Codes are base64url and may start with '-'.
    let (code, err) = cmd(&["sync", "login", "--email", "a@example.com", "--code", "-iPhD7m9", "--server", "http://sync.example.com"]);
    assert_eq!(code, 2);
    assert!(err.contains("https") && !err.contains("unexpected argument"), "{err}");
    let (_, err) = cmd(&["sync", "accept", "-iPhD7m9"]);
    assert!(!err.contains("unexpected argument"), "{err}");
    let o = Command::cargo_bin("hitop").unwrap().env("HITOP_SYNC_RUNNER", "nonsense").args(["sync", "pull"]).output().unwrap();
    assert_eq!(o.status.code(), Some(2));
    assert!(String::from_utf8_lossy(&o.stderr).contains("runner credential"));
}

/// Keycloak's login form, as a browser fills it: returns where Keycloak redirects to.
fn keycloak_login(authorize: &str, username: &str, password: &str) -> String {
    let http = reqwest::blocking::Client::builder().redirect(reqwest::redirect::Policy::none()).build().unwrap();
    let res = http.get(authorize).send().unwrap();
    let cookies: Vec<String> = res.headers().get_all("set-cookie").iter().map(|v| v.to_str().unwrap().split(';').next().unwrap().to_string()).collect();
    let page = res.text().unwrap();
    let form = &page[page.find("id=\"kc-form-login\"").expect("Keycloak's login form")..];
    let action = &form[form.find("action=\"").unwrap() + 8..];
    let action = action[..action.find('"').unwrap()].replace("&amp;", "&");
    let res = http.post(action).header("Cookie", cookies.join("; ")).form(&[("username", username), ("password", password)]).send().unwrap();
    assert_eq!(res.status().as_u16(), 302);
    res.headers()["location"].to_str().unwrap().to_string()
}

#[test]
fn sso_login_through_keycloak() {
    let Some(kc) = std::env::var("HITOP_SYNC_TEST_KEYCLOAK").ok().filter(|s| !s.is_empty()) else {
        eprintln!("HITOP_SYNC_TEST_KEYCLOAK not set: skipped");
        return;
    };
    let secret = TempDir::new().unwrap();
    let secret_file = secret.path().join("client-secret");
    std::fs::write(&secret_file, "hitop-sync-test-secret\n").unwrap();
    let Some(server) = server_with(&[
        ("HITOP_SYNC_OIDC_ISSUER", format!("{}/realms/hitop", kc.trim_end_matches('/'))),
        ("HITOP_SYNC_OIDC_CLIENT_ID", "hitop-sync".into()),
        ("HITOP_SYNC_OIDC_CLIENT_SECRET_FILE", secret_file.display().to_string()),
        ("HITOP_SYNC_OIDC_ALLOWED_DOMAINS", "corp.test".into()),
        ("HITOP_SYNC_SSO_REQUIRED", "1".into()),
    ]) else {
        return;
    };
    let alice = Client::new(&server);
    let mut child = std::process::Command::new(assert_cmd::cargo::cargo_bin("hitop"));
    for (k, v) in alice.cmd().get_envs() {
        match v {
            Some(v) => child.env(k, v),
            None => child.env_remove(k),
        };
    }
    let mut child = child
        .current_dir(alice.home.path())
        .env("HITOP_NO_BROWSER", "1")
        .args(["sync", "login", "--sso", "--name", "ci laptop"])
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    // The CLI prints the provider's link (no browser here) and listens on loopback.
    let mut stderr = BufReader::new(child.stderr.take().unwrap());
    let mut link = String::new();
    while !link.contains("/protocol/openid-connect/auth") {
        link.clear();
        assert!(stderr.read_line(&mut link).unwrap() > 0, "no sign-in link");
    }
    // The browser: Keycloak's login, back to the server's callback, then to the CLI's listener.
    let callback = keycloak_login(link.trim(), "alice", "alice-test-pw");
    assert!(callback.starts_with(&format!("{}/v1/auth/sso/callback?", server.url)), "{callback}");
    let http = reqwest::blocking::Client::builder().redirect(reqwest::redirect::Policy::none()).build().unwrap();
    let res = http.get(&callback).send().unwrap();
    assert_eq!(res.status().as_u16(), 302, "{}", res.text().unwrap_or_default());
    let to_cli = res.headers()["location"].to_str().unwrap().to_string();
    assert!(to_cli.starts_with("http://127.0.0.1:") && to_cli.contains("?sso="), "{to_cli}");
    let page = reqwest::blocking::get(&to_cli).unwrap().text().unwrap();
    assert!(page.contains("Signed in"), "{page}");
    let out = child.wait_with_output().unwrap();
    assert!(out.status.success(), "{}", String::from_utf8_lossy(&out.stdout));
    assert!(String::from_utf8_lossy(&out.stdout).contains("as alice@corp.test"));
    assert!(alice.ok(&["sync", "whoami"]).contains("alice@corp.test"));
    // SSO is required for corp.test: no mailed links.
    let o = alice.run(&["sync", "login", "--email", "alice@corp.test"]);
    assert_eq!(o.code, 2);
    assert!(o.stderr.contains("single sign-on"), "{}", o.stderr);
    // The sign-in is in her account log.
    let mine = alice.ok(&["sync", "audit", "--mine", "--json"]);
    assert!(mine.contains("\"sso_linked\"") && mine.contains("\"method\": \"sso\""), "{mine}");
}
