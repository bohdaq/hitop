//! `hitop schedule <folder>`: run a collection folder on a schedule.
//! `--github` writes `.github/workflows/hitop-<name>.yml` (the
//! `bohdaq/hitop-action`, `schedule:` + `workflow_dispatch:`); `--cron`
//! alone prints a crontab line. Neither installs anything else, and secret
//! values never go into what is written: only the names of the
//! `HITOP_SECRET_<ENV>_<NAME>` variables.
use std::collections::BTreeSet;
use std::path::{Path, PathBuf};

use anyhow::{bail, Context, Result};
use clap::Args;
use colored::Colorize;

use hitop_core::{format, secrets};

const AFTER_HELP: &str = "\
Examples:
  hitop schedule ./my-api --github --env prod
      writes .github/workflows/hitop-my-api-prod.yml: every day at 06:00 UTC
      and on demand, through bohdaq/hitop-action, with the environment's secret
      variables taken from repository secrets HITOP_SECRET_PROD_<NAME>
  hitop schedule ./my-api --cron \"*/30 * * * *\" --env local
      prints a crontab line (install it with `crontab -e`) that runs the folder
      every 30 minutes and keeps every run's reports under --log-dir

Cron expressions have five fields: minute (0-59), hour (0-23), day of month
(1-31), month (1-12 or JAN-DEC), day of week (0-6, Sunday is 0, or SUN-SAT),
each `*`, a number, a range `a-b`, a step `*/n` or `a-b/n`, or a list of
these. GitHub runs schedules in UTC, at most every 5 minutes.

Exit codes: 0 written or printed, 2 usage or configuration error.";

#[derive(Args)]
#[command(after_help = AFTER_HELP)]
pub struct ScheduleArgs {
    /// Collection folder (with hitop.yaml)
    folder: PathBuf,

    /// Write a GitHub Actions workflow into the repository the folder is in
    #[arg(long)]
    github: bool,

    /// When to run, a five-field cron expression (with --github: default
    /// "0 6 * * *", in UTC)
    #[arg(long, value_name = "EXPR")]
    cron: Option<String>,

    /// Environment to run with (one of the folder's environments/)
    #[arg(short = 'e', long = "env")]
    env: Option<String>,

    /// Secret variables the run needs, comma separated (default: the
    /// environment's and the folder globals' secret variables)
    #[arg(long, value_name = "NAME,...", value_delimiter = ',', requires = "env")]
    secrets: Vec<String>,

    /// Name for the workflow file (hitop-<name>.yml); default: the
    /// collection name, then the environment
    #[arg(long)]
    name: Option<String>,

    /// Replace an existing workflow file
    #[arg(long, requires = "github")]
    force: bool,

    /// hitop image version the workflow runs (default: this hitop's)
    #[arg(long, value_name = "VERSION", requires = "github")]
    hitop_version: Option<String>,

    /// Pull the shared collection (hitop sync) before each run. GitHub: with a read-only runner
    /// credential from the repository secret HITOP_SYNC_RUNNER (`hitop sync runner create`);
    /// cron: as this device, or HITOP_SYNC_RUNNER when set
    #[arg(long)]
    sync: bool,

    /// Directory for the cron runs' reports and log (default:
    /// $XDG_STATE_HOME/hitop/runs/<name>, else ~/.local/state/hitop/runs/<name>)
    #[arg(long, value_name = "DIR", conflicts_with = "github")]
    log_dir: Option<PathBuf>,
}

/// What a scheduled run needs to know about the folder.
struct Target {
    /// The collection's name.
    collection: String,
    env: Option<String>,
    /// The environment variables that carry the secret values.
    secret_vars: Vec<String>,
    /// For file names and job names: `[a-z0-9-]`.
    slug: String,
    /// `hitop sync pull` before the run.
    sync: bool,
}

