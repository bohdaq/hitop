//! `hitop cookies ls|clear`: the cookie jar of a workspace (global storage,
//! a collection, or a folder), and the `--no-cookies` flag.
use anyhow::Result;
use clap::{Args, Subcommand};
use colored::Colorize;

use hitop_core::cookies::{jar_path, now_ms, Jar};
use hitop_core::secrets::GLOBAL_WORKSPACE;

use super::source::Source;

#[derive(Subcommand)]
pub enum CookiesCmd {
    /// List the cookies hitop keeps and sends back
    #[command(alias = "list")]
    Ls(Which),
    /// Delete every cookie of the jar
    Clear(Which),
}

#[derive(Args)]
pub struct Which {
    /// Collection name or folder path (default: the jar `hitop send` uses without -c)
    #[arg(short = 'c', long)]
    collection: Option<String>,
}

#[derive(Args, Clone, Copy, Default)]
pub struct CookieFlags {
    /// Don't send cookies from the jar or keep the ones responses set
    #[arg(long)]
    pub no_cookies: bool,
}

/// The jar's workspace id for an optional collection argument.
fn workspace(collection: Option<&str>) -> Result<String> {
    Ok(match collection {
        Some(c) => Source::open(c)?.workspace(),
        None => GLOBAL_WORKSPACE.into(),
    })
}

/// The jar for `workspace`, unless `--no-cookies`.
pub fn open(workspace: &str, flags: CookieFlags) -> Result<Option<Jar>> {
    if flags.no_cookies {
        return Ok(None);
    }
    Ok(Some(Jar::load(workspace)?))
}

pub fn save(workspace: &str, jar: Option<&Jar>) -> Result<()> {
    match jar {
        Some(j) => j.save(workspace),
        None => Ok(()),
    }
}

pub fn run(cmd: CookiesCmd) -> Result<()> {
    match cmd {
        CookiesCmd::Ls(w) => {
            let ws = workspace(w.collection.as_deref())?;
            let jar = Jar::load(&ws)?;
            let now = now_ms();
            let live: Vec<_> = jar.cookies.iter().filter(|c| c.expires.is_none_or(|e| e > now)).collect();
            if live.is_empty() {
                println!("{}", "No cookies.".dimmed());
            }
            for c in live {
                let expires = match c.expires {
                    Some(ms) => format!("expires in {}", human(ms - now)),
                    None => "session".into(),
                };
                let secure = if c.secure { ", https only" } else { "" };
                println!("{} {}={} {}", c.domain.bold(), c.name, c.value, format!("(path {}{secure}, {expires})", c.path).dimmed());
            }
            println!("{}", format!("Jar: {}", jar_path(&ws)?.display()).dimmed());
        }
        CookiesCmd::Clear(w) => {
            let ws = workspace(w.collection.as_deref())?;
            let jar = Jar::load(&ws)?;
            Jar::default().save(&ws)?;
            println!("{} Deleted {} cookie(s)", "✓".green(), jar.cookies.len());
        }
    }
    Ok(())
}

fn human(ms: i64) -> String {
    let s = ms / 1000;
    match s {
        s if s < 120 => format!("{s} s"),
        s if s < 7200 => format!("{} min", s / 60),
        s if s < 172_800 => format!("{} h", s / 3600),
        s => format!("{} days", s / 86_400),
    }
}
