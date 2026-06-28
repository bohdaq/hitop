use anyhow::Result;
use clap::Args;
use colored::Colorize;
use indicatif::{ProgressBar, ProgressStyle};

use hitop_core::{runner, storage};


#[derive(Args)]
pub struct RunArgs {
    /// Collection name (omit to run all)
    collection: Option<String>,

    /// Run all collections
    #[arg(long)]
    all: bool,

    /// Continue even if a request fails
    #[arg(long)]
    no_stop_on_fail: bool,
}

pub fn run(args: RunArgs) -> Result<()> {
    let collections = storage::load_collections()?;

    if args.all || args.collection.is_none() {
        // Run all
        let mut total = 0usize;
        let mut success = 0usize;
        for col in &collections {
            println!("\n{} {}", "▶".cyan(), col.name.bold());
            let results = run_one_collection(col, !args.no_stop_on_fail);
            for r in &results {
                total += 1;
                if r.result.as_ref().map(|res| res.status < 400).unwrap_or(false) {
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
        return Ok(());
    }

    let col_name = args.collection.as_deref().unwrap();
    let col = collections
        .iter()
        .find(|c| c.name == col_name)
        .ok_or_else(|| anyhow::anyhow!("collection '{}' not found", col_name))?;

    println!("{} {}", "▶".cyan(), col.name.bold());
    let results = run_one_collection(col, !args.no_stop_on_fail);

    let success = results
        .iter()
        .filter(|r| r.result.as_ref().map(|res| res.status < 400).unwrap_or(false))
        .count();

    println!(
        "\n{} {}/{} requests succeeded",
        "Summary:".bold(),
        success,
        results.len()
    );

    Ok(())
}

fn run_one_collection(col: &hitop_core::model::Collection, stop_on_fail: bool) -> Vec<runner::RunStepResult> {
    let bar = ProgressBar::new(col.requests.len() as u64);
    bar.set_style(
        ProgressStyle::default_bar()
            .template("  {bar:30} {pos}/{len} {msg}")
            .unwrap_or_else(|_| ProgressStyle::default_bar()),
    );

    let results = runner::run_collection(col, stop_on_fail, &mut |step| {
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
                bar.println(format!(
                    "  {} {} {} {} ({}ms)",
                    if res.status < 400 { "✓".green().to_string() } else { "✗".red().to_string() },
                    step.method.dimmed(),
                    step.request_name.bold(),
                    status_colored,
                    res.duration_ms,
                ));
            }
            Err(e) => {
                bar.println(format!(
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
