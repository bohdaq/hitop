//! Temp repositories with a local bare remote. Global and system git config
//! are pointed at an empty directory, so the developer's ~/.gitconfig
//! doesn't leak in (commits need the repository's own user.name/email).

use std::fs;
use std::path::{Path, PathBuf};
use std::sync::Once;

use git2::{Repository, RepositoryInitOptions};
use serde_json::json;
use tempfile::TempDir;

use crate::*;

fn hermetic() {
    static ONCE: Once = Once::new();
    ONCE.call_once(|| {
        let empty = std::env::temp_dir().join(format!("hitop-git-noconfig-{}", std::process::id()));
        fs::create_dir_all(&empty).unwrap();
        for level in [git2::ConfigLevel::Global, git2::ConfigLevel::XDG, git2::ConfigLevel::System, git2::ConfigLevel::ProgramData] {
            unsafe { git2::opts::set_search_path(level, &empty).unwrap() };
        }
    });
}

struct World {
    _tmp: TempDir,
    remote: PathBuf,
    root: PathBuf,
}

impl World {
    /// A bare remote on `main`; `a` has one commit (hitop.yaml) pushed to it.
    fn new() -> World {
        hermetic();
        let tmp = TempDir::new().unwrap();
        let root = tmp.path().to_path_buf();
        let remote = root.join("remote.git");
        let mut opts = RepositoryInitOptions::new();
        opts.bare(true).initial_head("main");
        Repository::init_opts(&remote, &opts).unwrap();
        let w = World { _tmp: tmp, remote, root };
        let a = w.dir("a");
        let mut opts = RepositoryInitOptions::new();
        opts.initial_head("main");
        let repo = Repository::init_opts(&a, &opts).unwrap();
        repo.remote("origin", w.remote.to_str().unwrap()).unwrap();
        set_identity(&a, "Ada", "ada@example.test").unwrap();
        write(&a, "hitop.yaml", "format: 1\nname: Shop\n");
        write(&a, "requests/get-user.yaml", "name: Get user\nmethod: GET\nurl: https://api.test/users/1\n");
        stage(&a, &["hitop.yaml".into(), "requests/get-user.yaml".into()]).unwrap();
        commit(&a, "Start").unwrap();
        push(&a, &Auth::default()).unwrap();
        w
    }

    fn dir(&self, name: &str) -> PathBuf {
        self.root.join(name)
    }

    /// A second clone, with its own identity.
    fn clone(&self, name: &str, who: &str) -> PathBuf {
        let dir = self.dir(name);
        Repository::clone(self.remote.to_str().unwrap(), &dir).unwrap();
        set_identity(&dir, who, &format!("{}@example.test", who.to_lowercase())).unwrap();
        dir
    }
}

fn write(dir: &Path, file: &str, text: &str) {
    let p = dir.join(file);
    fs::create_dir_all(p.parent().unwrap()).unwrap();
    fs::write(p, text).unwrap();
}

fn read(dir: &Path, file: &str) -> String {
    fs::read_to_string(dir.join(file)).unwrap()
}

fn commit_file(dir: &Path, file: &str, text: &str, message: &str) {
    write(dir, file, text);
    stage(dir, &[file.into()]).unwrap();
    commit(dir, message).unwrap();
}

fn file<'a>(s: &'a Status, path: &str) -> &'a FileStatus {
    s.files.iter().find(|f| f.path == path).unwrap_or_else(|| panic!("{path} not in {:?}", s.files))
}

const USER: &str = "requests/get-user.yaml";

