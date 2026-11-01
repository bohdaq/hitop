//! Fetch, pull (fetch, then fast-forward or merge) and push for the
//! current branch.

use std::cell::RefCell;
use std::path::Path;

use git2::{BranchType, FetchOptions, PushOptions, Repository};
use serde::{Deserialize, Serialize};

use crate::auth::{callbacks, Auth};
use crate::merge::{merge_commit, MergeOutcome};
use crate::{current_branch, err, head_commit, open, Error, Result};

/// The remote the current branch works with: its upstream's, else "origin",
/// else the only remote.
fn remote_name(repo: &Repository, branch: Option<&str>) -> Result<String> {
    if let Some(b) = branch {
        if let Ok(name) = repo.branch_upstream_remote(&format!("refs/heads/{b}")) {
            if let Ok(n) = name.as_str() {
                return Ok(n.to_string());
            }
        }
    }
    let remotes = repo.remotes()?;
    let names: Vec<&str> = remotes.iter().filter_map(|n| n.ok().flatten()).collect();
    if names.contains(&"origin") {
        return Ok("origin".into());
    }
    match names.as_slice() {
        [one] => Ok(one.to_string()),
        [] => err("this repository has no remote: add one with git remote add"),
        _ => err(format!("set an upstream for this branch: there are several remotes ({})", names.join(", "))),
    }
}

/// The host of the current branch's remote ("github.com"), for looking up
/// its token; None for local paths and file:// remotes.
pub fn remote_host(path: impl AsRef<Path>) -> Result<Option<String>> {
    let repo = open(path)?;
    let branch = current_branch(&repo)?;
    let Ok(name) = remote_name(&repo, branch.as_deref()) else { return Ok(None) };
    let remote = repo.find_remote(&name)?;
    Ok(remote.url().ok().and_then(host_of))
}

/// "https://user@host:443/x" → "host"; "git@host:x" → "host"; local → None.
pub fn host_of(url: &str) -> Option<String> {
    if let Some((scheme, rest)) = url.split_once("://") {
        if scheme == "file" {
            return None;
        }
        let authority = rest.split('/').next()?;
        let host = authority.rsplit('@').next()?;
        let host = if host.starts_with('[') { host.split(']').next().map(|h| format!("{h}]"))? } else { host.split(':').next()?.to_string() };
        return (!host.is_empty()).then_some(host.to_lowercase());
    }
    // scp-like: [user@]host:path (not a Windows drive like C:\)
    let (before, _) = url.split_once(':')?;
    if before.len() <= 1 || before.contains('/') || before.contains('\\') {
        return None;
    }
    Some(before.rsplit('@').next()?.to_lowercase())
}

fn auth_error(e: git2::Error, url: &str) -> Error {
    if e.code() == git2::ErrorCode::Auth || e.class() == git2::ErrorClass::Callback {
        return Error(e.message().to_string());
    }
    let msg = e.message();
    if e.class() == git2::ErrorClass::Http && (msg.contains("401") || msg.contains("403") || msg.contains("authentication")) {
        return Error(format!("Authentication failed for {url}: {msg}"));
    }
    if e.class() == git2::ErrorClass::Ssh && msg.contains("known") {
        return Error(format!("{msg}. Connect to the host once with ssh so it's in ~/.ssh/known_hosts"));
    }
    Error(format!("{url}: {msg}"))
}

fn fetch_remote(repo: &Repository, name: &str, auth: &Auth) -> Result<()> {
    let mut remote = repo.find_remote(name).map_err(|_| Error(format!("no remote named '{name}'")))?;
    let url = remote.url().unwrap_or(name).to_string();
    let tried = RefCell::new(Vec::new());
    let mut opts = FetchOptions::new();
    opts.remote_callbacks(callbacks(auth, &tried));
    let refspecs: [&str; 0] = [];
    remote.fetch(&refspecs, Some(&mut opts), None).map_err(|e| auth_error(e, &url))?;
    Ok(())
}

/// Fetches the current branch's remote (or "origin").
pub fn fetch(path: impl AsRef<Path>, auth: &Auth) -> Result<()> {
    let repo = open(path)?;
    let branch = current_branch(&repo)?;
    let name = remote_name(&repo, branch.as_deref())?;
    fetch_remote(&repo, &name, auth)
}

