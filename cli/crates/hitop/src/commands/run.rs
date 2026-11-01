//! `hitop run`: runs a collection (or all of them) for CI, with reports,
//! data-driven iterations and exit codes.
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::time::Duration;

use anyhow::{bail, Context, Result};
use clap::{Args, ValueEnum};
use colored::Colorize;
use indicatif::{ProgressBar, ProgressStyle};

use hitop_core::{
    format,
    model::{Collection, Environment},
    report::{Report, ReportBuilder},
    runner::{self, RunOptions},
    secrets::Secrets,
    storage,
};

use super::data;
use super::secret::{self, SecretFlags};
use super::source::Source;

const AFTER_HELP: &str = "\
Exit codes:
  0  every request passed (sent, status below 400, every assertion true)
  1  a request or an assertion failed
  2  usage or configuration error (unknown collection or environment, bad flag,
     unreadable --env-file or --data, missing secret, reports not written)

Variables, later ones win: collection, environment (--env or --env-file),
the --data row, --var, then secrets. --var values are not masked: give secret
values with --secret or HITOP_SECRET_<ENV>_<NAME>.

Reports: --reporter junit writes <out>/junit.xml, --reporter json writes
<out>/report.json (format: spec/run-report.md). Secret values are •••• in both.";

#[derive(Clone, Copy, PartialEq, Eq, ValueEnum)]
pub enum Reporter {
    /// Progress and results on stdout
    Cli,
    /// JUnit XML in <out>/junit.xml
    Junit,
    /// JSON report in <out>/report.json
    Json,
}

#[derive(Args)]
#[command(after_help = AFTER_HELP)]
pub struct RunArgs {
    /// Collection name, or path to a collection folder (omit to run all)
    collection: Option<String>,

    /// Run all collections in global storage
    #[arg(long)]
    all: bool,

    /// Environment to use instead of the active one (for a folder: one of
    /// its environments/, none by default)
    #[arg(short = 'e', long = "env", conflicts_with = "env_file")]
    env: Option<String>,

    /// Environment file to use (YAML, as in a folder's environments/)
    #[arg(long, value_name = "YAML")]
    env_file: Option<PathBuf>,

    /// Variable for this run, as KEY=VALUE (repeatable); overrides the
    /// collection, the environment and the --data row
    #[arg(long = "var", value_name = "KEY=VALUE")]
    vars: Vec<String>,

    /// Stop at the first request that fails
    #[arg(long)]
    bail: bool,

    /// Before 0.3 a run stopped at the first failure unless this was given;
    /// now it is the default. Kept so old scripts still parse.
    #[arg(long, hide = true, conflicts_with = "bail")]
    no_stop_on_fail: bool,

    /// Time limit for each request, in milliseconds
    #[arg(long, value_name = "MS", value_parser = clap::value_parser!(u64).range(1..))]
    timeout: Option<u64>,

    /// Run the collection this many times
    #[arg(long, value_name = "N", default_value_t = 1, value_parser = clap::value_parser!(u64).range(1..), conflicts_with = "data")]
    iterations: u64,

    /// CSV (header row) or JSON array of objects: one iteration per row,
    /// the row's values as variables
    #[arg(long, value_name = "CSV|JSON")]
    data: Option<PathBuf>,

    /// Reporters, comma separated
    #[arg(long, value_enum, value_delimiter = ',', default_value = "cli")]
    reporter: Vec<Reporter>,

    /// Directory for the junit and json reports
    #[arg(long, value_name = "DIR", default_value = "hitop-reports")]
    out: PathBuf,

    #[command(flatten)]
    secrets: SecretFlags,
}

fn var_pairs(vars: &[String]) -> Result<HashMap<String, String>> {
    vars.iter()
        .map(|s| match s.split_once('=') {
            Some((k, v)) if !k.is_empty() => Ok((k.to_string(), v.to_string())),
            _ => bail!("--var takes KEY=VALUE, got '{s}'"),
        })
        .collect()
}

/// What to run: collections with one environment and resolved secrets.
struct Plan {
    collections: Vec<Collection>,
    environment: Option<Environment>,
    secrets: Secrets,
}

fn plan(args: &RunArgs) -> Result<Plan> {
    let env_file = match &args.env_file {
        Some(p) => Some(format::read_environment_file(p).with_context(|| format!("--env-file {}", p.display()))?),
        None => None,
    };
    if args.all || args.collection.is_none() {
        let collections = storage::load_collections()?;
        let environment = match env_file {
            Some(e) => Some(e),
            None => storage::load_environments()?.pick(args.env.as_deref())?.cloned(),
        };
        let secrets = secret::resolve(hitop_core::secrets::GLOBAL_WORKSPACE, environment.as_ref(), &args.secrets)?;
        return Ok(Plan { collections, environment, secrets });
    }
    let source = Source::open(args.collection.as_deref().unwrap_or_default())?;
    let environment = match env_file {
        Some(e) => Some(e),
        None => source.environment(args.env.as_deref())?,
    };
    let secrets = secret::resolve(&source.workspace(), environment.as_ref(), &args.secrets)?;
    Ok(Plan { collections: vec![source.collection], environment, secrets })
}

