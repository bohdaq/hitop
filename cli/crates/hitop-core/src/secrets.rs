//! Secret variables (spec/collection-format.md, section 5).
//!
//! An environment lists the names of its secret variables; their values
//! never go into collection files, history, exports or reports. At run time
//! each one is resolved, first match wins:
//!
//! 1. the `HITOP_SECRET_<ENV>_<NAME>` environment variable ([`env_var`]),
//! 2. `--secret NAME=value` on the command line,
//! 3. the OS keychain: service `hitop`, account `<workspace>/<env>/<name>`
//!    ([`account`], [`workspace_id`]),
//! 4. an interactive prompt, only when a terminal is attached.
//!
//! Where no keychain is reachable (headless Linux without a Secret Service,
//! a build without the `keychain` feature) step 3 is skipped with a note,
//! never a panic, so CI runs on env vars alone. Whatever a run prints or
//! keeps passes through a [`Masker`], which writes `••••` for every value.
use std::collections::{BTreeMap, BTreeSet, HashMap};
use std::sync::Mutex;

use anyhow::{bail, Result};

use crate::model::{Collection, Environment};

/// The keychain service every hitop secret is stored under.
pub const SERVICE: &str = "hitop";
/// The workspace id of collections in global storage (`~/.config/hitop`).
pub const GLOBAL_WORKSPACE: &str = "global";
/// What a secret value is replaced with in output.
pub const MASK: &str = "••••";

/// The keychain account of a secret: `<workspace>/<env>/<name>`.
pub fn account(workspace: &str, env: &str, name: &str) -> String {
    format!("{workspace}/{env}/{name}")
}

/// The environment variable that gives a secret's value:
/// `HITOP_SECRET_<ENV>_<NAME>`, where ENV and NAME are upper-cased and every
/// character other than an ASCII letter or digit becomes `_`
/// (`my-env` and `api.key` give `HITOP_SECRET_MY_ENV_API_KEY`).
pub fn env_var(env: &str, name: &str) -> String {
    let part = |s: &str| s.chars().map(|c| if c.is_ascii_alphanumeric() { c.to_ascii_uppercase() } else { '_' }).collect::<String>();
    format!("HITOP_SECRET_{}_{}", part(env), part(name))
}

/// The workspace a collection's secrets belong to. A folder collection uses
/// the `workspace` key of its `hitop.yaml` (written by `hitop init`), or,
/// without one, its collection id in decimal. Collections in global storage
/// share [`GLOBAL_WORKSPACE`].
pub fn workspace_id(collection: &Collection, is_folder: bool) -> String {
    if !is_folder {
        return GLOBAL_WORKSPACE.to_string();
    }
    match collection.workspace.as_deref() {
        Some(w) if !w.is_empty() => w.to_string(),
        _ => collection.id.to_string(),
    }
}

/// The environment name globals' secrets go by: env var
/// `HITOP_SECRET_GLOBAL_<NAME>`, keychain account `<workspace>/global/<name>`.
/// (An environment named `global` shares them.)
pub const GLOBALS_ENV: &str = "global";

/// The globals as [`resolve`] takes them: an environment named [`GLOBALS_ENV`].
pub fn globals_lookup_env(globals: &Environment) -> Environment {
    Environment { name: GLOBALS_ENV.into(), ..globals.clone() }
}

/// A new random workspace id: 32 hex digits.
pub fn new_workspace_id() -> String {
    use std::hash::{BuildHasher, Hasher};
    // RandomState is seeded from the OS's random source; two of them, fed
    // the time and process id, give 128 bits without a rand dependency.
    let part = || {
        let mut h = std::collections::hash_map::RandomState::new().build_hasher();
        let nanos = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap_or_default().as_nanos();
        h.write_u128(nanos);
        h.write_u32(std::process::id());
        h.finish()
    };
    format!("{:016x}{:016x}", part(), part())
}

/// Why a keychain call failed.
#[derive(Debug, Clone, PartialEq, thiserror::Error)]
pub enum StoreError {
    /// No keychain here (no Secret Service, locked, not built in).
    #[error("no OS keychain available: {0}")]
    Unavailable(String),
    #[error("OS keychain error: {0}")]
    Failed(String),
}

