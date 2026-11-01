//! Git for a hitop collection folder: status, diff, stage, commit, branches,
//! pull (fetch + fast-forward or merge, never a rebase), push and merge
//! conflicts, through libgit2 (`git2`, vendored), so it works where the `git`
//! binary can't run (the Mac App Store sandbox). The desktop app's Tauri
//! commands are thin wrappers over these functions; every function opens the
//! repository that contains `path` (the folder may be a subfolder of it).
//!
//! Remote access ([`Auth`]): an HTTPS token passed in by the caller (the
//! desktop keeps it in the OS keychain), then for HTTPS git's credential
//! helpers, for SSH the SSH agent. Each way is tried once.

use std::fmt;
use std::path::{Path, PathBuf};

use git2::{BranchType, ErrorCode, Repository, RepositoryState, Status as S, StatusOptions};
use serde::{Deserialize, Serialize};

mod auth;
mod diff;
pub mod json;
mod merge;
mod remote;

pub use auth::{token_account, username_account, Auth, Method};
pub use diff::{diff, Diff, Hunk, Line};
pub use merge::{abort_merge, conflict, merge_branch, resolve, Conflict, MergeOutcome, Resolution};
pub use remote::{fetch, pull, push, remote_host, PushOutcome};

/// An error with a message for the user.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Error(pub String);

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

impl std::error::Error for Error {}

impl From<git2::Error> for Error {
    fn from(e: git2::Error) -> Self {
        Error(e.message().to_string())
    }
}

impl From<std::io::Error> for Error {
    fn from(e: std::io::Error) -> Self {
        Error(e.to_string())
    }
}

pub type Result<T> = std::result::Result<T, Error>;

pub(crate) fn err<T>(message: impl Into<String>) -> Result<T> {
    Err(Error(message.into()))
}

/// The repository that contains `path`.
pub fn open(path: impl AsRef<Path>) -> Result<Repository> {
    let path = path.as_ref();
    match Repository::discover(path) {
        Ok(repo) if repo.is_bare() => err(format!("{} is in a bare repository", path.display())),
        Ok(repo) => Ok(repo),
        Err(e) if e.code() == ErrorCode::NotFound => err(format!("{} isn't in a git repository", path.display())),
        Err(e) => Err(e.into()),
    }
}

pub(crate) fn workdir(repo: &Repository) -> Result<PathBuf> {
    repo.workdir().map(Path::to_path_buf).ok_or_else(|| Error("the repository has no working tree".into()))
}

