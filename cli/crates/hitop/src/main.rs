mod commands;
mod output;

use anyhow::Result;
use clap::{CommandFactory, Parser, Subcommand};
use clap_complete::{generate, Shell};

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
    /// Send a single HTTP request
    Send(commands::send::SendArgs),
    /// Run a collection or all collections
    Run(commands::run::RunArgs),
    /// Manage collections
    #[command(subcommand)]
    Collections(commands::collections::CollectionsCmd),
    /// Manage requests within a collection
    #[command(subcommand)]
    Requests(commands::requests::RequestsCmd),
    /// Manage collection variables
    #[command(subcommand)]
    Variables(commands::variables::VariablesCmd),
    /// View request history
    History(commands::history::HistoryArgs),
    /// Import collections from a file
    Import(commands::import_export::ImportArgs),
    /// Export a collection to a file
    Export(commands::import_export::ExportArgs),
    /// Generate shell completions
    Completions {
        #[arg(value_enum)]
        shell: Shell,
    },
}

fn main() -> Result<()> {
    let cli = Cli::parse();

    match cli.command {
        Cmd::Send(args) => commands::send::run(args),
        Cmd::Run(args) => commands::run::run(args),
        Cmd::Collections(cmd) => commands::collections::run(cmd),
        Cmd::Requests(cmd) => commands::requests::run(cmd),
        Cmd::Variables(cmd) => commands::variables::run(cmd),
        Cmd::History(args) => commands::history::run(args),
        Cmd::Import(args) => commands::import_export::run_import(args),
        Cmd::Export(args) => commands::import_export::run_export(args),
        Cmd::Completions { shell } => {
            let mut cmd = Cli::command();
            generate(shell, &mut cmd, "hitop", &mut std::io::stdout());
            Ok(())
        }
    }
}
