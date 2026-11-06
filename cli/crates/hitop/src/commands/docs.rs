//! `hitop docs <collection> [-o file]`: the collection's documentation as one self-contained
//! HTML page (hitop-core docs.rs): variables, and each request's method, URL, headers, body,
//! auth kind and saved examples. Credentials that aren't `${variables}` show as ••••.
use std::fs;
use std::path::PathBuf;

use anyhow::{Context, Result};
use clap::Args;
use colored::Colorize;

use super::source::Source;

#[derive(Args)]
pub struct DocsArgs {
    /// A collection folder or a collection name in global storage
    pub collection: String,
    /// Where to write the page (default: <collection name>.html in the current directory); `-` for stdout
    #[arg(short, long)]
    pub out: Option<PathBuf>,
}

pub fn run(args: DocsArgs) -> Result<()> {
    let source = Source::open(&args.collection)?;
    let page = hitop_core::docs::to_html(&source.collection);
    let out = args.out.unwrap_or_else(|| PathBuf::from(format!("{}.html", hitop_core::format::names::slug(&source.collection.name))));
    if out.as_os_str() == "-" {
        print!("{page}");
        return Ok(());
    }
    fs::write(&out, page).with_context(|| format!("writing {}", out.display()))?;
    println!("{} Wrote {} ({} requests)", "✓".green(), out.display().to_string().bold(), source.collection.requests.len());
    Ok(())
}
