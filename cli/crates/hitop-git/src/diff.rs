//! A file's diff as lines: staged (HEAD → index) or unstaged (index →
//! working tree, untracked files as all new).

use std::path::Path;

use git2::{DiffFormat, DiffOptions};
use serde::{Deserialize, Serialize};

use crate::{head_commit, open, Result};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Line {
    /// "context", "add", "del", or "note" ("\ No newline at end of file").
    pub kind: String,
    pub old: Option<u32>,
    pub new: Option<u32>,
    pub text: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Hunk {
    /// "@@ -1,3 +1,4 @@ …"
    pub header: String,
    pub lines: Vec<Line>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Diff {
    pub path: String,
    pub staged: bool,
    pub binary: bool,
    pub hunks: Vec<Hunk>,
}

fn text(bytes: &[u8]) -> String {
    let s = String::from_utf8_lossy(bytes);
    s.strip_suffix('\n').map(|t| t.strip_suffix('\r').unwrap_or(t)).unwrap_or(&s).to_string()
}

/// The diff of one file (`file` relative to the working tree's root).
pub fn diff(path: impl AsRef<Path>, file: &str, staged: bool) -> Result<Diff> {
    let repo = open(path)?;
    let mut opts = DiffOptions::new();
    opts.pathspec(file).disable_pathspec_match(true).context_lines(3);
    let d = if staged {
        let tree = head_commit(&repo)?.map(|c| c.tree()).transpose()?;
        repo.diff_tree_to_index(tree.as_ref(), Some(&repo.index()?), Some(&mut opts))?
    } else {
        opts.include_untracked(true).recurse_untracked_dirs(true).show_untracked_content(true);
        repo.diff_index_to_workdir(Some(&repo.index()?), Some(&mut opts))?
    };
    let mut out = Diff { path: file.to_string(), staged, binary: false, hunks: Vec::new() };
    d.print(DiffFormat::Patch, |delta, hunk, line| {
        if delta.flags().is_binary() {
            out.binary = true;
        }
        let kind = match line.origin() {
            ' ' => "context",
            '+' => "add",
            '-' => "del",
            '=' | '>' | '<' => "note",
            'H' => {
                let header = hunk.map(|h| text(h.header())).unwrap_or_else(|| text(line.content()));
                out.hunks.push(Hunk { header, lines: Vec::new() });
                return true;
            }
            // file headers ('F') and binary notices ('B')
            'B' => {
                out.binary = true;
                return true;
            }
            _ => return true,
        };
        if out.hunks.is_empty() {
            out.hunks.push(Hunk { header: hunk.map(|h| text(h.header())).unwrap_or_default(), lines: Vec::new() });
        }
        let content = if kind == "note" { text(line.content()).trim().to_string() } else { text(line.content()) };
        out.hunks.last_mut().expect("a hunk").lines.push(Line { kind: kind.into(), old: line.old_lineno(), new: line.new_lineno(), text: content });
        true
    })?;
    Ok(out)
}
