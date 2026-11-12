//! `hitop sync`: end-to-end encrypted sync of a collection folder through a hitop sync server
//! (sync-server/, AGPL). The decisions are hitop-core's `sync` module; the encryption is
//! `hitop-sync-crypto`; this file is the command line around them. Secret values never sync:
//! environment files carry only their names.

mod audit;
mod client;
mod password;
mod recovery;
mod runs;
mod shares;
mod sso;
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

use client::{b64, unb64, Api, Device, Trust, Workspace};
use store::{Account, Link, RunnerCredential, Secret};

#[derive(Subcommand)]
pub enum SyncCmd {
    /// Sign in with your email and password (asked for), or a mailed code (--code), single
    /// sign-on (--sso), Google (--google), GitHub (--github) or Apple (--apple). A password sign-in on a new
    /// computer unlocks your end-to-end keys: no trust step
    Login {
        /// Your address (with --sso: optional, a hint for the sign-in page)
        #[arg(long, required_unless_present_any = ["sso", "google", "github", "apple"])]
        email: Option<String>,
        /// Sign in with a code mailed to you instead of the password. Alone, it mails one (put it
        /// last); with the code from the mail, it signs in
        #[arg(long, num_args = 0..=1, default_missing_value = "", allow_hyphen_values = true, value_name = "CODE")]
        code: Option<String>,
        /// Sign in with the server's identity provider (SSO) in a browser
        #[arg(long, conflicts_with_all = ["google", "github", "apple", "code", "password_stdin"])]
        sso: bool,
        /// Continue with Google in a browser
        #[arg(long, conflicts_with_all = ["github", "apple", "code", "password_stdin"])]
        google: bool,
        /// Continue with GitHub in a browser
        #[arg(long, conflicts_with_all = ["apple", "code", "password_stdin"])]
        github: bool,
        /// Continue with Apple in a browser
        #[arg(long, conflicts_with_all = ["code", "password_stdin"])]
        apple: bool,
        /// With --sso, --google, --github or --apple: don't open a browser here; print the link and paste
        /// the code shown after signing in (for SSH sessions)
        #[arg(long)]
        no_browser: bool,
        /// Read the password from the first line of stdin (scripts)
        #[arg(long)]
        password_stdin: bool,
        /// Sync server (default https://sync.hitop.rws8.tech, or HITOP_SYNC_SERVER)
        #[arg(long)]
        server: Option<String>,
        /// A name for this device, shown to your team next to its key fingerprint
        #[arg(long)]
        name: Option<String>,
    },
    /// Make an account with your email and a password: confirm the address with the code mailed
    /// to you (asked for on a terminal, or run again with --code)
    Signup {
        #[arg(long)]
        email: String,
        /// The code from the confirmation mail (finishes a sign-up started earlier)
        #[arg(long, allow_hyphen_values = true)]
        code: Option<String>,
        /// Read the password from the first line of stdin (scripts)
        #[arg(long)]
        password_stdin: bool,
        #[arg(long)]
        server: Option<String>,
        #[arg(long)]
        name: Option<String>,
    },
    /// Your password: set or change it, reset a forgotten one by mail, or unlock your keys with it
    #[command(subcommand)]
    Password(PasswordCmd),
    /// Sign out on this device (forgets the session and the device key)
    Logout,
    /// The account, this device and its key fingerprint
    Whoami,
    /// Your workspaces on the server, with the organization each belongs to
    Ls,
    /// Share a collection folder: make a new end-to-end encrypted workspace and push it
    Init {
        /// Create it in this organization (name or id; default your personal one)
        #[arg(long)]
        org: Option<String>,
        #[command(flatten)]
        folder: FolderArgs,
    },
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
    Accept {
        // Codes are base64url: one may start with '-'.
        #[arg(allow_hyphen_values = true)]
        code: String,
    },
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
    /// Rotate the workspace key (owner): a new key for every member device, then every file
    /// re-encrypted. For a lost device, or when an org admin asked for it
    Rotate {
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
    /// The workspace's run history (uploaded with `hitop run --sync-report` or `hitop sync
    /// report`): status rows, or one run's report opened on this device (--id)
    Runs {
        /// One run: its full report, decrypted here
        #[arg(long)]
        id: Option<String>,
        /// Print JSON (the rows, or with --id the report.json)
        #[arg(long)]
        json: bool,
        /// At most this many runs, newest first
        #[arg(long, default_value_t = 30)]
        limit: usize,
        #[command(flatten)]
        folder: FolderArgs,
    },
    /// Upload a run's report.json (from `hitop run --reporter json`, or the GitHub Action) to the
    /// workspace's history: a status row and the report sealed end to end. As a member or with
    /// HITOP_SYNC_RUNNER
    Report {
        /// The report.json a run wrote
        file: PathBuf,
        #[command(flatten)]
        folder: FolderArgs,
    },
    /// Alerts when a scheduled run fails, passes again or doesn't come (owners): mail to members,
    /// a JSON webhook, Slack. Without flags, shows the settings
    Alerts {
        /// Mail alerts to this member (repeatable)
        #[arg(long, value_name = "ADDRESS")]
        email: Vec<String>,
        /// Stop all alert mail (then --email adds addresses again)
        #[arg(long)]
        no_email: bool,
        /// POST a JSON body to this https URL on each alert (repeatable)
        #[arg(long, value_name = "URL")]
        webhook: Vec<String>,
        /// A Slack incoming webhook URL (repeatable)
        #[arg(long, value_name = "URL")]
        slack: Vec<String>,
        /// Remove every webhook (then --webhook/--slack add them again)
        #[arg(long)]
        no_webhooks: bool,
        /// Whether status rows and alerts may name the failed requests
        #[arg(long, value_parser = ["on", "off"])]
        failed_names: Option<String>,
        /// off = history only, no status rows and no alerts
        #[arg(long, value_parser = ["on", "off"])]
        status_rows: Option<String>,
        /// Alert once when no run came for this many minutes (0 = off)
        #[arg(long, value_name = "MINUTES")]
        missed_after: Option<i64>,
        /// The last alerts sent and how they went
        #[arg(long)]
        log: bool,
        #[command(flatten)]
        folder: FolderArgs,
    },
    /// Share an environment's secret values end to end with the team's devices (opt-in), list or
    /// stop shares. Values never go into the folder's files
    #[command(subcommand)]
    Secrets(SecretsCmd),
    /// A recovery phrase: brings this device back on a new computer if you lose it
    #[command(subcommand)]
    Recovery(RecoveryCmd),
    /// The workspace's audit log (owners): sign-ins aside, who added, removed or invited whom,
    /// role changes, key rotations, runner credentials. --verify checks its hash chain here.
    /// An organization's signed export (HITOP Cloud Business): --export --org <org>, checked
    /// against the daily anchors mailed to its owners with --verify --anchor <file>
    Audit {
        /// Print the events as JSON
        #[arg(long)]
        json: bool,
        /// Recompute the log's hash chain on this computer and compare it with what it saw last
        /// time (exit 1 when it is broken)
        #[arg(long)]
        verify: bool,
        /// Your own account's events (sign-ins, failed sign-ins, keys) instead of a workspace's
        #[arg(long, conflicts_with = "verify")]
        mine: bool,
        /// Only this event (member_added, signin_failed, key_rotated, ...)
        #[arg(long)]
        event: Option<String>,
        /// Only events by this address
        #[arg(long)]
        actor: Option<String>,
        /// At most this many events, newest first (0 = all)
        #[arg(long, default_value_t = 50)]
        limit: usize,
        /// Download the organization's signed audit export (owners and admins) and check it
        #[arg(long, requires = "org", conflicts_with_all = ["mine", "verify"])]
        export: bool,
        /// The organization (name or id) for --export
        #[arg(long)]
        org: Option<String>,
        /// Where --export writes the file (default hitop-audit-<org>-<date>.jsonl here)
        #[arg(long, requires = "export")]
        out: Option<PathBuf>,
        /// With --verify: check this saved export instead of a workspace's log (offline)
        #[arg(long, requires = "verify")]
        file: Option<PathBuf>,
        /// A daily anchor (the JSON, or the saved mail) whose heads the export must contain
        /// (repeatable; with --verify alone the anchor's organization's export is fetched)
        #[arg(long = "anchor", value_name = "FILE")]
        anchor: Vec<PathBuf>,
        #[command(flatten)]
        folder: FolderArgs,
    },
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

#[derive(Subcommand)]
pub enum SecretsCmd {
    /// Seal the environment's secret values (from your keychain or HITOP_SECRET_*) for the
    /// trusted devices of the workspace's owners and editors (and --viewers, --runner); asks
    /// before sharing, naming who gets them
    Share {
        #[arg(long = "env", value_name = "ENV")]
        env: String,
        /// Viewers' devices get them too
        #[arg(long)]
        viewers: bool,
        /// This runner credential (user id, see `hitop sync members`) gets them (repeatable)
        #[arg(long, value_name = "ID")]
        runner: Vec<String>,
        /// Every runner credential of the workspace gets them
        #[arg(long)]
        runners: bool,
        /// Don't ask (after checking who gets them)
        #[arg(long)]
        yes: bool,
        #[command(flatten)]
        trust: TrustArgs,
        #[command(flatten)]
        secrets: crate::commands::secret::SecretFlags,
        #[command(flatten)]
        folder: FolderArgs,
    },
    /// The shared environments: their secret names (never the values) and who has them
    Ls(FolderArgs),
    /// Stop sharing an environment's values
    Unshare {
        #[arg(long = "env", value_name = "ENV")]
        env: String,
        #[command(flatten)]
        folder: FolderArgs,
    },
}

#[derive(Subcommand)]
pub enum PasswordCmd {
    /// Add a password to your account, or change it (asks for the current one). Re-seals your
    /// key bundle; other devices are signed out
    Set {
        /// Read the passwords from stdin: the current one on the first line (when the account has
        /// one), then the new one
        #[arg(long)]
        password_stdin: bool,
    },
    /// Forgot your password: mails a reset code
    Forgot {
        #[arg(long)]
        email: String,
        #[arg(long)]
        server: Option<String>,
    },
    /// Choose a new password with the code from the reset mail. Your old password's key bundle
    /// is gone after this: unlock your workspaces with the recovery phrase or another device
    Reset {
        #[arg(long, allow_hyphen_values = true)]
        code: String,
        #[arg(long)]
        password_stdin: bool,
        #[arg(long)]
        server: Option<String>,
        #[arg(long)]
        name: Option<String>,
    },
    /// After signing in another way (code, Google, GitHub): unlock your keys with the password,
    /// so this computer becomes your device
    Unlock {
        #[arg(long)]
        password_stdin: bool,
    },
}

#[derive(Subcommand)]
pub enum RecoveryCmd {
    /// Make a 24-word recovery phrase (printed once; write it down) and keep this device's
    /// recovery data, sealed with it, on the server. Replaces an earlier phrase
    Create,
    /// On a new computer, after `hitop sync login`: become your lost device again with its phrase
    Use {
        /// Read the phrase from this file (`-` = stdin); otherwise asked on a terminal, or read
        /// from stdin
        #[arg(long, value_name = "FILE")]
        phrase_file: Option<String>,
    },
    /// Whether a recovery phrase is set, and when its data was last updated
    Status,
    /// Delete the recovery data from the server (the phrase then recovers nothing)
    Remove,
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
    let refresh = !matches!(cmd, SyncCmd::Login { .. } | SyncCmd::Signup { .. } | SyncCmd::Logout | SyncCmd::Recovery(_) | SyncCmd::Password(_));
    let result = run_cmd(cmd);
    // Keep the recovery data up to date with this device's trust and key memory.
    if refresh && result.is_ok() && std::env::var_os(store::RUNNER_ENV).is_none() {
        recovery::refresh();
    }
    result
}

fn run_cmd(cmd: SyncCmd) -> Result<i32> {
    match cmd {
        SyncCmd::Login { email, code, sso, google, github, apple, no_browser, password_stdin, server, name } => {
            let browser = if sso {
                Some("sso")
            } else if google {
                Some("google")
            } else if github {
                Some("github")
            } else if apple {
                Some("apple")
            } else {
                None
            };
            if no_browser && browser.is_none() {
                bail!("--no-browser goes with --sso, --google, --github or --apple");
            }
            let email = email.unwrap_or_default();
            match (browser, code) {
                (Some(kind), _) => browser_login(kind, Some(email.as_str()).filter(|e| !e.is_empty()), no_browser, server, name),
                (None, Some(code)) if code_is_flag(&code) => bail!("--code took {code} as the code: put --code last to have a code mailed, or give the code after it"),
                (None, Some(code)) => login(&email, Some(code).filter(|c| !c.is_empty()), server, name),
                (None, None) => password::login(&email, password_stdin, server, name),
            }
            .map(|_| 0)
        }
        SyncCmd::Signup { email, code, password_stdin, server, name } => password::signup(&email, code, password_stdin, server, name).map(|_| 0),
        SyncCmd::Password(PasswordCmd::Set { password_stdin }) => password::set(password_stdin).map(|_| 0),
        SyncCmd::Password(PasswordCmd::Forgot { email, server }) => password::forgot(&email, server).map(|_| 0),
        SyncCmd::Password(PasswordCmd::Reset { code, password_stdin, server, name }) => password::reset(&code, password_stdin, server, name).map(|_| 0),
        SyncCmd::Password(PasswordCmd::Unlock { password_stdin }) => password::unlock(password_stdin).map(|_| 0),
        SyncCmd::Logout => logout().map(|_| 0),
        SyncCmd::Whoami => whoami().map(|_| 0),
        SyncCmd::Ls => ls().map(|_| 0),
        SyncCmd::Init { org, folder } => init(&folder.path(), org.as_deref()).map(|_| 0),
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
        SyncCmd::Rotate { trust, folder } => rotate(&folder.path(), &trust).map(|_| 0),
        SyncCmd::Remove { email, trust, folder } => remove(&folder.path(), &email, &trust).map(|_| 0),
        SyncCmd::Role { email, role, folder } => set_role(&folder.path(), &email, &role).map(|_| 0),
        SyncCmd::Trust { fingerprints, folder } => trust_cmd(&folder.path(), &fingerprints).map(|_| 0),
        SyncCmd::Runner(RunnerCmd::Create { name, folder }) => runner_create(&folder.path(), &name).map(|_| 0),
        SyncCmd::Runs { id, json, limit, folder } => runs::list(&folder.path(), json, id.as_deref(), limit).map(|_| 0),
        SyncCmd::Report { file, folder } => runs::report_file(&file, &folder.path()).map(|_| 0),
        SyncCmd::Alerts { email, no_email, webhook, slack, no_webhooks, failed_names, status_rows, missed_after, log, folder } => runs::alerts(
            &folder.path(),
            runs::AlertChanges { email, no_email, webhook, slack, no_webhooks, failed_names: failed_names.map(|v| v == "on"), status_rows: status_rows.map(|v| v == "on"), missed_after, log },
        )
        .map(|_| 0),
        SyncCmd::Secrets(SecretsCmd::Share { env, viewers, runner, runners, yes, trust, secrets, folder }) => shares::share(&folder.path(), &env, viewers, runner, runners, yes, &trust, &secrets).map(|_| 0),
        SyncCmd::Secrets(SecretsCmd::Ls(folder)) => shares::ls(&folder.path()).map(|_| 0),
        SyncCmd::Secrets(SecretsCmd::Unshare { env, folder }) => shares::unshare(&folder.path(), &env).map(|_| 0),
        SyncCmd::Recovery(RecoveryCmd::Create) => recovery::create().map(|_| 0),
        SyncCmd::Recovery(RecoveryCmd::Use { phrase_file }) => recovery::use_phrase(phrase_file.as_deref()).map(|_| 0),
        SyncCmd::Recovery(RecoveryCmd::Status) => recovery::status().map(|_| 0),
        SyncCmd::Recovery(RecoveryCmd::Remove) => recovery::remove().map(|_| 0),
        SyncCmd::Audit { json, verify, mine, event, actor, limit, export, org, out, file, anchor, folder } => {
            if export {
                let s = session()?;
                let org = org.unwrap_or_default();
                let id = org_id(&s.api, &org)?;
                audit::export(&s.api, &s.account.server, &id, &org, out, &anchor)?;
                return Ok(0);
            }
            if verify && (file.is_some() || !anchor.is_empty()) {
                match file {
                    Some(f) => audit::verify_org(None, Some(&f), &anchor)?,
                    None => {
                        let s = session()?;
                        audit::verify_org(Some((&s.api, &s.account.server)), None, &anchor)?;
                    }
                }
                return Ok(0);
            }
            if !anchor.is_empty() || org.is_some() {
                bail!("--anchor goes with --verify or --export, --org with --export");
            }
            let filter = audit::Filter { event, actor, limit };
            if mine {
                let s = session()?;
                audit::run(&s.api, None, filter, json, false)?;
            } else {
                let t = target(&folder.path())?;
                if t.runner {
                    bail!("a runner credential can't read the audit log");
                }
                audit::run(&t.api, Some(&t.workspace), filter, json, verify)?;
            }
            Ok(0)
        }
    }
}

/// `hitop run --sync-report`: uploads the run to the folder's workspace history (a warning when it
/// can't; the run's exit code stays).
pub fn upload_run(dir: &Path, report: &hitop_core::report::Report) {
    runs::upload_after_run(dir, report)
}

/// `hitop run --sync-secrets`: the shared values of `env` as keychain accounts of `keychain_ws`.
pub fn shared_values(dir: &Path, env: &str, keychain_ws: &str) -> Result<BTreeMap<String, String>> {
    shares::values_for_run(dir, env, keychain_ws)
}

fn interactive() -> bool {
    std::io::stdin().is_terminal() && std::io::stderr().is_terminal()
}

pub(crate) fn ask(prompt: &str) -> Result<String> {
    eprint!("{prompt}");
    std::io::stderr().flush()?;
    let mut line = String::new();
    std::io::stdin().lock().read_line(&mut line)?;
    Ok(line.trim().to_string())
}

pub(crate) fn default_server() -> String {
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
    let device = Device { key_id: account.key_id.clone(), keys: secret.device()?, trust: Trust::Local };
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
    finish_login(&server, &r, email, name, None)
}

/// `--code` with nothing after it swallowed the next flag (`--code --server ...`).
fn code_is_flag(code: &str) -> bool {
    code.starts_with("--")
}

/// `--sso`, `--google`, `--github`: a browser sign-in through the server.
fn browser_login(kind: &str, email: Option<&str>, no_browser: bool, server: Option<String>, name: Option<String>) -> Result<()> {
    let server = server.unwrap_or_else(default_server);
    let anon = Api::new(&server, None)?;
    let r = sso::sign_in(&anon, kind, email, no_browser, interactive(), ask)?;
    let email = r["user"]["email"].as_str().unwrap_or("").to_string();
    finish_login(&server, &r, &email, name, None)
}

/// After a sign-in (`r` = `{session, user}`): keep this device's key or register a new one, and
/// save the account. `bundle_key` (a password sign-in): on a computer without this account's
/// device, the password's key bundle makes it that device (no new key, no trust step); a device
/// that is the bundle's device (or the first with a password) keeps the key to re-seal it.
pub(crate) fn finish_login(server: &str, r: &Value, email: &str, name: Option<String>, bundle_key: Option<[u8; 32]>) -> Result<()> {
    let server = server.to_string();
    let session = r["session"].as_str().context("no session in the answer")?.to_string();
    let user_id = r["user"]["id"].as_str().unwrap_or("").to_string();
    let email = r["user"]["email"].as_str().unwrap_or(email).to_string();
    let api = Api::new(&server, Some(session.clone()))?;

    // The same account on this device again: keep its key.
    let previous = Account::load()?.filter(|a| a.server.trim_end_matches('/') == server.trim_end_matches('/') && a.user_id == user_id);
    let reuse = previous.as_ref().and_then(|a| store::load_secret(&a.server).ok().map(|s| (a.clone(), s)));
    let me = api.call("GET", "/v1/me", None)?;
    let reuse = reuse.filter(|(a, _)| me["keys"].as_array().is_some_and(|k| k.iter().any(|k| k["id"] == a.key_id.as_str())));
    if let (None, Some(bk)) = (&reuse, &bundle_key) {
        if let Some(fp) = password::adopt(&api, &server, &email, &user_id, &session, bk)? {
            let a = Account::load()?.context("no account after unlocking")?;
            println!("Signed in to {server} as {email}.");
            println!("Unlocked with your password: this computer is your device \"{}\", key fingerprint {}", a.device_name, fp.bold());
            println!("Your workspaces: hitop sync ls; link a folder: hitop sync link <workspace> -c <folder>");
            return Ok(());
        }
    }
    let (key_id, keys, device_name, recovery, password) = match reuse {
        Some((a, s)) => (a.key_id.clone(), s.device()?, a.device_name.clone(), s.recovery, s.password),
        None => {
            let keys = DeviceKeyPair::generate();
            let device_name = name.unwrap_or_else(|| format!("hitop CLI on {}", hostname()));
            let r = api.call("POST", "/v1/me/keys", Some(json!({ "name": device_name, "publicKey": b64(&keys.public_key()) })))?;
            (r["id"].as_str().context("no key id")?.to_string(), keys, device_name, None, None)
        }
    };
    let new_key = !me["keys"].as_array().is_some_and(|k| k.iter().any(|k| k["id"] == key_id.as_str()));
    store::save_secret(&server, &Secret { session, secret: b64(&keys.secret_bytes()), recovery, password })?;
    let mut pins = store::load_pins()?;
    pins.insert(key_id.clone(), b64(&keys.public_key()));
    store::save_pins(&pins)?;
    let account = Account { server: server.clone(), email: email.clone(), user_id, key_id, device_name: device_name.clone() };
    account.save()?;
    if let Some(bk) = &bundle_key {
        password::claim_bundle(&api, &account, bk)?;
    }
    println!("Signed in to {server} as {email}.");
    println!("This device: {device_name}, key fingerprint {}", crypto::fingerprint(&keys.public_key()).bold());
    println!("Tell your team this fingerprint when they add you, and check theirs.");
    if new_key && bundle_key.is_none() && !me["bundles"]["password"].is_null() {
        println!("Your keys are sealed with your password: hitop sync password unlock makes this computer your device (no trust step).");
    }
    if !me["recovery"].is_null() && new_key {
        println!("Lost your other device? Its recovery phrase brings it back here: hitop sync recovery use");
    }
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
        // Servers before HITOP Cloud C1 send no org.
        let org = match &w["org"] {
            Value::Object(o) if o.get("personal").and_then(Value::as_bool) == Some(true) => "  personal".to_string(),
            Value::Object(o) => format!("  org {}", o.get("name").and_then(Value::as_str).unwrap_or("")),
            _ => String::new(),
        };
        println!("{}  {}{org}", w["id"].as_str().unwrap_or(""), w["role"].as_str().unwrap_or(""));
    }
    Ok(())
}

/// The id of the org `wanted` names (id, or name ignoring case), from the caller's orgs.
fn org_id(api: &client::Api, wanted: &str) -> Result<String> {
    let r = api.raw("GET", "/v1/orgs", None)?;
    if r.status == 404 {
        bail!("this sync server has no organizations");
    }
    if !(200..300).contains(&r.status) {
        return Err(client::api_error(&r));
    }
    let orgs = r.json["orgs"].as_array().cloned().unwrap_or_default();
    let name = |o: &Value| o["name"].as_str().unwrap_or("").to_string();
    let found: Vec<&Value> = orgs.iter().filter(|o| o["id"] == wanted).collect();
    let found = if found.is_empty() { orgs.iter().filter(|o| o["personal"] != true && name(o).eq_ignore_ascii_case(wanted)).collect() } else { found };
    match found.as_slice() {
        [o] => Ok(o["id"].as_str().unwrap_or("").to_string()),
        [] => {
            let names: Vec<String> = orgs.iter().filter(|o| o["personal"] != true).map(|o| format!("{} ({})", name(o), o["id"].as_str().unwrap_or(""))).collect();
            bail!("you are in no organization named {wanted}{}", if names.is_empty() { String::new() } else { format!("; yours: {}", names.join(", ")) })
        }
        _ => bail!("more than one organization is named {wanted}: use its id ({})", found.iter().map(|o| o["id"].as_str().unwrap_or("")).collect::<Vec<_>>().join(", ")),
    }
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

/// Authenticated (format 2) wraps of `key`, epoch `epoch`, made by this device for each
/// recipient; `head` is the workspace's current epoch.
fn wraps_for(me: &Device, recipients: &[Recipient], ws: &str, epoch: u32, head: u32, key: &WorkspaceKey) -> Result<Value> {
    let mut out = Vec::new();
    for r in recipients {
        let to = crypto::Recipient { key_id: &r.key_id, public_key: &r.public_key };
        let w = crypto::seal_wrap(&me.crypto(), &to, ws, epoch, head, key).map_err(|e| anyhow!("{e}"))?;
        out.push(json!({ "keyId": r.key_id, "wrapped": b64(&w) }));
    }
    Ok(Value::Array(out))
}

/// Replaces format 1 (unauthenticated, hitop 1.1) wraps in a workspace with authenticated ones,
/// as an owner: for every trusted device key of every member (runner credentials made by hitop
/// 1.1 can't verify them, so theirs stay), for every epoch this device holds. Members' clients
/// then accept keys only from trusted devices.
fn upgrade_wraps(me: &Device, api: &Api, ws: &Workspace, info: &Value) -> Result<()> {
    if info["role"] != "owner" {
        return Ok(());
    }
    let pins = store::load_pins()?;
    let mut by_epoch: BTreeMap<u32, Vec<Recipient>> = BTreeMap::new();
    let mut untrusted = Vec::new();
    for m in info["members"].as_array().cloned().unwrap_or_default() {
        let runner = m["runner"].as_bool().unwrap_or(false);
        let who = m["email"].as_str().unwrap_or("").to_string();
        for k in m["keys"].as_array().cloned().unwrap_or_default() {
            let epochs: Vec<u32> = k["legacyEpochs"].as_array().cloned().unwrap_or_default().iter().filter_map(|e| e.as_u64()).map(|e| e as u32).filter(|e| ws.keys.contains_key(e)).collect();
            if epochs.is_empty() {
                continue;
            }
            let pk = unb64(k["publicKey"].as_str().unwrap_or(""))?;
            let id = k["id"].as_str().unwrap_or("").to_string();
            if runner {
                eprintln!("{} the runner credential {} was made by hitop 1.1 and keeps an unauthenticated key: make a new one (`hitop sync runner create`), then remove it (`hitop sync remove {}`)", "note:".yellow(), m["userId"].as_str().unwrap_or(""), m["userId"].as_str().unwrap_or(""));
                continue;
            }
            if pins.get(&id) != Some(&b64(&pk)) {
                untrusted.push(format!("{who} ({})", crypto::fingerprint(&pk)));
                continue;
            }
            for e in epochs {
                by_epoch.entry(e).or_default().push(Recipient { key_id: id.clone(), public_key: pk.clone(), owner: who.clone(), name: String::new() });
            }
        }
    }
    let mut n = 0;
    for (epoch, recipients) in &by_epoch {
        let r = api.call("POST", &format!("/v1/workspaces/{}/wraps", ws.id), Some(json!({ "epoch": epoch, "wraps": wraps_for(me, recipients, &ws.id, *epoch, ws.epoch, &ws.keys[epoch])? })))?;
        n += r["stored"].as_u64().unwrap_or(0);
    }
    if n > 0 {
        eprintln!("Upgraded {n} workspace key wrap(s) to authenticated ones (hitop 1.2). Members confirm your fingerprint once: {}", crypto::fingerprint(&me.keys.public_key()));
    }
    if !untrusted.is_empty() {
        eprintln!("{} not upgraded, device keys you haven't trusted (compare, then `hitop sync members --trust <fingerprint>`): {}", "note:".yellow(), untrusted.join(", "));
    }
    Ok(())
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
        let pins = c.pins.iter().map(|(id, pk)| Ok((id.clone(), unb64(pk)?))).collect::<Result<BTreeMap<_, _>>>()?;
        let device = Device { key_id: c.key_id.clone(), keys: DeviceKeyPair::from_secret_bytes(&unb64(&c.secret)?).map_err(|e| anyhow!("runner key: {e}"))?, trust: Trust::Runner(pins) };
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
    if t.runner {
        return runner_pull(&ws, t, dir);
    }
    if ws.legacy {
        // Keys from hitop 1.1 here: an owner's sync upgrades them (others are told by the warning).
        let info = t.api.call("GET", &format!("/v1/workspaces/{}", t.workspace), None)?;
        upgrade_wraps(&t.device, &t.api, &ws, &info)?;
    }
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
            shares::reseal(&t.api, &t.device, &t.workspace);
            return Ok((plan, 0));
        }
    }
    bail!("the workspace kept changing while syncing; try again")
}

