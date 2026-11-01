use super::*;

/// The server's rules (compare-and-swap on version, descending version vectors), in memory and
/// without encryption: what the sync server does with the envelopes.
#[derive(Default)]
struct Sim {
    seq: u64,
    blobs: BTreeMap<String, (RemoteBlob, u64)>,
}

fn descends(a: &VersionVector, b: &VersionVector) -> bool {
    b.iter().all(|(k, v)| a.get(k).copied().unwrap_or(0) >= *v) && a.iter().any(|(k, v)| *v > b.get(k).copied().unwrap_or(0))
}

impl Sim {
    fn changes(&self, since: u64) -> (Vec<RemoteBlob>, u64) {
        let mut v: Vec<_> = self.blobs.values().filter(|(_, s)| *s > since).cloned().collect();
        v.sort_by_key(|(_, s)| *s);
        (v.into_iter().map(|(b, _)| b).collect(), self.seq)
    }

    fn put(&mut self, p: &Push) -> Result<(), ()> {
        let (version, vv) = self.blobs.get(&p.blob).map_or((0, VersionVector::new()), |(b, _)| (b.version, b.vv.clone()));
        if p.version != version + 1 || !descends(&p.vv, &vv) {
            return Err(());
        }
        // The path lives inside the (encrypted) content; a tombstone has none.
        let path = p.text.as_ref().map(|_| p.path.clone());
        self.seq += 1;
        self.blobs.insert(p.blob.clone(), (RemoteBlob { blob: p.blob.clone(), version: p.version, vv: p.vv.clone(), deleted: p.text.is_none(), path, text: p.text.clone() }, self.seq));
        Ok(())
    }
}

struct Client {
    me: String,
    files: Files,
    state: State,
    next: u32,
}

impl Client {
    fn new(me: &str) -> Client {
        Client { me: me.into(), files: Files::new(), state: State::default(), next: 0 }
    }

    /// One sync: pull, apply, push; retried on a 409 like the real clients.
    fn sync(&mut self, server: &mut Sim) -> Plan {
        for _ in 0..5 {
            let (changes, seq) = server.changes(self.state.seq);
            let me = self.me.clone();
            let mut next = self.next;
            let mut ids = || {
                next += 1;
                format!("{me}{next:04}")
            };
            let mut plan = plan(&self.files, &self.state, &changes, &self.me, &mut ids);
            self.next = next;
            plan.state.seq = seq;
            for (p, t) in &plan.write {
                self.files.insert(p.clone(), t.clone());
            }
            for p in &plan.remove {
                self.files.remove(p);
            }
            self.state = plan.state.clone();
            let mut clean = true;
            for push in &plan.push {
                if server.put(push).is_err() {
                    clean = false;
                    break;
                }
                self.state.pushed(push);
            }
            if clean {
                return plan;
            }
        }
        panic!("sync didn't settle");
    }
}

fn request(name: &str, url: &str) -> String {
    format!("name: {name}\nmethod: GET\nurl: {url}\n")
}

#[test]
fn edits_on_one_side_reach_the_other() {
    let mut server = Sim::default();
    let (mut a, mut b) = (Client::new("a"), Client::new("b"));
    a.files.insert("hitop.yaml".into(), "format: 1\nname: Shop\n".into());
    a.files.insert("requests/list.yaml".into(), request("List", "https://x/list"));
    let p = a.sync(&mut server);
    assert_eq!(p.push.len(), 2);
    b.sync(&mut server);
    assert_eq!(b.files, a.files);

    // b edits, a pulls; a deletes, b pulls.
    b.files.insert("requests/list.yaml".into(), request("List", "https://x/v2/list"));
    b.sync(&mut server);
    let p = a.sync(&mut server);
    assert_eq!(p.write.keys().collect::<Vec<_>>(), ["requests/list.yaml"]);
    assert_eq!(a.files, b.files);
    a.files.remove("requests/list.yaml");
    a.sync(&mut server);
    let p = b.sync(&mut server);
    assert_eq!(p.remove, ["requests/list.yaml"]);
    assert!(!b.files.contains_key("requests/list.yaml"));
    // A recreated file reuses its blob.
    b.files.insert("requests/list.yaml".into(), request("List", "https://x/v3"));
    let p = b.sync(&mut server);
    assert_eq!(p.push[0].version, 4);
    a.sync(&mut server);
    assert_eq!(a.files, b.files);
    // Nothing to do when nothing changed.
    let p = a.sync(&mut server);
    assert!(p.push.is_empty() && p.write.is_empty() && p.remove.is_empty());
}

