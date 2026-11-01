//! `hitop sync`: end-to-end encrypted sync of a collection folder through a hitop sync server
//! (sync-server/, AGPL). The decisions are hitop-core's `sync` module; the encryption is
//! `hitop-sync-crypto`; this file is the command line around them. Secret values never sync:
//! environment files carry only their names.

mod client;
mod store;

use std::collections::BTreeMap;
use std::io::{BufRead, IsTerminal, Write};
use std::path::{Path, PathBuf};

use anyhow::{anyhow, bail, Context, Result};
use clap::{Args, Subcommand};
use colored::Colorize;
use hitop_core::format::files::Files;
use hitop_core::sync::{self as engine, Plan, Resolution, State};
use hitop_sync_crypto::{self as crypto, DeviceKeyPair, WorkspaceKey};
use serde_json::{json, Value};

use client::{b64, unb64, Api, Device, Workspace};
use store::{Account, Link, RunnerCredential, Secret};

#[derive(Subcommand)]
pub enum SyncCmd {
    /// Sign in by email (a code is mailed to you) and register this device's key
    Login {
        #[arg(long)]
        email: String,
        /// The code from the sign-in mail (asked for on a terminal when left out)
        #[arg(long)]
        code: Option<String>,
        /// Sync server (default https://sync.hitop.rws8.tech, or HITOP_SYNC_SERVER)
        #[arg(long)]
        server: Option<String>,
        /// A name for this device, shown to your team next to its key fingerprint
        #[arg(long)]
        name: Option<String>,
    },
    /// Sign out on this device (forgets the session and the device key)
    Logout,
    /// The account, this device and its key fingerprint
    Whoami,
    /// Your workspaces on the server
    Ls,
    /// Share a collection folder: make a new end-to-end encrypted workspace and push it
    Init(FolderArgs),
    /// Sync a folder with an existing workspace (empty folder, or one to merge), then pull
    Link {
        workspace: String,
        #[command(flatten)]
        folder: FolderArgs,
    },
    /// Stop syncing a folder (nothing on the server changes)
    Unlink(FolderArgs),
    /// What a sync would pull and push, and the conflicts
    Status(FolderArgs),
    /// Get the team's changes (yours stay unpushed). With HITOP_SYNC_RUNNER set, pulls the
    /// runner's workspace into the folder (scheduled runs)
    Pull(FolderArgs),
    /// Sync: pull, then push your changes
    Push(FolderArgs),
    /// Resolve a conflict: keep mine, take theirs, or show both
    Resolve {
        /// Path in the folder, as `hitop sync status` shows it (requests/get-user.yaml)
        path: String,
        #[arg(long, conflicts_with_all = ["theirs", "show"])]
        mine: bool,
        #[arg(long, conflicts_with_all = ["mine", "show"])]
        theirs: bool,
        /// Print the base, mine and theirs
        #[arg(long, conflicts_with_all = ["mine", "theirs"])]
        show: bool,
        #[command(flatten)]
        folder: FolderArgs,
    },
    /// Members, roles, device key fingerprints and accepted invites. Also hands the workspace
    /// key to members' new devices whose keys you trust
    Members {
        #[command(flatten)]
        trust: TrustArgs,
        #[command(flatten)]
        folder: FolderArgs,
    },
    /// Invite someone (owner): mailed to EMAIL, or a link to share when left out
    Invite {
        email: Option<String>,
        #[arg(long, default_value = "editor", value_parser = ["owner", "editor", "viewer"])]
        role: String,
        #[command(flatten)]
        folder: FolderArgs,
    },
    /// Accept an invite (the code from the invite mail or link)
    Accept { code: String },
    /// Add an account that accepted an invite, after checking its key fingerprints (owner)
    Add {
        email: String,
        #[arg(long, value_parser = ["owner", "editor", "viewer"])]
        role: Option<String>,
        #[command(flatten)]
        trust: TrustArgs,
        #[command(flatten)]
        folder: FolderArgs,
    },
    /// Remove a member (owner): rotates the workspace key and re-encrypts every file
    Remove {
        email: String,
        #[command(flatten)]
        trust: TrustArgs,
        #[command(flatten)]
        folder: FolderArgs,
    },
    /// Change a member's role (owner)
    Role {
        email: String,
        #[arg(value_parser = ["owner", "editor", "viewer"])]
        role: String,
        #[command(flatten)]
        folder: FolderArgs,
    },
    /// Trust device keys by fingerprint (compare it with the person first)
    Trust {
        #[arg(required = true)]
        fingerprints: Vec<String>,
        #[command(flatten)]
        folder: FolderArgs,
    },
    /// Runner credentials for scheduled runs (CI, cron)
    #[command(subcommand)]
    Runner(RunnerCmd),
}

#[derive(Subcommand)]
pub enum RunnerCmd {
    /// A read-only credential for this workspace: put it in your CI secret HITOP_SYNC_RUNNER
    Create {
        #[arg(long, default_value = "runner")]
        name: String,
        #[command(flatten)]
        folder: FolderArgs,
    },
}