/// A runner's pull: the server wins. The folder ends up holding exactly the workspace's files
/// (a CI checkout may hold an older copy of the collection; it is overwritten, and synced files
/// the workspace doesn't have are removed). Nothing is pushed and no state is kept between runs.
fn runner_pull(ws: &Workspace, t: &Target, dir: &Path) -> Result<(Plan, usize)> {
    let (changes, seq) = ws.changes(0)?;
    let mut plan = engine::plan(&Files::new(), &State::default(), &changes, &t.me, &mut new_blob_id);
    plan.state.seq = seq;
    let on_disk = read_folder(dir)?;
    plan.write.retain(|p, text| on_disk.get(p) != Some(text));
    plan.remove = on_disk.keys().filter(|p| !plan.state.entries.get(*p).is_some_and(|e| e.base.is_some())).cloned().collect();
    plan.push.clear();
    apply(dir, &plan)?;
    Ok((plan, 0))
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
    rotation_note(&t);
    Ok(report(&plan, waiting, push))
}

/// Tells an owner when an org admin asked for the workspace key to be rotated (HITOP Cloud admin
/// console); quiet on any error, older servers don't send it.
fn rotation_note(t: &Target) {
    if t.runner {
        return;
    }
    let Ok(r) = t.api.raw("GET", &format!("/v1/workspaces/{}", t.workspace), None) else { return };
    if r.json["role"] == "owner" && r.json["rotationRequested"].is_object() {
        let by = r.json["rotationRequested"]["by"].as_str().unwrap_or("an org admin");
        eprintln!("{} {by} (an admin of the organization) asked to rotate this workspace's key (a lost device, say): run `hitop sync rotate`", "note:".yellow());
    }
}

