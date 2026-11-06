mod commands;
mod output;

use anyhow::Result;
use clap::{CommandFactory, Parser, Subcommand};
use clap_complete::{generate, Shell};
use colored::Colorize;

#[derive(Parser)]
#[command(
    name = "hitop",
    about = "HITOP: the API client your terminal, your CI and your AI agent can all drive",
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
    /// Run a collection folder on a schedule: write a GitHub Actions workflow (--github) or print a crontab line (--cron)
    Schedule(commands::schedule::ScheduleArgs),
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
    /// List or clear the cookies HITOP keeps between requests
    #[command(subcommand)]
    Cookies(commands::cookies::CookiesCmd),
    /// View request history
    History(commands::history::HistoryArgs),
    /// Import HITOP or Postman collections, Postman environments and globals, or an OpenAPI spec (`hitop import openapi <file|url>`)
    Import(commands::import_export::ImportArgs),
    /// OpenAPI specs: `hitop openapi sync <file|url> <folder>` updates a folder imported from one
    #[command(subcommand)]
    Openapi(commands::openapi::OpenapiCmd),
    /// Export a collection to a file
    Export(commands::import_export::ExportArgs),
    /// Write a collection's documentation as one HTML page (variables, requests, examples; no credentials)
    Docs(commands::docs::DocsArgs),
    /// The anonymous weekly usage ping: off unless you turn it on
    #[cfg(feature = "telemetry")]
    #[command(subcommand)]
    Telemetry(commands::telemetry::TelemetryCmd),
    /// Open a WebSocket connection: send typed lines or --send messages, print what arrives (exit 1 if it fails or --count isn't reached)
    Ws(commands::ws::WsArgs),
    /// Call a gRPC method (unary or server streaming), or list and describe a server's services (exit 1 on a status other than OK)
    Grpc(commands::grpc::GrpcArgs),
    /// Serve a collection folder (or global storage) to AI tools over MCP (stdio)
    Mcp(commands::mcp::McpArgs),
    /// Share a collection folder with your team, end-to-end encrypted (hitop sync server; secret values never sync)
    #[command(subcommand)]
    Sync(commands::sync::SyncCmd),
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
        // 0 written or printed, 2 usage or config error.
        Cmd::Schedule(args) => {
            return match commands::schedule::run(args) {
                Ok(()) => 0,
                Err(e) => {
                    eprintln!("{} {e:#}", "error:".red().bold());
                    2
                }
            }
        }
        Cmd::Ws(args) => {
            return match commands::ws::run(args) {
                Ok(code) => code,
                Err(e) => {
                    eprintln!("{} {e:#}", "error:".red().bold());
                    1
                }
            }
        }
        Cmd::Sync(cmd) => {
            return match commands::sync::run(cmd) {
                Ok(code) => code,
                Err(e) => {
                    eprintln!("{} {e:#}", "error:".red().bold());
                    2
                }
            }
        }
        Cmd::Grpc(args) => {
            return match commands::grpc::run(args) {
                Ok(code) => code,
                Err(e) => {
                    eprintln!("{} {e:#}", "error:".red().bold());
                    1
                }
            }
        }
        Cmd::Collections(cmd) => commands::collections::run(cmd),
        Cmd::Requests(cmd) => commands::requests::run(cmd),
        Cmd::Variables(cmd) => commands::variables::run(cmd),
        Cmd::Env(cmd) => commands::env::run(cmd),
        Cmd::Globals(cmd) => commands::globals::run(cmd),
        Cmd::Secret(cmd) => commands::secret::run(cmd),
        Cmd::Cookies(cmd) => commands::cookies::run(cmd),
        Cmd::History(args) => commands::history::run(args),
        Cmd::Import(args) => commands::import_export::run_import(args),
        Cmd::Openapi(cmd) => commands::openapi::run(cmd),
        Cmd::Export(args) => commands::import_export::run_export(args),
        Cmd::Docs(args) => commands::docs::run(args),
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