#[test]
fn a_request_edited_on_both_sides_is_a_conflict_until_resolved() {
    let mut server = Sim::default();
    let (mut a, mut b) = (Client::new("a"), Client::new("b"));
    a.files.insert("hitop.yaml".into(), "format: 1\n".into());
    a.files.insert("requests/get.yaml".into(), request("Get", "https://x/1"));
    a.sync(&mut server);
    b.sync(&mut server);
    a.files.insert("requests/get.yaml".into(), request("Get", "https://x/mine"));
    b.files.insert("requests/get.yaml".into(), request("Get", "https://x/theirs"));
    b.sync(&mut server);
    let p = a.sync(&mut server);
    assert_eq!(p.conflicts, ["requests/get.yaml"]);
    assert!(p.push.is_empty(), "a conflicted file is never pushed");
    let c = &a.state.conflicts["requests/get.yaml"];
    assert_eq!(c.base.as_deref(), Some(request("Get", "https://x/1").as_str()));
    assert_eq!(c.theirs.as_deref(), Some(request("Get", "https://x/theirs").as_str()));
    assert_eq!(a.files["requests/get.yaml"], request("Get", "https://x/mine"), "mine stays on disk");
    // Still a conflict on the next sync.
    assert_eq!(a.sync(&mut server).conflicts, ["requests/get.yaml"]);

    // Keep mine: pushed over theirs, b gets it.
    assert_eq!(a.state.resolve("requests/get.yaml", true), Some(Resolution::KeepMine));
    let p = a.sync(&mut server);
    assert_eq!(p.push.len(), 1);
    assert_eq!(p.push[0].version, 3);
    b.sync(&mut server);
    assert_eq!(b.files["requests/get.yaml"], request("Get", "https://x/mine"));

    // Again, and take theirs.
    a.files.insert("requests/get.yaml".into(), request("Get", "https://x/a2"));
    b.files.insert("requests/get.yaml".into(), request("Get", "https://x/b2"));
    a.sync(&mut server);
    assert_eq!(b.sync(&mut server).conflicts, ["requests/get.yaml"]);
    assert_eq!(b.state.resolve("requests/get.yaml", false), Some(Resolution::Write(request("Get", "https://x/a2"))));
    b.files.insert("requests/get.yaml".into(), request("Get", "https://x/a2"));
    let p = b.sync(&mut server);
    assert!(p.push.is_empty() && p.conflicts.is_empty());
    assert_eq!(b.state.resolve("requests/get.yaml", true), None);
}

#[test]
fn delete_against_edit_is_a_conflict_and_delete_against_delete_is_not() {
    let mut server = Sim::default();
    let (mut a, mut b) = (Client::new("a"), Client::new("b"));
    a.files.insert("hitop.yaml".into(), "format: 1\n".into());
    a.files.insert("requests/x.yaml".into(), request("X", "https://x"));
    a.files.insert("requests/y.yaml".into(), request("Y", "https://y"));
    a.sync(&mut server);
    b.sync(&mut server);
    a.files.remove("requests/x.yaml");
    b.files.insert("requests/x.yaml".into(), request("X", "https://x/edited"));
    a.files.remove("requests/y.yaml");
    b.files.remove("requests/y.yaml");
    a.sync(&mut server);
    let p = b.sync(&mut server);
    assert_eq!(p.conflicts, ["requests/x.yaml"]);
    assert_eq!(b.state.conflicts["requests/x.yaml"].theirs, None);
    assert_eq!(b.state.resolve("requests/x.yaml", false), Some(Resolution::Remove));
}

