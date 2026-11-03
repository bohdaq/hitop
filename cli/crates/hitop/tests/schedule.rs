//! `hitop schedule`: the GitHub Actions workflow (exact text, checked with
//! actionlint), overwrite protection, the crontab line, and exit codes.
//!
//! actionlint: `HITOP_ACTIONLINT=<path>` or `actionlint` on PATH. Without
//! either the lint is skipped, except under CI (`CI` set), where it fails.
use std::fs;
use std::path::{Path, PathBuf};

use assert_cmd::Command;
use predicates::prelude::PredicateBooleanExt;
use predicates::str::contains;
use tempfile::TempDir;

fn hitop(home: &TempDir, cwd: &Path) -> Command {
    let mut c = Command::cargo_bin("hitop").unwrap();
    c.env("HOME", home.path())
        .env("XDG_CONFIG_HOME", home.path().join("config"))
        .env_remove("XDG_STATE_HOME")
        .env("HITOP_KEYCHAIN", "off")
        .current_dir(cwd);
    c
}

/// A repository (`.git` is enough to find its root) with the collection
/// folder `apis/shop`: environment `prod` with secrets `token` and
/// `api.key`, and a secret global `signing`.
fn repo() -> (TempDir, PathBuf) {
    let tmp = TempDir::new().unwrap();
    fs::create_dir(tmp.path().join(".git")).unwrap();
    let dir = tmp.path().join("apis/shop");
    fs::create_dir_all(dir.join("requests")).unwrap();
    fs::create_dir_all(dir.join("environments")).unwrap();
    fs::write(dir.join("hitop.yaml"), "format: 1\nname: Shop API\norder:\n- ping\n").unwrap();
    fs::write(dir.join("requests/ping.yaml"), "name: Ping\nurl: ${base}/ping\n").unwrap();
    fs::write(dir.join("environments/prod.yaml"), "variables:\n  base: https://api.example.com\n  token:\n    secret: true\n  api.key:\n    secret: true\n").unwrap();
    fs::write(dir.join("environments/local.yaml"), "variables:\n  base: http://localhost:8080\n").unwrap();
    fs::write(dir.join("globals.yaml"), "variables:\n  signing:\n    secret: true\n").unwrap();
    (tmp, dir)
}

fn actionlint() -> Option<PathBuf> {
    if let Some(p) = std::env::var_os("HITOP_ACTIONLINT").filter(|p| !p.is_empty()) {
        return Some(PathBuf::from(p));
    }
    std::env::var_os("PATH")
        .and_then(|paths| std::env::split_paths(&paths).map(|d| d.join("actionlint")).find(|p| p.is_file()))
}

fn lint(file: &Path) {
    let Some(bin) = actionlint() else {
        assert!(std::env::var_os("CI").is_none(), "CI must run actionlint: set HITOP_ACTIONLINT");
        eprintln!("actionlint not found (HITOP_ACTIONLINT or PATH): skipped for {}", file.display());
        return;
    };
    let out = std::process::Command::new(&bin).arg("-no-color").arg(file).output().unwrap_or_else(|e| panic!("{}: {e}", bin.display()));
    assert!(
        out.status.success(),
        "actionlint rejects {}:\n{}{}\n{}",
        file.display(),
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr),
        fs::read_to_string(file).unwrap()
    );
}

const PROD_WORKFLOW: &str = r#"# Written by `hitop schedule --github` (hitop VERSION).
# Runs the hitop collection in apis/shop on a schedule (0 6 * * * UTC) and on demand.
# Secret values come from repository secrets with the same names:
#   gh secret set HITOP_SECRET_GLOBAL_SIGNING
#   gh secret set HITOP_SECRET_PROD_API_KEY
#   gh secret set HITOP_SECRET_PROD_TOKEN
name: "hitop: Shop API (prod)"
on:
  schedule:
    - cron: "0 6 * * *"
  workflow_dispatch:
permissions:
  contents: read
concurrency:
  group: hitop-shop-api-prod
  cancel-in-progress: false