/// One changed path. `index` is the staged change, `worktree` the unstaged
/// one: "added", "modified", "deleted", "renamed", "typechange", or
/// "untracked" (worktree only). A conflicted path has `conflicted` and no
/// index/worktree change.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FileStatus {
    pub path: String,
    pub index: Option<String>,
    pub worktree: Option<String>,
    pub conflicted: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Identity {
    pub name: String,
    pub email: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RemoteInfo {
    pub name: String,
    pub url: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Status {
    /// The working tree's root.
    pub root: String,
    /// The folder relative to the root ("" when the folder is the root).
    pub prefix: String,
    /// The current branch; None when HEAD is detached.
    pub branch: Option<String>,
    /// HEAD's commit (short id); None before the first commit.
    pub head: Option<String>,
    /// The branch's upstream ("origin/main").
    pub upstream: Option<String>,
    pub ahead: usize,
    pub behind: usize,
    /// "clean", "merge", "revert", "cherry-pick", "rebase", "bisect", …
    pub state: String,
    /// The prepared message while a merge is in progress.
    pub merge_message: Option<String>,
    pub files: Vec<FileStatus>,
    /// Who commits are by (git config user.name and user.email), if set.
    pub identity: Option<Identity>,
    pub remotes: Vec<RemoteInfo>,
}

pub(crate) fn state_name(state: RepositoryState) -> &'static str {
    match state {
        RepositoryState::Clean => "clean",
        RepositoryState::Merge => "merge",
        RepositoryState::Revert | RepositoryState::RevertSequence => "revert",
        RepositoryState::CherryPick | RepositoryState::CherryPickSequence => "cherry-pick",
        RepositoryState::Bisect => "bisect",
        RepositoryState::Rebase | RepositoryState::RebaseInteractive | RepositoryState::RebaseMerge => "rebase",
        RepositoryState::ApplyMailbox | RepositoryState::ApplyMailboxOrRebase => "am",
    }
}

fn change(s: S, index: bool) -> Option<String> {
    let name = if index {
        match () {
            _ if s.contains(S::INDEX_NEW) => "added",
            _ if s.contains(S::INDEX_MODIFIED) => "modified",
            _ if s.contains(S::INDEX_DELETED) => "deleted",
            _ if s.contains(S::INDEX_RENAMED) => "renamed",
            _ if s.contains(S::INDEX_TYPECHANGE) => "typechange",
            _ => return None,
        }
    } else {
        match () {
            _ if s.contains(S::WT_NEW) => "untracked",
            _ if s.contains(S::WT_MODIFIED) => "modified",
            _ if s.contains(S::WT_DELETED) => "deleted",
            _ if s.contains(S::WT_RENAMED) => "renamed",
            _ if s.contains(S::WT_TYPECHANGE) => "typechange",
            _ => return None,
        }
    };
    Some(name.into())
}

pub(crate) fn file_statuses(repo: &Repository) -> Result<Vec<FileStatus>> {
    let mut opts = StatusOptions::new();
    opts.include_untracked(true).recurse_untracked_dirs(true).include_ignored(false).exclude_submodules(true);
    let statuses = repo.statuses(Some(&mut opts))?;
    let mut files: Vec<FileStatus> = statuses
        .iter()
        .filter_map(|e| {
            let path = e.path().ok()?.to_string();
            let s = e.status();
            if s.contains(S::CONFLICTED) {
                return Some(FileStatus { path, index: None, worktree: None, conflicted: true });
            }
            let f = FileStatus { path, index: change(s, true), worktree: change(s, false), conflicted: false };
            (f.index.is_some() || f.worktree.is_some()).then_some(f)
        })
        .collect();
    files.sort_by(|a, b| a.path.cmp(&b.path));
    Ok(files)
}

/// The current branch's short name; None when HEAD is detached.
pub(crate) fn current_branch(repo: &Repository) -> Result<Option<String>> {
    match repo.head() {
        Ok(head) if head.is_branch() => Ok(head.shorthand().ok().map(str::to_string)),
        Ok(_) => Ok(None),
        // Before the first commit HEAD names a branch that doesn't exist yet.
        Err(e) if e.code() == ErrorCode::UnbornBranch || e.code() == ErrorCode::NotFound => {
            let target = repo.find_reference("HEAD")?.symbolic_target()?.map(str::to_string);
            Ok(target.and_then(|t| t.strip_prefix("refs/heads/").map(str::to_string)))
        }
        Err(e) => Err(e.into()),
    }
}

pub(crate) fn head_commit(repo: &Repository) -> Result<Option<git2::Commit<'_>>> {
    match repo.head() {
        Ok(head) => Ok(Some(head.peel_to_commit()?)),
        Err(e) if e.code() == ErrorCode::UnbornBranch || e.code() == ErrorCode::NotFound => Ok(None),
        Err(e) => Err(e.into()),
    }
}

/// The commits being merged in (MERGE_HEAD).
pub(crate) fn merge_heads(repo: &Repository) -> Result<Vec<git2::Oid>> {
    // mergehead_foreach wants a &mut Repository: a second handle on the same repository.
    let mut ids = Vec::new();
    Repository::open(repo.path())?.mergehead_foreach(|id| {
        ids.push(*id);
        true
    })?;
    Ok(ids)
}

pub(crate) fn short(id: git2::Oid) -> String {
    id.to_string()[..7].to_string()
}

fn identity(repo: &Repository) -> Option<Identity> {
    let config = repo.config().ok()?;
    let name = config.get_string("user.name").ok().filter(|s| !s.trim().is_empty())?;
    let email = config.get_string("user.email").ok().filter(|s| !s.trim().is_empty())?;
    Some(Identity { name, email })
}

