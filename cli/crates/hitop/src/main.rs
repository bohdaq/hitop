mod commands;
mod output;

use anyhow::Result;
use clap::{CommandFactory, Parser, Subcommand};
use clap_complete::{generate, Shell};
use colored::Colorize;

#[derive(Parser)]
#[command(
    name = "hitop",
    about = "HTTP API client — send requests, run collections, and script with JS from the terminal",
    version
)]
struct Cli {
    #[command(subcommand)]
    command: Cmd,
}

#[derive(Subcommand)]
enum Cmd {
    /// Create a collection folder (hitop.yaml, requests/, environments/)
    Init(commands::folder::InitArgs),
    /// Convert a HITOP JSON export to a collection folder, or back
    Convert(commands::folder::ConvertArgs),
    /// Send a single HTTP request
    Send(commands::send::SendArgs),
    /// Run a collection or all collections, with reports for CI (exit codes: 0 passed, 1 failed, 2 usage or config error)
    Run(commands::run::RunArgs),
    /// Print a saved request as code: curl, fetch, python, go, java or rust (secrets as {{NAME}})
    Snippet(commands::snippet::SnippetArgs),
    /// Manage collections
    #[command(subcommand)]
    Collections(commands::collections::CollectionsCmd),
    /// Manage requests within a collection
    #[command(subcommand)]
    Requests(commands::requests::RequestsCmd),
    /// Manage collection variables
    #[command(subcommand)]
    Variables(commands::variables::VariablesCmd),
    /// Manage environments (variable sets such as dev, staging, prod)
    #[command(subcommand)]
    Env(commands::env::EnvCmd),
    /// Manage the globals: variables below every collection's (globals.json, or a folder's globals.yaml)
    #[command(subcommand)]
    Globals(commands::globals::GlobalsCmd),
    /// Manage secret variables in the OS keychain
    #[command(subcommand)]
    Secret(commands::secret::SecretCmd),
    /// List or clear the cookies hitop keeps between requests
    #[command(subcommand)]
    Cookies(commands::cookies::CookiesCmd),
    /// View request history
    History(commands::history::HistoryArgs),
    /// Import HITOP or Postman collections, Postman environments and globals
    Import(commands::import_export::ImportArgs),
    /// Export a collection to a file
    Export(commands::import_export::ExportArgs),
    /// The anonymous weekly usage ping: off unless you turn it on
    #[cfg(feature = "telemetry")]
    #[command(subcommand)]
    Telemetry(commands::telemetry::TelemetryCmd),
    /// Serve a collection folder (or global storage) to AI tools over MCP (stdio)
    Mcp(commands::mcp::McpArgs),
    /// Generate shell completions
    Completions {
        #[arg(value_enum)]
        shell: Shell,
    },
}

fn main() {
    let cli = Cli::parse();
    #[cfg(feature = "telemetry")]
    let ping = match &cli.command {
        Cmd::Telemetry(_) | Cmd::Completions { .. } => None,
        _ => commands::telemetry::start(),
    };
    let code = dispatch(cli.command);
    #[cfg(feature = "telemetry")]
    commands::telemetry::finish(ping);
    let _ = std::io::Write::flush(&mut std::io::stdout());
    std::process::exit(code)
}

/// Runs the command; the process exit code.
fn dispatch(command: Cmd) -> i32 {
    let result: Result<()> = match command {
        Cmd::Init(args) => commands::folder::run_init(args),
        Cmd::Convert(args) => commands::folder::run_convert(args),
        Cmd::Send(args) => commands::send::run(args),
        // 0 passed, 1 a request or assertion failed, 2 usage or config error.
        Cmd::Run(args) => {
            return match commands::run::run(args) {
                Ok(code) => code,
                Err(e) => {
                    eprintln!("{} {e:#}", "error:".red().bold());
                    2
                }
            }
        }
        Cmd::Snippet(args) => commands::snippet::run(args),
        Cmd::Collections(cmd) => commands::collections::run(cmd),
        Cmd::Requests(cmd) => commands::requests::run(cmd),
        Cmd::Variables(cmd) => commands::variables::run(cmd),
        Cmd::Env(cmd) => commands::env::run(cmd),
        Cmd::Globals(cmd) => commands::globals::run(cmd),
        Cmd::Secret(cmd) => commands::secret::run(cmd),
        Cmd::Cookies(cmd) => commands::cookies::run(cmd),
        Cmd::History(args) => commands::history::run(args),
        Cmd::Import(args) => commands::import_export::run_import(args),
        Cmd::Export(args) => commands::import_export::run_export(args),
        Cmd::Mcp(args) => commands::mcp::run(args),
        #[cfg(feature = "telemetry")]
        Cmd::Telemetry(cmd) => commands::telemetry::run(cmd),
        Cmd::Completions { shell } => {
            let mut cmd = Cli::command();
            generate(shell, &mut cmd, "hitop", &mut std::io::stdout());
            Ok(())
        }
    };
    match result {
        Ok(()) => 0,
        // What returning the error from main printed before.
        Err(e) => {
            eprintln!("Error: {e:?}");
            1
        }
    }
}