jobs:
  run:
    name: "Shop API (prod)"
    runs-on: ubuntu-24.04
    timeout-minutes: 30
    steps:
      - uses: actions/checkout@v6
        with:
          persist-credentials: false
      - name: "Run Shop API (prod)"
        id: hitop
        uses: bohdaq/hitop-action@v1
        env:
          HITOP_SECRET_GLOBAL_SIGNING: ${{ secrets.HITOP_SECRET_GLOBAL_SIGNING }}
          HITOP_SECRET_PROD_API_KEY: ${{ secrets.HITOP_SECRET_PROD_API_KEY }}
          HITOP_SECRET_PROD_TOKEN: ${{ secrets.HITOP_SECRET_PROD_TOKEN }}
        with:
          version: "0.3.0"
          path: "apis/shop"
          env: "prod"
          reporter: cli,junit,json
          artifact-name: hitop-shop-api-prod-junit
"#;

#[test]
fn github_workflow_is_written_and_passes_actionlint() {
    let (repo, _) = repo();
    let home = TempDir::new().unwrap();
    // From a subdirectory: the workflow still goes to the repository root.
    hitop(&home, &repo.path().join("apis"))
        .args(["schedule", "shop", "--github", "--env", "prod", "--hitop-version", "0.3.0"])
        .assert()
        .code(0)
        .stdout(contains("hitop-shop-api-prod.yml").and(contains("gh secret set HITOP_SECRET_PROD_TOKEN")));
    let file = repo.path().join(".github/workflows/hitop-shop-api-prod.yml");
    let text = fs::read_to_string(&file).unwrap();
    assert_eq!(text, PROD_WORKFLOW.replace("VERSION", env!("CARGO_PKG_VERSION")));
    lint(&file);

    // Exists: refused without --force, kept as it was.
    fs::write(&file, "mine\n").unwrap();
    hitop(&home, repo.path())
        .args(["schedule", "apis/shop", "--github", "--env", "prod"])
        .assert()
        .code(2)
        .stderr(contains("exists; pass --force"));
    assert_eq!(fs::read_to_string(&file).unwrap(), "mine\n");
    hitop(&home, repo.path()).args(["schedule", "apis/shop", "--github", "--env", "prod", "--hitop-version", "0.3.0", "--force"]).assert().code(0);
    assert_eq!(fs::read_to_string(&file).unwrap(), PROD_WORKFLOW.replace("VERSION", env!("CARGO_PKG_VERSION")));
}

#[test]
fn html_capable_versions_upload_the_html_report() {
    let (repo, _) = repo();
    let home = TempDir::new().unwrap();
    hitop(&home, repo.path())
        .args(["schedule", "./apis/shop", "--github", "--env", "local", "--cron", "*/30 8-18 * * 1-5", "--hitop-version", "0.4.0", "--name", "Shop smoke"])
        .assert()
        .code(0);
    let file = repo.path().join(".github/workflows/hitop-shop-smoke.yml");
    let text = fs::read_to_string(&file).unwrap();
    for needle in [
        "    - cron: \"*/30 8-18 * * 1-5\"\n",
        "          version: \"0.4.0\"\n",
        "          env: \"local\"\n",
        "          reporter: cli,junit,json,html\n",
        "uses: actions/upload-artifact@v7",
        "path: ${{ steps.hitop.outputs.report-dir }}/report.html",
        "if: always() && steps.hitop.outputs.report-dir != ''",
        // The folder globals' secret, no environment secrets.
        "HITOP_SECRET_GLOBAL_SIGNING: ${{ secrets.HITOP_SECRET_GLOBAL_SIGNING }}",
    ] {
        assert!(text.contains(needle), "{needle}\n{text}");
    }
    assert!(!text.contains("HITOP_SECRET_LOCAL"), "{text}");
    lint(&file);
}