/// Where secret values are kept: the OS keychain ([`Keychain`]), or memory
/// in tests ([`MemoryStore`]).
pub trait SecretStore {
    fn get(&self, account: &str) -> Result<Option<String>, StoreError>;
    fn set(&self, account: &str, value: &str) -> Result<(), StoreError>;
    /// True when there was something to delete.
    fn delete(&self, account: &str) -> Result<bool, StoreError>;
}

/// The OS keychain: macOS Keychain, Windows Credential Manager, or the
/// Secret Service (GNOME Keyring, KWallet) over D-Bus on Linux.
#[cfg(feature = "keychain")]
pub struct Keychain;

#[cfg(feature = "keychain")]
impl Keychain {
    fn entry(account: &str) -> Result<keyring::Entry, StoreError> {
        keyring::Entry::new(SERVICE, account).map_err(store_error)
    }
}

#[cfg(feature = "keychain")]
fn store_error(e: keyring::Error) -> StoreError {
    use keyring::Error as E;
    match e {
        // The store couldn't start; say why (no D-Bus session, no Secret Service, ...).
        E::NoDefaultStore => StoreError::Unavailable(match keyring::Entry::store_status() {
            Err(why) => why.to_string(),
            Ok(()) => e.to_string(),
        }),
        E::NoStorageAccess(_) | E::PlatformFailure(_) => StoreError::Unavailable(e.to_string()),
        other => StoreError::Failed(other.to_string()),
    }
}

#[cfg(feature = "keychain")]
impl SecretStore for Keychain {
    fn get(&self, account: &str) -> Result<Option<String>, StoreError> {
        match Self::entry(account)?.get_password() {
            Ok(v) => Ok(Some(v)),
            Err(keyring::Error::NoEntry) => Ok(None),
            Err(e) => Err(store_error(e)),
        }
    }

    fn set(&self, account: &str, value: &str) -> Result<(), StoreError> {
        Self::entry(account)?.set_password(value).map_err(store_error)
    }

    fn delete(&self, account: &str) -> Result<bool, StoreError> {
        match Self::entry(account)?.delete_credential() {
            Ok(()) => Ok(true),
            Err(keyring::Error::NoEntry) => Ok(false),
            Err(e) => Err(store_error(e)),
        }
    }
}

/// A store that is never there; every call is [`StoreError::Unavailable`].
pub struct NoStore(pub String);

impl SecretStore for NoStore {
    fn get(&self, _: &str) -> Result<Option<String>, StoreError> {
        Err(StoreError::Unavailable(self.0.clone()))
    }
    fn set(&self, _: &str, _: &str) -> Result<(), StoreError> {
        Err(StoreError::Unavailable(self.0.clone()))
    }
    fn delete(&self, _: &str) -> Result<bool, StoreError> {
        Err(StoreError::Unavailable(self.0.clone()))
    }
}

/// The OS keychain, or [`NoStore`] in a build without the `keychain` feature.
pub fn os_store() -> Box<dyn SecretStore> {
    #[cfg(feature = "keychain")]
    return Box::new(Keychain);
    #[cfg(not(feature = "keychain"))]
    Box::new(NoStore("this build of hitop has no keychain support".into()))
}

/// An in-memory store: the mock keychain for tests.
#[derive(Default)]
pub struct MemoryStore(Mutex<BTreeMap<String, String>>);

impl MemoryStore {
    pub fn new() -> Self {
        Self::default()
    }
}

impl SecretStore for MemoryStore {
    fn get(&self, account: &str) -> Result<Option<String>, StoreError> {
        Ok(self.0.lock().expect("store lock").get(account).cloned())
    }
    fn set(&self, account: &str, value: &str) -> Result<(), StoreError> {
        self.0.lock().expect("store lock").insert(account.into(), value.into());
        Ok(())
    }
    fn delete(&self, account: &str) -> Result<bool, StoreError> {
        Ok(self.0.lock().expect("store lock").remove(account).is_some())
    }
}

/// Where a secret's value came from.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Origin {
    EnvVar,
    Flag,
    Keychain,
    Prompt,
}

impl std::fmt::Display for Origin {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        f.write_str(match self {
            Origin::EnvVar => "env var",
            Origin::Flag => "--secret",
            Origin::Keychain => "keychain",
            Origin::Prompt => "prompt",
        })
    }
}