#[test]
fn manifest_and_environments_merge_key_by_key() {
    let mut server = Sim::default();
    let (mut a, mut b) = (Client::new("a"), Client::new("b"));
    a.files.insert("hitop.yaml".into(), "format: 1\nname: Shop\norder:\n- list\n- get\nvariables:\n  baseUrl: https://x\n".into());
    a.files.insert("environments/dev.yaml".into(), "variables:\n  host: dev\n  token:\n    secret: true\n".into());
    a.sync(&mut server);
    b.sync(&mut server);
    // Both add a request (order) and change different variables.
    a.files.insert("hitop.yaml".into(), "format: 1\nname: Shop\norder:\n- list\n- create\n- get\nvariables:\n  baseUrl: https://x\n".into());
    a.files.insert("requests/create.yaml".into(), request("Create", "https://x"));
    a.files.insert("environments/dev.yaml".into(), "variables:\n  host: dev2\n  token:\n    secret: true\n".into());
    b.files.insert("hitop.yaml".into(), "format: 1\nname: Shop\norder:\n- list\n- get\n- delete\nvariables:\n  baseUrl: https://x\n  version: v2\n".into());
    b.files.insert("requests/delete.yaml".into(), request("Delete", "https://x"));
    b.files.insert("environments/dev.yaml".into(), "variables:\n  host: dev\n  port: '8080'\n  token:\n    secret: true\n".into());
    a.sync(&mut server);
    let p = b.sync(&mut server);
    assert!(p.conflicts.is_empty(), "{:?}", p.state.conflicts);
    assert_eq!(p.merged, ["environments/dev.yaml", "hitop.yaml"]);
    let manifest: yaml_serde::Value = yaml_serde::from_str(&b.files["hitop.yaml"]).unwrap();
    let order: Vec<&str> = manifest["order"].as_sequence().unwrap().iter().map(|v| v.as_str().unwrap()).collect();
    assert_eq!(order, ["list", "create", "get", "delete"]);
    assert_eq!(manifest["variables"]["version"].as_str(), Some("v2"));
    let env: yaml_serde::Value = yaml_serde::from_str(&b.files["environments/dev.yaml"]).unwrap();
    assert_eq!(env["variables"]["host"].as_str(), Some("dev2"));
    assert_eq!(env["variables"]["port"].as_str(), Some("8080"));
    a.sync(&mut server);
    assert_eq!(a.files, b.files);

    // The same variable changed differently: a conflict.
    a.files.insert("environments/dev.yaml".into(), b.files["environments/dev.yaml"].replace("dev2", "a"));
    b.files.insert("environments/dev.yaml".into(), b.files["environments/dev.yaml"].replace("dev2", "b"));
    a.sync(&mut server);
    assert_eq!(b.sync(&mut server).conflicts, ["environments/dev.yaml"]);
}

#[test]
fn the_same_new_path_on_both_sides_converges_on_one_blob() {
    let mut server = Sim::default();
    let (mut a, mut b) = (Client::new("a"), Client::new("b"));
    // Both create the same file before either syncs: two blobs for one path on the server.
    a.files.insert("requests/new.yaml".into(), request("New", "https://same"));
    b.files.insert("requests/new.yaml".into(), request("New", "https://same"));
    b.files.insert("requests/other.yaml".into(), request("Other", "https://b"));
    a.sync(&mut server);
    // b pulls a's blob for the same path before it ever pushed: same text, adopted, no duplicate.
    b.sync(&mut server);
    assert_eq!(server.blobs.len(), 2);

    // A real race: both push before seeing the other.
    let mut server = Sim::default();
    let (mut a, mut b) = (Client::new("a"), Client::new("b"));
    a.files.insert("requests/r.yaml".into(), request("R", "https://a"));
    b.files.insert("requests/r.yaml".into(), request("R", "https://b"));
    a.sync(&mut server); // blob a0001
    // b pushes without pulling first (a stale change feed).
    let mut ids = || "b0001".to_string();
    let p = plan(&b.files, &b.state, &[], "b", &mut ids);
    server.put(&p.push[0]).unwrap();
    b.state = p.state.clone();
    b.state.pushed(&p.push[0]);
    // Now a sees b's blob: b0001 > a0001, so a's stays and a ignores b's.
    let p = a.sync(&mut server);
    assert!(p.conflicts.is_empty() && p.write.is_empty());
    // b sees a's: b's blob lost; b gets a conflict (different text) and deletes its blob.
    let p = b.sync(&mut server);
    assert_eq!(p.conflicts, ["requests/r.yaml"]);
    assert!(server.blobs["b0001"].0.deleted, "the losing blob is deleted");
    assert!(b.state.losers.is_empty());
    b.state.resolve("requests/r.yaml", true);
    b.sync(&mut server);
    a.sync(&mut server);
    assert_eq!(a.files["requests/r.yaml"], request("R", "https://b"));
    assert_eq!(server.blobs.values().filter(|(r, _)| !r.deleted).count(), 1);
}