#[test]
fn explicit_secrets_and_no_environment() {
    let (repo, _) = repo();
    let home = TempDir::new().unwrap();
    hitop(&home, repo.path())
        .args(["schedule", "apis/shop", "--github", "--env", "prod", "--secrets", "token,other-one", "--hitop-version", "0.3.0"])
        .assert()
        .code(0);
    let text = fs::read_to_string(repo.path().join(".github/workflows/hitop-shop-api-prod.yml")).unwrap();
    assert!(text.contains("HITOP_SECRET_PROD_OTHER_ONE: ${{ secrets.HITOP_SECRET_PROD_OTHER_ONE }}"), "{text}");
    assert!(text.contains("HITOP_SECRET_PROD_TOKEN:") && !text.contains("API_KEY") && !text.contains("SIGNING"), "{text}");

    // No --env: no env input, only the globals' secret.
    hitop(&home, repo.path()).args(["schedule", "apis/shop", "--github"]).assert().code(0);
    let file = repo.path().join(".github/workflows/hitop-shop-api.yml");
    let text = fs::read_to_string(&file).unwrap();
    assert!(!text.contains("          env:") && text.contains("HITOP_SECRET_GLOBAL_SIGNING"), "{text}");
    lint(&file);
}

#[test]
fn usage_errors_exit_2() {
    let (repo, _) = repo();
    let home = TempDir::new().unwrap();
    let outside = TempDir::new().unwrap();
    fs::create_dir_all(outside.path().join("api")).unwrap();
    fs::write(outside.path().join("api/hitop.yaml"), "format: 1\nname: Lone\n").unwrap();
    for (args, why) in [
        (vec!["schedule", "apis/shop"], "--github"),
        (vec!["schedule", "apis/shop", "--github", "--cron", "* * * * *"], "at most every 5 minutes"),
        (vec!["schedule", "apis/shop", "--github", "--cron", "0 6 * *"], "5 fields"),
        (vec!["schedule", "apis/shop", "--cron", "61 * * * *"], "minute: 61"),
        (vec!["schedule", "apis/shop", "--github", "--env", "staging"], "environment 'staging' not found"),
        (vec!["schedule", "apis", "--github"], "not a collection folder"),
        (vec!["schedule", "apis/shop", "--github", "--secrets", "token"], "--env"),
        (vec!["schedule", "apis/shop", "--github", "--env", "prod", "--secrets", "token=abc"], "never values"),
        (vec!["schedule", "apis/shop", "--cron", "0 6 * * *", "--force"], "--github"),
    ] {
        hitop(&home, repo.path()).args(&args).assert().code(2).stderr(contains(why));
    }
    hitop(&home, outside.path()).args(["schedule", "api", "--github"]).assert().code(2).stderr(contains("not in a git repository"));
    assert!(!repo.path().join(".github").exists());
}

#[test]
fn cron_prints_a_crontab_line_and_installs_nothing() {
    let (repo, dir) = repo();
    let home = TempDir::new().unwrap();
    let out = hitop(&home, repo.path())
        .args(["schedule", "apis/shop", "--cron", "*/30 * * * *", "--env", "prod"])
        .assert()
        .code(0)
        .stderr(contains("crontab -e").and(contains("HITOP_SECRET_PROD_TOKEN")))
        .get_output()
        .clone();
    let line = String::from_utf8(out.stdout).unwrap();
    assert_eq!(line.lines().count(), 1, "{line}");
    let folder = dir.canonicalize().unwrap();
    let logs = home.path().join(".local/state/hitop/runs/shop-api-prod");
    let logs = logs.to_string_lossy();
    assert!(line.starts_with("*/30 * * * * mkdir -p '"), "{line}");
    assert!(line.contains(&format!(" run '{}' --env 'prod' --reporter cli,junit,json,html --out '{logs}'/", folder.display())), "{line}");
    assert!(line.contains("date -u +\\%Y\\%m\\%dT\\%H\\%M\\%SZ") && line.contains(&format!(">> '{logs}/hitop.log' 2>&1")), "{line}");
    // Every % is escaped for cron.
    assert!(!line.replace("\\%", "").contains('%'), "{line}");
    assert!(!repo.path().join(".github").exists());

    // --log-dir, relative to the working directory.
    let out = hitop(&home, repo.path()).args(["schedule", "apis/shop", "--cron", "0 6 * * *", "--log-dir", "logs"]).assert().code(0).get_output().clone();
    let line = String::from_utf8(out.stdout).unwrap();
    assert!(line.contains(&format!("--out '{}'/", repo.path().join("logs").display())), "{line}");
    assert!(!line.contains("--env"), "{line}");
}