/// Status of the repository that contains `path`.
pub fn status(path: impl AsRef<Path>) -> Result<Status> {
    let path = path.as_ref();
    let repo = open(path)?;
    let root = workdir(&repo)?;
    let prefix = path
        .canonicalize()
        .ok()
        .and_then(|p| root.canonicalize().ok().and_then(|r| p.strip_prefix(&r).ok().map(|x| x.to_string_lossy().replace('\\', "/"))))
        .unwrap_or_default();
    let branch = current_branch(&repo)?;
    let head = head_commit(&repo)?;
    let (mut upstream, mut ahead, mut behind) = (None, 0, 0);
    if let (Some(name), Some(head)) = (&branch, &head) {
        if let Ok(local) = repo.find_branch(name, BranchType::Local) {
            if let Ok(up) = local.upstream() {
                upstream = up.name()?.map(str::to_string);
                if let Some(target) = up.get().target() {
                    (ahead, behind) = repo.graph_ahead_behind(head.id(), target)?;
                }
            }
        }
    }
    let merge_message = (repo.state() == RepositoryState::Merge).then(|| repo.message().ok()).flatten();
    let remotes = repo
        .remotes()?
        .iter()
        .filter_map(|n| n.ok().flatten())
        .map(|name| RemoteInfo { name: name.to_string(), url: repo.find_remote(name).ok().and_then(|r| r.url().ok().map(str::to_string)) })
        .collect();
    Ok(Status {
        root: root.to_string_lossy().into_owned(),
        prefix,
        branch,
        head: head.map(|c| short(c.id())),
        upstream,
        ahead,
        behind,
        state: state_name(repo.state()).into(),
        merge_message,
        files: file_statuses(&repo)?,
        identity: identity(&repo),
        remotes,
    })
}

/// Sets user.name and user.email in the repository's own config (for when
/// the global config isn't there, as in the Mac App Store sandbox).
pub fn set_identity(path: impl AsRef<Path>, name: &str, email: &str) -> Result<()> {
    let (name, email) = (name.trim(), email.trim());
    if name.is_empty() || email.is_empty() {
        return err("give both a name and an email");
    }
    let repo = open(path)?;
    let mut config = repo.config()?.open_level(git2::ConfigLevel::Local)?;
    config.set_str("user.name", name)?;
    config.set_str("user.email", email)?;
    Ok(())
}

fn exists(root: &Path, file: &str) -> bool {
    root.join(file).symlink_metadata().is_ok()
}

/// Stages files: their content (a deleted file: its removal). Staging a
/// conflicted file marks it resolved.
pub fn stage(path: impl AsRef<Path>, files: &[String]) -> Result<()> {
    let repo = open(path)?;
    let root = workdir(&repo)?;
    let mut index = repo.index()?;
    for f in files {
        if exists(&root, f) {
            index.add_path(Path::new(f))?;
        } else {
            index.remove_path(Path::new(f))?;
        }
    }
    index.write()?;
    Ok(())
}

/// Unstages files: the index entry goes back to HEAD's (the file stays as it is).
pub fn unstage(path: impl AsRef<Path>, files: &[String]) -> Result<()> {
    let repo = open(path)?;
    match head_commit(&repo)? {
        Some(head) => repo.reset_default(Some(head.as_object()), files.iter().map(String::as_str))?,
        None => {
            let mut index = repo.index()?;
            for f in files {
                index.remove_path(Path::new(f))?;
            }
            index.write()?;
        }
    }
    Ok(())
}

/// Puts files back to their committed content (untracked files are deleted).
pub fn discard(path: impl AsRef<Path>, files: &[String]) -> Result<()> {
    let repo = open(path)?;
    let root = workdir(&repo)?;
    let head = head_commit(&repo)?;
    let tree = head.as_ref().map(|c| c.tree()).transpose()?;
    let mut tracked = Vec::new();
    for f in files {
        if tree.as_ref().is_some_and(|t| t.get_path(Path::new(f)).is_ok()) {
            tracked.push(f.as_str());
        } else if exists(&root, f) {
            std::fs::remove_file(root.join(f))?;
        }
    }
    if !tracked.is_empty() {
        if let Some(head) = &head {
            repo.reset_default(Some(head.as_object()), tracked.iter().copied())?;
        }
        let mut co = git2::build::CheckoutBuilder::new();
        co.force();
        for f in &tracked {
            co.path(*f);
        }
        repo.checkout_head(Some(&mut co))?;
    }
    Ok(())
}