/// Resolved secret values, by variable name.
#[derive(Debug, Clone, Default)]
pub struct Secrets {
    values: BTreeMap<String, (String, Origin)>,
}

impl Secrets {
    pub fn is_empty(&self) -> bool {
        self.values.is_empty()
    }

    pub fn get(&self, name: &str) -> Option<&str> {
        self.values.get(name).map(|(v, _)| v.as_str())
    }

    pub fn origin(&self, name: &str) -> Option<Origin> {
        self.values.get(name).map(|(_, o)| *o)
    }

    pub fn names(&self) -> impl Iterator<Item = &str> {
        self.values.keys().map(String::as_str)
    }

    /// Puts the secrets into a variable map, over any value already there.
    pub fn apply(&self, variables: &mut HashMap<String, String>) {
        for (k, (v, _)) in &self.values {
            variables.insert(k.clone(), v.clone());
        }
    }

    pub fn masker(&self) -> Masker {
        Masker::new(self.values.values().map(|(v, _)| v.as_str()))
    }

    /// A masker for these and `other`'s values (environment and globals secrets).
    pub fn masker_with(&self, other: &Secrets) -> Masker {
        Masker::new(self.values.values().chain(other.values.values()).map(|(v, _)| v.as_str()))
    }
}

/// Asks for a secret's value, given a label like `token (prod)`.
pub type Prompt<'a> = &'a dyn Fn(&str) -> Option<String>;

/// What [`resolve`] needs.
pub struct Lookup<'a> {
    /// See [`workspace_id`].
    pub workspace: &'a str,
    /// The environment's name, as given to `--env`; None without one.
    pub environment: Option<&'a Environment>,
    /// `--secret NAME=value` pairs. A name that isn't one of the
    /// environment's secrets becomes a secret for this run.
    pub flags: &'a [(String, String)],
    pub store: &'a dyn SecretStore,
    /// Reads a process environment variable (`std::env::var` outside tests).
    pub lookup_env: &'a dyn Fn(&str) -> Option<String>,
    /// Asks for a value; Some only when a terminal is attached.
    pub prompt: Option<Prompt<'a>>,
}

/// The secrets resolved and the notes worth printing (to stderr), such as
/// "no keychain here".
pub struct Resolved {
    pub secrets: Secrets,
    pub notes: Vec<String>,
}

/// Resolves every secret of the environment, plus the `--secret` names, in
/// the order of the module docs. Fails, naming each way to give a value, when
/// one has none.
pub fn resolve(req: &Lookup) -> Result<Resolved> {
    let mut names: BTreeSet<&str> = req.environment.map(|e| e.secrets.iter().map(String::as_str).collect()).unwrap_or_default();
    names.extend(req.flags.iter().map(|(k, _)| k.as_str()));
    let env_name = req.environment.map(|e| e.name.as_str());

    let mut secrets = Secrets::default();
    let mut notes = vec![];
    let mut keychain_down: Option<String> = None;
    let mut missing = vec![];
    for name in names {
        if let Some(env) = env_name {
            if let Some(v) = (req.lookup_env)(&env_var(env, name)) {
                secrets.values.insert(name.into(), (v, Origin::EnvVar));
                continue;
            }
        }
        if let Some((_, v)) = req.flags.iter().rev().find(|(k, _)| k == name) {
            secrets.values.insert(name.into(), (v.clone(), Origin::Flag));
            continue;
        }
        if let (Some(env), None) = (env_name, &keychain_down) {
            match req.store.get(&account(req.workspace, env, name)) {
                Ok(Some(v)) => {
                    secrets.values.insert(name.into(), (v, Origin::Keychain));
                    continue;
                }
                Ok(None) => {}
                Err(e) => {
                    notes.push(format!("{e}; secrets come from HITOP_SECRET_* env vars and --secret instead"));
                    keychain_down = Some(e.to_string());
                }
            }
        }
        if let Some(prompt) = req.prompt {
            let label = match env_name {
                Some(env) => format!("{name} ({env})"),
                None => name.to_string(),
            };
            if let Some(v) = prompt(&label) {
                secrets.values.insert(name.into(), (v, Origin::Prompt));
                continue;
            }
        }
        missing.push(name);
    }
    if !missing.is_empty() {
        let env = env_name.unwrap_or("<env>");
        let ways: Vec<String> = missing
            .iter()
            .map(|n| format!("  {n}: set {}, pass --secret {n}=..., or run `hitop secret set {n} --env {env}`", env_var(env, n)))
            .collect();
        let why = keychain_down.map(|e| format!("\n({e})")).unwrap_or_default();
        bail!("no value for secret{} {}:\n{}{why}", if missing.len() == 1 { "" } else { "s" }, missing.join(", "), ways.join("\n"));
    }
    Ok(Resolved { secrets, notes })
}

