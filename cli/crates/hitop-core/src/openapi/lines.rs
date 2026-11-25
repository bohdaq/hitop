//! Which line a JSON pointer is on, in JSON or in block-style YAML. Best effort, for showing a
//! problem: what isn't found (flow style, anchors) takes the line of the nearest parent found.
use std::collections::HashMap;

pub type Lines = HashMap<String, usize>;

/// The line of `pointer`, else of its nearest parent.
pub fn find(lines: &Lines, pointer: &str) -> Option<usize> {
    let mut p = pointer;
    loop {
        if let Some(l) = lines.get(p) {
            return Some(*l);
        }
        p = &p[..p.rfind('/')?];
    }
}

pub fn index(text: &str) -> Lines {
    let text = text.trim_start_matches('\u{feff}');
    if text.trim_start().starts_with('{') && serde_json::from_str::<serde::de::IgnoredAny>(text).is_ok() {
        json(text)
    } else {
        yaml(text)
    }
}

fn esc(k: &str) -> String {
    k.replace('~', "~0").replace('/', "~1")
}

/// `text` is valid JSON.
fn json(text: &str) -> Lines {
    enum Frame {
        Object(Option<String>),
        Array(usize),
    }
    let mut out = Lines::new();
    let mut stack: Vec<Frame> = vec![];
    let mut line = 1;
    let b = text.as_bytes();
    let mut i = 0;
    let path = |stack: &[Frame]| -> String {
        stack
            .iter()
            .map(|f| match f {
                Frame::Object(k) => k.as_deref().map(|k| format!("/{}", esc(k))).unwrap_or_default(),
                Frame::Array(n) => format!("/{n}"),
            })
            .collect()
    };
    while i < b.len() {
        match b[i] {
            b'\n' => line += 1,
            b'"' => {
                let start = i;
                i += 1;
                while i < b.len() && b[i] != b'"' {
                    i += if b[i] == b'\\' { 2 } else { 1 };
                }
                if let Some(Frame::Object(k @ None)) = stack.last_mut() {
                    *k = serde_json::from_str(&text[start..=i]).ok();
                    out.entry(path(&stack)).or_insert(line);
                } else if let Some(Frame::Array(_)) = stack.last() {
                    out.entry(path(&stack)).or_insert(line);
                }
            }
            c @ (b'{' | b'[') => {
                if !stack.is_empty() {
                    out.entry(path(&stack)).or_insert(line);
                }
                stack.push(if c == b'{' { Frame::Object(None) } else { Frame::Array(0) });
            }
            b'}' | b']' => {
                stack.pop();
            }
            b',' => match stack.last_mut() {
                Some(Frame::Object(k)) => *k = None,
                Some(Frame::Array(n)) => *n += 1,
                None => {}
            },
            _ => {}
        }
        i += 1;
    }
    out
}

fn yaml(text: &str) -> Lines {
    struct Node {
        indent: usize,
        pointer: String,
        item: bool,
    }
    let mut out = Lines::new();
    let mut items: HashMap<String, usize> = HashMap::new();
    let mut open: Vec<Node> = vec![];
    // Lines deeper than this belong to a block scalar.
    let mut scalar: Option<usize> = None;
    for (n, raw) in text.lines().enumerate() {
        let trimmed = raw.trim_start();
        let mut indent = raw.len() - trimmed.len();
        if trimmed.is_empty() || scalar.is_some_and(|s| indent > s) {
            continue;
        }
        scalar = None;
        if trimmed.starts_with(['#', '%']) || trimmed.starts_with("---") {
            continue;
        }
        let mut rest = trimmed;
        while rest == "-" || rest.starts_with("- ") {
            // A key's items may stand at the key's own indent.
            while open.last().is_some_and(|o| o.indent > indent || (o.indent == indent && o.item)) {
                open.pop();
            }
            let parent = open.last().map_or("", |o| &o.pointer);
            let k = items.entry(parent.to_string()).or_insert(0);
            let pointer = format!("{parent}/{k}");
            *k += 1;
            out.entry(pointer.clone()).or_insert(n + 1);
            open.push(Node { indent, pointer, item: true });
            scalar = Some(indent);
            let after = rest[1..].trim_start();
            indent += rest.len() - after.len();
            rest = after;
        }
        if let Some((key, value)) = key_of(rest) {
            scalar = None;
            while open.last().is_some_and(|o| o.indent >= indent) {
                open.pop();
            }
            let pointer = format!("{}/{}", open.last().map_or("", |o| &o.pointer), esc(&key));
            out.entry(pointer.clone()).or_insert(n + 1);
            open.push(Node { indent, pointer, item: false });
            scalar = block_scalar(value).then_some(indent).or(scalar);
        } else if !block_scalar(rest) {
            scalar = None;
        }
    }
    out
}

fn block_scalar(value: &str) -> bool {
    let v = value.split(" #").next().unwrap_or("").trim_end();
    v.starts_with(['|', '>']) && v[1..].chars().all(|c| matches!(c, '+' | '-') || c.is_ascii_digit())
}

/// `key: value` (plain or quoted key), else None.
fn key_of(s: &str) -> Option<(String, &str)> {
    let (key, rest) = match s.as_bytes().first()? {
        b'"' | b'\'' => {
            let q = s.chars().next()?;
            let end = s[1..].find(q)? + 1;
            (&s[1..end], &s[end + 1..])
        }
        b'{' | b'[' | b'&' | b'*' | b'!' | b'|' | b'>' => return None,
        _ => {
            let end = s.find(": ").or_else(|| s.ends_with(':').then(|| s.len() - 1))?;
            (&s[..end], &s[end..])
        }
    };
    let value = rest.trim_start().strip_prefix(':')?;
    (value.is_empty() || value.starts_with(' ')).then(|| (key.trim_end().to_string(), value.trim_start()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn yaml_pointers() {
        let l = index("a:\n  b: |\n    c: no\n    - no\n  'd/e': 1\nlist:\n- x: 1\n  y: 2\n-   - deep: 1\n# c\n  - no\nother:\n  - 'k': v\n    z:\n      - 1\n      - 2\n");
        for (p, n) in [("/a", 1), ("/a/b", 2), ("/a/d~1e", 5), ("/list", 6), ("/list/0", 7), ("/list/0/x", 7), ("/list/0/y", 8), ("/list/1", 9), ("/list/1/0/deep", 9), ("/other", 12), ("/other/0/k", 13), ("/other/0/z/1", 16)] {
            assert_eq!(l.get(p), Some(&n), "{p}");
        }
        assert!(!l.contains_key("/a/c") && !l.contains_key("/a/b/c"));
        assert_eq!(find(&l, "/other/0/z/1/q/r"), Some(16));
        assert_eq!(find(&l, "/nowhere"), None);
    }
}
