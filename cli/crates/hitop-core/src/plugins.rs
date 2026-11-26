//! Plugins (docs/PLUGINS_THREAT_MODEL.md; spec/collection-format.md, section 5c).
//!
//! A plugin is one JavaScript file a person installs on one device. Its
//! `value` hook is called from a request as `${plugin.name(arg, "text")}`:
//! each argument is a variable name (its value is passed) or a quoted text.
//! This module parses calls and manifests and holds the process-wide host
//! that runs them; it has no dependencies, so hitop-wasm builds it too.
//!
//! Nothing here runs JavaScript. A surface that can run plugins in a sandbox
//! (the CLI's `hitop plugin-run` child process) installs a [`Host`]; without
//! one, every call is refused by name, so a request that needs a plugin is
//! never sent with the call left in it as text.
use std::collections::{BTreeSet, HashMap, VecDeque};
use std::sync::{Mutex, OnceLock};

use serde::{Deserialize, Serialize};

use crate::model::{Collection, Environment, Request};

/// The start of a call's inner text.
pub const PREFIX: &str = "plugin.";

/// The hooks a plugin may have.
pub const HOOKS: [&str; 4] = ["value", "viewer", "importer", "snippet"];

/// Most bytes a plugin file or an answer may have.
pub const MAX_BYTES: usize = 1024 * 1024;

/// How long one call may take, in milliseconds, before it is ended.
pub const TIME_LIMIT_MS: u64 = 5_000;

/// One argument of a call.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Arg {
    /// A variable's name: its value is passed.
    Variable(String),
    /// A quoted text, passed as written (`\"` and `\\` unescaped).
    Text(String),
}

/// `plugin.<name>(<args>)`, parsed.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Call {
    pub plugin: String,
    pub args: Vec<Arg>,
}

/// True for a valid plugin name: a lowercase letter, then lowercase letters,
/// digits, `-` or `_`, at most 64 in all.
pub fn valid_name(name: &str) -> bool {
    let mut chars = name.chars();
    matches!(chars.next(), Some('a'..='z'))
        && name.len() <= 64
        && chars.all(|c| matches!(c, 'a'..='z' | '0'..='9' | '-' | '_'))
}

fn valid_variable(name: &str) -> bool {
    !name.is_empty() && name.chars().all(|c| c.is_ascii_alphanumeric() || matches!(c, '_' | '-' | '.'))
}

/// Parses the inner text of a `${...}` as a plugin call. `None` for anything
/// else, including `plugin.name` without parentheses (a variable name, as it
/// always was).
pub fn parse_call(inner: &str) -> Option<Call> {
    let rest = inner.trim().strip_prefix(PREFIX)?;
    let open = rest.find('(')?;
    let plugin = &rest[..open];
    if !valid_name(plugin) {
        return None;
    }
    let body = rest[open + 1..].strip_suffix(')')?;
    let mut args = vec![];
    let mut chars = body.chars().peekable();
    loop {
        while chars.peek().is_some_and(|c| c.is_whitespace()) {
            chars.next();
        }
        match chars.peek() {
            None if args.is_empty() => break,
            None => return None,
            Some('"') => {
                chars.next();
                let mut text = String::new();
                loop {
                    match chars.next()? {
                        '"' => break,
                        '\\' => match chars.next()? {
                            c @ ('"' | '\\') => text.push(c),
                            _ => return None,
                        },
                        c => text.push(c),
                    }
                }
                args.push(Arg::Text(text));
            }
            Some(_) => {
                let mut name = String::new();
                while let Some(&c) = chars.peek() {
                    if c == ',' || c.is_whitespace() {
                        break;
                    }
                    name.push(c);
                    chars.next();
                }
                if !valid_variable(&name) {
                    return None;
                }
                args.push(Arg::Variable(name));
            }
        }
        while chars.peek().is_some_and(|c| c.is_whitespace()) {
            chars.next();
        }
        match chars.next() {
            None => break,
            Some(',') => continue,
            Some(_) => return None,
        }
    }
    Some(Call { plugin: plugin.to_string(), args })
}

