//! `hitop sync` end to end: two clients with separate config directories and key files against a
//! real sync server on localhost (the sync-server binary from `HITOP_SYNC_SERVER_BIN`; skipped
//! without it locally, required when `CI` is set). Create, edit, conflict, resolve, delete; teams
//! (invite, accept, trust, add, remove with rotation); a runner credential; the audit log;
//! authenticated key wraps against a malicious server (the test edits the server's database) and
//! the upgrade of a hitop 1.1 workspace; and `login --sso` against Keycloak when
//! `HITOP_SYNC_TEST_KEYCLOAK` is set (sync-server/tests/keycloak.rs).

use std::io::{BufRead, BufReader};
use std::path::{Path, PathBuf};
use std::process::{Child, Stdio};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use assert_cmd::Command;
use base64::engine::general_purpose::URL_SAFE_NO_PAD;
use base64::Engine;
use hitop_sync_crypto as crypto;
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

    /// The server's database, to play a malicious server.
    fn db(&self) -> rusqlite::Connection {
        let db = rusqlite::Connection::open(self.dir.path().join("sync.db")).unwrap();
        db.busy_timeout(Duration::from_secs(5)).unwrap();
        db
    }

    fn wrap(&self, ws: &str, key_id: &str, epoch: i64) -> Vec<u8> {
        self.db().query_row("SELECT wrapped FROM wraps WHERE workspace_id = ?1 AND key_id = ?2 AND epoch = ?3", rusqlite::params![ws, key_id, epoch], |r| r.get(0)).unwrap()
    }

    /// Stores `wrapped` as the wrap of `key_id` for `epoch` (replacing or adding it).
    fn set_wrap(&self, ws: &str, key_id: &str, epoch: i64, wrapped: &[u8]) {
        self.db().execute("INSERT OR REPLACE INTO wraps (workspace_id, epoch, key_id, wrapped) VALUES (?1, ?2, ?3, ?4)", rusqlite::params![ws, epoch, key_id, wrapped]).unwrap();
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

    fn config(&self, file: &str) -> PathBuf {
        self.home.path().join("config/hitop/sync").join(file)
    }

    /// This client's device: its key id and key pair (from account.json and the key file).
    fn device(&self) -> (String, crypto::DeviceKeyPair) {
        let account: serde_json::Value = serde_json::from_str(&std::fs::read_to_string(self.config("account.json")).unwrap()).unwrap();
        let secret: serde_json::Value = serde_json::from_str(&std::fs::read_to_string(self.home.path().join("sync-key.json")).unwrap()).unwrap();
        let keys = crypto::DeviceKeyPair::from_secret_bytes(&URL_SAFE_NO_PAD.decode(secret["secret"].as_str().unwrap()).unwrap()).unwrap();
        (account["keyId"].as_str().unwrap().to_string(), keys)
    }

    /// Signs in with a mailed code; returns this device's key fingerprint.
    fn login(&self, server: &SyncServer, email: &str) -> String {
        let first = self.ok(&["sync", "login", "--email", email, "--code"]);
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

    // Invite Bob by mail; Ada also hands him the code with her key's fingerprint (the mail can't
    // carry it: the server writes it); he accepts; Ada checks his fingerprint and adds him.
    let invited = ada.ok(&["sync", "invite", "bob@example.com", "--role", "editor", "-c", a.to_str().unwrap()]);
    assert!(invited.contains("mailed") && invited.contains(&ada_fp), "{invited}");
    let code = invited.lines().find_map(|l| l.trim().strip_prefix("hitop sync accept ")).unwrap().to_string();
    let mailed = server.code_for("bob@example.com", "hitop sync accept");
    assert_eq!(code, format!("{mailed}.{}", ada_fp.replace(' ', "")));
    let accepted = bob.ok(&["sync", "accept", &code]);
    assert!(accepted.contains(&bob_fp) && accepted.contains(&ws) && accepted.contains(&ada_fp), "{accepted}");
    let members = ada.ok(&["sync", "members", "-c", a.to_str().unwrap()]);
    assert!(members.contains("Invite accepted by bob@example.com") && members.contains(&bob_fp), "{members}");
    // Without trusting his key first, adding him is refused (no terminal here to ask).
    let refused = ada.run(&["sync", "add", "bob@example.com", "-c", a.to_str().unwrap()]);
    assert_eq!(refused.code, 2);
    assert!(refused.stderr.contains(&bob_fp), "{}", refused.stderr);
    ada.ok(&["sync", "add", "bob@example.com", "--trust", &bob_fp, "-c", a.to_str().unwrap()]);

    // Bob links an empty folder and gets everything: the key came from the device his invite
    // code named.
    let b = bob.folder("shop");
    let o = bob.run(&["sync", "link", &ws, "-c", b.to_str().unwrap()]);
    assert_eq!(o.code, 0, "{}", o.stderr);
    assert!(o.stderr.contains("trusted device") && o.stderr.contains(&ada_fp), "{}", o.stderr);
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

    // A viewer can pull but not push. Cy accepts the mailed code, which names no key: her hitop
    // refuses the workspace key until she has compared Ada's fingerprint and trusted it.
    let cy = Client::new(&server);
    let cy_fp = cy.login(&server, "cy@example.com");
    ada.ok(&["sync", "invite", "cy@example.com", "--role", "viewer", "-c", a.to_str().unwrap()]);
    assert!(cy.ok(&["sync", "accept", &server.code_for("cy@example.com", "hitop sync accept")]).contains("doesn't name the inviter's key"));
    ada.ok(&["sync", "add", "cy@example.com", "--trust", &cy_fp, "-c", a.to_str().unwrap()]);
    let cf = cy.folder("shop");
    let o = cy.run(&["sync", "link", &ws, "-c", cf.to_str().unwrap()]);
    assert_eq!(o.code, 2);
    assert!(o.stderr.contains("haven't trusted") && o.stderr.contains(&format!("hitop sync trust '{ada_fp}'")), "{}", o.stderr);
    assert!(!cf.join("hitop.yaml").exists());
    cy.ok(&["sync", "trust", &ada_fp, "-c", cf.to_str().unwrap()]);
    cy.ok(&["sync", "pull", "-c", cf.to_str().unwrap()]);
    assert_eq!(snapshot(&cf), snapshot(&a));
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

/// Ada shares a folder, invites Bob with a link code carrying her fingerprint, adds him. Returns
/// the workspace id, Ada's folder and Bob's (not linked yet).
fn share_with_bob(server: &SyncServer, ada: &Client, bob: &Client) -> (String, PathBuf, PathBuf) {
    let bob_fp = bob.login(server, "bob@example.com");
    let a = ada.folder("shop");
    ada.ok(&["init", a.to_str().unwrap(), "--name", "Shop"]);
    write(&a, "requests/list-orders.yaml", &request("List orders", "https://internal.example.test/orders"));
    let out = ada.ok(&["sync", "init", "-c", a.to_str().unwrap()]);
    let ws = out.split("workspace ").nth(1).unwrap().split_whitespace().next().unwrap().to_string();
    let invited = ada.ok(&["sync", "invite", "--role", "editor", "-c", a.to_str().unwrap()]);
    let link = invited.lines().find(|l| l.contains("/invite#")).unwrap();
    bob.ok(&["sync", "accept", link.trim()]);
    ada.ok(&["sync", "add", "bob@example.com", "--trust", &bob_fp, "-c", a.to_str().unwrap()]);
    (ws, a, bob.folder("shop"))
}

/// A sync server that hands out workspace keys of its choosing is refused: an unauthenticated
/// wrap, a wrap forged in the owner's name, one from a device the server made up, a swapped key
/// for an epoch already known, a hidden rotation, an old wrap replayed after a rotation, and a
/// fake next epoch. Nothing is written to the folder, nothing is pushed under such a key.
#[test]
fn a_malicious_server_cant_hand_out_its_own_workspace_key() {
    let Some(server) = server() else { return };
    let (ada, bob) = (Client::new(&server), Client::new(&server));
    let ada_fp = ada.login(&server, "ada@example.com");
    let (ws, a, b) = share_with_bob(&server, &ada, &bob);
    let (ada_key, ada_keys) = ada.device();
    let (bob_key, bob_keys) = bob.device();
    let honest = server.wrap(&ws, &bob_key, 1);
    let theirs = crypto::WorkspaceKey::generate();
    let bob_pk = bob_keys.public_key();
    let to_bob = crypto::Recipient { key_id: &bob_key, public_key: &bob_pk };
    let refused = |what: &str, wrapped: &[u8], needle: &str| {
        server.set_wrap(&ws, &bob_key, 1, wrapped);
        let o = bob.run(&["sync", "link", &ws, "-c", b.to_str().unwrap()]);
        assert_eq!(o.code, 2, "{what}: {}", o.stdout);
        assert!(o.stderr.contains(needle), "{what}: {}", o.stderr);
        assert!(!b.join("hitop.yaml").exists(), "{what}: nothing written");
    };

    // 1. An unauthenticated (hitop 1.1 style) wrap of the server's key: Bob joined with hitop 1.2.
    refused("legacy wrap", &crypto::wrap_key(&bob_pk, &ws, 1, &theirs).unwrap(), "without authentication");
    // 2. A wrap in Ada's name, made with the server's own key pair.
    let server_keys = crypto::DeviceKeyPair::generate();
    let forged = crypto::seal_wrap(&crypto::Device { key_id: &ada_key, keys: &server_keys }, &to_bob, &ws, 1, 1, &theirs).unwrap();
    refused("forged in Ada's name", &forged, "doesn't verify");
    // 3. A device the server adds to Ada's account and wraps from: not the key the code named.
    let ada_user: String = server.db().query_row("SELECT user_id FROM device_keys WHERE id = ?1", [&ada_key], |r| r.get(0)).unwrap();
    server.db().execute("INSERT INTO device_keys (id, user_id, name, public_key, created_at) VALUES ('ffffffffffffffffffffffffffffffff', ?1, 'Ada new laptop', ?2, 0)", rusqlite::params![ada_user, server_keys.public_key().to_vec()]).unwrap();
    let made_up = crypto::seal_wrap(&crypto::Device { key_id: "ffffffffffffffffffffffffffffffff", keys: &server_keys }, &to_bob, &ws, 1, 1, &theirs).unwrap();
    refused("a made-up device", &made_up, &crypto::fingerprint(&server_keys.public_key()));
    // The honest wrap works.
    server.set_wrap(&ws, &bob_key, 1, &honest);
    bob.ok(&["sync", "link", &ws, "-c", b.to_str().unwrap()]);
    assert_eq!(snapshot(&b), snapshot(&a));

    // 4. A different key for epoch 1, even authenticated by Ada's real key (as a member colluding
    //    with the server could make): Bob already holds epoch 1's key.
    let swapped = crypto::seal_wrap(&crypto::Device { key_id: &ada_key, keys: &ada_keys }, &to_bob, &ws, 1, 1, &theirs).unwrap();
    server.set_wrap(&ws, &bob_key, 1, &swapped);
    let o = bob.run(&["sync", "pull", "-c", b.to_str().unwrap()]);
    assert_eq!(o.code, 2);
    assert!(o.stderr.contains("SECURITY") && o.stderr.contains("not the one this device accepted before"), "{}", o.stderr);
    server.set_wrap(&ws, &bob_key, 1, &honest);

    // A real rotation: Ada removes Cy (epoch 2); Bob follows.
    let cy = Client::new(&server);
    let cy_fp = cy.login(&server, "cy@example.com");
    let invited = ada.ok(&["sync", "invite", "--role", "viewer", "-c", a.to_str().unwrap()]);
    cy.ok(&["sync", "accept", invited.lines().find(|l| l.contains("/invite#")).unwrap().trim()]);
    ada.ok(&["sync", "add", "cy@example.com", "--trust", &cy_fp, "-c", a.to_str().unwrap()]);
    // The device the server added to Ada's account is shown to her as untrusted, and her client
    // won't wrap the next key for it.
    let members = ada.ok(&["sync", "members", "-c", a.to_str().unwrap()]);
    assert!(members.contains("Ada new laptop") && members.contains("not trusted yet") && members.contains(&ada_fp), "{members}");
    let o = ada.run(&["sync", "remove", "cy@example.com", "-c", a.to_str().unwrap()]);
    assert_eq!(o.code, 2);
    assert!(o.stderr.contains("Ada new laptop") && o.stderr.contains(&crypto::fingerprint(&server_keys.public_key())), "{}", o.stderr);
    server.db().execute("DELETE FROM device_keys WHERE id = 'ffffffffffffffffffffffffffffffff'", []).unwrap();
    assert!(ada.ok(&["sync", "remove", "cy@example.com", "-c", a.to_str().unwrap()]).contains("epoch 2"));
    bob.ok(&["sync", "pull", "-c", b.to_str().unwrap()]);
    let honest2 = server.wrap(&ws, &bob_key, 2);

    // 5. The server hides the rotation (back to epoch 1, whose key Cy has).
    server.db().execute("UPDATE workspaces SET epoch = 1 WHERE id = ?1", [&ws]).unwrap();
    let o = bob.run(&["sync", "push", "-c", b.to_str().unwrap()]);
    assert_eq!(o.code, 2);
    assert!(o.stderr.contains("SECURITY") && o.stderr.contains("rotation is being hidden"), "{}", o.stderr);
    server.db().execute("UPDATE workspaces SET epoch = 2 WHERE id = ?1", [&ws]).unwrap();
    // 6. The old epoch's wrap replayed as the new epoch's.
    server.set_wrap(&ws, &bob_key, 2, &honest);
    let o = bob.run(&["sync", "pull", "-c", b.to_str().unwrap()]);
    assert_eq!(o.code, 2);
    assert!(o.stderr.contains("doesn't verify"), "{}", o.stderr);
    server.set_wrap(&ws, &bob_key, 2, &honest2);
    // 7. A fake next epoch with an unauthenticated wrap of the server's key.
    server.db().execute("UPDATE workspaces SET epoch = 3 WHERE id = ?1", [&ws]).unwrap();
    server.set_wrap(&ws, &bob_key, 3, &crypto::wrap_key(&bob_pk, &ws, 3, &theirs).unwrap());
    write(&b, "requests/secret-plan.yaml", &request("Secret plan", "https://plan.example.test"));
    let o = bob.run(&["sync", "push", "-c", b.to_str().unwrap()]);
    assert_eq!(o.code, 2);
    assert!(o.stderr.contains("without authentication"), "{}", o.stderr);
    let blobs: i64 = server.db().query_row("SELECT COUNT(*) FROM blobs WHERE workspace_id = ?1 AND epoch = 3", [&ws], |r| r.get(0)).unwrap();
    assert_eq!(blobs, 0, "nothing pushed under the server's key");
    server.db().execute("DELETE FROM wraps WHERE workspace_id = ?1 AND epoch = 3", [&ws]).unwrap();
    server.db().execute("UPDATE workspaces SET epoch = 2 WHERE id = ?1", [&ws]).unwrap();
    assert!(bob.ok(&["sync", "push", "-c", b.to_str().unwrap()]).contains("pushed  requests/secret-plan.yaml"));
}

/// A workspace made by hitop 1.1 (format 1 wraps, no key memory on the devices) keeps working
/// with a warning; the owner's next sync replaces the wraps with authenticated ones; a member
/// who never trusted the owner's device keeps the keys already held but must trust it for the
/// next rotation; a hitop 1.1 runner credential keeps its unauthenticated key, with a warning.
#[test]
fn a_hitop_1_1_workspace_keeps_working_and_is_upgraded() {
    let Some(server) = server() else { return };
    let (ada, bob) = (Client::new(&server), Client::new(&server));
    let ada_fp = ada.login(&server, "ada@example.com");
    let (ws, a, b) = share_with_bob(&server, &ada, &bob);
    bob.ok(&["sync", "link", &ws, "-c", b.to_str().unwrap()]);
    let cred = ada.ok(&["sync", "runner", "create", "-c", a.to_str().unwrap()]).trim().to_string();

    // Turn it into what hitop 1.1 left behind: every wrap format 1, no keys.json, Bob never
    // pinned Ada's device, the runner credential has no pins.
    let (ada_key, ada_keys) = ada.device();
    let (bob_key, bob_keys) = bob.device();
    let rows: Vec<(String, i64, Vec<u8>)> = {
        let db = server.db();
        let mut q = db.prepare("SELECT key_id, epoch, wrapped FROM wraps WHERE workspace_id = ?1").unwrap();
        q.query_map([&ws], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?))).unwrap().map(Result::unwrap).collect()
    };
    let body = cred.strip_prefix("hitop-runner-1.").unwrap();
    let mut c: serde_json::Value = serde_json::from_slice(&URL_SAFE_NO_PAD.decode(body).unwrap()).unwrap();
    let runner_keys = crypto::DeviceKeyPair::from_secret_bytes(&URL_SAFE_NO_PAD.decode(c["secret"].as_str().unwrap()).unwrap()).unwrap();
    let runner_key = c["keyId"].as_str().unwrap().to_string();
    c.as_object_mut().unwrap().remove("pins");
    let old_cred = format!("hitop-runner-1.{}", URL_SAFE_NO_PAD.encode(serde_json::to_vec(&c).unwrap()));
    for (key_id, epoch, wrapped) in &rows {
        let me = if *key_id == ada_key { &ada_keys } else if *key_id == bob_key { &bob_keys } else { &runner_keys };
        let key = crypto::open_wrap(me, key_id, &ada_keys.public_key(), &ws, *epoch as u32, wrapped).unwrap().key;
        server.set_wrap(&ws, key_id, *epoch, &crypto::wrap_key(&me.public_key(), &ws, *epoch as u32, &key).unwrap());
    }
    std::fs::remove_file(ada.config("keys.json")).unwrap();
    std::fs::remove_file(bob.config("keys.json")).unwrap();
    let mut pins: serde_json::Value = serde_json::from_str(&std::fs::read_to_string(bob.config("pins.json")).unwrap()).unwrap();
    pins.as_object_mut().unwrap().remove(&ada_key);
    std::fs::write(bob.config("pins.json"), pins.to_string()).unwrap();

    // Both still sync, with a warning.
    let o = bob.run(&["sync", "pull", "-c", b.to_str().unwrap()]);
    assert_eq!(o.code, 0, "{}", o.stderr);
    assert!(o.stderr.contains("without authentication"), "{}", o.stderr);
    // Ada (owner) upgrades every wrap she can: hers and Bob's, not the 1.1 runner's.
    let o = ada.run(&["sync", "pull", "-c", a.to_str().unwrap()]);
    assert_eq!(o.code, 0, "{}", o.stderr);
    assert!(o.stderr.contains("Upgraded 2 workspace key wrap(s)") && o.stderr.contains("hitop 1.1 and keeps an unauthenticated key"), "{}", o.stderr);
    assert_eq!(server.wrap(&ws, &bob_key, 1)[0], 2);
    assert_eq!(server.wrap(&ws, &runner_key, 1).len(), 80);
    assert!(ada.run(&["sync", "pull", "-c", a.to_str().unwrap()]).stderr.is_empty(), "no warning once upgraded");
    // Bob: the same key, now from a device he hasn't trusted: used, with a note.
    let o = bob.run(&["sync", "pull", "-c", b.to_str().unwrap()]);
    assert_eq!(o.code, 0, "{}", o.stderr);
    assert!(o.stderr.contains("re-sent the workspace key") && o.stderr.contains(&ada_fp), "{}", o.stderr);
    // From now on his device refuses unauthenticated wraps for this workspace.
    let current = server.wrap(&ws, &bob_key, 1);
    server.set_wrap(&ws, &bob_key, 1, &crypto::wrap_key(&bob_keys.public_key(), &ws, 1, &crypto::WorkspaceKey::generate()).unwrap());
    let o = bob.run(&["sync", "pull", "-c", b.to_str().unwrap()]);
    assert_eq!(o.code, 2);
    assert!(o.stderr.contains("without authentication"), "{}", o.stderr);
    server.set_wrap(&ws, &bob_key, 1, &current);
    // The 1.1 runner credential still pulls, with a warning to replace it.
    let ci = Client::new(&server);
    let r = ci.folder("checkout");
    let o = Client::out(ci.cmd().env("HITOP_SYNC_RUNNER", &old_cred).args(["sync", "pull", "-c", r.to_str().unwrap()]).output().unwrap());
    assert_eq!(o.code, 0, "{}", o.stderr);
    assert!(o.stderr.contains("made by hitop 1.1"), "{}", o.stderr);
    assert_eq!(snapshot(&r), snapshot(&a));

    // A rotation (Ada removes the runner): Bob must trust Ada's device for the new key.
    let runner_user: String = server.db().query_row("SELECT user_id FROM device_keys WHERE id = ?1", [&runner_key], |r| r.get(0)).unwrap();
    ada.ok(&["sync", "remove", &runner_user, "-c", a.to_str().unwrap()]);
    let o = bob.run(&["sync", "pull", "-c", b.to_str().unwrap()]);
    assert_eq!(o.code, 2);
    assert!(o.stderr.contains("haven't trusted") && o.stderr.contains(&ada_fp), "{}", o.stderr);
    bob.ok(&["sync", "trust", &ada_fp, "-c", b.to_str().unwrap()]);
    bob.ok(&["sync", "pull", "-c", b.to_str().unwrap()]);
    assert_eq!(snapshot(&b), snapshot(&a));
}