pub(crate) fn signature(repo: &Repository) -> Result<git2::Signature<'static>> {
    repo.signature().map_err(|_| Error("Set your name and email for commits first (git config user.name and user.email)".into()))
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CommitInfo {
    pub id: String,
    pub summary: String,
}

/// Commits what is staged, as git config's user.name and user.email. During
/// a merge this is the merge commit (all conflicts must be resolved).
pub fn commit(path: impl AsRef<Path>, message: &str) -> Result<CommitInfo> {
    let repo = open(path)?;
    let message = message.trim();
    if message.is_empty() {
        return err("write a commit message");
    }
    let merging = repo.state() == RepositoryState::Merge;
    if !merging && repo.state() != RepositoryState::Clean {
        return err(format!("a {} is in progress: finish it with git first", state_name(repo.state())));
    }
    let mut index = repo.index()?;
    if index.has_conflicts() {
        return err("resolve the conflicts first");
    }
    let sig = signature(&repo)?;
    let tree = repo.find_tree(index.write_tree()?)?;
    let head = head_commit(&repo)?;
    let mut parents: Vec<git2::Commit> = head.iter().cloned().collect();
    if merging {
        for id in merge_heads(&repo)? {
            parents.push(repo.find_commit(id)?);
        }
    } else if head.as_ref().map(|h| h.tree_id()) == Some(tree.id()) || (head.is_none() && tree.is_empty()) {
        return err("nothing to commit: stage some changes first");
    }
    let refs: Vec<&git2::Commit> = parents.iter().collect();
    let id = repo.commit(Some("HEAD"), &sig, &sig, &format!("{message}\n"), &tree, &refs)?;
    if merging {
        repo.cleanup_state()?;
    }
    Ok(CommitInfo { id: short(id), summary: message.lines().next().unwrap_or_default().to_string() })
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Branch {
    pub name: String,
    pub current: bool,
    pub upstream: Option<String>,
    /// Merged into the current branch (so it can be deleted).
    pub merged: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Branches {
    pub local: Vec<Branch>,
    /// Remote branches ("origin/feature"), without the remotes' HEAD.
    pub remote: Vec<String>,
}

fn merged_into(repo: &Repository, head: Option<git2::Oid>, id: Option<git2::Oid>) -> bool {
    match (head, id) {
        (Some(h), Some(b)) => h == b || repo.graph_descendant_of(h, b).unwrap_or(false),
        _ => false,
    }
}

pub fn branches(path: impl AsRef<Path>) -> Result<Branches> {
    let repo = open(path)?;
    let head = head_commit(&repo)?.map(|c| c.id());
    let current = current_branch(&repo)?;
    let mut local = Vec::new();
    for b in repo.branches(Some(BranchType::Local))? {
        let (b, _) = b?;
        let Some(name) = b.name()?.map(str::to_string) else { continue };
        let upstream = b.upstream().ok().and_then(|u| u.name().ok().flatten().map(str::to_string));
        let merged = merged_into(&repo, head, b.get().target());
        local.push(Branch { current: current.as_deref() == Some(name.as_str()), name, upstream, merged });
    }
    // An unborn branch (no commit yet) still shows as the current one.
    if let Some(name) = current.filter(|n| !local.iter().any(|b| &b.name == n)) {
        local.push(Branch { name, current: true, upstream: None, merged: false });
    }
    local.sort_by(|a, b| a.name.cmp(&b.name));
    let mut remote = Vec::new();
    for b in repo.branches(Some(BranchType::Remote))? {
        let (b, _) = b?;
        if b.get().symbolic_target()?.is_some() {
            continue;
        }
        if let Some(name) = b.name()? {
            remote.push(name.to_string());
        }
    }
    remote.sort();
    Ok(Branches { local, remote })
}

fn valid_branch_name(name: &str) -> Result<()> {
    if git2::Branch::name_is_valid(name)? {
        Ok(())
    } else {
        err(format!("'{name}' isn't a valid branch name"))
    }
}

/// Creates a branch at the current commit; `checkout` switches to it.
pub fn create_branch(path: impl AsRef<Path>, name: &str, checkout: bool) -> Result<()> {
    let path = path.as_ref();
    let repo = open(path)?;
    let name = name.trim();
    valid_branch_name(name)?;
    let Some(head) = head_commit(&repo)? else { return err("make the first commit before creating a branch") };
    if repo.find_branch(name, BranchType::Local).is_ok() {
        return err(format!("a branch named '{name}' already exists"));
    }
    repo.branch(name, &head, false)?;
    if checkout {
        switch_branch(path, name)?;
    }
    Ok(())
}

/// Switches to a branch. A remote branch ("origin/feature", or "feature"
/// when only a remote has it) gets a local branch that tracks it. Local
/// changes the switch would overwrite stop it.
pub fn switch_branch(path: impl AsRef<Path>, name: &str) -> Result<()> {
    let repo = open(path)?;
    if repo.state() != RepositoryState::Clean {
        return err(format!("finish the {} first", state_name(repo.state())));
    }
    let local = match repo.find_branch(name, BranchType::Local) {
        Ok(b) => b,
        Err(_) => {
            let remote_name = if repo.find_branch(name, BranchType::Remote).is_ok() {
                name.to_string()
            } else {
                let found: Vec<String> = branches(workdir(&repo)?)?.remote.into_iter().filter(|r| r.split_once('/').map(|x| x.1) == Some(name)).collect();
                match found.as_slice() {
                    [one] => one.clone(),
                    [] => return err(format!("no branch named '{name}'")),
                    _ => return err(format!("'{name}' is on more than one remote: pick one of {}", found.join(", "))),
                }
            };
            let remote = repo.find_branch(&remote_name, BranchType::Remote)?;
            let local_name = remote_name.split_once('/').map(|x| x.1).unwrap_or(&remote_name).to_string();
            if repo.find_branch(&local_name, BranchType::Local).is_ok() {
                return err(format!("a local branch '{local_name}' exists already: switch to that one"));
            }
            let mut b = repo.branch(&local_name, &remote.get().peel_to_commit()?, false)?;
            b.set_upstream(Some(&remote_name))?;
            b
        }
    };
    let refname = local.get().name()?.to_string();
    let target = local.get().peel_to_commit()?;
    let mut co = git2::build::CheckoutBuilder::new();
    co.safe();
    repo.checkout_tree(target.as_object(), Some(&mut co)).map_err(|e| {
        if e.code() == ErrorCode::Conflict {
            Error(format!("Your changes to {} would be overwritten: commit or discard them first", conflicting_files(&repo).unwrap_or_else(|| "some files".into())))
        } else {
            e.into()
        }
    })?;
    repo.set_head(&refname)?;
    Ok(())
}

fn conflicting_files(repo: &Repository) -> Option<String> {
    let files: Vec<String> = file_statuses(repo).ok()?.into_iter().filter(|f| f.index.is_some() || f.worktree.as_deref().is_some_and(|w| w != "untracked")).map(|f| f.path).collect();
    (!files.is_empty()).then(|| files.join(", "))
}

/// Deletes a local branch that is merged into the current one.
pub fn delete_branch(path: impl AsRef<Path>, name: &str) -> Result<()> {
    let repo = open(path)?;
    let mut b = repo.find_branch(name, BranchType::Local).map_err(|_| Error(format!("no branch named '{name}'")))?;
    if b.is_head() {
        return err(format!("'{name}' is the current branch: switch to another one first"));
    }
    let head = head_commit(&repo)?.map(|c| c.id());
    if !merged_into(&repo, head, b.get().target()) {
        let current = current_branch(&repo)?.unwrap_or_else(|| "HEAD".into());
        return err(format!("'{name}' isn't merged into {current}: merge it first, or delete it with git"));
    }
    b.delete()?;
    Ok(())
}

#[cfg(test)]
mod tests;