#[test]
fn status_stage_unstage_diff_and_commit() {
    let w = World::new();
    let a = w.dir("a");
    let s = status(&a).unwrap();
    assert_eq!(s.branch.as_deref(), Some("main"));
    assert_eq!(s.upstream.as_deref(), Some("origin/main"));
    assert_eq!((s.ahead, s.behind, s.state.as_str()), (0, 0, "clean"));
    assert!(s.files.is_empty(), "{:?}", s.files);
    assert_eq!(s.identity, Some(Identity { name: "Ada".into(), email: "ada@example.test".into() }));
    assert_eq!(s.remotes[0].name, "origin");

    // Untracked, modified and deleted files.
    write(&a, "requests/new.yaml", "name: New\n");
    write(&a, USER, "name: Get user\nmethod: GET\nurl: https://api.test/users/2\n");
    fs::remove_file(a.join("hitop.yaml")).unwrap();
    let s = status(&a).unwrap();
    assert_eq!(file(&s, "requests/new.yaml").worktree.as_deref(), Some("untracked"));
    assert_eq!(file(&s, USER).worktree.as_deref(), Some("modified"));
    assert_eq!(file(&s, "hitop.yaml").worktree.as_deref(), Some("deleted"));

    // The unstaged diff, then the staged one.
    let d = diff(&a, USER, false).unwrap();
    assert!(!d.binary);
    let lines: Vec<(&str, &str)> = d.hunks[0].lines.iter().map(|l| (l.kind.as_str(), l.text.as_str())).collect();
    assert!(lines.contains(&("del", "url: https://api.test/users/1")), "{lines:?}");
    assert!(lines.contains(&("add", "url: https://api.test/users/2")), "{lines:?}");
    assert!(d.hunks[0].header.starts_with("@@ -1,3 +1,3 @@"), "{}", d.hunks[0].header);
    let untracked = diff(&a, "requests/new.yaml", false).unwrap();
    assert_eq!(untracked.hunks[0].lines[0].kind, "add");
    assert!(diff(&a, USER, true).unwrap().hunks.is_empty());

    stage(&a, &[USER.into(), "hitop.yaml".into(), "requests/new.yaml".into()]).unwrap();
    let s = status(&a).unwrap();
    assert_eq!(file(&s, USER).index.as_deref(), Some("modified"));
    assert_eq!(file(&s, USER).worktree, None);
    assert_eq!(file(&s, "hitop.yaml").index.as_deref(), Some("deleted"));
    assert_eq!(file(&s, "requests/new.yaml").index.as_deref(), Some("added"));
    assert_eq!(diff(&a, USER, true).unwrap().hunks[0].lines.iter().filter(|l| l.kind == "add").count(), 1);

    // Unstage puts the index back; the file stays changed.
    unstage(&a, &["hitop.yaml".into(), "requests/new.yaml".into()]).unwrap();
    let s = status(&a).unwrap();
    assert_eq!(file(&s, "hitop.yaml").index, None);
    assert_eq!(file(&s, "hitop.yaml").worktree.as_deref(), Some("deleted"));
    assert_eq!(file(&s, "requests/new.yaml").worktree.as_deref(), Some("untracked"));
    assert_eq!(file(&s, USER).index.as_deref(), Some("modified"));

    assert_eq!(commit(&a, "  ").unwrap_err().0, "write a commit message");
    let c = commit(&a, "Second user\n\nwith a body").unwrap();
    assert_eq!(c.summary, "Second user");
    let repo = Repository::open(&a).unwrap();
    let head = repo.head().unwrap().peel_to_commit().unwrap();
    assert_eq!(head.author().name().ok(), Some("Ada"));
    assert_eq!(head.author().email().ok(), Some("ada@example.test"));
    assert_eq!(head.message().ok(), Some("Second user\n\nwith a body\n"));
    let s = status(&a).unwrap();
    assert_eq!(s.ahead, 1);
    assert!(s.files.iter().all(|f| f.index.is_none()));
    assert!(commit(&a, "again").unwrap_err().0.contains("nothing to commit"));

    // Discard: tracked back to HEAD, untracked removed.
    discard(&a, &["hitop.yaml".into(), "requests/new.yaml".into()]).unwrap();
    assert!(status(&a).unwrap().files.is_empty());
    assert_eq!(read(&a, "hitop.yaml"), "format: 1\nname: Shop\n");
}

#[test]
fn a_subfolder_collection_and_errors_outside_a_repository() {
    let w = World::new();
    let a = w.dir("a");
    let s = status(a.join("requests")).unwrap();
    assert_eq!(s.prefix, "requests");
    assert!(Path::new(&s.root).join("hitop.yaml").exists());
    let none = w.dir("plain");
    fs::create_dir_all(&none).unwrap();
    assert!(status(&none).unwrap_err().0.contains("isn't in a git repository"));
}

