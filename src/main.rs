use clap::{Parser, Subcommand};
use dreamer::{
    collector::{self, CollectOptions, github::GhApi},
    config::Config,
    lesson::Lesson,
    mcp,
    store::{LessonStore, SearchQuery, WriteOutcome, sqlite::SqliteStore},
};
use std::{error::Error, fs, path::PathBuf, process::ExitCode};

#[derive(Parser)]
#[command(
    version,
    about = "Dreamer: a local evidence-backed knowledge foundation"
)]
struct Cli {
    #[arg(long, global = true, default_value = "dreamer.toml")]
    config: PathBuf,
    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    /// Collect merged PR evidence and unverified draft prompts locally; never publishes.
    Collect {
        #[arg(long)]
        repository: String,
        #[arg(long, default_value_t = 10, value_parser = clap::value_parser!(u32).range(1..=50))]
        count: u32,
        #[arg(long, default_value_t = 100, value_parser = clap::value_parser!(u32).range(1..=1000))]
        scan_limit: u32,
        /// Exclude this PR and use its creation time as the training cutoff.
        #[arg(long)]
        holdout: Option<u64>,
        /// Optional RFC3339 merge/review cutoff, such as 2026-11-03T15:40:15Z.
        #[arg(long)]
        before: Option<chrono::DateTime<chrono::Utc>>,
        /// New output directory. Existing batches are never overwritten.
        #[arg(long)]
        output: PathBuf,
    },
    /// Validate configuration and report capabilities; performs no writes.
    Status {
        /// Print deployment metadata as JSON; never includes credentials.
        #[arg(long)]
        json: bool,
    },
    /// Create or migrate the local knowledge database.
    Db {
        #[command(subcommand)]
        command: DbCommand,
    },
    /// Add, retrieve and search local lessons. No network or model calls.
    Lesson {
        #[command(subcommand)]
        command: LessonCommand,
    },
    /// Reserved: GitHub ingestion (not implemented; future Mac component).
    Ingest,
    /// Reserved: propose lessons using a model (not implemented).
    Consolidate,
    /// Serve MCP over stdio. Requires an existing, current database.
    Mcp {
        /// Expose candidate submission in addition to read tools.
        #[arg(long)]
        allow_writes: bool,
    },
    /// Run the always-on authenticated HTTP MCP server.
    Serve {
        /// Enable a separate write token for candidate submission.
        #[arg(long)]
        allow_writes: bool,
    },
}

#[derive(Subcommand)]
enum DbCommand {
    /// Initialize SQLite and apply pending versioned migrations.
    Init,
}

#[derive(Subcommand)]
enum LessonCommand {
    /// Import a candidate JSON file for the configured repository; identical retries are no-ops.
    Add {
        #[arg(long)]
        file: PathBuf,
    },
    /// Return a lesson with evidence, provenance and its storage timestamp as JSON.
    Get {
        #[arg(long)]
        id: String,
    },
    /// Search summary/applicability by literal substring; excludes deprecated lessons.
    Search {
        #[arg(long)]
        query: String,
        #[arg(long, value_parser = clap::value_parser!(u32).range(1..=100))]
        limit: Option<u32>,
    },
}

async fn run(cli: Cli) -> Result<ExitCode, Box<dyn Error>> {
    if let Command::Collect {
        repository,
        count,
        scan_limit,
        holdout,
        before,
        output,
    } = cli.command
    {
        let report = collector::collect(
            &GhApi,
            &CollectOptions {
                repository,
                count: count as usize,
                scan_limit: scan_limit as usize,
                holdout,
                before,
                output,
            },
        )?;
        println!("Local preview: {} (not published)", report.display());
        return Ok(ExitCode::SUCCESS);
    }
    let reserved = match &cli.command {
        Command::Ingest => Some("ingest"),
        Command::Consolidate => Some("consolidate"),
        _ => None,
    };
    if let Some(name) = reserved {
        eprintln!("{name}: not implemented; no work performed");
        return Ok(ExitCode::from(2));
    }
    let config = Config::load(&cli.config)?;
    let database = config.data_dir.join("knowledge.sqlite3");
    match cli.command {
        Command::Status { json: true } => {
            println!(
                "{}",
                serde_json::json!({
                    "data_dir": config.data_dir,
                    "repository": config.github.repository,
                    "http": {"listen": config.http.listen.to_string(), "allowed_hosts": config.http.allowed_hosts}
                })
            );
        }
        Command::Status { json: false } => {
            println!(
                "Dreamer {} — knowledge foundation",
                env!("CARGO_PKG_VERSION")
            );
            println!("Configuration: valid");
            println!("Repository scope: {}", config.github.repository);
            println!(
                "Data directory (not created): {}",
                config.data_dir.display()
            );
            println!("Persistent storage: SQLite (database not opened by status)");
            println!(
                "GitHub collection: manual dry-run with collect; automatic ingestion: not implemented"
            );
            println!("Consolidation: not implemented");
            println!("MCP: stdio (read-only by default); authenticated HTTP with serve");
        }
        Command::Db {
            command: DbCommand::Init,
        } => {
            fs::create_dir_all(&config.data_dir)?;
            let store = SqliteStore::open(&database)?;
            println!(
                "Database: {} (schema {})",
                database.display(),
                store.schema_version()?
            );
        }
        Command::Lesson { command } => match command {
            LessonCommand::Add { file } => {
                let lesson: Lesson = serde_json::from_str(&fs::read_to_string(file)?)?;
                lesson.validate_candidate()?;
                if lesson.repository != config.github.repository {
                    return Err(
                        "lesson repository does not match the configured repository scope".into(),
                    );
                }
                fs::create_dir_all(&config.data_dir)?;
                let mut store = SqliteStore::open(&database)?;
                let outcome = match store.put_candidate(&lesson)? {
                    WriteOutcome::Inserted => "Inserted",
                    WriteOutcome::Unchanged => "Unchanged",
                };
                println!("{outcome} candidate {}/{}", lesson.repository, lesson.id);
            }
            LessonCommand::Get { id } => {
                let store = SqliteStore::open_read_only(&database)?;
                let lesson = store
                    .get(&config.github.repository, &id)?
                    .ok_or("lesson not found in this repository")?;
                println!("{}", serde_json::to_string_pretty(&lesson)?);
            }
            LessonCommand::Search { query, limit } => {
                let store = SqliteStore::open_read_only(&database)?;
                let lessons = store.search(&SearchQuery {
                    repository: config.github.repository,
                    text: query,
                    limit: limit
                        .map(|value| value as usize)
                        .unwrap_or(config.memory.default_search_limit),
                })?;
                println!("{}", serde_json::to_string_pretty(&lessons)?);
            }
        },
        Command::Mcp { allow_writes } => mcp::serve_stdio(&config, allow_writes).await?,
        Command::Serve { allow_writes } => mcp::http::serve(&config, allow_writes).await?,
        Command::Ingest | Command::Consolidate => unreachable!(),
        Command::Collect { .. } => unreachable!(),
    }
    Ok(ExitCode::SUCCESS)
}

#[tokio::main(flavor = "multi_thread", worker_threads = 2)]
async fn main() -> ExitCode {
    match run(Cli::parse()).await {
        Ok(code) => code,
        Err(error) => {
            eprintln!("Dreamer error: {error}");
            ExitCode::from(1)
        }
    }
}