#[derive(Args, Clone)]
pub struct FolderArgs {
    /// The collection folder (default: the current directory)
    #[arg(short = 'c', long = "collection", value_name = "DIR")]
    pub dir: Option<PathBuf>,
}

impl FolderArgs {
    fn path(&self) -> PathBuf {
        self.dir.clone().unwrap_or_else(|| PathBuf::from("."))
    }
}

#[derive(Args, Clone, Default)]
pub struct TrustArgs {
    /// Trust a device key with this fingerprint (repeatable); otherwise asked on a terminal
    #[arg(long = "trust", value_name = "FINGERPRINT")]
    pub trust: Vec<String>,
}

pub fn run(cmd: SyncCmd) -> Result<i32> {
    match cmd {
        SyncCmd::Login { email, code, server, name } => login(&email, code, server, name).map(|_| 0),
        SyncCmd::Logout => logout().map(|_| 0),
        SyncCmd::Whoami => whoami().map(|_| 0),
        SyncCmd::Ls => ls().map(|_| 0),
        SyncCmd::Init(f) => init(&f.path()).map(|_| 0),
        SyncCmd::Link { workspace, folder } => link(&workspace, &folder.path()),
        SyncCmd::Unlink(f) => {
            store::save_link(&f.path(), None)?;
            println!("{} no longer synced (the workspace is unchanged)", f.path().display());
            Ok(0)
        }
        SyncCmd::Status(f) => status(&f.path()),
        SyncCmd::Pull(f) => sync(&f.path(), false),
        SyncCmd::Push(f) => sync(&f.path(), true),
        SyncCmd::Resolve { path, mine, theirs, show, folder } => resolve(&folder.path(), &path, mine, theirs, show),
        SyncCmd::Members { trust, folder } => members(&folder.path(), &trust).map(|_| 0),
        SyncCmd::Invite { email, role, folder } => invite(&folder.path(), email.as_deref(), &role).map(|_| 0),
        SyncCmd::Accept { code } => accept(&code).map(|_| 0),
        SyncCmd::Add { email, role, trust, folder } => add(&folder.path(), &email, role.as_deref(), &trust).map(|_| 0),
        SyncCmd::Remove { email, trust, folder } => remove(&folder.path(), &email, &trust).map(|_| 0),
        SyncCmd::Role { email, role, folder } => set_role(&folder.path(), &email, &role).map(|_| 0),
        SyncCmd::Trust { fingerprints, folder } => trust_cmd(&folder.path(), &fingerprints).map(|_| 0),
        SyncCmd::Runner(RunnerCmd::Create { name, folder }) => runner_create(&folder.path(), &name).map(|_| 0),
    }
}

fn interactive() -> bool {
    std::io::stdin().is_terminal() && std::io::stderr().is_terminal()
}

fn ask(prompt: &str) -> Result<String> {
    eprint!("{prompt}");
    std::io::stderr().flush()?;
    let mut line = String::new();
    std::io::stdin().lock().read_line(&mut line)?;
    Ok(line.trim().to_string())
}

fn default_server() -> String {
    std::env::var("HITOP_SYNC_SERVER").ok().filter(|s| !s.is_empty()).unwrap_or_else(|| store::DEFAULT_SERVER.to_string())
}

/// The signed-in account, its API client and this device.
struct Session {
    account: Account,
    api: Api,
    device: Device,
}

fn session() -> Result<Session> {
    let account = Account::load()?.ok_or_else(|| anyhow!("not signed in: run `hitop sync login --email <you>`"))?;
    let secret = store::load_secret(&account.server)?;
    let device = Device { key_id: account.key_id.clone(), keys: secret.device()? };
    let api = Api::new(&account.server, Some(secret.session))?;
    Ok(Session { account, api, device })
}

fn login(email: &str, code: Option<String>, server: Option<String>, name: Option<String>) -> Result<()> {
    let server = server.unwrap_or_else(default_server);
    let anon = Api::new(&server, None)?;
    let code = match code {
        Some(c) => c,
        None => {
            anon.call("POST", "/v1/auth/email", Some(json!({ "email": email })))?;
            eprintln!("A sign-in mail is on its way to {email}.");
            if !interactive() {
                println!("Then: hitop sync login --email {email} --code <code from the mail>");
                return Ok(());
            }
            ask("Code from the mail (or the part after # in its link): ")?
        }
    };
    let code = code.rsplit('#').next().unwrap_or(&code).trim().to_string();
    let r = anon.call("POST", "/v1/auth/email/verify", Some(json!({ "token": code })))?;
    let session = r["session"].as_str().context("no session in the answer")?.to_string();
    let user_id = r["user"]["id"].as_str().unwrap_or("").to_string();
    let api = Api::new(&server, Some(session.clone()))?;

    // The same account on this device again: keep its key.
    let previous = Account::load()?.filter(|a| a.server.trim_end_matches('/') == server.trim_end_matches('/') && a.user_id == user_id);
    let reuse = previous.as_ref().and_then(|a| store::load_secret(&a.server).ok().map(|s| (a.clone(), s)));
    let me = api.call("GET", "/v1/me", None)?;
    let (key_id, keys, device_name) = match reuse.filter(|(a, _)| me["keys"].as_array().is_some_and(|k| k.iter().any(|k| k["id"] == a.key_id.as_str()))) {
        Some((a, s)) => (a.key_id.clone(), s.device()?, a.device_name.clone()),
        None => {
            let keys = DeviceKeyPair::generate();
            let device_name = name.unwrap_or_else(|| format!("hitop CLI on {}", hostname()));
            let r = api.call("POST", "/v1/me/keys", Some(json!({ "name": device_name, "publicKey": b64(&keys.public_key()) })))?;
            (r["id"].as_str().context("no key id")?.to_string(), keys, device_name)
        }
    };
    store::save_secret(&server, &Secret { session, secret: b64(&keys.secret_bytes()) })?;
    let mut pins = store::load_pins()?;
    pins.insert(key_id.clone(), b64(&keys.public_key()));
    store::save_pins(&pins)?;
    Account { server: server.clone(), email: r["user"]["email"].as_str().unwrap_or(email).to_string(), user_id, key_id, device_name: device_name.clone() }.save()?;
    println!("Signed in to {server} as {email}.");
    println!("This device: {device_name}, key fingerprint {}", crypto::fingerprint(&keys.public_key()).bold());
    println!("Tell your team this fingerprint when they add you, and check theirs.");
    Ok(())
}