fn target(args: &ScheduleArgs) -> Result<Target> {
    let dir = &args.folder;
    if !format::is_folder(dir) {
        bail!("{} is not a collection folder (no hitop.yaml); hitop schedule takes a folder, since the scheduled run reads it from disk", dir.display());
    }
    let folder = format::read_folder(dir)?;
    let environment = match &args.env {
        Some(name) => match folder.environments.iter().find(|e| &e.name == name) {
            Some(e) => Some(e),
            None => {
                let names: Vec<&str> = folder.environments.iter().map(|e| e.name.as_str()).collect();
                bail!(
                    "environment '{name}' not found in {}/environments/ ({})",
                    dir.display(),
                    if names.is_empty() { "none".to_string() } else { names.join(", ") }
                )
            }
        },
        None => None,
    };
    let mut vars = BTreeSet::new();
    if !args.secrets.is_empty() {
        let env = args.env.as_deref().expect("clap: --secrets requires --env");
        for name in &args.secrets {
            let name = name.trim();
            if name.is_empty() || name.contains('=') {
                bail!("--secrets takes secret variable names, never values (got '{name}')");
            }
            vars.insert(secrets::env_var(env, name));
        }
    } else {
        if let Some(e) = environment {
            vars.extend(e.secrets.iter().map(|n| secrets::env_var(&e.name, n)));
        }
        if let Some(g) = &folder.globals {
            vars.extend(g.secrets.iter().map(|n| secrets::env_var(secrets::GLOBALS_ENV, n)));
        }
    }
    let base = match &args.name {
        Some(n) => n.clone(),
        None => match &args.env {
            Some(e) => format!("{} {e}", folder.collection.name),
            None => folder.collection.name.clone(),
        },
    };
    let slug = slugify(&base);
    if slug.is_empty() {
        bail!("can't make a file name from '{base}'; pass --name");
    }
    Ok(Target { collection: folder.collection.name.clone(), env: args.env.clone(), secret_vars: vars.into_iter().collect(), slug, sync: args.sync })
}

pub fn run(args: ScheduleArgs) -> Result<()> {
    if !args.github && args.cron.is_none() {
        bail!("say where to schedule the run: --github (a GitHub Actions workflow) or --cron \"<expr>\" (a crontab line)");
    }
    let expr = args.cron.clone().unwrap_or_else(|| "0 6 * * *".to_string());
    let cron = Cron::parse(&expr).with_context(|| format!("--cron \"{expr}\""))?;
    let t = target(&args)?;
    if args.github {
        if let Some(m) = cron.shortest_interval_minutes() {
            if m < 5 {
                bail!("--cron \"{expr}\" runs every {m} minute(s); GitHub runs scheduled workflows at most every 5 minutes");
            }
        }
        github(&args, &t, &cron)
    } else {
        crontab(&args, &t, &cron)
    }
}

// --- GitHub Actions -------------------------------------------------------

/// The repository the folder is in (the nearest directory with `.git`),
/// and the folder's path inside it, `/`-separated.
fn repository(folder: &Path) -> Result<(PathBuf, String)> {
    let abs = folder.canonicalize().with_context(|| format!("{}", folder.display()))?;
    let root = abs
        .ancestors()
        .find(|d| d.join(".git").exists())
        .with_context(|| format!("{} is not in a git repository; the workflow goes into the repository's .github/workflows/", folder.display()))?
        .to_path_buf();
    let rel = abs.strip_prefix(&root).expect("an ancestor").components().map(|c| c.as_os_str().to_string_lossy().into_owned()).collect::<Vec<_>>();
    Ok((root, if rel.is_empty() { ".".to_string() } else { rel.join("/") }))
}

/// `--reporter html` came after 0.3.0: older images don't know it.
fn has_html_reporter(version: &str) -> bool {
    let core = version.trim_start_matches('v').split(['-', '+']).next().unwrap_or_default();
    let mut parts = core.split('.').map(|p| p.parse::<u64>().unwrap_or(0));
    let v = (parts.next().unwrap_or(0), parts.next().unwrap_or(0), parts.next().unwrap_or(0));
    v > (0, 3, 0)
}

/// `hitop sync report` (run history, HITOP Cloud C4) came after 1.2.
fn has_sync_report(version: &str) -> bool {
    let core = version.trim_start_matches('v').split(['-', '+']).next().unwrap_or_default();
    let mut parts = core.split('.').map(|p| p.parse::<u64>().unwrap_or(0));
    (parts.next().unwrap_or(0), parts.next().unwrap_or(0)) > (1, 2)
}