/// HITOP Cloud C2 recovery key: Ada makes a recovery phrase, keeps working (she trusts a new
/// member afterwards, which updates the recovery data), then loses her computer. On a new one she
/// signs in and recovers with the phrase: she is her old device again (same key, the key the new
/// sign-in registered is gone), reads the workspace, and her trust decisions came along. A wrong
/// or mistyped phrase recovers nothing; the server never holds the phrase or a key.
#[test]
fn a_lost_device_comes_back_with_its_recovery_phrase() {
    let Some(server) = server() else { return };
    let (ada, bob, cy) = (Client::new(&server), Client::new(&server), Client::new(&server));
    ada.login(&server, "ada@example.com");
    let (ws, a, _) = share_with_bob(&server, &ada, &bob);
    let (old_key, old_keys) = ada.device();
    let out = ada.run(&["sync", "recovery", "create"]);
    assert_eq!(out.code, 0, "{}", out.stderr);
    let phrase = out.stdout.trim().to_string();
    assert_eq!(phrase.split(' ').count(), 24, "{phrase}");
    assert!(out.stderr.contains("Write it down"), "{}", out.stderr);
    assert!(ada.ok(&["sync", "recovery", "status"]).contains("Recovery phrase set"));

    // Later: Cy joins (Ada pins Cy's device), and a file changes. The recovery data follows.
    let cy_fp = cy.login(&server, "cy@example.com");
    let invited = ada.ok(&["sync", "invite", "--role", "viewer", "-c", a.to_str().unwrap()]);
    cy.ok(&["sync", "accept", invited.lines().find(|l| l.contains("/invite#")).unwrap().trim()]);
    ada.ok(&["sync", "add", "cy@example.com", "--trust", &cy_fp, "-c", a.to_str().unwrap()]);
    write(&a, "requests/new-order.yaml", &request("New order", "https://internal.example.test/orders/new"));
    ada.ok(&["sync", "push", "-c", a.to_str().unwrap()]);

    // The server holds the blob, never the phrase or the device key.
    let db = server.database();
    assert!(!String::from_utf8_lossy(&db).contains(&phrase), "no phrase on the server");
    let first_word = phrase.split(' ').next().unwrap();
    assert!(!db.windows(32).any(|w| w == old_keys.secret_bytes()), "no device secret on the server");

    // The computer is lost. A new one signs in (registering a new key) and recovers.
    let ada2 = Client::new(&server);
    ada2.login(&server, "ada@example.com");
    let fresh = ada2.device().0;
    assert_ne!(fresh, old_key);
    assert!(ada2.ok(&["sync", "whoami"]).contains("ada@example.com"));

    // A wrong phrase (valid words, another phrase) and a mistyped one recover nothing.
    let other = crypto::RecoveryPhrase::generate().to_words();
    let o = Client::out(ada2.cmd().args(["sync", "recovery", "use", "--phrase-file", "-"]).write_stdin(other).output().unwrap());
    assert_eq!(o.code, 2, "{}", o.stdout);
    assert!(o.stderr.contains("doesn't open"), "{}", o.stderr);
    let typo = phrase.replacen(first_word, if first_word == "zoo" { "abandon" } else { "zoo" }, 1);
    let o = Client::out(ada2.cmd().args(["sync", "recovery", "use"]).write_stdin(format!("{}\n", phrase.split(' ').take(23).collect::<Vec<_>>().join(" "))).output().unwrap());
    assert_eq!(o.code, 2);
    assert!(o.stderr.contains("24 words"), "{}", o.stderr);
    let o = Client::out(ada2.cmd().args(["sync", "recovery", "use"]).write_stdin(typo).output().unwrap());
    assert_eq!(o.code, 2, "a mistyped word");
    assert_eq!(ada2.device().0, fresh, "nothing changed");

    // The right phrase (as typed: other case, line breaks).
    let typed = phrase.to_uppercase().replacen(' ', "\n", 5);
    let o = Client::out(ada2.cmd().args(["sync", "recovery", "use"]).write_stdin(typed).output().unwrap());
    assert_eq!(o.code, 0, "{}\n{}", o.stdout, o.stderr);
    assert!(o.stdout.contains(&crypto::fingerprint(&old_keys.public_key())), "{}", o.stdout);
    let (key, keys) = ada2.device();
    assert_eq!((key.as_str(), keys.public_key()), (old_key.as_str(), old_keys.public_key()), "the old device, back");
    let ada_keys: i64 = server.db().query_row("SELECT COUNT(*) FROM device_keys k JOIN users u ON u.id = k.user_id WHERE u.email = 'ada@example.com'", [], |r| r.get(0)).unwrap();
    assert_eq!(ada_keys, 1, "the key the new sign-in registered is gone");

    // It reads the workspace, and still trusts Bob's and Cy's devices.
    let a2 = ada2.folder("shop");
    ada2.ok(&["sync", "link", &ws, "-c", a2.to_str().unwrap()]);
    assert_eq!(snapshot(&a2), snapshot(&a));
    let members = ada2.ok(&["sync", "members", "-c", a2.to_str().unwrap()]);
    assert!(!members.contains("not trusted"), "{members}");
    assert_eq!(members.matches("trusted").count(), 3, "{members}");
    // And can keep working as the owner: an edit syncs to Bob.
    write(&a2, "requests/new-order.yaml", &request("New order", "https://internal.example.test/orders/v2"));
    ada2.ok(&["sync", "push", "-c", a2.to_str().unwrap()]);
    let b = bob.folder("shop");
    bob.ok(&["sync", "link", &ws, "-c", b.to_str().unwrap()]);
    assert!(read(&b, "requests/new-order.yaml").contains("/orders/v2"));

    // Remove: the phrase recovers nothing any more.
    ada2.ok(&["sync", "recovery", "remove"]);
    assert!(ada2.ok(&["sync", "recovery", "status"]).contains("No recovery phrase"));
    let ada3 = Client::new(&server);
    ada3.login(&server, "ada@example.com");
    let o = Client::out(ada3.cmd().args(["sync", "recovery", "use"]).write_stdin(phrase.clone()).output().unwrap());
    assert_eq!(o.code, 2);
    assert!(o.stderr.contains("no recovery phrase"), "{}", o.stderr);
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
    // A password is asked for by default: without a terminal it must come on stdin.
    let (code, err) = cmd(&["sync", "login", "--email", "a@example.com", "--server", "https://sync.example.com"]);
    assert_eq!(code, 2);
    assert!(err.contains("--password-stdin"), "{err}");
    let (code, err) = cmd(&["sync", "login", "--email", "a@example.com", "--server", "https://sync.example.com", "--code", "--name"]);
    assert_eq!(code, 2);
    assert!(err.contains("put --code last"), "{err}");
    let (code, err) = cmd(&["sync", "login", "--google", "--github"]);
    assert_eq!(code, 2);
    assert!(err.contains("cannot be used with"), "{err}");
    let (_, err) = cmd(&["sync", "accept", "-iPhD7m9"]);
    assert!(!err.contains("unexpected argument"), "{err}");
    let o = Command::cargo_bin("hitop").unwrap().env("HITOP_SYNC_RUNNER", "nonsense").args(["sync", "pull"]).output().unwrap();
    assert_eq!(o.status.code(), Some(2));
    assert!(String::from_utf8_lossy(&o.stderr).contains("runner credential"));
}