/// Runs and reports. Ok(0) when everything passed, Ok(1) when a request or
/// assertion failed; an Err is a usage or configuration error (exit 2).
pub fn run(args: RunArgs) -> Result<i32> {
    let vars = var_pairs(&args.vars)?;
    let rows: Vec<Option<data::Row>> = match &args.data {
        Some(path) => data::read(path)?.into_iter().map(Some).collect(),
        None => (0..args.iterations).map(|_| None).collect(),
    };
    let reporters = &args.reporter;
    let show = reporters.contains(&Reporter::Cli);
    let plan = plan(&args)?;
    let env = plan.environment.as_ref();
    let options = RunOptions {
        stop_on_fail: args.bail,
        timeout: args.timeout.map(Duration::from_millis),
        overrides: HashMap::new(),
    };

    let mut builder = ReportBuilder::new(env!("CARGO_PKG_VERSION"));
    let mut bailed = false;
    'collections: for col in &plan.collections {
        builder.collection(&col.name, env.map(|e| e.name.as_str()));
        if show {
            println!("{} {}", "▶".cyan(), col.name.bold());
        }
        for (i, row) in rows.iter().enumerate() {
            if show && rows.len() > 1 {
                let data = row.as_ref().map(|r| format!(" {}", describe(r))).unwrap_or_default();
                println!("  {}{}", format!("Iteration {} of {}", i + 1, rows.len()).bold(), data.dimmed());
            }
            let mut options = options.clone();
            options.overrides = row.iter().flatten().map(|(k, v)| (k.clone(), v.clone())).collect();
            options.overrides.extend(vars.clone());
            builder.start_iteration();
            let steps = run_one(col, env, &plan.secrets, &options, show);
            let failed = steps.iter().any(|s| !s.passed());
            builder.iteration(row.clone(), &steps);
            if failed && args.bail {
                bailed = true;
                break 'collections;
            }
        }
    }
    let report = builder.finish(bailed);
    if show {
        print_summary(&report);
    }
    write_reports(&report, reporters, &args.out)?;
    Ok(if report.passed { 0 } else { 1 })
}

fn describe(row: &data::Row) -> String {
    let parts: Vec<String> = row.iter().map(|(k, v)| format!("{k}={v}")).collect();
    let s = parts.join(" ");
    if s.chars().count() > 60 {
        format!("{}…", s.chars().take(59).collect::<String>())
    } else {
        s
    }
}

fn print_summary(r: &Report) {
    let s = &r.summary;
    let assertions = if s.assertions.total == 0 {
        String::new()
    } else {
        format!(", {}/{} assertions passed", s.assertions.passed, s.assertions.total)
    };
    let iterations = if s.iterations > s.collections { format!(" in {} iterations", s.iterations) } else { String::new() };
    let bailed = if r.bailed { " (stopped at the first failure)" } else { "" };
    println!("\n{} {}/{} requests succeeded{assertions}{iterations}{bailed}", "Summary:".bold(), s.requests.passed, s.requests.total);
}

fn write_reports(report: &Report, reporters: &[Reporter], out: &Path) -> Result<()> {
    let files: Vec<(&str, String)> = reporters
        .iter()
        .filter_map(|r| match r {
            Reporter::Cli => None,
            Reporter::Junit => Some(("junit.xml", report.to_junit())),
            Reporter::Json => Some(("report.json", report.to_json())),
        })
        .collect();
    if files.is_empty() {
        return Ok(());
    }
    std::fs::create_dir_all(out).with_context(|| format!("creating --out {}", out.display()))?;
    for (name, content) in files {
        let path = out.join(name);
        std::fs::write(&path, content).with_context(|| format!("writing {}", path.display()))?;
        eprintln!("{} {}", "Report:".dimmed(), path.display());
    }
    Ok(())
}

fn run_one(col: &Collection, env: Option<&Environment>, secrets: &Secrets, options: &RunOptions, show: bool) -> Vec<runner::RunStepResult> {
    let bar = if show { ProgressBar::new(col.requests.len() as u64) } else { ProgressBar::hidden() };
    bar.set_style(
        ProgressStyle::default_bar()
            .template("  {bar:30} {pos}/{len} {msg}")
            .unwrap_or_else(|_| ProgressStyle::default_bar()),
    );

    // Without a terminal (CI logs) the bar is hidden and so is anything
    // printed through it, so print lines directly there.
    let say = |line: String| match (show, bar.is_hidden()) {
        (false, _) => {}
        (true, true) => println!("{line}"),
        (true, false) => bar.println(line),
    };
    // Steps come back masked: no secret value reaches the log.
    let results = runner::run_collection_opts(col, env, secrets, options, &mut |step| {
        bar.inc(1);
        let mark = if step.passed() { "✓".green().to_string() } else { "✗".red().to_string() };
        match &step.result {
            Ok(res) => {
                let status = res.status.to_string();
                let status = if res.status < 300 {
                    status.green()
                } else if res.status < 400 {
                    status.yellow()
                } else {
                    status.red()
                };
                say(format!("  {mark} {} {} {status} ({}ms)", step.method.dimmed(), step.request_name.bold(), res.duration_ms));
            }
            Err(e) => say(format!("  {mark} {} {} {}", step.method.dimmed(), step.request_name.bold(), format!("{e:#}").red())),
        }
        for t in &step.tests {
            if t.pass {
                say(format!("      {} {}", "✓".green(), t.name.dimmed()));
            } else {
                say(format!("      {} {} {}", "✗".red(), t.name, format!("({})", t.detail).red()));
            }
        }
    });

    bar.finish_and_clear();
    results
}