fn hostname() -> String {
    std::env::var("HOSTNAME").ok().or_else(|| std::fs::read_to_string("/etc/hostname").ok()).map(|s| s.trim().to_string()).filter(|s| !s.is_empty()).unwrap_or_else(|| "this computer".into())
}

fn logout() -> Result<()> {
    if let Some(a) = Account::load()? {
        if let Ok(s) = session() {
            let _ = s.api.raw("POST", "/v1/auth/logout", None);
        }
        store::delete_secret(&a.server)?;
        Account::forget()?;
        println!("Signed out of {} on this device.", a.server);
    } else {
        println!("Not signed in.");
    }
    Ok(())
}

fn whoami() -> Result<()> {
    let s = session()?;
    let me = s.api.call("GET", "/v1/me", None)?;
    println!("{} on {}", me["email"].as_str().unwrap_or(""), s.account.server);
    println!("This device: {} ({}), fingerprint {}", s.account.device_name, s.device.key_id, crypto::fingerprint(&s.device.keys.public_key()));
    let pins = store::load_pins()?;
    for k in me["keys"].as_array().cloned().unwrap_or_default() {
        if k["id"] == s.device.key_id.as_str() {
            continue;
        }
        let pk = unb64(k["publicKey"].as_str().unwrap_or(""))?;
        let mark = trust_mark(&pins, k["id"].as_str().unwrap_or(""), &pk);
        println!("Other device: {} {} {}", k["name"].as_str().unwrap_or(""), crypto::fingerprint(&pk), mark);
    }
    Ok(())
}

fn ls() -> Result<()> {
    let s = session()?;
    let r = s.api.call("GET", "/v1/workspaces", None)?;
    let list = r["workspaces"].as_array().cloned().unwrap_or_default();
    if list.is_empty() {
        println!("No workspaces yet: `hitop sync init` shares a collection folder.");
    }
    for w in list {
        println!("{}  {}", w["id"].as_str().unwrap_or(""), w["role"].as_str().unwrap_or(""));
    }
    Ok(())
}

// ---- trust on first use ----

fn trust_mark(pins: &store::Pins, key_id: &str, pk: &[u8]) -> String {
    match pins.get(key_id) {
        Some(p) if *p == b64(pk) => "trusted".green().to_string(),
        Some(_) => "KEY CHANGED: not trusted".red().bold().to_string(),
        None => "not trusted yet".yellow().to_string(),
    }
}

/// A device key someone else will receive the workspace key for.
struct Recipient {
    key_id: String,
    public_key: Vec<u8>,
    owner: String,
    name: String,
}

/// Every recipient must be pinned (or trusted now by fingerprint: `--trust`, or a yes on a
/// terminal). A pinned key id whose public key changed is refused outright: that is what a
/// key substitution by the server looks like.
fn check_trust(recipients: &[Recipient], trust: &TrustArgs) -> Result<()> {
    let mut pins = store::load_pins()?;
    let wanted: Vec<String> = trust.trust.iter().map(|f| normalize_fp(f)).collect();
    let mut unknown = Vec::new();
    for r in recipients {
        let pk = b64(&r.public_key);
        match pins.get(&r.key_id) {
            Some(p) if *p == pk => continue,
            Some(_) => bail!("the key of {} ({}) changed since you trusted it; refusing to share the workspace key with it. If they really have a new key, they register a new device.", r.owner, r.name),
            None => {}
        }
        let fp = crypto::fingerprint(&r.public_key);
        if wanted.contains(&normalize_fp(&fp)) {
            pins.insert(r.key_id.clone(), pk);
            continue;
        }
        if interactive() {
            let a = ask(&format!("Trust the device \"{}\" of {} with fingerprint {}? Compare it with them first. [y/N] ", r.name, r.owner, fp))?;
            if a.eq_ignore_ascii_case("y") || a.eq_ignore_ascii_case("yes") {
                pins.insert(r.key_id.clone(), pk);
                continue;
            }
        }
        unknown.push(format!("  {} ({}): {}", r.owner, r.name, fp));
    }
    store::save_pins(&pins)?;
    if !unknown.is_empty() {
        bail!(
            "these device keys aren't trusted yet. Compare each fingerprint with its owner (in person, a call, a chat you trust), then pass --trust '<fingerprint>':\n{}",
            unknown.join("\n")
        );
    }
    Ok(())
}