/// HITOP Cloud (a server with plan limits): `sync ls` names the org, `sync init --org` shares into
/// an organization, and a create beyond the plan is refused with the server's message (plan,
/// limit, where to upgrade), exit 2, nothing linked.
#[test]
fn organizations_and_a_refused_create() {
    let Some(server) = server_with(&[("HITOP_SYNC_LIMITS", "plans".into()), ("HITOP_SYNC_PORTAL_URL", "https://cloud.test.example/".into())]) else { return };
    let ada = Client::new(&server);
    ada.login(&server, "ada@example.com");
    let session = serde_json::from_str::<serde_json::Value>(&std::fs::read_to_string(ada.home.path().join("sync-key.json")).unwrap()).unwrap()["session"].as_str().unwrap().to_string();
    let http = reqwest::blocking::Client::new();
    let org: serde_json::Value = http.post(format!("{}/v1/orgs", server.url)).bearer_auth(&session).json(&serde_json::json!({ "name": "Acme" })).send().unwrap().json().unwrap();
    let org_id = org["id"].as_str().unwrap().to_string();

    let share = |name: &str, extra: &[&str]| {
        let dir = ada.folder(name);
        ada.ok(&["init", dir.to_str().unwrap(), "--name", name]);
        write(&dir, "requests/a.yaml", &request("A", "https://example.test/a"));
        let mut args = vec!["sync", "init", "-c", dir.to_str().unwrap()];
        args.extend_from_slice(extra);
        (ada.run(&args), dir)
    };
    // Into the org by name (any case) or id; an unknown name is explained.
    let (o, _) = share("in-org", &["--org", "acme"]);
    assert_eq!(o.code, 0, "{}", o.stderr);
    let (o, _) = share("nowhere", &["--org", "Initech"]);
    assert_eq!(o.code, 2);
    assert!(o.stderr.contains("no organization named Initech") && o.stderr.contains("Acme"), "{}", o.stderr);
    let (o, _) = share("in-org-2", &["--org", &org_id]);
    assert_eq!(o.code, 0, "{}", o.stderr);
    let ls = ada.ok(&["sync", "ls"]);
    assert_eq!(ls.matches("org Acme").count(), 2, "{ls}");

    // Free allows 3 workspaces per org: the personal org's 4th is refused.
    for name in ["p1", "p2", "p3"] {
        let (o, _) = share(name, &[]);
        assert_eq!(o.code, 0, "{}", o.stderr);
    }
    let (o, dir) = share("p4", &[]);
    assert_eq!(o.code, 2, "{}", o.stdout);
    for part in ["plan limit", "free plan", "your personal account", "3 shared workspaces", "https://cloud.test.example/#/orgs/personal-", "402 limit"] {
        assert!(o.stderr.contains(part), "{part} in {}", o.stderr);
    }
    assert_eq!(ada.run(&["sync", "status", "-c", dir.to_str().unwrap()]).code, 2, "not linked");
    let ls = ada.ok(&["sync", "ls"]);
    assert_eq!((ls.matches("personal").count(), ls.lines().count()), (3, 5), "{ls}");
    // The org's own limit is separate: a third workspace in it still fits.
    let (o, _) = share("in-org-3", &["--org", "Acme"]);
    assert_eq!(o.code, 0, "{}", o.stderr);
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
    let o = alice.run(&["sync", "login", "--email", "alice@corp.test", "--code"]);
    assert_eq!(o.code, 2);
    assert!(o.stderr.contains("single sign-on"), "{}", o.stderr);
    // The sign-in is in her account log.
    let mine = alice.ok(&["sync", "audit", "--mine", "--json"]);
    assert!(mine.contains("\"sso_linked\"") && mine.contains("\"method\": \"sso\""), "{mine}");
}

