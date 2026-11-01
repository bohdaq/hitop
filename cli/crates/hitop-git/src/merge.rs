//! Merging (what pull does after fetching, and merging a branch), and
//! resolving the conflicts a merge leaves: show base, ours and theirs, take
//! one side or an edited text, then commit the merge.

use std::path::Path;

use git2::{AnnotatedCommit, BranchType, Repository, RepositoryState};
use serde::{Deserialize, Serialize};

use crate::{err, file_statuses, head_commit, merge_heads, open, short, signature, state_name, workdir, Error, Result};

/// What a pull or merge did.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "result", rename_all = "camelCase")]
pub enum MergeOutcome {
    /// Nothing new.
    UpToDate,
    /// The branch moved forward to `head`.
    FastForward { head: String },
    /// A merge commit `head` was made.
    Merged { head: String },
    /// The merge stopped with conflicts in `files`: resolve them, then commit.
    Conflicts { files: Vec<String> },
}

/// Refuses to merge over a merge in progress or uncommitted changes to
/// tracked files (untracked files are fine; a fast-forward checks them itself).
fn ready_to_merge(repo: &Repository, need_clean: bool) -> Result<()> {
    if repo.state() != RepositoryState::Clean {
        return err(format!("a {} is in progress: finish or abort it first", state_name(repo.state())));
    }
    if need_clean {
        let dirty: Vec<String> = file_statuses(repo)?.into_iter().filter(|f| f.conflicted || f.index.is_some() || f.worktree.as_deref().is_some_and(|w| w != "untracked")).map(|f| f.path).collect();
        if !dirty.is_empty() {
            return err(format!("Commit or discard your changes first: {}", dirty.join(", ")));
        }
    }
    Ok(())
}

/// Merges `theirs` into the current branch: fast-forward when possible,
/// otherwise a merge commit, or the conflicts. Never rebases.
pub(crate) fn merge_commit(repo: &Repository, theirs: &AnnotatedCommit) -> Result<MergeOutcome> {
    let (analysis, _) = repo.merge_analysis(&[theirs])?;
    if analysis.is_up_to_date() {
        ready_to_merge(repo, false)?;
        return Ok(MergeOutcome::UpToDate);
    }
    let checkout_err = |e: git2::Error| {
        if e.code() == git2::ErrorCode::Conflict {
            Error("Your uncommitted changes would be overwritten by the merge: commit or discard them first".into())
        } else {
            e.into()
        }
    };
    if analysis.is_fast_forward() || analysis.is_unborn() {
        ready_to_merge(repo, false)?;
        let target = repo.find_commit(theirs.id())?;
        let mut co = git2::build::CheckoutBuilder::new();
        co.safe();
        repo.checkout_tree(target.as_object(), Some(&mut co)).map_err(checkout_err)?;
        let head = repo.find_reference("HEAD")?;
        let refname = head.symbolic_target()?.ok_or_else(|| Error("HEAD is detached: switch to a branch first".into()))?.to_string();
        repo.reference(&refname, target.id(), true, &format!("pull: fast-forward to {}", short(target.id())))?;
        return Ok(MergeOutcome::FastForward { head: short(target.id()) });
    }
    if !analysis.is_normal() {
        return err("these histories can't be merged");
    }
    ready_to_merge(repo, true)?;
    let mut co = git2::build::CheckoutBuilder::new();
    co.safe().allow_conflicts(true).conflict_style_merge(true);
    repo.merge(&[theirs], None, Some(&mut co)).map_err(checkout_err)?;
    let index = repo.index()?;
    if index.has_conflicts() {
        let files = conflicted_paths(&index)?;
        return Ok(MergeOutcome::Conflicts { files });
    }
    let message = repo.message().unwrap_or_else(|_| "Merge".into());
    let id = finish(repo, message.trim())?;
    Ok(MergeOutcome::Merged { head: short(id) })
}

/// Commits a merge that has no conflicts left.
fn finish(repo: &Repository, message: &str) -> Result<git2::Oid> {
    let sig = signature(repo)?;
    let mut index = repo.index()?;
    let tree = repo.find_tree(index.write_tree()?)?;
    let head = head_commit(repo)?.ok_or_else(|| Error("no commit to merge into".into()))?;
    let mut parents = vec![head];
    for id in merge_heads(repo)? {
        parents.push(repo.find_commit(id)?);
    }
    let refs: Vec<&git2::Commit> = parents.iter().collect();
    let id = repo.commit(Some("HEAD"), &sig, &sig, &format!("{message}\n"), &tree, &refs)?;
    repo.cleanup_state()?;
    Ok(id)
}

fn conflicted_paths(index: &git2::Index) -> Result<Vec<String>> {
    let mut files = Vec::new();
    for c in index.conflicts()? {
        let c = c?;
        let entry = c.our.as_ref().or(c.their.as_ref()).or(c.ancestor.as_ref());
        if let Some(e) = entry {
            files.push(String::from_utf8_lossy(&e.path).into_owned());
        }
    }
    files.sort();
    files.dedup();
    Ok(files)
}