fn status(dir: &Path) -> Result<i32> {
    let t = target(dir)?;
    rotation_note(&t);
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

fn init(dir: &Path, org: Option<&str>) -> Result<()> {
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
    let org = org.map(|o| org_id(&s.api, o)).transpose()?;
    let id = crypto::random_hex::<16>();
    let key = WorkspaceKey::generate();
    let mut body = json!({ "id": id, "wraps": wraps_for(&s.device, &recipients, &id, 1, 1, &key)? });
    if let Some(o) = &org {
        body["org"] = json!(o);
    }
    s.api.call("POST", "/v1/workspaces", Some(body))?;
    // Made with hitop 1.2: only authenticated keys from now on.
    store::save_memory(&id, &crypto::WorkspaceMemory { strict: true, ..Default::default() })?;
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
            s.api.call("POST", &format!("/v1/workspaces/{ws_id}/wraps"), Some(json!({ "epoch": epoch, "wraps": wraps_for(&s.device, &missing, &ws_id, *epoch, ws.epoch, key)? })))?;
        }
        println!("Gave the workspace key to {} new device(s).", missing.len());
    }
    if info["role"] != "viewer" {
        shares::reseal(&s.api, &s.device, &ws_id);
    }
    let legacy = info["members"].as_array().cloned().unwrap_or_default().iter().flat_map(|m| m["keys"].as_array().cloned().unwrap_or_default()).any(|k| k["legacyEpochs"].as_array().is_some_and(|e| !e.is_empty()));
    if legacy {
        if info["role"] == "owner" {
            let ws = Workspace::open(&s.api, &s.device, &ws_id)?;
            upgrade_wraps(&s.device, &s.api, &ws, &s.api.call("GET", &format!("/v1/workspaces/{ws_id}"), None)?)?;
        } else {
            eprintln!("{} some keys in this workspace were handed out by hitop 1.1 without authentication; an owner's next sync with hitop 1.2 upgrades them", "note:".yellow());
        }
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
    // The code carries this device's fingerprint, so their hitop checks the key it gets came from
    // here. A mailed invite can't: the server writes the mail.
    let fp = crypto::normalize_fingerprint(&crypto::fingerprint(&s.device.keys.public_key()));
    let token = r["token"].as_str().unwrap_or("");
    let link = r["link"].as_str().unwrap_or("");
    let link = match link.rsplit_once('#') {
        Some((base, _)) => format!("{base}#{token}.{fp}"),
        None => link.to_string(),
    };
    if !(email.is_some() && r["mailed"].as_bool().unwrap_or(false)) {
        println!("{link}");
    } else {
        println!("For them to check your key, also send them this code yourself (chat, in person), to use instead of the one in the mail:");
        println!("  hitop sync accept {token}.{fp}");
    }
    println!("Your key fingerprint: {}", crypto::fingerprint(&s.device.keys.public_key()));
    println!("When they've accepted: hitop sync members (check their key fingerprint), then hitop sync add <email>");
    Ok(())
}