fn normalize_fp(f: &str) -> String {
    f.chars().filter(|c| c.is_ascii_hexdigit()).collect::<String>().to_lowercase()
}

fn wraps_for(recipients: &[Recipient], ws: &str, epoch: u32, key: &WorkspaceKey) -> Result<Value> {
    let mut out = Vec::new();
    for r in recipients {
        let w = crypto::wrap_key(&r.public_key, ws, epoch, key).map_err(|e| anyhow!("{e}"))?;
        out.push(json!({ "keyId": r.key_id, "wrapped": b64(&w) }));
    }
    Ok(Value::Array(out))
}

fn keys_of(owner: &str, keys: &Value) -> Result<Vec<Recipient>> {
    keys.as_array()
        .cloned()
        .unwrap_or_default()
        .iter()
        .map(|k| {
            Ok(Recipient {
                key_id: k["id"].as_str().unwrap_or("").to_string(),
                public_key: unb64(k["publicKey"].as_str().unwrap_or(""))?,
                owner: owner.to_string(),
                name: k["name"].as_str().unwrap_or("").to_string(),
            })
        })
        .collect()
}

/// Every device key of every member (runners included), from `GET /v1/workspaces/:ws`.
fn member_keys(info: &Value, except_user: Option<&str>) -> Result<Vec<Recipient>> {
    let mut out = Vec::new();
    for m in info["members"].as_array().cloned().unwrap_or_default() {
        if Some(m["userId"].as_str().unwrap_or("")) == except_user {
            continue;
        }
        out.extend(keys_of(m["email"].as_str().unwrap_or(""), &m["keys"])?);
    }
    Ok(out)
}

// ---- folders ----

fn read_folder(dir: &Path) -> Result<Files> {
    let mut files = Files::new();
    fn walk(root: &Path, dir: &Path, files: &mut Files) -> Result<()> {
        let Ok(rd) = std::fs::read_dir(dir) else { return Ok(()) };
        for entry in rd {
            let entry = entry?;
            let path = entry.path();
            let rel = path.strip_prefix(root).unwrap_or(&path).to_string_lossy().replace('\\', "/");
            if entry.file_type()?.is_dir() {
                if !rel.split('/').any(|s| s.starts_with('.')) {
                    walk(root, &path, files)?;
                }
            } else if engine::is_synced_path(&rel) {
                files.insert(rel, std::fs::read_to_string(&path).with_context(|| path.display().to_string())?);
            }
        }
        Ok(())
    }
    walk(dir, dir, &mut files)?;
    Ok(files)
}

fn apply(dir: &Path, plan: &Plan) -> Result<()> {
    for (rel, text) in &plan.write {
        // Checked again here: nothing is written outside the folder.
        if !engine::is_synced_path(rel) {
            bail!("refusing to write {rel}");
        }
        let path = dir.join(rel);
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        std::fs::write(&path, text).with_context(|| path.display().to_string())?;
    }
    for rel in &plan.remove {
        if !engine::is_synced_path(rel) {
            continue;
        }
        let path = dir.join(rel);
        if path.exists() {
            std::fs::remove_file(&path)?;
        }
        // Empty folders under requests/ go too.
        let mut parent = path.parent();
        while let Some(p) = parent {
            if p == dir || p == dir.join("requests") || std::fs::remove_dir(p).is_err() {
                break;
            }
            parent = p.parent();
        }
    }
    Ok(())
}

/// The workspace a folder syncs with, and how to reach it: the signed-in account, or a runner
/// credential from HITOP_SYNC_RUNNER.
struct Target {
    workspace: String,
    api: Api,
    device: Device,
    /// The version-vector id: this device's key id.
    me: String,
    runner: bool,
}

fn target(dir: &Path) -> Result<Target> {
    if let Some(c) = RunnerCredential::from_env()? {
        let device = Device { key_id: c.key_id.clone(), keys: DeviceKeyPair::from_secret_bytes(&unb64(&c.secret)?).map_err(|e| anyhow!("runner key: {e}"))? };
        return Ok(Target { workspace: c.workspace.clone(), api: Api::new(&c.server, Some(c.token.clone()))?, me: c.key_id.clone(), device, runner: true });
    }
    let s = session()?;
    let link = store::link_of(dir)?.ok_or_else(|| anyhow!("{} isn't synced: `hitop sync init` shares it, `hitop sync link <workspace>` joins one", dir.display()))?;
    if link.server.trim_end_matches('/') != s.account.server.trim_end_matches('/') {
        bail!("{} syncs with {}, but you're signed in to {}", dir.display(), link.server, s.account.server);
    }
    Ok(Target { workspace: link.workspace, me: s.device.key_id.clone(), api: s.api, device: s.device, runner: false })
}