/// The plugin names called in a text.
pub fn find_in_text(text: &str, out: &mut BTreeSet<String>) {
    let mut rest = text;
    while let Some(start) = rest.find("${") {
        let after = &rest[start + 2..];
        let Some(end) = after.find('}') else { break };
        if let Some(call) = parse_call(&after[..end]) {
            out.insert(call.plugin);
        }
        rest = &after[end + 1..];
    }
}

/// The plugin names a request calls (URL, headers, body, auth, GraphQL,
/// form fields, WebSocket messages, gRPC message: every string in it).
pub fn find_in_request(request: &Request, out: &mut BTreeSet<String>) {
    if let Ok(value) = serde_json::to_value(request) {
        find_in_value(&value, out);
    }
}

/// The plugin names a collection calls: its requests, variables and auth.
pub fn find_in_collection(collection: &Collection, out: &mut BTreeSet<String>) {
    if let Ok(value) = serde_json::to_value(collection) {
        find_in_value(&value, out);
    }
}

/// The plugin names an environment's variable values call.
pub fn find_in_environment(environment: &Environment, out: &mut BTreeSet<String>) {
    find_in_variables(&environment.variables, out);
}

/// The plugin names in variable values.
pub fn find_in_variables(variables: &HashMap<String, String>, out: &mut BTreeSet<String>) {
    for v in variables.values() {
        find_in_text(v, out);
    }
}

fn find_in_value(value: &serde_json::Value, out: &mut BTreeSet<String>) {
    match value {
        serde_json::Value::String(s) => find_in_text(s, out),
        serde_json::Value::Array(a) => a.iter().for_each(|v| find_in_value(v, out)),
        serde_json::Value::Object(o) => o.values().for_each(|v| find_in_value(v, out)),
        _ => {}
    }
}

/// A plugin's manifest: the JSON in the comment that opens its file,
/// `/* hitop-plugin {"name": "...", "version": "...", "hooks": ["value"]} */`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Manifest {
    pub name: String,
    pub version: String,
    pub hooks: Vec<String>,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub description: String,
}

/// Reads and checks the manifest at the top of a plugin file.
pub fn manifest(source: &str) -> Result<Manifest, String> {
    if source.len() > MAX_BYTES {
        return Err(format!("a plugin file may have at most {} bytes", MAX_BYTES));
    }
    let text = source.trim_start_matches('\u{feff}').trim_start();
    let body = text
        .strip_prefix("/*")
        .and_then(|t| t.split_once("*/"))
        .map(|(c, _)| c.trim())
        .and_then(|c| c.strip_prefix("hitop-plugin"))
        .ok_or("a plugin file starts with its manifest: /* hitop-plugin {\"name\": ..., \"version\": ..., \"hooks\": [...]} */")?;
    let m: Manifest = serde_json::from_str(body.trim()).map_err(|e| format!("the manifest isn't valid: {e}"))?;
    if !valid_name(&m.name) {
        return Err(format!(
            "plugin name '{}' isn't valid: a lowercase letter, then lowercase letters, digits, - or _ (64 at most)",
            m.name
        ));
    }
    if m.version.trim().is_empty() || m.version.len() > 64 {
        return Err("the manifest needs a version (64 characters at most)".into());
    }
    if m.description.len() > 500 {
        return Err("the description may have at most 500 characters".into());
    }
    if m.hooks.is_empty() {
        return Err(format!("the manifest names no hooks (one or more of {})", HOOKS.join(", ")));
    }
    if let Some(h) = m.hooks.iter().find(|h| !HOOKS.contains(&h.as_str())) {
        return Err(format!("unknown hook '{h}' (hooks: {})", HOOKS.join(", ")));
    }
    Ok(m)
}