/// Merges a local or remote branch ("feature", "origin/main") into the current one.
pub fn merge_branch(path: impl AsRef<Path>, name: &str) -> Result<MergeOutcome> {
    let repo = open(path)?;
    let branch = repo
        .find_branch(name, BranchType::Local)
        .or_else(|_| repo.find_branch(name, BranchType::Remote))
        .map_err(|_| Error(format!("no branch named '{name}'")))?;
    let theirs = repo.reference_to_annotated_commit(branch.get())?;
    merge_commit(&repo, &theirs)
}

/// A conflicted file: the three versions (None where that side has no
/// file, or it isn't text) and the working tree's text with conflict markers.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Conflict {
    pub path: String,
    pub base: Option<String>,
    pub ours: Option<String>,
    pub theirs: Option<String>,
    pub merged: Option<String>,
    pub binary: bool,
}

fn blob_text(repo: &Repository, entry: Option<&git2::IndexEntry>, binary: &mut bool) -> Result<Option<String>> {
    let Some(e) = entry else { return Ok(None) };
    let blob = repo.find_blob(e.id)?;
    match std::str::from_utf8(blob.content()) {
        Ok(s) if !blob.is_binary() => Ok(Some(s.to_string())),
        _ => {
            *binary = true;
            Ok(None)
        }
    }
}

pub fn conflict(path: impl AsRef<Path>, file: &str) -> Result<Conflict> {
    let repo = open(path)?;
    let index = repo.index()?;
    for c in index.conflicts()? {
        let c = c?;
        let entry = c.our.as_ref().or(c.their.as_ref()).or(c.ancestor.as_ref());
        if entry.map(|e| String::from_utf8_lossy(&e.path) == file) != Some(true) {
            continue;
        }
        let mut binary = false;
        let base = blob_text(&repo, c.ancestor.as_ref(), &mut binary)?;
        let ours = blob_text(&repo, c.our.as_ref(), &mut binary)?;
        let theirs = blob_text(&repo, c.their.as_ref(), &mut binary)?;
        let merged = std::fs::read(workdir(&repo)?.join(file)).ok().and_then(|b| String::from_utf8(b).ok());
        return Ok(Conflict { path: file.to_string(), base, ours, theirs, merged, binary });
    }
    err(format!("{file} has no conflict"))
}

/// How a conflict is resolved.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "take", rename_all = "camelCase")]
pub enum Resolution {
    /// Our version (the current branch's); a side without the file deletes it.
    Ours,
    /// Their version (the branch merged in).
    Theirs,
    /// This text.
    Content { text: String },
    /// The file as it is in the working tree now (edited by hand).
    Worktree,
}

/// Resolves a conflicted file and marks it resolved (staged).
pub fn resolve(path: impl AsRef<Path>, file: &str, how: &Resolution) -> Result<()> {
    let repo = open(path)?;
    let root = workdir(&repo)?;
    let mut index = repo.index()?;
    let side = {
        let mut found = None;
        for c in index.conflicts()? {
            let c = c?;
            let entry = c.our.as_ref().or(c.their.as_ref()).or(c.ancestor.as_ref());
            if entry.map(|e| String::from_utf8_lossy(&e.path) == file) == Some(true) {
                found = Some((c.our, c.their));
                break;
            }
        }
        found.ok_or_else(|| Error(format!("{file} has no conflict")))?
    };
    let target = root.join(file);
    let write = |bytes: &[u8]| -> Result<()> {
        if let Some(dir) = target.parent() {
            std::fs::create_dir_all(dir)?;
        }
        std::fs::write(&target, bytes)?;
        Ok(())
    };
    let pick = |entry: Option<git2::IndexEntry>| -> Result<()> {
        match entry {
            Some(e) => write(repo.find_blob(e.id)?.content()),
            None => {
                if target.exists() {
                    std::fs::remove_file(&target)?;
                }
                Ok(())
            }
        }
    };
    match how {
        Resolution::Ours => pick(side.0)?,
        Resolution::Theirs => pick(side.1)?,
        Resolution::Content { text } => write(text.as_bytes())?,
        Resolution::Worktree => {
            let text = std::fs::read_to_string(&target).unwrap_or_default();
            if text.lines().any(|l| l.starts_with("<<<<<<< ") || l.starts_with(">>>>>>> ")) {
                return err(format!("{file} still has conflict markers (<<<<<<< / >>>>>>>)"));
            }
        }
    }
    if target.symlink_metadata().is_ok() {
        index.add_path(Path::new(file))?;
    } else {
        index.remove_path(Path::new(file))?;
    }
    index.write()?;
    Ok(())
}

/// Abandons a merge in progress: back to the last commit (changes made
/// during the merge are lost).
pub fn abort_merge(path: impl AsRef<Path>) -> Result<()> {
    let repo = open(path)?;
    if repo.state() != RepositoryState::Merge {
        return err("no merge in progress");
    }
    let head = head_commit(&repo)?.ok_or_else(|| Error("no commit to go back to".into()))?;
    repo.reset(head.as_object(), git2::ResetType::Hard, None)?;
    repo.cleanup_state()?;
    Ok(())
}
