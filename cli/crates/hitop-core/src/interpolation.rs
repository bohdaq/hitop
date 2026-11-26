use std::collections::HashMap;
use anyhow::{bail, Result};
use regex::Regex;

use crate::plugins::{self, Outcome};

/// What a `${...}` comes to: a variable's value, or a plugin call's answer
/// (spec section 5c). A variable named exactly like a call wins, as before
/// plugins.
enum Got {
    Value(String),
    /// No value (yet): the variable's name, or a call's missing argument.
    Missing(String),
    Failed(String),
}

fn lookup(key: &str, variables: &HashMap<String, String>) -> Got {
    if let Some(v) = variables.get(key) {
        // A variable's value may call a plugin (`nonce: ${plugin.nonce()}`):
        // the call is made, never sent as text. Other `${...}` in a value
        // stay as written (one pass).
        return if v.contains("${plugin.") { calls_in(v, variables) } else { Got::Value(v.clone()) };
    }
    match plugins::parse_call(key) {
        Some(call) => call_of(&call, variables),
        None => Got::Missing(key.to_string()),
    }
}

fn call_of(call: &plugins::Call, variables: &HashMap<String, String>) -> Got {
    match plugins::evaluate(call, variables) {
        Outcome::Value(v) => Got::Value(v),
        Outcome::Missing(name) => Got::Missing(name),
        Outcome::Failed(e) => Got::Failed(e),
    }
}

/// A variable's value with the plugin calls in it answered.
fn calls_in(value: &str, variables: &HashMap<String, String>) -> Got {
    let re = Regex::new(r"\$\{([^}]+)\}").unwrap();
    let mut bad = None;
    let out = re.replace_all(value, |caps: &regex::Captures| {
        let Some(call) = plugins::parse_call(&caps[1]) else { return caps[0].to_string() };
        match call_of(&call, variables) {
            Got::Value(v) => v,
            other => {
                bad.get_or_insert(other);
                caps[0].to_string()
            }
        }
    });
    bad.unwrap_or_else(|| Got::Value(out.into_owned()))
}

fn not_defined(name: &str) -> String {
    format!("variable '{}' is not defined", name)
}

/// The error for a `${...}` left without a value: names the variable, or a
/// plugin call's missing argument.
pub fn undefined(key: &str, variables: &HashMap<String, String>) -> String {
    match lookup(key, variables) {
        Got::Missing(name) => not_defined(&name),
        Got::Failed(e) => e,
        Got::Value(_) => not_defined(key),
    }
}

pub fn interpolate(template: &str, variables: &HashMap<String, String>) -> Result<String> {
    let re = Regex::new(r"\$\{([^}]+)\}").unwrap();
    let mut error: Option<String> = None;

    let result = re.replace_all(template, |caps: &regex::Captures| match lookup(&caps[1], variables) {
        Got::Value(val) => val,
        Got::Missing(name) => {
            error = Some(not_defined(&name));
            caps[0].to_string()
        }
        Got::Failed(e) => {
            error.get_or_insert(e);
            caps[0].to_string()
        }
    });

    if let Some(e) = error {
        bail!(e);
    }
    Ok(result.into_owned())
}

/// Like [`interpolate`], but a `${name}` with no value is left as written
/// and its name added to `missing` (once, in order). A plugin call that
/// fails is left as written too: [`interpolate_lenient_in`] reports it.
pub fn interpolate_lenient(template: &str, variables: &HashMap<String, String>, missing: &mut Vec<String>) -> String {
    let mut failed = None;
    lenient(template, variables, missing, &mut failed)
}

/// [`interpolate_lenient`], with a plugin call that fails (not available,
/// stopped, a wrong answer) as the error.
pub fn interpolate_lenient_in(template: &str, variables: &HashMap<String, String>, missing: &mut Vec<String>) -> Result<String> {
    let mut failed = None;
    let out = lenient(template, variables, missing, &mut failed);
    match failed {
        Some(e) => bail!(e),
        None => Ok(out),
    }
}

fn lenient(template: &str, variables: &HashMap<String, String>, missing: &mut Vec<String>, failed: &mut Option<String>) -> String {
    let re = Regex::new(r"\$\{([^}]+)\}").unwrap();
    re.replace_all(template, |caps: &regex::Captures| match lookup(&caps[1], variables) {
        Got::Value(val) => val,
        other => {
            if let Got::Failed(e) = other {
                failed.get_or_insert(e);
            }
            if !missing.iter().any(|m| m == &caps[1]) {
                missing.push(caps[1].to_string());
            }
            caps[0].to_string()
        }
    })
    .into_owned()
}

/// Replaces only the `${name}`s whose name is in `names`; one that still has
/// no value is an error, as in [`interpolate`].
pub fn fill(template: &str, names: &[String], variables: &HashMap<String, String>) -> Result<String> {
    let re = Regex::new(r"\$\{([^}]+)\}").unwrap();
    let mut error: Option<String> = None;
    let out = re.replace_all(template, |caps: &regex::Captures| {
        let key = &caps[1];
        if !names.iter().any(|n| n == key) {
            return caps[0].to_string();
        }
        match lookup(key, variables) {
            Got::Value(v) => v,
            Got::Missing(name) => {
                error.get_or_insert_with(|| not_defined(&name));
                caps[0].to_string()
            }
            Got::Failed(e) => {
                error.get_or_insert(e);
                caps[0].to_string()
            }
        }
    });
    if let Some(e) = error {
        bail!(e);
    }
    Ok(out.into_owned())
}