#[test]
fn commit_needs_a_name_and_email() {
    hermetic();
    let tmp = TempDir::new().unwrap();
    let mut opts = RepositoryInitOptions::new();
    opts.initial_head("main");
    Repository::init_opts(tmp.path(), &opts).unwrap();
    write(tmp.path(), "hitop.yaml", "format: 1\n");
    stage(tmp.path(), &["hitop.yaml".into()]).unwrap();
    assert_eq!(status(tmp.path()).unwrap().identity, None);
    assert_eq!(status(tmp.path()).unwrap().head, None);
    assert!(commit(tmp.path(), "first").unwrap_err().0.contains("user.name and user.email"));
    assert!(set_identity(tmp.path(), "", "x@y").is_err());
    set_identity(tmp.path(), "Bo", "bo@example.test").unwrap();
    commit(tmp.path(), "first").unwrap();
    assert!(status(tmp.path()).unwrap().head.is_some());
}

#[test]
fn branches_create_switch_and_delete_merged() {
    let w = World::new();
    let a = w.dir("a");
    create_branch(&a, "feature", true).unwrap();
    assert_eq!(status(&a).unwrap().branch.as_deref(), Some("feature"));
    assert!(create_branch(&a, "feature", false).unwrap_err().0.contains("already exists"));
    assert!(create_branch(&a, "bad name..", false).unwrap_err().0.contains("isn't a valid branch name"));
    commit_file(&a, "requests/orders.yaml", "name: Orders\n", "Orders");

    let b = branches(&a).unwrap();
    let names: Vec<(&str, bool, bool)> = b.local.iter().map(|x| (x.name.as_str(), x.current, x.merged)).collect();
    assert_eq!(names, [("feature", true, true), ("main", false, true)]);
    assert_eq!(b.local[1].upstream.as_deref(), Some("origin/main"));
    assert_eq!(b.remote, ["origin/main"]);

    switch_branch(&a, "main").unwrap();
    assert_eq!(status(&a).unwrap().branch.as_deref(), Some("main"));
    assert!(!a.join("requests/orders.yaml").exists(), "the switch checked out main's files");
    // Not merged into main: refused, and so is deleting the current branch.
    assert!(delete_branch(&a, "feature").unwrap_err().0.contains("isn't merged into main"));
    assert!(delete_branch(&a, "main").unwrap_err().0.contains("current branch"));
    assert!(!branches(&a).unwrap().local.iter().find(|x| x.name == "feature").unwrap().merged);

    // A change the switch would overwrite stops it.
    write(&a, USER, "changed on main\n");
    switch_branch(&a, "feature").unwrap();
    assert_eq!(status(&a).unwrap().branch.as_deref(), Some("feature"), "unrelated change carried over");
    switch_branch(&a, "main").unwrap();
    discard(&a, &[USER.into()]).unwrap();
    switch_branch(&a, "feature").unwrap();
    write(&a, "requests/orders.yaml", "local edit\n");
    commit_file(&a, USER, "feature edit\n", "feature edits user");
    write(&a, USER, "uncommitted\n");
    let e = switch_branch(&a, "main").unwrap_err().0;
    assert!(e.contains("would be overwritten"), "{e}");
    assert_eq!(status(&a).unwrap().branch.as_deref(), Some("feature"));
    discard(&a, &[USER.into(), "requests/orders.yaml".into()]).unwrap();

    // Merge feature into main (a fast-forward), then delete it.
    switch_branch(&a, "main").unwrap();
    assert!(matches!(merge_branch(&a, "feature").unwrap(), MergeOutcome::FastForward { .. }));
    assert_eq!(read(&a, USER), "feature edit\n");
    assert!(matches!(merge_branch(&a, "feature").unwrap(), MergeOutcome::UpToDate));
    delete_branch(&a, "feature").unwrap();
    assert_eq!(branches(&a).unwrap().local.len(), 1);
    assert!(switch_branch(&a, "nope").unwrap_err().0.contains("no branch named"));
}