/// `hitop sync` came after 1.0.0.
fn has_sync(version: &str) -> bool {
    let core = version.trim_start_matches('v').split(['-', '+']).next().unwrap_or_default();
    let mut parts = core.split('.').map(|p| p.parse::<u64>().unwrap_or(0));
    (parts.next().unwrap_or(0), parts.next().unwrap_or(0), parts.next().unwrap_or(0)) > (1, 0, 0)
}

fn github(args: &ScheduleArgs, t: &Target, cron: &Cron) -> Result<()> {
    let (root, path) = repository(&args.folder)?;
    let version = args.hitop_version.clone().unwrap_or_else(|| env!("CARGO_PKG_VERSION").to_string());
    let file = root.join(".github/workflows").join(format!("hitop-{}.yml", t.slug));
    if file.exists() && !args.force {
        bail!("{} exists; pass --force to replace it", file.display());
    }
    let text = workflow(t, cron, &path, &version)?;
    std::fs::create_dir_all(file.parent().expect("a parent")).with_context(|| format!("creating {}", root.join(".github/workflows").display()))?;
    std::fs::write(&file, text).with_context(|| format!("writing {}", file.display()))?;
    println!("{} {}", "Wrote".green(), file.display());
    println!("Runs at \"{}\" (UTC) and on demand from the Actions tab.", cron.expr);
    if t.sync {
        println!("Add the read-only runner credential as a repository secret:");
        println!("  hitop sync runner create -c {} | gh secret set HITOP_SYNC_RUNNER", sh(&args.folder.to_string_lossy()));
        if !has_sync(&version) {
            eprintln!("{} hitop {version}'s image has no `hitop sync`; pass --hitop-version with a release after 1.0.0", "Note:".yellow().bold());
        } else if has_sync_report(&version) {
            println!("Each run's report goes to the workspace's history (`hitop sync runs`; alerts: `hitop sync alerts`).");
        } else {
            eprintln!("{} hitop {version} can't upload runs to the workspace's history; a release after 1.2 can (--hitop-version)", "Note:".yellow().bold());
        }
    }
    if !t.secret_vars.is_empty() {
        println!("Add these repository secrets (Settings → Secrets and variables → Actions):");
        for v in &t.secret_vars {
            println!("  gh secret set {v}");
        }
    }
    Ok(())
}

/// A YAML double-quoted scalar. Refuses `${{`, which GitHub would
/// evaluate as an expression wherever it appears.
fn yq(s: &str) -> Result<String> {
    if s.contains("${{") {
        bail!("'{s}' contains '${{{{', which GitHub Actions would evaluate");
    }
    let mut out = String::from("\"");
    for c in s.chars() {
        match c {
            '\\' => out.push_str("\\\\"),
            '"' => out.push_str("\\\""),
            c if c.is_control() => out.push_str(&format!("\\u{:04X}", c as u32)),
            c => out.push(c),
        }
    }
    out.push('"');
    Ok(out)
}

/// The collection name for comments and titles: printable, one line.
fn display(s: &str) -> String {
    s.chars().filter(|c| !c.is_control()).collect::<String>().replace("${{", "$ {{")
}