fn accept(code: &str) -> Result<()> {
    let s = session()?;
    let code = code.rsplit('#').next().unwrap_or(code).trim();
    // `<token>.<inviter's fingerprint>` from hitop 1.2; a bare token from a mail or hitop 1.1.
    let (token, inviter) = match code.split_once('.') {
        Some((t, fp)) => (t, Some(crypto::normalize_fingerprint(fp))),
        None => (code, None),
    };
    if inviter.as_ref().is_some_and(|f| f.len() != 32) {
        bail!("this invite code is damaged (the part after the '.' must be a key fingerprint)");
    }
    let r = s.api.call("POST", "/v1/invites/accept", Some(json!({ "token": token })))?;
    let ws = r["workspaceId"].as_str().unwrap_or("").to_string();
    // Joined with hitop 1.2: only authenticated keys, and the inviter's device is trusted on sight.
    let mut memory = store::load_memory(&ws)?;
    memory.strict = true;
    if let Some(fp) = &inviter {
        if !memory.expected_senders.contains(fp) {
            memory.expected_senders.push(fp.clone());
        }
    }
    store::save_memory(&ws, &memory)?;
    println!("Accepted: workspace {} as {}.", ws, r["role"].as_str().unwrap_or(""));
    match &inviter {
        Some(fp) => println!("The workspace key will be accepted from the inviter's device with fingerprint {}.", fp.as_bytes().chunks(4).map(|c| std::str::from_utf8(c).unwrap_or("")).collect::<Vec<_>>().join(" ")),
        None => println!("This code doesn't name the inviter's key: when you link, hitop shows the fingerprint of the device that sent the key; compare it with them before trusting it."),
    }
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
    s.api.call("POST", &format!("/v1/workspaces/{ws_id}/members"), Some(json!({ "email": email, "role": role, "wraps": wraps_for(&s.device, &recipients, &ws_id, ws.epoch, ws.epoch, ws.current_key())? })))?;
    // Older epochs too, so they can read files not rewritten since a rotation.
    for (epoch, key) in ws.keys.iter().filter(|(e, _)| **e != ws.epoch) {
        s.api.call("POST", &format!("/v1/workspaces/{ws_id}/wraps"), Some(json!({ "epoch": epoch, "wraps": wraps_for(&s.device, &recipients, &ws_id, *epoch, ws.epoch, key)? })))?;
    }
    shares::reseal(&s.api, &s.device, &ws_id);
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
    s.api.call("DELETE", &format!("/v1/workspaces/{ws_id}/members/{user}"), Some(json!({ "epoch": next, "wraps": wraps_for(&s.device, &remaining, &ws_id, next, next, &key)? })))?;
    // Their devices are no longer trusted here: keys they hand out from now on are refused.
    let mut pins = store::load_pins()?;
    for k in keys_of(&email, &member["keys"])? {
        pins.remove(&k.key_id);
    }
    store::save_pins(&pins)?;
    let ws = Workspace::open(&s.api, &s.device, &ws_id)?;
    let n = ws.reencrypt_all(&s.device.key_id)?;
    println!("Removed {email}. New workspace key (epoch {next}); re-encrypted {n} file(s).");
    println!("They keep any copies they already have; nothing written from now on is readable to them.");
    match shares::rekey_all(&s.api, &s.device, &ws_id) {
        Ok((0, 0)) => {}
        Ok((k, closed)) => {
            println!("Re-keyed {k} shared environment(s) for the remaining devices.");
            if closed > 0 {
                eprintln!("{} {closed} shared environment(s) weren't shared with this device: whoever shared them re-shares them (`hitop sync secrets share`)", "note:".yellow());
            }
            eprintln!("{} {email} could see the shared secret values: rotate those secrets where they come from", "note:".yellow());
        }
        Err(e) => eprintln!("{} shared environments not re-keyed: {e:#}", "warning:".yellow()),
    }
    Ok(())
}