/// Runs a plugin's hook: `(plugin name, hook, arguments) -> answer`.
pub type Host = Box<dyn Fn(&str, &str, &[String]) -> Result<String, String> + Send + Sync>;

static HOST: OnceLock<Host> = OnceLock::new();

/// Lets this process run plugins (once; false when one was installed already).
pub fn install_host(host: Host) -> bool {
    HOST.set(host).is_ok()
}

/// True when this process can run plugins.
pub fn available() -> bool {
    HOST.get().is_some()
}

/// The refusal of a surface that can't run plugins.
pub fn unavailable(plugin: &str) -> String {
    format!("this request uses plugin '{plugin}', which can't run here: plugins run only where they are installed (hitop CLI: hitop plugin install <file>)")
}

/// What a call came to.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Outcome {
    /// The answer.
    Value(String),
    /// An argument variable has no value (yet): its name.
    Missing(String),
    /// The plugin isn't available, failed, or answered something wrong.
    Failed(String),
}

/// Calls a plugin's `value` hook with the call's arguments, variables
/// filled in from `variables`.
pub fn evaluate(call: &Call, variables: &HashMap<String, String>) -> Outcome {
    let mut args = Vec::with_capacity(call.args.len());
    for a in &call.args {
        match a {
            Arg::Text(t) => args.push(t.clone()),
            Arg::Variable(name) => match variables.get(name) {
                Some(v) => args.push(v.clone()),
                None => return Outcome::Missing(name.clone()),
            },
        }
    }
    let Some(host) = HOST.get() else {
        return Outcome::Failed(unavailable(&call.plugin));
    };
    match host(&call.plugin, "value", &args) {
        Ok(answer) if answer.len() > MAX_BYTES => {
            Outcome::Failed(format!("plugin '{}' answered more than {} bytes", call.plugin, MAX_BYTES))
        }
        Ok(answer) => {
            remember(args, &answer);
            Outcome::Value(answer)
        }
        Err(e) => Outcome::Failed(e),
    }
}

/// Answers and the arguments they were computed from, newest last: a
/// [`crate::secrets::Masker`] masks an answer whose arguments held a secret
/// value it masks (threat T11: base64 or an HMAC of a secret).
static DERIVED: Mutex<VecDeque<(Vec<String>, String)>> = Mutex::new(VecDeque::new());
const DERIVED_KEPT: usize = 1024;

fn remember(args: Vec<String>, answer: &str) {
    if answer.is_empty() || args.iter().all(|a| a.is_empty()) {
        return;
    }
    let mut d = DERIVED.lock().unwrap_or_else(|e| e.into_inner());
    if d.len() >= DERIVED_KEPT {
        d.pop_front();
    }
    d.push_back((args, answer.to_string()));
}

/// For tests elsewhere in the crate: as if a plugin had answered.
#[cfg(test)]
pub(crate) fn tests_remember(args: Vec<String>, answer: &str) {
    remember(args, answer);
}