fn workflow(t: &Target, cron: &Cron, path: &str, version: &str) -> Result<String> {
    let html = has_html_reporter(version);
    let title = match &t.env {
        Some(e) => format!("{} ({})", display(&t.collection), display(e)),
        None => display(&t.collection),
    };
    let mut y = String::new();
    y.push_str(&format!("# Written by `hitop schedule --github` (hitop {}).\n", env!("CARGO_PKG_VERSION")));
    y.push_str(&format!("# Runs the hitop collection in {path} on a schedule ({} UTC) and on demand.\n", cron.expr));
    if t.sync {
        y.push_str("# The collection comes from hitop sync; add a read-only runner credential once:\n");
        y.push_str("#   hitop sync runner create -c <folder> | gh secret set HITOP_SYNC_RUNNER\n");
    }
    if !t.secret_vars.is_empty() {
        y.push_str("# Secret values come from repository secrets with the same names:\n");
        for v in &t.secret_vars {
            y.push_str(&format!("#   gh secret set {v}\n"));
        }
    }
    y.push_str(&format!("name: {}\n", yq(&format!("hitop: {title}"))?));
    y.push_str("on:\n  schedule:\n");
    y.push_str(&format!("    - cron: {}\n", yq(&cron.expr)?));
    y.push_str("  workflow_dispatch:\n");
    y.push_str("permissions:\n  contents: read\n");
    y.push_str(&format!("concurrency:\n  group: hitop-{}\n  cancel-in-progress: false\n", t.slug));
    y.push_str("jobs:\n  run:\n");
    y.push_str(&format!("    name: {}\n", yq(&title)?));
    y.push_str("    runs-on: ubuntu-24.04\n    timeout-minutes: 30\n    steps:\n");
    y.push_str("      - uses: actions/checkout@v6\n        with:\n          persist-credentials: false\n");
    if t.sync {
        // The runner credential decrypts the workspace in the job (read-only, this workspace
        // only); HOME is the container's /tmp because the image runs as the runner's uid.
        y.push_str("      - name: Pull the shared collection (hitop sync, read-only runner credential)\n");
        y.push_str("        env:\n          HITOP_SYNC_RUNNER: ${{ secrets.HITOP_SYNC_RUNNER }}\n");
        y.push_str("        run: |\n");
        y.push_str(&format!(
            "          docker run --rm --user \"$(id -u)\" -e HOME=/tmp -e HITOP_KEYCHAIN=off -e HITOP_SYNC_RUNNER -v \"$PWD:/work\" {} sync pull -c {}\n",
            sh(&format!("ghcr.io/bohdaq/hitop:{version}")),
            sh(path)
        ));
    }
    y.push_str(&format!("      - name: {}\n        id: hitop\n        uses: bohdaq/hitop-action@v1\n", yq(&format!("Run {title}"))?));
    if !t.secret_vars.is_empty() {
        y.push_str("        env:\n");
        for v in &t.secret_vars {
            y.push_str(&format!("          {v}: ${{{{ secrets.{v} }}}}\n"));
        }
    }
    y.push_str("        with:\n");
    y.push_str(&format!("          version: {}\n", yq(version)?));
    y.push_str(&format!("          path: {}\n", yq(path)?));
    if let Some(e) = &t.env {
        y.push_str(&format!("          env: {}\n", yq(e)?));
    }
    y.push_str(&format!("          reporter: {}\n", if html { "cli,junit,json,html" } else { "cli,junit,json" }));
    y.push_str(&format!("          artifact-name: hitop-{}-junit\n", t.slug));
    if t.sync && has_sync_report(version) {
        // The run history (HITOP Cloud C4): the report the Action wrote, sealed in the job with
        // the runner credential; the server gets a status row and ciphertext.
        y.push_str("      - name: Upload the run to the workspace history (hitop sync)\n        if: always() && steps.hitop.outputs.report-dir != ''\n");
        y.push_str("        env:\n          HITOP_SYNC_RUNNER: ${{ secrets.HITOP_SYNC_RUNNER }}\n          REPORT_DIR: ${{ steps.hitop.outputs.report-dir }}\n");
        y.push_str("        run: |\n");
        y.push_str(&format!(
            "          docker run --rm --user \"$(id -u)\" -e HOME=/tmp -e HITOP_KEYCHAIN=off -e HITOP_SYNC_RUNNER -v \"$(realpath \"$REPORT_DIR\"):/report:ro\" {} sync report /report/report.json\n",
            sh(&format!("ghcr.io/bohdaq/hitop:{version}"))
        ));
    }
    if html {
        y.push_str("      - name: HTML report\n        if: always() && steps.hitop.outputs.report-dir != ''\n");
        y.push_str("        uses: actions/upload-artifact@v7\n        with:\n");
        y.push_str(&format!("          name: hitop-{}-report\n", t.slug));
        y.push_str("          path: ${{ steps.hitop.outputs.report-dir }}/report.html\n          if-no-files-found: ignore\n");
    }
    Ok(y)
}

// --- crontab ---------------------------------------------------------------

/// A word for `sh`, single-quoted.
fn sh(s: &str) -> String {
    format!("'{}'", s.replace('\'', "'\\''"))
}