#[test]
fn push_pull_fast_forward_merge_and_rejected_push() {
    let w = World::new();
    let a = w.dir("a");
    let b = w.clone("b", "Bob");
    assert_eq!(status(&b).unwrap().upstream.as_deref(), Some("origin/main"));
    assert_eq!(pull(&b, &Auth::default()).unwrap(), MergeOutcome::UpToDate);

    // a pushes; b fetches (behind 1) and fast-forwards.
    commit_file(&a, USER, "v2\n", "v2");
    assert_eq!(status(&a).unwrap().ahead, 1);
    push(&a, &Auth::default()).unwrap();
    assert_eq!(status(&a).unwrap().ahead, 0);
    fetch(&b, &Auth::default()).unwrap();
    assert_eq!(status(&b).unwrap().behind, 1);
    assert!(matches!(pull(&b, &Auth::default()).unwrap(), MergeOutcome::FastForward { .. }));
    assert_eq!(read(&b, USER), "v2\n");

    // Both commit different files: b's push goes first, a's is refused.
    commit_file(&b, "requests/b.yaml", "name: B\n", "b's request");
    push(&b, &Auth::default()).unwrap();
    commit_file(&a, "requests/a.yaml", "name: A\n", "a's request");
    let e = push(&a, &Auth::default()).unwrap_err().0;
    assert!(e.contains("pull first"), "{e}");
    // The pull merges (no conflict) and the push goes through.
    let MergeOutcome::Merged { head } = pull(&a, &Auth::default()).unwrap() else { panic!("expected a merge commit") };
    let repo = Repository::open(&a).unwrap();
    let merge = repo.head().unwrap().peel_to_commit().unwrap();
    assert!(merge.id().to_string().starts_with(&head));
    assert_eq!(merge.parent_count(), 2);
    assert!(merge.message().unwrap().starts_with("Merge"), "{:?}", merge.message().ok());
    assert_eq!(status(&a).unwrap().state, "clean");
    assert!(a.join("requests/b.yaml").exists());
    push(&a, &Auth::default()).unwrap();
    assert!(matches!(pull(&b, &Auth::default()).unwrap(), MergeOutcome::FastForward { .. }));
    assert!(b.join("requests/a.yaml").exists());

    // A merge needs a clean tree (tracked files).
    commit_file(&b, "requests/b.yaml", "name: B2\n", "b2");
    push(&b, &Auth::default()).unwrap();
    commit_file(&a, "requests/a.yaml", "name: A2\n", "a2");
    write(&a, USER, "dirty\n");
    assert!(pull(&a, &Auth::default()).unwrap_err().0.contains("Commit or discard your changes first"));
}

#[test]
fn a_new_branch_is_pushed_and_tracked_and_a_remote_branch_checked_out() {
    let w = World::new();
    let a = w.dir("a");
    let b = w.clone("b", "Bob");
    create_branch(&a, "feature", true).unwrap();
    commit_file(&a, "requests/f.yaml", "name: F\n", "f");
    assert_eq!(status(&a).unwrap().upstream, None);
    assert!(pull(&a, &Auth::default()).unwrap_err().0.contains("push it first"));
    let p = push(&a, &Auth::default()).unwrap();
    assert_eq!((p.remote.as_str(), p.branch.as_str()), ("origin", "feature"));
    assert_eq!(status(&a).unwrap().upstream.as_deref(), Some("origin/feature"));

    fetch(&b, &Auth::default()).unwrap();
    assert!(branches(&b).unwrap().remote.contains(&"origin/feature".to_string()));
    switch_branch(&b, "feature").unwrap();
    let s = status(&b).unwrap();
    assert_eq!((s.branch.as_deref(), s.upstream.as_deref()), (Some("feature"), Some("origin/feature")));
    assert!(b.join("requests/f.yaml").exists());
}

/// a and b change the same line; b pushes first; a's pull stops with a conflict.
fn conflicted() -> (World, PathBuf) {
    let w = World::new();
    let a = w.dir("a");
    let b = w.clone("b", "Bob");
    commit_file(&b, USER, "name: Get user\nmethod: GET\nurl: https://api.test/v2/users/1\n", "v2 path");
    push(&b, &Auth::default()).unwrap();
    commit_file(&a, USER, "name: Get user\nmethod: GET\nurl: https://api.test/users/1?full=1\n", "full user");
    let out = pull(&a, &Auth::default()).unwrap();
    assert_eq!(out, MergeOutcome::Conflicts { files: vec![USER.into()] });
    (w, a)
}

