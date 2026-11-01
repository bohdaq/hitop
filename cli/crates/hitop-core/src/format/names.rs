//! File names, ids and path checks for collection folders.
use std::collections::HashSet;

use anyhow::{bail, Result};

/// A file name from a request or environment name: lower case, letters and
/// digits kept, every other run of characters turned into one `-`.
pub fn slug(name: &str) -> String {
    let mut out = String::new();
    let mut dash = false;
    for c in name.chars().flat_map(char::to_lowercase) {
        if c.is_alphanumeric() {
            if dash && !out.is_empty() {
                out.push('-');
            }
            dash = false;
            out.push(c);
        } else {
            dash = true;
        }
        // Well under the 255-byte file name limit, numbering included.
        if out.len() >= 80 {
            break;
        }
    }
    if out.is_empty() {
        return "request".into();
    }
    // Names Windows can't create a file for.
    const RESERVED: [&str; 22] = [
        "con", "prn", "aux", "nul", "com1", "com2", "com3", "com4", "com5", "com6", "com7", "com8", "com9", "lpt1",
        "lpt2", "lpt3", "lpt4", "lpt5", "lpt6", "lpt7", "lpt8", "lpt9",
    ];
    if RESERVED.contains(&out.as_str()) {
        out.push_str("-file");
    }
    out
}

/// Checks a path read from a collection file (`order` entries, request
/// `file`s): relative, `/`-separated, no `.`/`..` or hidden segments, no
/// characters that mean something else on Windows. Folders come from
/// untrusted repositories, so nothing may point outside the folder.
pub fn check_rel_path(path: &str) -> Result<()> {
    if path.is_empty() {
        bail!("empty path");
    }
    for seg in path.split('/') {
        if seg.is_empty() || seg.starts_with('.') || seg.contains(['\\', ':', '\0']) {
            bail!("invalid path '{path}': use relative paths like 'orders/create-order'");
        }
    }
    Ok(())
}

/// The file for every request: the explicit one, or the slug of its name
/// at the top level, numbered (`-2`, `-3`) when taken. Explicit files are
/// reserved first, so a default name never takes one.
pub fn assign_files(requests: &[(&str, Option<&str>)]) -> Result<Vec<String>> {
    let mut used: HashSet<String> = HashSet::new();
    for (_, file) in requests {
        if let Some(f) = file {
            check_rel_path(f)?;
            if !used.insert(f.to_lowercase()) {
                bail!("two requests use the file '{f}'");
            }
        }
    }
    let mut out = Vec::with_capacity(requests.len());
    for (name, file) in requests {
        match file {
            Some(f) => out.push(f.to_string()),
            None => {
                let base = slug(name);
                let mut candidate = base.clone();
                let mut n = 2;
                while used.contains(&candidate.to_lowercase()) {
                    candidate = format!("{base}-{n}");
                    n += 1;
                }
                used.insert(candidate.to_lowercase());
                out.push(candidate);
            }
        }
    }
    Ok(out)
}

/// The inverse of [`assign_files`]: which files are the default for their
/// request and so need no `file` in the JSON model. Returns `Some(file)`
/// only for the ones that must be kept.
pub fn explicit_files(requests: &[(&str, &str)]) -> Vec<Option<String>> {
    let looks_default = |name: &str, file: &str| {
        let base = slug(name);
        file == base
            || file
                .strip_prefix(&base)
                .and_then(|rest| rest.strip_prefix('-'))
                .is_some_and(|n| !n.is_empty() && n.bytes().all(|b| b.is_ascii_digit()))
    };
    let mut default: Vec<bool> = requests.iter().map(|(n, f)| looks_default(n, f)).collect();
    loop {
        let input: Vec<(&str, Option<&str>)> = requests
            .iter()
            .zip(&default)
            .map(|((n, f), d)| (*n, if *d { None } else { Some(*f) }))
            .collect();
        let Ok(assigned) = assign_files(&input) else { break };
        let mut changed = false;
        for (i, (_, f)) in requests.iter().enumerate() {
            if default[i] && assigned[i] != *f {
                default[i] = false;
                changed = true;
            }
        }
        if !changed {
            break;
        }
    }
    requests.iter().zip(default).map(|((_, f), d)| if d { None } else { Some(f.to_string()) }).collect()
}

/// A stable id from a string (FNV-1a), small enough for a JavaScript
/// number. Files without an `id` get one from their path, so reading the
/// same folder twice gives the same ids.
pub fn derived_id(key: &str) -> u64 {
    let mut h: u64 = 0xcbf2_9ce4_8422_2325;
    for b in key.bytes() {
        h ^= b as u64;
        h = h.wrapping_mul(0x0000_0100_0000_01b3);
    }
    h & ((1 << 53) - 1)
}

pub fn request_id(file: &str) -> u64 {
    derived_id(&format!("requests/{file}"))
}

pub fn collection_id(name: &str) -> u64 {
    derived_id(&format!("collection/{name}"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn slugs() {
        assert_eq!(slug("Get user"), "get-user");
        assert_eq!(slug("  POST /orders/{id}  "), "post-orders-id");
        assert_eq!(slug("Список"), "список");
        assert_eq!(slug("!!!"), "request");
        assert_eq!(slug("CON"), "con-file");
    }

    #[test]
    fn rejects_paths_that_leave_the_folder() {
        for bad in ["../x", "a/../b", "/abs", "a\\b", "c:x", "a//b", ".hidden", ""] {
            assert!(check_rel_path(bad).is_err(), "{bad}");
        }
        check_rel_path("orders/create-order").unwrap();
    }

    #[test]
    fn duplicate_names_are_numbered_and_explicit_files_win() {
        let files = assign_files(&[("A", None), ("A", None), ("B", Some("a")), ("Z", Some("dir/z"))]).unwrap();
        assert_eq!(files, ["a-2", "a-3", "a", "dir/z"]);
        assert!(assign_files(&[("A", Some("x")), ("B", Some("X"))]).is_err());
    }

    #[test]
    fn explicit_files_undoes_assign_files() {
        let input = [("A", None), ("A", None), ("B", Some("a")), ("Z", Some("dir/z")), ("Q", Some("q-2"))];
        let files = assign_files(&input).unwrap();
        let pairs: Vec<(&str, &str)> = input.iter().zip(&files).map(|((n, _), f)| (*n, f.as_str())).collect();
        let back = explicit_files(&pairs);
        let back: Vec<Option<&str>> = back.iter().map(|f| f.as_deref()).collect();
        assert_eq!(back, [None, None, Some("a"), Some("dir/z"), Some("q-2")]);
    }
}