fn absolute(p: &Path) -> Result<PathBuf> {
    if p.is_absolute() {
        Ok(p.to_path_buf())
    } else {
        Ok(std::env::current_dir()?.join(p))
    }
}

fn crontab(args: &ScheduleArgs, t: &Target, cron: &Cron) -> Result<()> {
    let folder = args.folder.canonicalize().with_context(|| format!("{}", args.folder.display()))?;
    let log_dir = match &args.log_dir {
        Some(d) => absolute(d)?,
        None => {
            let state = match std::env::var_os("XDG_STATE_HOME").filter(|v| !v.is_empty()) {
                Some(s) => PathBuf::from(s),
                None => PathBuf::from(std::env::var_os("HOME").filter(|v| !v.is_empty()).context("HOME is not set; pass --log-dir")?).join(".local/state"),
            };
            state.join("hitop/runs").join(&t.slug)
        }
    };
    let exe = std::env::current_exe().context("finding the hitop binary")?;
    let line = cron_line(cron, &exe, &folder, t.env.as_deref(), &log_dir, t.sync);
    println!("{line}");
    eprintln!("{} add the line above with `crontab -e` (cron runs it in the system time zone).", "Next:".bold());
    if t.sync {
        eprintln!("Before each run it pulls the shared collection (hitop sync) as this device; on a server, put a read-only runner credential in the crontab as HITOP_SYNC_RUNNER=... instead (`hitop sync runner create`).");
    }
    eprintln!("Each run writes {}/<time>/ (junit.xml, report.json, report.html) and appends to {}/hitop.log.", log_dir.display(), log_dir.display());
    if !t.secret_vars.is_empty() {
        eprintln!("Secrets ({}): cron can't prompt, so keep them in the keychain (`hitop secret set <name> --env {} -c {}`; a cron job reads it only while you are logged in) or set the variables in the crontab or a file only you can read. Never put a value in the line itself.", t.secret_vars.join(", "), t.env.as_deref().unwrap_or("<env>"), sh(&folder.to_string_lossy()));
    }
    Ok(())
}

fn cron_line(cron: &Cron, exe: &Path, folder: &Path, env: Option<&str>, log_dir: &Path, sync: bool) -> String {
    let log = log_dir.to_string_lossy();
    let mut cmd = format!("mkdir -p {}", sh(&log));
    if sync {
        cmd.push_str(&format!(" && {} sync pull -c {} >> {} 2>&1", sh(&exe.to_string_lossy()), sh(&folder.to_string_lossy()), sh(&format!("{log}/hitop.log"))));
    }
    cmd.push_str(&format!(" && {} run {}", sh(&exe.to_string_lossy()), sh(&folder.to_string_lossy())));
    if let Some(e) = env {
        cmd.push_str(&format!(" --env {}", sh(e)));
    }
    if sync {
        // The run history and shared values of the workspace (HITOP Cloud C4).
        cmd.push_str(" --sync-report --sync-secrets");
    }
    cmd.push_str(&format!(
        " --reporter cli,junit,json,html --out {}/\"$(date -u +%Y%m%dT%H%M%SZ)\" >> {} 2>&1",
        sh(&log),
        sh(&format!("{log}/hitop.log"))
    ));
    // cron turns an unescaped % into a newline.
    format!("{} {}", cron.expr, cmd.replace('%', "\\%"))
}

// --- cron expressions ------------------------------------------------------

/// A five-field cron expression, checked against what both cron and
/// GitHub Actions accept.
#[derive(Debug)]
pub struct Cron {
    /// The fields joined by one space.
    pub expr: String,
    minutes: BTreeSet<u32>,
    hours: BTreeSet<u32>,
}

const FIELDS: [(&str, u32, u32); 5] = [("minute", 0, 59), ("hour", 0, 23), ("day of month", 1, 31), ("month", 1, 12), ("day of week", 0, 6)];
const MONTHS: [&str; 12] = ["JAN", "FEB", "MAR", "APR", "MAY", "JUN", "JUL", "AUG", "SEP", "OCT", "NOV", "DEC"];
const DAYS: [&str; 7] = ["SUN", "MON", "TUE", "WED", "THU", "FRI", "SAT"];