/// The answers computed from an argument that contains one of `secrets`.
pub fn derived_from(secrets: &[String]) -> Vec<String> {
    if secrets.is_empty() {
        return vec![];
    }
    let d = DERIVED.lock().unwrap_or_else(|e| e.into_inner());
    d.iter()
        .filter(|(args, _)| args.iter().any(|a| secrets.iter().any(|s| !s.is_empty() && a.contains(s.as_str()))))
        .map(|(_, answer)| answer.clone())
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn v(n: &str) -> Arg {
        Arg::Variable(n.into())
    }
    fn t(s: &str) -> Arg {
        Arg::Text(s.into())
    }

    #[test]
    fn calls_parse_and_other_names_are_variables() {
        assert_eq!(parse_call("plugin.uuid()"), Some(Call { plugin: "uuid".into(), args: vec![] }));
        assert_eq!(
            parse_call(r#"plugin.hmac(key, "a \"b\" \\ c", body.x)"#).unwrap().args,
            vec![v("key"), t(r#"a "b" \ c"#), v("body.x")]
        );
        assert_eq!(parse_call(" plugin.b64( token ) ").unwrap().args, vec![v("token")]);
        assert_eq!(parse_call(r#"plugin.x("")"#).unwrap().args, vec![t("")]);
        for not_a_call in [
            "plugin.uuid",
            "plugin.Uuid()",
            "plugin.()",
            "plugin.x(a,)",
            "plugin.x(,a)",
            "plugin.x(a b)",
            r#"plugin.x("open)"#,
            r#"plugin.x("\n")"#,
            "plugin.x(a)(b)",
            "plugin.x(a(b))",
            "plugins.x()",
            "secret:vault:a#b",
            "name",
        ] {
            assert_eq!(parse_call(not_a_call), None, "{not_a_call}");
        }
    }

    #[test]
    fn finds_the_plugins_a_request_calls() {
        let r: Request = serde_json::from_value(serde_json::json!({
            "id": 1, "name": "r", "method": "GET",
            "url": "https://x/${plugin.sig(key)}?n=${plugin.nonce()}",
            "headers": [{"name": "X", "value": "${plugin.b64(t)} ${plugin.Bad()} ${plugin.var}"}],
            "body": "${plugin.sig(\"x\")}"
        }))
        .unwrap();
        let mut out = BTreeSet::new();
        find_in_request(&r, &mut out);
        assert_eq!(out.into_iter().collect::<Vec<_>>(), ["b64", "nonce", "sig"]);
    }

    #[test]
    fn manifests_are_checked() {
        let m = manifest("\u{feff}\n/* hitop-plugin {\"name\": \"sig\", \"version\": \"1.0.0\", \"hooks\": [\"value\"]} */\nfunction value(){}").unwrap();
        assert_eq!((m.name.as_str(), m.hooks.len()), ("sig", 1));
        for (bad, says) in [
            ("function value(){}", "starts with its manifest"),
            ("/* hitop-plugin {\"name\": \"Sig\", \"version\": \"1\", \"hooks\": [\"value\"]} */", "isn't valid"),
            ("/* hitop-plugin {\"name\": \"sig\", \"version\": \"\", \"hooks\": [\"value\"]} */", "version"),
            ("/* hitop-plugin {\"name\": \"sig\", \"version\": \"1\", \"hooks\": []} */", "no hooks"),
            ("/* hitop-plugin {\"name\": \"sig\", \"version\": \"1\", \"hooks\": [\"fetch\"]} */", "unknown hook 'fetch'"),
            ("/* hitop-plugin {name: sig} */", "isn't valid"),
        ] {
            let e = manifest(bad).unwrap_err();
            assert!(e.contains(says), "{bad}: {e}");
        }
        assert!(manifest(&format!("/* hitop-plugin {{}} */{}", " ".repeat(MAX_BYTES))).unwrap_err().contains("at most"));
    }

    #[test]
    fn without_a_host_a_call_is_refused_by_name() {
        // The test binary never installs a host.
        let call = parse_call("plugin.sig(key)").unwrap();
        assert_eq!(evaluate(&call, &HashMap::new()), Outcome::Missing("key".into()));
        let vars = HashMap::from([("key".to_string(), "k".to_string())]);
        match evaluate(&call, &vars) {
            Outcome::Failed(e) => assert!(e.contains("plugin 'sig'") && e.contains("can't run here"), "{e}"),
            other => panic!("{other:?}"),
        }
    }

    #[test]
    fn answers_from_a_secret_are_known_as_derived() {
        remember(vec!["Bearer s3cr3t-derived".into(), "x".into()], "czNjcjN0");
        remember(vec!["public".into()], "cHVibGlj");
        assert_eq!(derived_from(&["s3cr3t-derived".into()]), ["czNjcjN0"]);
        assert!(derived_from(&["nothing-like-it".into()]).is_empty());
        assert!(derived_from(&[]).is_empty());
    }
}
