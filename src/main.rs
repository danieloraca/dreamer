use clap::{Parser, Subcommand};
use dreamer::{
    collector::{self, CollectOptions, github::GhApi},
    config::Config,
    lesson::Lesson,
    lifecycle::{HistoryQuery, LessonChange},
    mcp,
    retrieval::RecallQuery,
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
    /// Mac-only agent tools: recall knowledge, capture PRs and remember sourced lessons.
    #[cfg(feature = "mac-client")]
    Mac {
        #[arg(long)]
        settings: PathBuf,
        #[command(subcommand)]
        command: dreamer::mac::MacCommand,
    },
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
        /// Repository to use; defaults to github.repository and must be allowlisted.
        #[arg(long, global = true)]
        repository: Option<String>,
        #[command(subcommand)]
        command: LessonCommand,
    },
    /// Reserved: GitHub ingestion (not implemented; future Mac component).
    Ingest,
    /// Reserved: unattended consolidation (not implemented).
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
    /// Read-only schema/integrity check; --file needs no service config.
    Check {
        #[arg(long)]
        file: Option<PathBuf>,
        #[arg(long)]
        allow_older: bool,
    },
    /// Initialize SQLite and apply pending versioned migrations.
    Init,
}

#[derive(Subcommand)]
enum LessonCommand {
    /// Rank active lessons by task terms and optional component/file context.
    Recall {
        #[arg(long, required = true)]
        query: Vec<String>,
        #[arg(long)]
        component: Vec<String>,
        #[arg(long)]
        path: Vec<String>,
        #[arg(long, default_value_t = 5, value_parser = clap::value_parser!(u32).range(1..=20))]
        limit: u32,
    },
    /// Append a source-backed supersede, retire or conflict event to an existing database.
    Change {
        #[arg(long)]
        file: PathBuf,
    },
    /// Read immutable lifecycle history, including incoming replacements.
    History {
        #[arg(long)]
        id: String,
        #[arg(long, default_value_t = 20)]
        limit: usize,
        #[arg(long, default_value_t = 0)]
        offset: usize,
    },
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
    #[cfg(feature = "mac-client")]
    if let Command::Mac { settings, command } = cli.command {
        dreamer::mac::run(settings, command).await?;
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
    if let Command::Db {
        command: DbCommand::Check {
            file: Some(file),
            allow_older,
        },
    } = &cli.command
    {
        let schema = SqliteStore::check_file(file, *allow_older)?;
        println!("{}", serde_json::json!({"schema":schema,"integrity":"ok"}));
        return Ok(ExitCode::SUCCESS);
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
                    "repositories": dreamer::repository::configured(&config.github.repository, &config.github.repositories),
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
            println!(
                "Consolidation: evidence-backed lifecycle tools; unattended scanning is not implemented"
            );
            println!("MCP: stdio (read-only by default); authenticated HTTP with serve");
        }
        Command::Db {
            command: DbCommand::Check { file, allow_older },
        } => {
            let schema =
                SqliteStore::check_file(file.as_deref().unwrap_or(&database), allow_older)?;
            println!("{}", serde_json::json!({"schema":schema,"integrity":"ok"}));
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
        Command::Lesson {
            command,
            repository,
        } => {
            let selected = dreamer::repository::select(
                &config.github.repository,
                &config.github.repositories,
                repository.as_deref(),
            )?;
            match command {
                LessonCommand::Recall {
                    query,
                    component,
                    path,
                    limit,
                } => {
                    let result = SqliteStore::open_read_only(&database)?.recall(&RecallQuery {
                        repository: selected,
                        queries: query,
                        components: component,
                        paths: path,
                        limit: limit as usize,
                    })?;
                    println!("{}", serde_json::to_string_pretty(&result)?);
                }
                LessonCommand::Change { file } => {
                    let change: LessonChange = serde_json::from_slice(&fs::read(file)?)?;
                    change.validate()?;
                    if change.repository != selected {
                        return Err("change repository does not match selected scope".into());
                    }
                    let (outcome, record) =
                        SqliteStore::open_existing(&database)?.change_lesson(&change)?;
                    println!(
                        "{}",
                        serde_json::to_string_pretty(
                            &serde_json::json!({"outcome":match outcome {WriteOutcome::Inserted=>"applied",WriteOutcome::Unchanged=>"unchanged"},"record":record})
                        )?
                    );
                }
                LessonCommand::History { id, limit, offset } => {
                    let page =
                        SqliteStore::open_read_only(&database)?.lesson_history(&HistoryQuery {
                            repository: selected,
                            id,
                            change_id: None,
                            limit,
                            offset,
                        })?;
                    println!("{}", serde_json::to_string_pretty(&page)?);
                }
                LessonCommand::Add { file } => {
                    let lesson: Lesson = serde_json::from_str(&fs::read_to_string(file)?)?;
                    lesson.validate_candidate()?;
                    if lesson.repository != selected {
                        return Err(
                            "lesson repository does not match the configured repository scope"
                                .into(),
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
                        .get(&selected, &id)?
                        .ok_or("lesson not found in this repository")?;
                    println!("{}", serde_json::to_string_pretty(&lesson)?);
                }
                LessonCommand::Search { query, limit } => {
                    let store = SqliteStore::open_read_only(&database)?;
                    let lessons = store.search(&SearchQuery {
                        repository: selected,
                        text: query,
                        limit: limit
                            .map(|value| value as usize)
                            .unwrap_or(config.memory.default_search_limit),
                    })?;
                    println!("{}", serde_json::to_string_pretty(&lessons)?);
                }
            }
        }
        Command::Mcp { allow_writes } => mcp::serve_stdio(&config, allow_writes).await?,
        Command::Serve { allow_writes } => mcp::http::serve(&config, allow_writes).await?,
        Command::Ingest | Command::Consolidate => unreachable!(),
        Command::Collect { .. } => unreachable!(),
        #[cfg(feature = "mac-client")]
        Command::Mac { .. } => unreachable!(),
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