/// Replaces secret values with [`MASK`] in text that leaves the process:
/// output, history, reports, logs. Each value is also masked in its
/// percent-encoded form (as it appears in a query string).
#[derive(Debug, Clone, Default)]
pub struct Masker {
    values: Vec<String>,
}

impl Masker {
    pub fn new<'a>(values: impl IntoIterator<Item = &'a str>) -> Self {
        let mut all: Vec<String> = vec![];
        for v in values.into_iter().filter(|v| !v.is_empty()) {
            all.push(v.to_string());
            let enc = percent_encode(v);
            if enc != v {
                all.push(enc);
            }
        }
        // Longest first, so a value containing another is masked whole.
        all.sort_by(|a, b| b.len().cmp(&a.len()).then_with(|| a.cmp(b)));
        all.dedup();
        Masker { values: all }
    }

    pub fn is_empty(&self) -> bool {
        self.values.is_empty()
    }

    pub fn mask(&self, text: &str) -> String {
        let mut out = text.to_string();
        for v in &self.values {
            if out.contains(v.as_str()) {
                out = out.replace(v.as_str(), MASK);
            }
        }
        out
    }
}

fn percent_encode(s: &str) -> String {
    let mut out = String::new();
    for b in s.bytes() {
        if b.is_ascii_alphanumeric() || b"-._~".contains(&b) {
            out.push(b as char);
        } else {
            out.push_str(&format!("%{b:02X}"));
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn env(name: &str, secrets: &[&str]) -> Environment {
        Environment { id: name.into(), name: name.into(), secrets: secrets.iter().map(|s| s.to_string()).collect(), ..Default::default() }
    }

    fn no_env(_: &str) -> Option<String> {
        None
    }

    #[test]
    fn names_and_accounts() {
        assert_eq!(env_var("prod", "token"), "HITOP_SECRET_PROD_TOKEN");
        assert_eq!(env_var("my-env", "api.key"), "HITOP_SECRET_MY_ENV_API_KEY");
        assert_eq!(account("ws1", "prod", "token"), "ws1/prod/token");
    }

    #[test]
    fn workspace_ids() {
        let mut c = Collection { id: 42, ..Default::default() };
        assert_eq!(workspace_id(&c, false), GLOBAL_WORKSPACE);
        assert_eq!(workspace_id(&c, true), "42");
        c.workspace = Some("abc".into());
        assert_eq!(workspace_id(&c, true), "abc");
        assert_eq!(workspace_id(&c, false), GLOBAL_WORKSPACE);
        let (a, b) = (new_workspace_id(), new_workspace_id());
        assert_eq!(a.len(), 32);
        assert_ne!(a, b);
    }

    #[test]
    fn env_var_beats_flag_beats_keychain_beats_prompt() {
        let store = MemoryStore::new();
        for n in ["a", "b", "c", "d"] {
            store.set(&account("ws", "prod", n), &format!("kc-{n}")).unwrap();
        }
        let e = env("prod", &["a", "b", "c", "d", "e"]);
        let flags = vec![("a".to_string(), "flag-a".to_string()), ("b".to_string(), "flag-b".to_string())];
        let lookup = |k: &str| (k == "HITOP_SECRET_PROD_A").then(|| "env-a".to_string());
        let prompt = |label: &str| Some(format!("typed {label}"));
        let r = resolve(&Lookup { workspace: "ws", environment: Some(&e), flags: &flags, store: &store, lookup_env: &lookup, prompt: Some(&prompt) }).unwrap();
        let s = &r.secrets;
        assert_eq!((s.get("a"), s.origin("a")), (Some("env-a"), Some(Origin::EnvVar)));
        assert_eq!((s.get("b"), s.origin("b")), (Some("flag-b"), Some(Origin::Flag)));
        assert_eq!((s.get("c"), s.origin("c")), (Some("kc-c"), Some(Origin::Keychain)));
        assert_eq!((s.get("e"), s.origin("e")), (Some("typed e (prod)"), Some(Origin::Prompt)));
        assert!(r.notes.is_empty());
    }

    #[test]
    fn keychain_is_per_workspace_and_environment() {
        let store = MemoryStore::new();
        store.set("other/prod/token", "x").unwrap();
        store.set("ws/dev/token", "y").unwrap();
        let e = env("prod", &["token"]);
        let r = resolve(&Lookup { workspace: "ws", environment: Some(&e), flags: &[], store: &store, lookup_env: &no_env, prompt: None });
        assert!(r.is_err());
    }

    #[test]
    fn missing_secret_without_a_terminal_says_how_to_give_it() {
        let store = MemoryStore::new();
        let e = env("prod", &["token"]);
        let err = resolve(&Lookup { workspace: "ws", environment: Some(&e), flags: &[], store: &store, lookup_env: &no_env, prompt: None })
            .err()
            .unwrap()
            .to_string();
        assert!(err.contains("HITOP_SECRET_PROD_TOKEN"), "{err}");
        assert!(err.contains("--secret token="), "{err}");
        assert!(err.contains("hitop secret set token --env prod"), "{err}");
    }

    #[test]
    fn no_keychain_falls_back_to_env_vars_with_a_note() {
        let store = NoStore("no Secret Service".into());
        let e = env("prod", &["a", "b"]);
        let lookup = |k: &str| (k == "HITOP_SECRET_PROD_A").then(|| "1".to_string());
        // Everything from env vars: the keychain isn't asked, so no note.
        let all = |k: &str| k.starts_with("HITOP_SECRET_PROD_").then(|| "v".to_string());
        let r = resolve(&Lookup { workspace: "ws", environment: Some(&e), flags: &[], store: &store, lookup_env: &all, prompt: None }).unwrap();
        assert!(r.notes.is_empty());
        // One missing: the error says why the keychain didn't help.
        let err = resolve(&Lookup { workspace: "ws", environment: Some(&e), flags: &[], store: &store, lookup_env: &lookup, prompt: None })
            .err()
            .unwrap()
            .to_string();
        assert!(err.contains("no Secret Service"), "{err}");
        assert!(err.contains("HITOP_SECRET_PROD_B"), "{err}");
        // With a prompt the run goes on, with a note.
        let prompt = |_: &str| Some("typed".to_string());
        let r = resolve(&Lookup { workspace: "ws", environment: Some(&e), flags: &[], store: &store, lookup_env: &lookup, prompt: Some(&prompt) }).unwrap();
        assert_eq!(r.secrets.get("b"), Some("typed"));
        assert_eq!(r.notes.len(), 1);
        assert!(r.notes[0].contains("no OS keychain available"));
    }

    #[test]
    fn flags_work_without_an_environment() {
        let store = NoStore("unused".into());
        let flags = vec![("token".to_string(), "t0k".to_string())];
        let r = resolve(&Lookup { workspace: GLOBAL_WORKSPACE, environment: None, flags: &flags, store: &store, lookup_env: &no_env, prompt: None }).unwrap();
        let mut vars = HashMap::from([("token".to_string(), "plain".to_string())]);
        r.secrets.apply(&mut vars);
        assert_eq!(vars["token"], "t0k");
    }

    #[test]
    fn memory_store_set_get_delete() {
        let s = MemoryStore::new();
        assert_eq!(s.get("a").unwrap(), None);
        s.set("a", "1").unwrap();
        assert_eq!(s.get("a").unwrap().as_deref(), Some("1"));
        assert!(s.delete("a").unwrap());
        assert!(!s.delete("a").unwrap());
    }

    #[test]
    fn masker_hides_values_and_their_encoded_form() {
        let m = Masker::new(["s3cr3t", "a b&c", "", "s3cr3t-long"]);
        assert_eq!(m.mask("token=s3cr3t-long, s3cr3t"), "token=••••, ••••");
        assert_eq!(m.mask("https://x/?q=a%20b%26c"), "https://x/?q=••••");
        assert_eq!(m.mask("nothing here"), "nothing here");
        assert!(Masker::new([""]).is_empty());
    }
}