impl Cron {
    pub fn parse(expr: &str) -> Result<Cron> {
        let expr = expr.trim();
        if expr.starts_with('@') {
            bail!("write '{expr}' as five fields (\"0 0 * * *\" for @daily); GitHub doesn't take @-shortcuts");
        }
        let fields: Vec<&str> = expr.split_whitespace().collect();
        if fields.len() != 5 {
            bail!("a cron expression has 5 fields (minute hour day-of-month month day-of-week), this has {}", fields.len());
        }
        let mut sets = vec![];
        for (i, (field, &(what, min, max))) in fields.iter().zip(FIELDS.iter()).enumerate() {
            let names: &[&str] = match i {
                3 => &MONTHS,
                4 => &DAYS,
                _ => &[],
            };
            sets.push(parse_field(field, what, min, max, names)?);
        }
        Ok(Cron { expr: fields.join(" "), minutes: sets[0].clone(), hours: sets[1].clone() })
    }

    /// The shortest gap between two runs in minutes, when it is under an
    /// hour (None: at most once an hour).
    pub fn shortest_interval_minutes(&self) -> Option<u32> {
        let m: Vec<u32> = self.minutes.iter().copied().collect();
        let mut best: Option<u32> = None;
        for w in m.windows(2) {
            best = Some(best.map_or(w[1] - w[0], |b| b.min(w[1] - w[0])));
        }
        // From the last minute of one hour to the first of the next.
        let next_hour = self.hours.iter().any(|h| self.hours.contains(&((h + 1) % 24)));
        if next_hour {
            let wrap = 60 - m[m.len() - 1] + m[0];
            if wrap < 60 {
                best = Some(best.map_or(wrap, |b| b.min(wrap)));
            }
        }
        best
    }
}

fn parse_field(field: &str, what: &str, min: u32, max: u32, names: &[&str]) -> Result<BTreeSet<u32>> {
    // A name (JAN, MON) stands alone: cron doesn't take them in ranges or lists.
    if let Some(i) = names.iter().position(|n| n.eq_ignore_ascii_case(field)) {
        return Ok([i as u32 + min].into());
    }
    let mut set = BTreeSet::new();
    for item in field.split(',') {
        let (range, step) = match item.split_once('/') {
            Some((r, s)) => {
                let step: u32 = s.parse().ok().filter(|s| *s >= 1).with_context(|| format!("{what}: step '{s}' in '{item}' is not a whole number from 1"))?;
                (r, Some(step))
            }
            None => (item, None),
        };
        let (lo, hi) = if range == "*" {
            (min, max)
        } else if let Some((a, b)) = range.split_once('-') {
            let (a, b) = (number(a, what, min, max, names)?, number(b, what, min, max, names)?);
            if a > b {
                bail!("{what}: range '{range}' goes backwards");
            }
            (a, b)
        } else {
            let n = number(range, what, min, max, names)?;
            if step.is_some() {
                bail!("{what}: write '{item}' as '{range}-{max}/{}' (a step needs * or a range)", step.unwrap_or(1));
            }
            (n, n)
        };
        let step = step.unwrap_or(1);
        set.extend((lo..=hi).step_by(step as usize));
    }
    Ok(set)
}

fn number(s: &str, what: &str, min: u32, max: u32, names: &[&str]) -> Result<u32> {
    if names.iter().any(|n| n.eq_ignore_ascii_case(s)) {
        bail!("{what}: use numbers in ranges and lists ('{s}' only works on its own)");
    }
    let n: u32 = s.parse().ok().with_context(|| format!("{what}: '{s}' is not a number"))?;
    if n < min || n > max {
        let hint = if what == "day of week" && n == 7 { " (Sunday is 0)" } else { "" };
        bail!("{what}: {n} is outside {min}-{max}{hint}");
    }
    Ok(n)
}