#[test]
fn a_conflict_resolved_by_taking_theirs() {
    let (_w, a) = conflicted();
    let s = status(&a).unwrap();
    assert_eq!(s.state, "merge");
    assert!(file(&s, USER).conflicted);
    assert!(s.merge_message.as_deref().unwrap().starts_with("Merge"), "{:?}", s.merge_message);

    let c = conflict(&a, USER).unwrap();
    assert_eq!(c.base.as_deref(), Some("name: Get user\nmethod: GET\nurl: https://api.test/users/1\n"));
    assert!(c.ours.as_deref().unwrap().contains("?full=1"));
    assert!(c.theirs.as_deref().unwrap().contains("/v2/"));
    let merged = c.merged.unwrap();
    assert!(merged.contains("<<<<<<< ") && merged.contains(">>>>>>> "), "{merged}");
    assert!(!c.binary);
    assert!(conflict(&a, "hitop.yaml").unwrap_err().0.contains("no conflict"));

    // Can't commit, push or switch in the middle of it.
    assert!(commit(&a, "merge").unwrap_err().0.contains("resolve the conflicts first"));
    assert!(switch_branch(&a, "main").unwrap_err().0.contains("merge"));
    // Keeping the markers isn't a resolution.
    assert!(resolve(&a, USER, &Resolution::Worktree).unwrap_err().0.contains("conflict markers"));

    resolve(&a, USER, &Resolution::Theirs).unwrap();
    assert_eq!(read(&a, USER), "name: Get user\nmethod: GET\nurl: https://api.test/v2/users/1\n");
    let s = status(&a).unwrap();
    assert!(!file(&s, USER).conflicted);
    let message = s.merge_message.unwrap();
    commit(&a, &message).unwrap();
    let repo = Repository::open(&a).unwrap();
    let head = repo.head().unwrap().peel_to_commit().unwrap();
    assert_eq!(head.parent_count(), 2);
    let s = status(&a).unwrap();
    assert_eq!((s.state.as_str(), s.ahead, s.behind), ("clean", 2, 0));
    push(&a, &Auth::default()).unwrap();
}

#[test]
fn a_conflict_resolved_with_an_edit() {
    let (w, a) = conflicted();
    let edited = "name: Get user\nmethod: GET\nurl: https://api.test/v2/users/1?full=1\n";
    // By hand in the working tree, then "mark resolved".
    write(&a, USER, edited);
    resolve(&a, USER, &Resolution::Worktree).unwrap();
    commit(&a, "Merge origin/main: keep both").unwrap();
    assert_eq!(read(&a, USER), edited);
    push(&a, &Auth::default()).unwrap();
    let b = w.dir("b");
    assert!(matches!(pull(&b, &Auth::default()).unwrap(), MergeOutcome::FastForward { .. }));
    assert_eq!(read(&b, USER), edited);
}

#[test]
fn a_conflict_resolved_with_given_text_or_ours_and_an_abort() {
    let (_w, a) = conflicted();
    resolve(&a, USER, &Resolution::Content { text: "name: Edited\n".into() }).unwrap();
    assert_eq!(read(&a, USER), "name: Edited\n");
    assert!(!diff(&a, USER, true).unwrap().hunks.is_empty());
    // Abort goes back to our commit.
    abort_merge(&a).unwrap();
    assert_eq!(status(&a).unwrap().state, "clean");
    assert!(read(&a, USER).contains("?full=1"));
    assert!(abort_merge(&a).unwrap_err().0.contains("no merge"));

    // Again, taking ours.
    assert!(matches!(pull(&a, &Auth::default()).unwrap(), MergeOutcome::Conflicts { .. }));
    resolve(&a, USER, &Resolution::Ours).unwrap();
    commit(&a, "Merge, keep ours").unwrap();
    assert!(read(&a, USER).contains("?full=1"));
}

#[test]
fn json_calls_answer_ok_or_error() {
    let w = World::new();
    let a = w.dir("a");
    let path = a.to_str().unwrap();
    let auth = Auth::default();
    let s = json::answer("status", &json!({ "path": path }), &auth);
    assert_eq!(s["ok"]["branch"], "main");
    assert_eq!(s["ok"]["upstream"], "origin/main");
    write(&a, USER, "x\n");
    assert_eq!(json::answer("stage", &json!({ "path": path, "files": [USER] }), &auth), json!({ "ok": null }));
    let c = json::answer("commit", &json!({ "path": path, "message": "x" }), &auth);
    assert_eq!(c["ok"]["summary"], "x");
    let p = json::answer("pull", &json!({ "path": path }), &auth);
    assert_eq!(p["ok"]["result"], "upToDate");
    assert_eq!(json::answer("resolve", &json!({ "path": path, "file": USER, "resolution": { "take": "theirs" } }), &auth)["error"], format!("{USER} has no conflict"));
    assert_eq!(json::answer("nope", &json!({ "path": path }), &auth)["error"], "unknown git command nope");
    assert!(json::answer("status", &json!({}), &auth)["error"].as_str().unwrap().starts_with("argument path"));
    assert_eq!(json::answer("remoteHost", &json!({ "path": path }), &auth), json!({ "ok": null }));
}