#[test]
fn only_collection_files_sync_and_paths_from_the_server_are_checked() {
    for ok in ["hitop.yaml", "globals.yaml", "environments/dev.yaml", "requests/a.yaml", "requests/orders/create.yml"] {
        assert!(is_synced_path(ok), "{ok}");
    }
    for bad in ["", "/etc/passwd", "../x.yaml", "requests/../../x.yaml", "requests/.git/x.yaml", "environments/a/b.yaml", "requests/a.txt", "README.md", ".hitop/x", "requests//a.yaml", "requests\\a.yaml", "requests/a\0.yaml"] {
        assert!(!is_synced_path(bad), "{bad:?}");
    }
    let mut files = Files::new();
    files.insert("README.md".into(), "x".into());
    files.insert("hitop.yaml".into(), "format: 1\n".into());
    assert_eq!(synced_files(&files).keys().collect::<Vec<_>>(), ["hitop.yaml"]);

    // A member's client can't make another write outside the folder.
    let remote = vec![RemoteBlob { blob: "x".into(), version: 1, vv: [("a".to_string(), 1)].into(), deleted: false, path: Some("../../.bashrc".into()), text: Some("rm -rf ~".into()) }];
    let p = plan(&Files::new(), &State::default(), &remote, "b", &mut || "id".into());
    assert!(p.write.is_empty() && p.state.entries.is_empty());
    assert_eq!(p.skipped.len(), 1);
}

#[test]
fn secret_values_never_reach_the_synced_files() {
    use crate::format::{files, Folder};
    use crate::model::{Collection, Environment};
    let mut env = Environment { id: "prod".into(), name: "prod".into(), ..Default::default() };
    env.variables.insert("host".into(), "api.example.com".into());
    env.variables.insert("token".into(), "s3cr3t-value-123".into());
    env.secrets.push("token".into());
    let folder = Folder { collection: Collection { name: "Shop".into(), ..Default::default() }, environments: vec![env], globals: None };
    let all = synced_files(&files::to_files(&folder).unwrap());
    let text: String = all.values().cloned().collect();
    assert!(!text.contains("s3cr3t-value-123"));
    assert!(text.contains("token"), "the secret's name syncs");
    // Pulled files read back to the same files (no phantom edits on the next sync).
    let again = files::to_files(&files::read_files(&all, "Shop").unwrap()).unwrap();
    assert_eq!(synced_files(&again), all);
}

#[test]
fn blob_text_round_trip() {
    let bytes = BlobText::encode("requests/a.yaml", "x: 1\n");
    assert_eq!(BlobText::decode(&bytes).unwrap(), BlobText { v: 1, path: "requests/a.yaml".into(), text: "x: 1\n".into() });
    assert!(BlobText::decode(br#"{"v":2,"path":"a","text":""}"#).unwrap_err().contains("newer"));
    assert!(BlobText::decode(b"garbage").is_err());
}

#[test]
fn a_rewrite_with_the_same_text_keeps_a_local_edit() {
    let mut server = Sim::default();
    let (mut a, mut b) = (Client::new("a"), Client::new("b"));
    a.files.insert("requests/x.yaml".into(), request("X", "https://x"));
    a.sync(&mut server);
    b.sync(&mut server);
    // a re-encrypts (same text, next version) while b edits.
    let e = a.state.entries["requests/x.yaml"].clone();
    let push = Push { path: "requests/x.yaml".into(), blob: e.blob.clone(), version: e.version + 1, vv: bump(&e.vv, "a"), text: e.base.clone() };
    server.put(&push).unwrap();
    b.files.insert("requests/x.yaml".into(), request("X", "https://x/b"));
    let p = b.sync(&mut server);
    assert!(p.conflicts.is_empty());
    assert_eq!(p.push.len(), 1);
    assert_eq!(p.push[0].version, 3);
    a.sync(&mut server);
    assert_eq!(a.files, b.files);
}