/// HITOP Cloud C1b passwords: Ada signs up with a password (the mailed code confirms it), shares
/// a folder with Bob. On a new computer her password signs her in **and** unlocks her device's
/// keys: same key, same trust, the workspace reads with no trust step. A password change re-seals
/// the bundle and signs her other computers out. A reset by mail on a third computer signs in
/// but can't unlock anything (it says so); once a device that has the keys signs in with the new
/// password, it seals them again and the third computer unlocks them.
#[test]
fn a_password_signs_in_and_unlocks_the_keys_on_a_new_computer() {
    let Some(server) = server() else { return };
    let (ada, bob) = (Client::new(&server), Client::new(&server));
    let stdin = |c: &Client, args: &[&str], input: &str| Client::out(c.cmd().args(args).write_stdin(input.to_string()).output().unwrap());

    // Sign-up in two runs (no terminal): the password, then the code from the mail.
    let short = stdin(&ada, &["sync", "signup", "--email", "ada@example.com", "--password-stdin"], "short\n");
    assert_eq!(short.code, 2);
    assert!(short.stderr.contains("at least 8"), "{}", short.stderr);
    let o = stdin(&ada, &["sync", "signup", "--email", "ada@example.com", "--password-stdin"], "first long password\n");
    assert_eq!(o.code, 0, "{}", o.stderr);
    assert!(o.stdout.contains("--code"), "{}", o.stdout);
    let code = server.code_for("ada@example.com", "/signin#");
    let o = stdin(&ada, &["sync", "signup", "--email", "ada@example.com", "--code", &code, "--password-stdin"], "first long password\n");
    assert_eq!(o.code, 0, "{}\n{}", o.stdout, o.stderr);
    assert!(o.stdout.contains("Account confirmed") && o.stdout.contains("key fingerprint"), "{}", o.stdout);
    let (ada_key, ada_keys) = ada.device();
    let (ws, a, _) = share_with_bob(&server, &ada, &bob);
    write(&a, "requests/new-order.yaml", &request("New order", "https://internal.example.test/orders/new"));
    ada.ok(&["sync", "push", "-c", a.to_str().unwrap()]);

    // The server keeps a hash of the auth key and a sealed bundle: never the password or a key.
    let hash: String = server.db().query_row("SELECT hash FROM password_credentials", [], |r| r.get(0)).unwrap();
    assert!(hash.starts_with("$argon2id$"), "{hash}");
    let bundles: i64 = server.db().query_row("SELECT COUNT(*) FROM recovery_blobs WHERE method = 'password'", [], |r| r.get(0)).unwrap();
    assert_eq!(bundles, 1);
    let db = server.database();
    assert!(!String::from_utf8_lossy(&db).contains("first long password"));
    assert!(!db.windows(32).any(|w| w == ada_keys.secret_bytes()), "no device secret on the server");

    // A new computer: a wrong password fails; the right one unlocks Ada's device.
    let ada2 = Client::new(&server);
    let o = stdin(&ada2, &["sync", "login", "--email", "ada@example.com", "--password-stdin"], "not my password\n");
    assert_eq!(o.code, 2);
    assert!(o.stderr.contains("the password is wrong"), "{}", o.stderr);
    let o = stdin(&ada2, &["sync", "login", "--email", "ada@example.com", "--password-stdin"], "first long password\n");
    assert_eq!(o.code, 0, "{}\n{}", o.stdout, o.stderr);
    assert!(o.stdout.contains("Unlocked with your password") && o.stdout.contains(&crypto::fingerprint(&ada_keys.public_key())), "{}", o.stdout);
    assert_eq!(ada2.device().0, ada_key, "the same device");
    let ada_keys_n: i64 = server.db().query_row("SELECT COUNT(*) FROM device_keys k JOIN users u ON u.id = k.user_id WHERE u.email = 'ada@example.com'", [], |r| r.get(0)).unwrap();
    assert_eq!(ada_keys_n, 1, "no new key registered");
    let a2 = ada2.folder("shop");
    ada2.ok(&["sync", "link", &ws, "-c", a2.to_str().unwrap()]);
    assert_eq!(snapshot(&a2), snapshot(&a));
    let members = ada2.ok(&["sync", "members", "-c", a2.to_str().unwrap()]);
    assert!(!members.contains("not trusted"), "{members}");

    // A change on the first computer: re-sealed, the second is signed out and needs the new one.
    let wrong = stdin(&ada, &["sync", "password", "set", "--password-stdin"], "not the current one\nsecond long password\n");
    assert_eq!(wrong.code, 2);
    assert!(wrong.stderr.contains("current password is wrong"), "{}", wrong.stderr);
    let o = stdin(&ada, &["sync", "password", "set", "--password-stdin"], "first long password\nsecond long password\n");
    assert_eq!(o.code, 0, "{}", o.stderr);
    assert!(o.stdout.contains("Password changed"), "{}", o.stdout);
    assert_eq!(ada2.run(&["sync", "ls"]).code, 2, "signed out");
    assert_eq!(stdin(&ada2, &["sync", "login", "--email", "ada@example.com", "--password-stdin"], "first long password\n").code, 2);
    let o = stdin(&ada2, &["sync", "login", "--email", "ada@example.com", "--password-stdin"], "second long password\n");
    assert_eq!(o.code, 0, "{}", o.stderr);
    assert_eq!(ada2.device().0, ada_key);
    ada2.ok(&["sync", "pull", "-c", a2.to_str().unwrap()]);

    // Forgot on a third computer: warned before, signed in after, but no keys.
    let ada3 = Client::new(&server);
    let o = ada3.run(&["sync", "password", "forgot", "--email", "ada@example.com"]);
    assert_eq!(o.code, 0, "{}", o.stderr);
    assert!(o.stderr.contains("can't unlock your end-to-end encrypted workspaces"), "{}", o.stderr);
    let code = server.code_for("ada@example.com", "/signin#");
    let o = stdin(&ada3, &["sync", "password", "reset", "--code", &code, "--password-stdin"], "third long password\n");
    assert_eq!(o.code, 0, "{}\n{}", o.stdout, o.stderr);
    assert!(o.stderr.contains("is gone") && o.stderr.contains("no workspace keys"), "{}", o.stderr);
    assert_ne!(ada3.device().0, ada_key);
    let a3 = ada3.folder("shop");
    assert_ne!(ada3.run(&["sync", "link", &ws, "-c", a3.to_str().unwrap()]).code, 0, "no workspace key here");
    assert_eq!(ada.run(&["sync", "ls"]).code, 2, "the reset signed every other session out");

    // The first computer signs in with the new password and seals its keys under it; then the
    // third unlocks them.
    let o = stdin(&ada, &["sync", "login", "--email", "ada@example.com", "--password-stdin"], "third long password\n");
    assert_eq!(o.code, 0, "{}", o.stderr);
    assert_eq!(ada.device().0, ada_key);
    let o = stdin(&ada3, &["sync", "password", "unlock", "--password-stdin"], "third long password\n");
    assert_eq!(o.code, 0, "{}\n{}", o.stdout, o.stderr);
    assert_eq!(ada3.device().0, ada_key);
    let a3 = ada3.folder("shop3");
    ada3.ok(&["sync", "link", &ws, "-c", a3.to_str().unwrap()]);
    assert_eq!(snapshot(&a3), snapshot(&a));
    let mine = ada.ok(&["sync", "audit", "--mine", "--json"]);
    for event in ["password_set", "password_changed", "password_reset"] {
        assert!(mine.contains(&format!("\"{event}\"")), "{event}: {mine}");
    }
}

/// `--google` / `--github` on a server without those apps: a clear refusal.
#[test]
fn google_and_github_need_the_server_to_offer_them() {
    let Some(server) = server() else { return };
    for flag in ["--google", "--github"] {
        let o = Client::new(&server).run(&["sync", "login", flag, "--no-browser"]);
        assert_eq!(o.code, 2);
        assert!(o.stderr.contains("doesn't offer sign-in with"), "{}", o.stderr);
    }
}