pub fn interpolate_headers(
    headers: &[(String, String)],
    variables: &HashMap<String, String>,
) -> Result<Vec<(String, String)>> {
    headers
        .iter()
        .map(|(k, v)| Ok((interpolate(k, variables)?, interpolate(v, variables)?)))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn vars(pairs: &[(&str, &str)]) -> HashMap<String, String> {
        pairs.iter().map(|(k, v)| (k.to_string(), v.to_string())).collect()
    }

    #[test]
    fn no_placeholders_returns_input_unchanged() {
        let result = interpolate("https://example.com/api", &vars(&[])).unwrap();
        assert_eq!(result, "https://example.com/api");
    }

    #[test]
    fn single_variable_replaced() {
        let result = interpolate("${host}/todos", &vars(&[("host", "https://api.example.com")])).unwrap();
        assert_eq!(result, "https://api.example.com/todos");
    }

    #[test]
    fn multiple_variables_replaced() {
        let result = interpolate(
            "${scheme}://${host}/${path}",
            &vars(&[("scheme", "https"), ("host", "api.example.com"), ("path", "users")]),
        )
        .unwrap();
        assert_eq!(result, "https://api.example.com/users");
    }

    #[test]
    fn same_variable_used_twice() {
        let result = interpolate("${a}+${a}", &vars(&[("a", "x")])).unwrap();
        assert_eq!(result, "x+x");
    }

    #[test]
    fn missing_variable_returns_error() {
        let err = interpolate("${missing}", &vars(&[])).unwrap_err();
        assert!(err.to_string().contains("missing"));
    }

    #[test]
    fn empty_template_returns_empty() {
        let result = interpolate("", &vars(&[("a", "b")])).unwrap();
        assert_eq!(result, "");
    }

    #[test]
    fn interpolate_headers_replaces_values() {
        let headers = vec![
            ("Authorization".to_string(), "Bearer ${token}".to_string()),
            ("X-App".to_string(), "hitop".to_string()),
        ];
        let result = interpolate_headers(&headers, &vars(&[("token", "abc123")])).unwrap();
        assert_eq!(result[0], ("Authorization".to_string(), "Bearer abc123".to_string()));
        assert_eq!(result[1], ("X-App".to_string(), "hitop".to_string()));
    }

    #[test]
    fn lenient_keeps_missing_and_fill_resolves_them() {
        let mut missing = vec![];
        let out = interpolate_lenient("${a}/${b}/${b}/${c}", &vars(&[("a", "1")]), &mut missing);
        assert_eq!(out, "1/${b}/${b}/${c}");
        assert_eq!(missing, ["b", "c"]);
        assert_eq!(fill("${b}-${x}", &missing, &vars(&[("b", "2")])).unwrap(), "2-${x}");
        assert!(fill("${c}", &missing, &vars(&[])).unwrap_err().to_string().contains("'c'"));
    }

    #[test]
    fn plugin_calls_without_a_host_are_refused_by_name() {
        let e = interpolate("a ${plugin.sig(key)} b", &vars(&[("key", "k")])).unwrap_err().to_string();
        assert!(e.contains("plugin 'sig'") && e.contains("can't run here"), "{e}");
        // A missing argument is named as a missing variable.
        let e = interpolate("${plugin.sig(key)}", &vars(&[])).unwrap_err().to_string();
        assert_eq!(e, "variable 'key' is not defined");
        assert_eq!(undefined("plugin.sig(key)", &vars(&[])), "variable 'key' is not defined");
        assert!(undefined("plugin.sig(key)", &vars(&[("key", "k")])).contains("plugin 'sig'"));
        let mut missing = vec![];
        let e = interpolate_lenient_in("${plugin.sig(\"x\")}", &vars(&[]), &mut missing).unwrap_err().to_string();
        assert!(e.contains("plugin 'sig'"), "{e}");
        assert!(fill("${plugin.sig()}", &["plugin.sig()".into()], &vars(&[])).is_err());
        // Without parentheses it is a variable name, as before; a variable
        // named exactly like a call is used as one.
        assert_eq!(interpolate("${plugin.sig}", &vars(&[("plugin.sig", "v")])).unwrap(), "v");
        assert_eq!(interpolate("${plugin.sig()}", &vars(&[("plugin.sig()", "w")])).unwrap(), "w");
        // A call in a variable's value is made too (and refused here); other
        // `${...}` in a value stay as written.
        let e = interpolate("${nonce}", &vars(&[("nonce", "n-${plugin.nonce()}")])).unwrap_err().to_string();
        assert!(e.contains("plugin 'nonce'"), "{e}");
        let mut missing = vec![];
        assert!(interpolate_lenient_in("${nonce}", &vars(&[("nonce", "${plugin.nonce()}")]), &mut missing).is_err());
        assert_eq!(interpolate("${a}", &vars(&[("a", "${b} ${plugin.x}")])).unwrap(), "${b} ${plugin.x}");
    }

    #[test]
    fn interpolate_headers_missing_variable_errors() {
        let headers = vec![("X-Key".to_string(), "${missing}".to_string())];
        assert!(interpolate_headers(&headers, &vars(&[])).is_err());
    }
}