fn new_blob_id() -> String {
    crypto::random_hex::<16>()
}

/// Pull (and push, when `push`). Retries when someone writes in between.
fn sync_folder(t: &Target, dir: &Path, push: bool) -> Result<(Plan, usize)> {
    let mut ws = Workspace::open(&t.api, &t.device, &t.workspace)?;
    let mut state = store::load_state(&t.workspace, dir)?;
    for _ in 0..5 {
        let (changes, seq) = ws.changes(state.seq)?;
        let local = read_folder(dir)?;
        let mut plan = engine::plan(&local, &state, &changes, &t.me, &mut new_blob_id);
        plan.state.seq = seq;
        apply(dir, &plan)?;
        state = plan.state.clone();
        store::save_state(&t.workspace, dir, &state)?;
        if !push {
            let waiting = plan.push.len();
            return Ok((plan, waiting));
        }
        let mut again = false;
        for p in &plan.push {
            match ws.put(p)? {
                None => {
                    state.pushed(p);
                    store::save_state(&t.workspace, dir, &state)?;
                }
                Some(code) => {
                    if code == "stale_epoch" {
                        ws.refresh_keys()?;
                    }
                    again = true;
                    break;
                }
            }
        }
        if !again {
            return Ok((plan, 0));
        }
    }
    bail!("the workspace kept changing while syncing; try again")
}

fn report(plan: &Plan, waiting: usize, pushed: bool) -> i32 {
    for p in &plan.write {
        println!("{} {}", "pulled ".green(), p.0);
    }
    for p in &plan.remove {
        println!("{} {}", "deleted".green(), p);
    }
    for p in &plan.merged {
        println!("{} {} (both changed different keys)", "merged ".cyan(), p);
    }
    if pushed {
        for p in plan.push.iter().filter(|p| !p.path.is_empty()) {
            println!("{} {}{}", "pushed ".blue(), p.path, if p.text.is_some() { "" } else { " (deleted)" });
        }
    } else if waiting > 0 {
        println!("{waiting} local change(s) not pushed: `hitop sync push`");
    }
    for s in &plan.skipped {
        eprintln!("{} {s}", "skipped".yellow());
    }
    for c in &plan.conflicts {
        println!("{} {c}: changed here and by someone else; `hitop sync resolve {c} --show`, then --mine or --theirs", "CONFLICT".red().bold());
    }
    if plan.write.is_empty() && plan.remove.is_empty() && (!pushed || plan.push.is_empty()) && plan.conflicts.is_empty() {
        println!("Up to date.");
    }
    i32::from(!plan.conflicts.is_empty())
}

fn sync(dir: &Path, push: bool) -> Result<i32> {
    let t = target(dir)?;
    if t.runner {
        if push {
            bail!("a runner credential is read-only: `hitop sync pull`");
        }
        std::fs::create_dir_all(dir)?;
    }
    let (plan, waiting) = sync_folder(&t, dir, push)?;
    Ok(report(&plan, waiting, push))
}

fn status(dir: &Path) -> Result<i32> {
    let t = target(dir)?;
    let ws = Workspace::open(&t.api, &t.device, &t.workspace)?;
    let state = store::load_state(&t.workspace, dir)?;
    let (changes, _) = ws.changes(state.seq)?;
    let plan = engine::plan(&read_folder(dir)?, &state, &changes, &t.me, &mut new_blob_id);
    println!("Workspace {} (epoch {})", t.workspace, ws.epoch);
    for p in plan.write.keys() {
        println!("  {} {p}", "incoming".green());
    }
    for p in &plan.remove {
        println!("  {} {p}", "incoming delete".green());
    }
    for p in plan.push.iter().filter(|p| !p.path.is_empty()) {
        println!("  {} {}", if p.text.is_some() { "to push".blue() } else { "to push (deleted)".blue() }, p.path);
    }
    for c in &plan.conflicts {
        println!("  {} {c}", "conflict".red().bold());
    }
    if plan.write.is_empty() && plan.remove.is_empty() && plan.push.is_empty() && plan.conflicts.is_empty() {
        println!("  up to date");
    }
    Ok(i32::from(!plan.conflicts.is_empty()))
}