#[test]
fn sync_pulls_the_shared_collection_before_the_run() {
    let (repo, dir) = repo();
    let home = TempDir::new().unwrap();
    let out = hitop(&home, repo.path())
        .args(["schedule", "./apis/shop", "--github", "--sync", "--env", "local", "--hitop-version", "1.1.0", "--name", "Shared"])
        .assert()
        .code(0)
        .get_output()
        .clone();
    let stdout = String::from_utf8(out.stdout).unwrap();
    assert!(stdout.contains("hitop sync runner create -c './apis/shop' | gh secret set HITOP_SYNC_RUNNER"), "{stdout}");
    let file = repo.path().join(".github/workflows/hitop-shared.yml");
    let text = fs::read_to_string(&file).unwrap();
    let pull = text.find("Pull the shared collection").expect("a pull step");
    let run = text.find("uses: bohdaq/hitop-action@v1").unwrap();
    assert!(pull < run, "{text}");
    for needle in [
        "          HITOP_SYNC_RUNNER: ${{ secrets.HITOP_SYNC_RUNNER }}\n",
        "-e HITOP_SYNC_RUNNER -v \"$PWD:/work\" 'ghcr.io/bohdaq/hitop:1.1.0' sync pull -c 'apis/shop'\n",
        "#   hitop sync runner create -c <folder> | gh secret set HITOP_SYNC_RUNNER\n",
    ] {
        assert!(text.contains(needle), "{needle}\n{text}");
    }
    // The credential is never in the file, only the secret's name.
    assert!(!text.contains("hitop-runner-1."));
    lint(&file);

    // An image without hitop sync gets a note.
    hitop(&home, repo.path())
        .args(["schedule", "./apis/shop", "--github", "--sync", "--hitop-version", "1.0.0", "--name", "Old", "--force"])
        .assert()
        .code(0)
        .stderr(contains("has no `hitop sync`"));

    // cron: pull as this device, then run.
    let out = hitop(&home, repo.path()).args(["schedule", "apis/shop", "--cron", "0 6 * * *", "--sync"]).assert().code(0).stderr(contains("HITOP_SYNC_RUNNER")).get_output().clone();
    let line = String::from_utf8(out.stdout).unwrap();
    let folder = dir.canonicalize().unwrap();
    let pull = line.find(&format!(" sync pull -c '{}'", folder.display())).expect("a pull");
    assert!(pull < line.find(" run '").unwrap(), "{line}");
    assert!(line.contains(" --sync-report --sync-secrets "), "C4: the run goes to the history: {line}");
    assert!(!text.contains("sync report"), "1.1.0 has no `hitop sync report`");
}

#[test]
fn sync_uploads_the_run_to_the_history_with_a_release_after_1_2() {
    let (repo, _dir) = repo();
    let home = TempDir::new().unwrap();
    hitop(&home, repo.path())
        .args(["schedule", "./apis/shop", "--github", "--sync", "--env", "local", "--hitop-version", "1.3.0", "--name", "History"])
        .assert()
        .code(0)
        .stdout(contains("workspace's history"));
    let file = repo.path().join(".github/workflows/hitop-history.yml");
    let text = fs::read_to_string(&file).unwrap();
    let run = text.find("uses: bohdaq/hitop-action@v1").unwrap();
    let upload = text.find("Upload the run to the workspace history").expect("an upload step");
    assert!(run < upload, "{text}");
    for needle in [
        "        if: always() && steps.hitop.outputs.report-dir != ''\n        env:\n          HITOP_SYNC_RUNNER: ${{ secrets.HITOP_SYNC_RUNNER }}\n          REPORT_DIR: ${{ steps.hitop.outputs.report-dir }}\n",
        "-v \"$(realpath \"$REPORT_DIR\"):/report:ro\" 'ghcr.io/bohdaq/hitop:1.3.0' sync report /report/report.json\n",
    ] {
        assert!(text.contains(needle), "{needle}\n{text}");
    }
    lint(&file);
    hitop(&home, repo.path())
        .args(["schedule", "./apis/shop", "--github", "--sync", "--hitop-version", "1.2.0", "--name", "Older", "--force"])
        .assert()
        .code(0)
        .stderr(contains("can't upload runs"));
}