/// `hitop sync rotate`: a new workspace key for every member device (trusted ones only), every
/// file re-encrypted, shared environments re-keyed. Clears an org admin's rotation request.
fn rotate(dir: &Path, trust: &TrustArgs) -> Result<()> {
    let (s, ws_id, info) = team(dir)?;
    if info["role"] != "owner" {
        bail!("only an owner rotates the workspace key");
    }
    let keys = member_keys(&info, None)?;
    check_trust(&keys, trust)?;
    let next = info["epoch"].as_u64().unwrap_or(0) as u32 + 1;
    let key = WorkspaceKey::generate();
    s.api.call("POST", &format!("/v1/workspaces/{ws_id}/rotate"), Some(json!({ "epoch": next, "wraps": wraps_for(&s.device, &keys, &ws_id, next, next, &key)? })))?;
    let ws = Workspace::open(&s.api, &s.device, &ws_id)?;
    let n = ws.reencrypt_all(&s.device.key_id)?;
    println!("New workspace key (epoch {next}); re-encrypted {n} file(s).");
    match shares::rekey_all(&s.api, &s.device, &ws_id) {
        Ok((0, 0)) => {}
        Ok((k, _)) => println!("Re-keyed {k} shared environment(s)."),
        Err(e) => eprintln!("{} shared environments not re-keyed: {e:#}", "warning:".yellow()),
    }
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
    // The linked workspace's members, or (outside a linked folder) every workspace's.
    let workspaces: Vec<String> = match store::link_of(dir) {
        Ok(Some(link)) => vec![link.workspace],
        _ => s.api.call("GET", "/v1/workspaces", None)?["workspaces"].as_array().cloned().unwrap_or_default().iter().filter_map(|w| w["id"].as_str().map(String::from)).collect(),
    };
    for ws in &workspaces {
        let info = s.api.call("GET", &format!("/v1/workspaces/{ws}"), None)?;
        candidates.extend(member_keys(&info, None)?);
        if info["role"] == "owner" {
            for i in s.api.call("GET", &format!("/v1/workspaces/{ws}/invites"), None)?["invites"].as_array().cloned().unwrap_or_default() {
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
    store::save_pins(&pins)?;
    for ws in &workspaces {
        shares::reseal(&s.api, &s.device, ws);
    }
    Ok(())
}

fn runner_create(dir: &Path, name: &str) -> Result<()> {
    let (s, ws_id, info) = team(dir)?;
    let ws = Workspace::open(&s.api, &s.device, &ws_id)?;
    let keys = DeviceKeyPair::generate();
    // The server names the runner's key only when it makes the runner, and an authenticated wrap
    // names its recipient: so the first wrap is a placeholder (format 1, which the runner never
    // accepts) that the authenticated one replaces right after.
    let placeholder = crypto::wrap_key(&keys.public_key(), &ws_id, ws.epoch, &WorkspaceKey::generate()).map_err(|e| anyhow!("{e}"))?;
    let r = s.api.call("POST", &format!("/v1/workspaces/{ws_id}/runners"), Some(json!({ "name": name, "publicKey": b64(&keys.public_key()), "wrapped": b64(&placeholder) })))?;
    let key_id = r["keyId"].as_str().unwrap_or("").to_string();
    // Every epoch (older ones so the runner reads files not rewritten since a rotation).
    let recipient = [Recipient { key_id: key_id.clone(), public_key: keys.public_key().to_vec(), owner: "runner".into(), name: name.into() }];
    for (epoch, key) in &ws.keys {
        s.api.call("POST", &format!("/v1/workspaces/{ws_id}/wraps"), Some(json!({ "epoch": epoch, "wraps": wraps_for(&s.device, &recipient, &ws_id, *epoch, ws.epoch, key)? })))?;
    }
    let mut pins = store::load_pins()?;
    pins.insert(key_id.clone(), b64(&keys.public_key()));
    store::save_pins(&pins)?;
    // The runner accepts keys from this device and from the owners' and editors' devices trusted
    // here (a later rotation may come from one of them).
    let mut runner_pins = BTreeMap::from([(s.device.key_id.clone(), b64(&s.device.keys.public_key()))]);
    for m in info["members"].as_array().cloned().unwrap_or_default().iter().filter(|m| m["role"] == "owner" || m["role"] == "editor") {
        for k in m["keys"].as_array().cloned().unwrap_or_default() {
            let id = k["id"].as_str().unwrap_or("");
            if let Some(pk) = pins.get(id).filter(|p| Some(p.as_str()) == k["publicKey"].as_str()) {
                runner_pins.insert(id.to_string(), pk.clone());
            }
        }
    }
    let cred = RunnerCredential { v: 1, server: s.account.server.clone(), workspace: ws_id.clone(), token: r["token"].as_str().unwrap_or("").into(), key_id, secret: b64(&keys.secret_bytes()), pins: runner_pins };
    eprintln!("A read-only runner credential for workspace {ws_id}, valid one year. It decrypts the workspace: keep it secret.");
    eprintln!("GitHub: gh secret set HITOP_SYNC_RUNNER (paste it); revoke with `hitop sync remove {}`", r["userId"].as_str().unwrap_or(""));
    println!("{}", cred.encode());
    Ok(())
}