fn slugify(s: &str) -> String {
    let mut out = String::new();
    for c in s.chars() {
        if c.is_ascii_alphanumeric() {
            out.push(c.to_ascii_lowercase());
        } else if !out.is_empty() && !out.ends_with('-') {
            out.push('-');
        }
    }
    out.trim_end_matches('-').chars().take(60).collect::<String>().trim_end_matches('-').to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ok(expr: &str) -> Cron {
        Cron::parse(expr).unwrap_or_else(|e| panic!("{expr}: {e:#}"))
    }

    fn err(expr: &str) -> String {
        format!("{:#}", Cron::parse(expr).expect_err(expr))
    }

    #[test]
    fn valid_expressions() {
        for e in ["0 6 * * *", "*/15 * * * *", "0 9-17 * * 1-5", "5,35 */2 1,15 * *", "0 0 1 JAN *", "30 4 * * sun", "0 0 29 2 *", "0-30/10 0 * * 0", "59 23 31 12 6"] {
            ok(e);
        }
        assert_eq!(ok("  0   6 * *  * ").expr, "0 6 * * *");
    }

    #[test]
    fn invalid_expressions_say_why() {
        for (e, why) in [
            ("0 6 * *", "5 fields"),
            ("0 6 * * * *", "5 fields"),
            ("@daily", "five fields"),
            ("60 * * * *", "minute: 60 is outside 0-59"),
            ("0 24 * * *", "hour: 24"),
            ("0 0 0 * *", "day of month: 0"),
            ("0 0 * 13 *", "month: 13"),
            ("0 0 * * 7", "Sunday is 0"),
            ("*/0 * * * *", "step '0'"),
            ("5/10 * * * *", "a step needs * or a range"),
            ("10-5 * * * *", "backwards"),
            ("a * * * *", "'a' is not a number"),
            ("0 0 * JAN-MAR *", "only works on its own"),
            ("0,, * * * *", "'' is not a number"),
            ("0 0 * * MON,TUE", "only works on its own"),
        ] {
            let m = err(e);
            assert!(m.contains(why), "{e}: {m}");
        }
    }

    #[test]
    fn shortest_interval() {
        assert_eq!(ok("0 6 * * *").shortest_interval_minutes(), None);
        assert_eq!(ok("0 * * * *").shortest_interval_minutes(), None);
        assert_eq!(ok("* * * * *").shortest_interval_minutes(), Some(1));
        assert_eq!(ok("*/5 * * * *").shortest_interval_minutes(), Some(5));
        assert_eq!(ok("*/7 * * * *").shortest_interval_minutes(), Some(4)); // 56 → 0
        assert_eq!(ok("0,58 6 * * *").shortest_interval_minutes(), Some(58));
        assert_eq!(ok("58,1 6-7 * * *").shortest_interval_minutes(), Some(3));
    }

    #[test]
    fn slugs_and_html_versions() {
        assert_eq!(slugify("Sample API prod"), "sample-api-prod");
        assert_eq!(slugify("  Shop & <API> "), "shop-api");
        assert_eq!(slugify("**"), "");
        assert!(!has_html_reporter("0.3.0"));
        assert!(!has_html_reporter("0.3.0-rc.2"));
        assert!(!has_html_reporter("v0.2.9"));
        assert!(has_html_reporter("0.3.1"));
        assert!(has_html_reporter("0.4.0"));
        assert!(has_html_reporter("1.0.0"));
    }

    #[test]
    fn yaml_quoting_and_expressions() {
        assert_eq!(yq("a \"b\" \\ c").unwrap(), "\"a \\\"b\\\" \\\\ c\"");
        assert_eq!(yq("x\ny").unwrap(), "\"x\\u000Ay\"");
        assert!(yq("${{ secrets.X }}").is_err());
        assert_eq!(display("a${{ b }}\n"), "a$ {{ b }}");
    }

    #[test]
    fn crontab_line_escapes_percent_and_quotes() {
        let line = cron_line(&ok("*/30 * * * *"), Path::new("/usr/bin/hitop"), Path::new("/home/a/it's api"), Some("prod"), Path::new("/var/log/h%"), false);
        assert_eq!(
            line,
            "*/30 * * * * mkdir -p '/var/log/h\\%' && '/usr/bin/hitop' run '/home/a/it'\\''s api' --env 'prod' --reporter cli,junit,json,html --out '/var/log/h\\%'/\"$(date -u +\\%Y\\%m\\%dT\\%H\\%M\\%SZ)\" >> '/var/log/h\\%/hitop.log' 2>&1"
        );
    }
}