fn resolve(dir: &Path, path: &str, mine: bool, theirs: bool, show: bool) -> Result<i32> {
    let t = target(dir)?;
    let mut state = store::load_state(&t.workspace, dir)?;
    let Some(c) = state.conflicts.get(path).cloned() else {
        bail!("{path} is not in conflict (`hitop sync status` lists conflicts)");
    };
    if show || (!mine && !theirs) {
        let local = std::fs::read_to_string(dir.join(path)).ok();
        let show = |label: &str, text: &Option<String>| {
            println!("{}", format!("===== {label} =====").bold());
            print!("{}", text.as_deref().unwrap_or("(deleted)\n"));
        };
        show("base (before both changes)", &c.base);
        show("mine (this folder)", &local);
        show("theirs (the server)", &c.theirs);
        println!("Then: hitop sync resolve {path} --mine | --theirs");
        return Ok(0);
    }
    match state.resolve(path, mine) {
        Some(Resolution::KeepMine) => println!("Keeping yours; `hitop sync push` sends it."),
        Some(Resolution::Write(text)) => {
            std::fs::write(dir.join(path), text)?;
            println!("Took theirs.");
        }
        Some(Resolution::Remove) => {
            let p = dir.join(path);
            if p.exists() {
                std::fs::remove_file(p)?;
            }
            println!("Took theirs (deleted).");
        }
        None => {}
    }
    store::save_state(&t.workspace, dir, &state)?;
    Ok(0)
}

fn init(dir: &Path) -> Result<()> {
    if !hitop_core::format::is_folder(dir) {
        bail!("{} is not a collection folder (no hitop.yaml): `hitop init` makes one", dir.display());
    }
    if store::link_of(dir)?.is_some() {
        bail!("{} is already synced", dir.display());
    }
    let s = session()?;
    let me = s.api.call("GET", "/v1/me", None)?;
    let recipients = keys_of(&s.account.email, &me["keys"])?;
    check_trust(&recipients, &TrustArgs::default())?;
    let id = crypto::random_hex::<16>();
    let key = WorkspaceKey::generate();
    s.api.call("POST", "/v1/workspaces", Some(json!({ "id": id, "wraps": wraps_for(&recipients, &id, 1, &key)? })))?;
    store::save_link(dir, Some(Link { workspace: id.clone(), server: s.account.server.clone() }))?;
    store::save_state(&id, dir, &State::default())?;
    let t = Target { workspace: id.clone(), me: s.device.key_id.clone(), api: s.api, device: s.device, runner: false };
    let (plan, _) = sync_folder(&t, dir, true)?;
    println!("Shared {} as workspace {id} ({} files, end-to-end encrypted).", dir.display(), plan.push.len());
    println!("Invite your team: hitop sync invite <email> --role editor -c {}", dir.display());
    Ok(())
}

fn link(workspace: &str, dir: &Path) -> Result<i32> {
    let s = session()?;
    s.api.call("GET", &format!("/v1/workspaces/{workspace}"), None).map_err(|e| anyhow!("{e} (are you a member? the owner adds you after you accept an invite)"))?;
    std::fs::create_dir_all(dir)?;
    store::save_link(dir, Some(Link { workspace: workspace.to_string(), server: s.account.server.clone() }))?;
    let t = Target { workspace: workspace.to_string(), me: s.device.key_id.clone(), api: s.api, device: s.device, runner: false };
    let (plan, waiting) = sync_folder(&t, dir, false)?;
    Ok(report(&plan, waiting, false))
}

/// The linked workspace, signed in (team commands).
fn team(dir: &Path) -> Result<(Session, String, Value)> {
    let s = session()?;
    let link = store::link_of(dir)?.ok_or_else(|| anyhow!("{} isn't synced (`hitop sync init` or `hitop sync link`)", dir.display()))?;
    let info = s.api.call("GET", &format!("/v1/workspaces/{}", link.workspace), None)?;
    Ok((s, link.workspace, info))
}

fn members(dir: &Path, trust: &TrustArgs) -> Result<()> {
    let (s, ws_id, info) = team(dir)?;
    let pins = store::load_pins()?;
    println!("Workspace {ws_id}, key epoch {}, you are {}", info["epoch"], info["role"].as_str().unwrap_or(""));
    let mut missing = Vec::new();
    for m in info["members"].as_array().cloned().unwrap_or_default() {
        let who = if m["runner"].as_bool().unwrap_or(false) { format!("runner credential {}", m["userId"].as_str().unwrap_or("")) } else { m["email"].as_str().unwrap_or("").to_string() };
        println!("{:<32} {}", who, m["role"].as_str().unwrap_or(""));
        for k in m["keys"].as_array().cloned().unwrap_or_default() {
            let pk = unb64(k["publicKey"].as_str().unwrap_or(""))?;
            let has = k["hasKey"].as_bool().unwrap_or(false);
            println!("    {:<24} {}  {}{}", k["name"].as_str().unwrap_or(""), crypto::fingerprint(&pk), trust_mark(&pins, k["id"].as_str().unwrap_or(""), &pk), if has { "" } else { "  (waiting for the workspace key)" });
            if !has {
                missing.push(Recipient { key_id: k["id"].as_str().unwrap_or("").into(), public_key: pk, owner: who.clone(), name: k["name"].as_str().unwrap_or("").into() });
            }
        }
    }
    if info["role"] == "owner" {
        let inv = s.api.call("GET", &format!("/v1/workspaces/{ws_id}/invites"), None)?;
        for i in inv["invites"].as_array().cloned().unwrap_or_default() {
            match i["accepted"]["email"].as_str() {
                Some(email) => {
                    println!("Invite accepted by {email} ({}): `hitop sync add {email}` after checking:", i["role"].as_str().unwrap_or(""));
                    for k in keys_of(email, &i["accepted"]["keys"])? {
                        println!("    {:<24} {}", k.name, crypto::fingerprint(&k.public_key));
                    }
                }
                None => println!("Invite open: {} ({}), until {}", if i["email"] == "" { "link" } else { i["email"].as_str().unwrap_or("") }, i["role"].as_str().unwrap_or(""), i["expiresAt"]),
            }
        }
    }
    // New devices of members: give them every epoch's key this device has, if trusted.
    if !missing.is_empty() && info["role"] != "viewer" {
        if let Err(e) = check_trust(&missing, trust) {
            eprintln!("{e}");
            return Ok(());
        }
        let ws = Workspace::open(&s.api, &s.device, &ws_id)?;
        for (epoch, key) in &ws.keys {
            s.api.call("POST", &format!("/v1/workspaces/{ws_id}/wraps"), Some(json!({ "epoch": epoch, "wraps": wraps_for(&missing, &ws_id, *epoch, key)? })))?;
        }
        println!("Gave the workspace key to {} new device(s).", missing.len());
    }
    Ok(())
}

