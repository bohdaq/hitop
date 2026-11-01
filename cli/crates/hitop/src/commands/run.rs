use anyhow::Result;
use clap::Args;
use colored::Colorize;
use indicatif::{ProgressBar, ProgressStyle};

use hitop_core::{runner, storage};

use super::source::Source;


#[derive(Args)]
pub struct RunArgs {
    /// Collection name, or path to a collection folder (omit to run all)
    collection: Option<String>,

    /// Run all collections
    #[arg(long)]
    all: bool,

    /// Continue even if a request fails
    #[arg(long)]
    no_stop_on_fail: bool,

    /// Environment to use instead of the active one (for a folder: one of
    /// its environments/, none by default)
    #[arg(short = 'e', long = "env")]
    env: Option<String>,
}

/// Exits with status 1 when a request fails or an assertion doesn't pass,
/// so CI jobs fail with it.
pub fn run(args: RunArgs) -> Result<()> {
    if args.all || args.collection.is_none() {
        let collections = storage::load_collections()?;
        let envs = storage::load_environments()?;
        let env = envs.pick(args.env.as_deref())?;
        // Run all
        let mut total = 0usize;
        let mut success = 0usize;
        for col in &collections {
            println!("\n{} {}", "▶".cyan(), col.name.bold());
            let results = run_one_collection(col, env, !args.no_stop_on_fail);
            for r in &results {
                total += 1;
                if r.passed() {
                    success += 1;
                }
            }
        }
        println!(
            "\n{} {}/{} requests succeeded",
            "Summary:".bold(),
            success,
            total
        );
        if success < total {
            std::process::exit(1);
        }
        return Ok(());
    }

    let source = Source::open(args.collection.as_deref().unwrap_or_default())?;
    let env = source.environment(args.env.as_deref())?;
    let col = &source.collection;

    println!("{} {}", "▶".cyan(), col.name.bold());
    let results = run_one_collection(col, env.as_ref(), !args.no_stop_on_fail);

    let success = results
        .iter()
        .filter(|r| r.passed())
        .count();

    println!(
        "\n{} {}/{} requests succeeded",
        "Summary:".bold(),
        success,
        results.len()
    );
    if success < results.len() {
        std::process::exit(1);
    }

    Ok(())
}

fn run_one_collection(
    col: &hitop_core::model::Collection,
    env: Option<&hitop_core::model::Environment>,
    stop_on_fail: bool,
) -> Vec<runner::RunStepResult> {
    let bar = ProgressBar::new(col.requests.len() as u64);
    bar.set_style(
        ProgressStyle::default_bar()
            .template("  {bar:30} {pos}/{len} {msg}")
            .unwrap_or_else(|_| ProgressStyle::default_bar()),
    );

    // Without a terminal (CI logs) the bar is hidden and so is anything
    // printed through it, so print lines directly there.
    let say = |line: String| if bar.is_hidden() { println!("{line}") } else { bar.println(line) };
    let results = runner::run_collection(col, env, stop_on_fail, &mut |step| {
        bar.inc(1);
        match &step.result {
            Ok(res) => {
                let status_colored = if res.status < 300 {
                    format!("{}", res.status).green()
                } else if res.status < 400 {
                    format!("{}", res.status).yellow()
                } else {
                    format!("{}", res.status).red()
                };
                let passed = step.tests.iter().filter(|t| t.pass).count();
                let tests = if step.tests.is_empty() { String::new() } else { format!(" {}/{} tests", passed, step.tests.len()) };
                say(format!(
                    "  {} {} {} {} ({}ms){}",
                    if step.passed() { "✓".green().to_string() } else { "✗".red().to_string() },
                    step.method.dimmed(),
                    step.request_name.bold(),
                    status_colored,
                    res.duration_ms,
                    tests.dimmed(),
                ));
                for t in step.tests.iter().filter(|t| !t.pass) {
                    say(format!("      {} {} {}", "✗".red(), t.name, format!("({})", t.detail).red()));
                }
            }
            Err(e) => {
                say(format!(
                    "  {} {} {} — {}",
                    "✗".red(),
                    step.method.dimmed(),
                    step.request_name.bold(),
                    e.to_string().red()
                ));
            }
        }
    });

    bar.finish_and_clear();
    results
}
