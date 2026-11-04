use clap::{Parser, Subcommand};
use dreamer::{config::Config, mcp::PLANNED_TOOLS};
use std::{path::PathBuf, process::ExitCode};

#[derive(Parser)]
#[command(version, about = "Dreamer: evidence-backed agent knowledge (skeleton)")]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    /// Validate configuration and report implementation status; performs no writes.
    Status {
        #[arg(long, default_value = "dreamer.toml")]
        config: PathBuf,
    },
    /// Reserved: read merged pull requests from GitHub (not implemented).
    Ingest,
    /// Reserved: propose evidence-backed lessons (not implemented).
    Consolidate,
    /// Reserved: serve lesson tools over MCP (not implemented).
    Mcp,
}

fn main() -> ExitCode {
    match Cli::parse().command {
        Command::Status { config } => match Config::load(&config) {
            Ok(config) => {
                println!("Dreamer {} — skeleton", env!("CARGO_PKG_VERSION"));
                println!("Configuration: valid");
                println!("Repository: {}", config.github.repository);
                println!(
                    "Data directory (not created): {}",
                    config.data_dir.display()
                );
                println!("GitHub ingestion: not implemented");
                println!("Consolidation and persistent storage: not implemented");
                println!(
                    "MCP: not implemented (planned tools: {})",
                    PLANNED_TOOLS.join(", ")
                );
                ExitCode::SUCCESS
            }
            Err(error) => {
                eprintln!("Configuration error: {error}");
                ExitCode::from(1)
            }
        },
        command => {
            let name = match command {
                Command::Ingest => "ingest",
                Command::Consolidate => "consolidate",
                Command::Mcp => "mcp",
                Command::Status { .. } => unreachable!(),
            };
            eprintln!("{name}: not implemented in this skeleton; no work performed");
            ExitCode::from(2)
        }
    }
}