fn invite(dir: &Path, email: Option<&str>, role: &str) -> Result<()> {
    let (s, ws_id, _) = team(dir)?;
    let mut body = json!({ "role": role });
    if let Some(e) = email {
        body["email"] = json!(e);
    }
    let r = s.api.call("POST", &format!("/v1/workspaces/{ws_id}/invites"), Some(body))?;
    match (email, r["mailed"].as_bool().unwrap_or(false)) {
        (Some(e), true) => println!("Invite mailed to {e} ({role})."),
        (Some(e), false) => println!("This server sends no mail; send {e} this link yourself:"),
        (None, _) => println!("Share this link privately (anyone with it can accept, once):"),
    }
    if !(email.is_some() && r["mailed"].as_bool().unwrap_or(false)) {
        println!("{}", r["link"].as_str().unwrap_or(""));
    }
    println!("When they've accepted: hitop sync members (check their key fingerprint), then hitop sync add <email>");
    Ok(())
}

fn accept(code: &str) -> Result<()> {
    let s = session()?;
    let code = code.rsplit('#').next().unwrap_or(code).trim();
    let r = s.api.call("POST", "/v1/invites/accept", Some(json!({ "token": code })))?;
    println!("Accepted: workspace {} as {}.", r["workspaceId"].as_str().unwrap_or(""), r["role"].as_str().unwrap_or(""));
    println!("Tell the owner your key fingerprint: {}", crypto::fingerprint(&s.device.keys.public_key()));
    println!("Once they add you: hitop sync link {} -c <folder>", r["workspaceId"].as_str().unwrap_or(""));
    Ok(())
}

fn add(dir: &Path, email: &str, role: Option<&str>, trust: &TrustArgs) -> Result<()> {
    let (s, ws_id, _) = team(dir)?;
    let email = email.trim().to_lowercase();
    let inv = s.api.call("GET", &format!("/v1/workspaces/{ws_id}/invites"), None)?;
    let accepted = inv["invites"].as_array().cloned().unwrap_or_default().into_iter().find(|i| i["accepted"]["email"] == email.as_str());
    let (keys, invited_role) = match &accepted {
        Some(i) => (i["accepted"]["keys"].clone(), i["role"].as_str().map(String::from)),
        None => (s.api.call("GET", &format!("/v1/users/keys?email={}", url_escape(&email)), None)?["keys"].clone(), None),
    };
    let role = role.map(String::from).or(invited_role).unwrap_or_else(|| "editor".into());
    let recipients = keys_of(&email, &keys)?;
    if recipients.is_empty() {
        bail!("{email} has no device key yet: they run `hitop sync login` first");
    }
    check_trust(&recipients, trust)?;
    let ws = Workspace::open(&s.api, &s.device, &ws_id)?;
    s.api.call("POST", &format!("/v1/workspaces/{ws_id}/members"), Some(json!({ "email": email, "role": role, "wraps": wraps_for(&recipients, &ws_id, ws.epoch, ws.current_key())? })))?;
    // Older epochs too, so they can read files not rewritten since a rotation.
    for (epoch, key) in ws.keys.iter().filter(|(e, _)| **e != ws.epoch) {
        s.api.call("POST", &format!("/v1/workspaces/{ws_id}/wraps"), Some(json!({ "epoch": epoch, "wraps": wraps_for(&recipients, &ws_id, *epoch, key)? })))?;
    }
    println!("Added {email} as {role}. They run: hitop sync link {ws_id} -c <folder>");
    Ok(())
}

fn url_escape(s: &str) -> String {
    url::form_urlencoded::byte_serialize(s.as_bytes()).collect()
}

