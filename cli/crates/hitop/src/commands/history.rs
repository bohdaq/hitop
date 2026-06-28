use anyhow::Result;
use clap::Args;
use colored::Colorize;

use hitop_core::storage;

#[derive(Args)]
pub struct HistoryArgs {
    /// Max number of items to show
    #[arg(long, default_value = "20")]
    limit: usize,

    /// Filter by HTTP method
    #[arg(long)]
    method: Option<String>,

    /// Filter by status code prefix (e.g. 2 for 2xx, 404 for exact)
    #[arg(long)]
    status: Option<String>,
}

pub fn run(args: HistoryArgs) -> Result<()> {
    let history = storage::load_history()?;

    if history.is_empty() {
        println!("{}", "No history yet.".dimmed());
        return Ok(());
    }

    let filtered: Vec<_> = history
        .iter()
        .rev()
        .filter(|item| {
            if let Some(m) = &args.method {
                if !item.method.eq_ignore_ascii_case(m) {
                    return false;
                }
            }
            if let Some(s) = &args.status {
                if !item.status_code.to_string().starts_with(s.as_str()) {
                    return false;
                }
            }
            true
        })
        .take(args.limit)
        .collect();

    if filtered.is_empty() {
        println!("{}", "No matching history items.".dimmed());
        return Ok(());
    }

    for item in filtered {
        let status_str = item.status_code.to_string();
        let colored_status = if item.status_code < 300 {
            status_str.green()
        } else if item.status_code < 400 {
            status_str.yellow()
        } else {
            status_str.red()
        };

        let suffix = match (&item.collection_name, &item.request_name) {
            (Some(col), Some(req)) => format!(" [{}›{}]", col, req).dimmed().to_string(),
            (Some(col), None) => format!(" [{}]", col).dimmed().to_string(),
            _ => String::new(),
        };

        println!(
            "  {} {} {} {}ms{}",
            colored_status,
            item.method.bold(),
            item.url,
            item.duration_ms,
            suffix,
        );
    }

    Ok(())
}