/// Fetches, then merges the upstream into the current branch: a
/// fast-forward when possible, else a merge commit, or conflicts to
/// resolve. Never rebases. A branch without an upstream takes
/// `<remote>/<branch>` and keeps it as its upstream.
pub fn pull(path: impl AsRef<Path>, auth: &Auth) -> Result<MergeOutcome> {
    let repo = open(path)?;
    let Some(branch) = current_branch(&repo)? else { return err("HEAD is detached: switch to a branch first") };
    let name = remote_name(&repo, Some(&branch))?;
    fetch_remote(&repo, &name, auth)?;
    let upstream = match repo.find_branch(&branch, BranchType::Local).ok().and_then(|b| b.upstream().ok()) {
        Some(up) => up,
        None => {
            let remote_branch = format!("{name}/{branch}");
            let up = repo.find_branch(&remote_branch, BranchType::Remote).map_err(|_| Error(format!("{name} has no branch '{branch}' yet: push it first")))?;
            if let Ok(mut local) = repo.find_branch(&branch, BranchType::Local) {
                local.set_upstream(Some(&remote_branch))?;
            }
            up
        }
    };
    let theirs = repo.reference_to_annotated_commit(upstream.get())?;
    merge_commit(&repo, &theirs)
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PushOutcome {
    pub remote: String,
    pub branch: String,
}

/// Pushes the current branch to its upstream (or `<remote>/<branch>`,
/// which then becomes the upstream). A remote that has commits the branch
/// doesn't is refused: pull first.
pub fn push(path: impl AsRef<Path>, auth: &Auth) -> Result<PushOutcome> {
    let repo = open(path)?;
    let Some(branch) = current_branch(&repo)? else { return err("HEAD is detached: switch to a branch first") };
    if head_commit(&repo)?.is_none() {
        return err("nothing to push: make the first commit");
    }
    let name = remote_name(&repo, Some(&branch))?;
    let local = repo.find_branch(&branch, BranchType::Local)?;
    // The upstream's branch name on the remote, when it has one.
    let remote_ref = repo
        .branch_upstream_name(&format!("refs/heads/{branch}"))
        .ok()
        .and_then(|b| b.as_str().ok().map(str::to_string))
        .and_then(|up| up.strip_prefix(&format!("refs/remotes/{name}/")).map(|b| format!("refs/heads/{b}")))
        .unwrap_or_else(|| format!("refs/heads/{branch}"));
    let mut remote = repo.find_remote(&name)?;
    let url = remote.url().unwrap_or(&name).to_string();
    let tried = RefCell::new(Vec::new());
    let rejected = RefCell::new(None::<String>);
    let mut cb = callbacks(auth, &tried);
    cb.push_update_reference(|refname, status| {
        if let Some(s) = status {
            *rejected.borrow_mut() = Some(format!("{refname}: {s}"));
        }
        Ok(())
    });
    let mut opts = PushOptions::new();
    opts.remote_callbacks(cb);
    let refspec = format!("refs/heads/{branch}:{remote_ref}");
    let pushed = remote.push(&[refspec.as_str()], Some(&mut opts));
    drop(opts);
    let non_ff = |m: &str| m.contains("fast-forward") || m.contains("fastforward") || m.contains("non-fast") || m.contains("fetch first") || m.contains("not present locally");
    if let Err(e) = pushed {
        if non_ff(e.message()) {
            return err(format!("{name} has commits that {branch} doesn't: pull first, then push"));
        }
        return Err(auth_error(e, &url));
    }
    if let Some(r) = rejected.into_inner() {
        if non_ff(&r) {
            return err(format!("{name} has commits that {branch} doesn't: pull first, then push"));
        }
        return err(format!("{name} refused the push ({r})"));
    }
    if local.upstream().is_err() {
        let tracking = format!("{name}/{}", remote_ref.trim_start_matches("refs/heads/"));
        // The remote-tracking branch exists once the push updated it.
        if repo.find_branch(&tracking, BranchType::Remote).is_ok() {
            repo.find_branch(&branch, BranchType::Local)?.set_upstream(Some(&tracking))?;
        }
    }
    Ok(PushOutcome { remote: name, branch })
}

#[cfg(test)]
mod tests {
    use super::host_of;

    #[test]
    fn hosts_of_remote_urls() {
        assert_eq!(host_of("https://github.com/bohdaq/hitop.git").as_deref(), Some("github.com"));
        assert_eq!(host_of("https://user@GitLab.example:8443/a/b").as_deref(), Some("gitlab.example"));
        assert_eq!(host_of("ssh://git@host.test:22/x").as_deref(), Some("host.test"));
        assert_eq!(host_of("git@github.com:bohdaq/hitop.git").as_deref(), Some("github.com"));
        assert_eq!(host_of("file:///tmp/remote.git"), None);
        assert_eq!(host_of("/tmp/remote.git"), None);
        assert_eq!(host_of("C:\\repos\\remote.git"), None);
        assert_eq!(host_of("../remote.git"), None);
    }
}