fn remove(dir: &Path, email: &str, trust: &TrustArgs) -> Result<()> {
    let (s, ws_id, info) = team(dir)?;
    let email = email.trim().to_lowercase();
    let member = info["members"].as_array().cloned().unwrap_or_default().into_iter().find(|m| m["email"] == email.as_str() || m["userId"] == email.as_str());
    let Some(member) = member else { bail!("{email} is not a member") };
    let user = member["userId"].as_str().unwrap_or("").to_string();
    let remaining = member_keys(&info, Some(&user))?;
    check_trust(&remaining, trust)?;
    let next = info["epoch"].as_u64().unwrap_or(0) as u32 + 1;
    let key = WorkspaceKey::generate();
    s.api.call("DELETE", &format!("/v1/workspaces/{ws_id}/members/{user}"), Some(json!({ "epoch": next, "wraps": wraps_for(&remaining, &ws_id, next, &key)? })))?;
    let ws = Workspace::open(&s.api, &s.device, &ws_id)?;
    let n = ws.reencrypt_all(&s.device.key_id)?;
    println!("Removed {email}. New workspace key (epoch {next}); re-encrypted {n} file(s).");
    println!("They keep any copies they already have; nothing written from now on is readable to them.");
    Ok(())
}

fn set_role(dir: &Path, email: &str, role: &str) -> Result<()> {
    let (s, ws_id, info) = team(dir)?;
    let email = email.trim().to_lowercase();
    let member = info["members"].as_array().cloned().unwrap_or_default().into_iter().find(|m| m["email"] == email.as_str());
    let Some(member) = member else { bail!("{email} is not a member") };
    s.api.call("PUT", &format!("/v1/workspaces/{ws_id}/members/{}", member["userId"].as_str().unwrap_or("")), Some(json!({ "role": role })))?;
    println!("{email} is now {role}.");
    Ok(())
}

fn trust_cmd(dir: &Path, fingerprints: &[String]) -> Result<()> {
    let s = session()?;
    let mut candidates = keys_of(&s.account.email, &s.api.call("GET", "/v1/me", None)?["keys"])?;
    if let Ok(Some(link)) = store::link_of(dir) {
        let info = s.api.call("GET", &format!("/v1/workspaces/{}", link.workspace), None)?;
        candidates.extend(member_keys(&info, None)?);
        if info["role"] == "owner" {
            for i in s.api.call("GET", &format!("/v1/workspaces/{}/invites", link.workspace), None)?["invites"].as_array().cloned().unwrap_or_default() {
                candidates.extend(keys_of(i["accepted"]["email"].as_str().unwrap_or(""), &i["accepted"]["keys"])?);
            }
        }
    }
    let mut pins = store::load_pins()?;
    let mut by_fp: BTreeMap<String, &Recipient> = BTreeMap::new();
    for c in &candidates {
        by_fp.insert(normalize_fp(&crypto::fingerprint(&c.public_key)), c);
    }
    for f in fingerprints {
        let Some(c) = by_fp.get(&normalize_fp(f)) else { bail!("no device key with fingerprint {f} in your account or this workspace") };
        if let Some(p) = pins.get(&c.key_id) {
            if *p != b64(&c.public_key) {
                bail!("key {} changed since you trusted it; not trusting the new one", c.key_id);
            }
        }
        pins.insert(c.key_id.clone(), b64(&c.public_key));
        println!("Trusted {} ({}).", c.owner, c.name);
    }
    store::save_pins(&pins)
}

fn runner_create(dir: &Path, name: &str) -> Result<()> {
    let (s, ws_id, _) = team(dir)?;
    let ws = Workspace::open(&s.api, &s.device, &ws_id)?;
    let keys = DeviceKeyPair::generate();
    let wrapped = crypto::wrap_key(&keys.public_key(), &ws_id, ws.epoch, ws.current_key()).map_err(|e| anyhow!("{e}"))?;
    let r = s.api.call("POST", &format!("/v1/workspaces/{ws_id}/runners"), Some(json!({ "name": name, "publicKey": b64(&keys.public_key()), "wrapped": b64(&wrapped) })))?;
    let key_id = r["keyId"].as_str().unwrap_or("").to_string();
    // Older epochs, so the runner reads files not rewritten since a rotation.
    let recipient = [Recipient { key_id: key_id.clone(), public_key: keys.public_key().to_vec(), owner: "runner".into(), name: name.into() }];
    for (epoch, key) in ws.keys.iter().filter(|(e, _)| **e != ws.epoch) {
        s.api.call("POST", &format!("/v1/workspaces/{ws_id}/wraps"), Some(json!({ "epoch": epoch, "wraps": wraps_for(&recipient, &ws_id, *epoch, key)? })))?;
    }
    let mut pins = store::load_pins()?;
    pins.insert(key_id.clone(), b64(&keys.public_key()));
    store::save_pins(&pins)?;
    let cred = RunnerCredential { v: 1, server: s.account.server.clone(), workspace: ws_id.clone(), token: r["token"].as_str().unwrap_or("").into(), key_id, secret: b64(&keys.secret_bytes()) };
    eprintln!("A read-only runner credential for workspace {ws_id}, valid one year. It decrypts the workspace: keep it secret.");
    eprintln!("GitHub: gh secret set HITOP_SYNC_RUNNER (paste it); revoke with `hitop sync remove {}`", r["userId"].as_str().unwrap_or(""));
    println!("{}", cred.encode());
    Ok(())
}
