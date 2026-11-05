//! Agent-driven Mac learning. GitHub stays on the Mac; SQLite stays behind Pi MCP.
pub mod activity;
pub mod conversations;
pub mod evidence;
pub mod reconciliation;
mod remote;
mod server;

use crate::collector::{self, github::GhApi};
use clap::Subcommand;
pub use evidence::{Draft, PrepareInput, ReadInput, RecallInput, RememberInput};
use serde::Deserialize;
use serde_json::Value;
use std::{
    error::Error,
    fs,
    path::{Path, PathBuf},
};
pub type Result<T> = std::result::Result<T, Box<dyn Error + Send + Sync>>;

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Settings {
    pub repository: String,
    #[serde(default)]
    pub repositories: Vec<String>,
    pub endpoint: String,
    pub writer_token_file: PathBuf,
    pub cache_dir: PathBuf,
    #[serde(default)]
    pub learn_conversations: bool,
}
impl Settings {
    pub fn load(path: &Path) -> Result<Self> {
        let mut settings: Self = toml::from_str(&fs::read_to_string(path)?)?;
        crate::repository::validate(&settings.repository, &settings.repositories)?;
        let url = url::Url::parse(&settings.endpoint)?;
        if url.scheme() != "http"
            || url.host_str().is_none()
            || !url.username().is_empty()
            || url.password().is_some()
            || url.path() != "/mcp"
            || url.query().is_some()
            || url.fragment().is_some()
        {
            return Err(
                "Mac endpoint must be the explicit private HTTP /mcp URL without URL credentials"
                    .into(),
            );
        }
        let parent = path
            .canonicalize()?
            .parent()
            .ok_or("settings need a parent directory")?
            .to_owned();
        for value in [&mut settings.writer_token_file, &mut settings.cache_dir] {
            if value.is_relative() {
                *value = parent.join(&*value);
            }
        }
        Ok(settings)
    }
    pub fn repositories(&self) -> Vec<String> {
        crate::repository::configured(&self.repository, &self.repositories)
    }
    pub fn select_repository(&self, requested: Option<&str>) -> Result<String> {
        Ok(crate::repository::select(
            &self.repository,
            &self.repositories,
            requested,
        )?)
    }
    fn cache(&self) -> Result<()> {
        if !self.cache_dir.exists() {
            fs::create_dir_all(self.cache_dir.parent().ok_or("cache needs a parent")?)?;
            collector::private_directory(&self.cache_dir)
                .map_err(|_| "could not create private Mac cache")?;
            collector::write_private(&self.cache_dir.join(".gitignore"), b"*\n")
                .map_err(|_| "could not protect Mac cache")?;
        }
        #[cfg(unix)]
        {
            use std::os::unix::fs::{MetadataExt, PermissionsExt};
            let metadata = fs::symlink_metadata(&self.cache_dir)?;
            if !metadata.is_dir()
                || metadata.permissions().mode() & 0o077 != 0
                || metadata.uid() != fs::metadata(&self.writer_token_file)?.uid()
            {
                return Err(
                    "Mac cache must be a private directory owned by the credential owner".into(),
                );
            }
        }
        Ok(())
    }
}

#[derive(Subcommand)]
pub enum MacCommand {
    /// Run on-demand Mac MCP tools over stdio; no polling or model API calls.
    Mcp,
    /// Capture one explicitly selected decoded Codex thread page (no crawling).
    PrepareChat {
        #[arg(long)]
        file: PathBuf,
    },
    /// Inspect sanitized chat source IDs before extracting lessons.
    ReadChat {
        #[arg(long)]
        repository: String,
        #[arg(long)]
        revision: String,
        #[arg(long, required = true)]
        id: Vec<String>,
    },
    /// Assess inspected chat drafts against current Pi knowledge, without publishing.
    AssessChat {
        #[arg(long)]
        repository: String,
        #[arg(long)]
        revision: String,
        #[arg(long)]
        file: PathBuf,
    },
    /// Preview source-checked chat lessons locally, without contacting the Pi.
    PreviewChat {
        #[arg(long)]
        repository: String,
        #[arg(long)]
        revision: String,
        #[arg(long)]
        file: PathBuf,
    },
    /// Publish source-checked chat lessons, or finish a justified no-new page.
    RememberChat {
        #[arg(long)]
        repository: String,
        #[arg(long)]
        revision: String,
        #[arg(
            long,
            conflicts_with = "skip_reason",
            required_unless_present = "skip_reason"
        )]
        file: Option<PathBuf>,
        #[arg(long, conflicts_with = "file")]
        skip_reason: Option<String>,
    },
    /// Inspect redacted local learning reports; optionally flush queued reports.
    Activity {
        #[arg(long)]
        sync: bool,
    },
    /// List the configured Mac repository scopes.
    Repositories,
    /// Capture one PR and return the evidence catalogue.
    Prepare {
        #[arg(long)]
        pr: String,
        #[arg(long)]
        force: bool,
    },
    /// Read captured evidence. Without IDs, returns a paginated catalogue.
    Read {
        #[arg(long)]
        revision: String,
        #[arg(long)]
        id: Vec<String>,
        #[arg(long, default_value_t = 0)]
        offset: usize,
    },
    /// Correct existing knowledge with inspected conversation evidence; omit file to resume.
    ReconcileChat {
        #[arg(long)]
        repository: String,
        #[arg(long)]
        revision: String,
        #[arg(long)]
        file: Option<PathBuf>,
    },
    /// Recall lessons using a few short task/component terms.
    Recall {
        #[arg(long)]
        repository: Option<String>,
        #[arg(long, required = true)]
        query: Vec<String>,
        #[arg(long)]
        component: Vec<String>,
        #[arg(long)]
        path: Vec<String>,
        #[arg(long, default_value_t = 5)]
        limit: usize,
    },
    /// Fetch the full stored record for one selected lesson.
    Get {
        #[arg(long)]
        repository: Option<String>,
        #[arg(long)]
        id: String,
    },
    /// Reconcile sourced existing lessons; omit file to resume a saved pending batch.
    Reconcile {
        #[arg(long)]
        revision: String,
        #[arg(long)]
        file: Option<PathBuf>,
    },
    /// Automatically publish an agent-curated, source-validated batch.
    Remember {
        #[arg(long)]
        revision: String,
        #[arg(
            long,
            conflicts_with = "skip_reason",
            required_unless_present = "skip_reason"
        )]
        file: Option<PathBuf>,
        #[arg(long, conflicts_with = "file")]
        skip_reason: Option<String>,
    },
}

pub async fn run(path: PathBuf, command: MacCommand) -> std::result::Result<(), Box<dyn Error>> {
    let result: Result<()> = async {
        let settings = Settings::load(&path)?;
        if matches!(command, MacCommand::Mcp) {
            return server::serve(settings).await;
        }
        let value: Value = match command {
            MacCommand::PrepareChat {file} => {
                let input: conversations::PrepareChatInput=serde_json::from_slice(&fs::read(file)?)?;
                let context=serde_json::json!({"repository":input.repository,"thread_id":input.thread_id});
                activity::track(&settings,"prepare_chat_learning",&context,async {conversations::prepare(&settings,input)}).await?
            }
            MacCommand::ReadChat {repository,revision,id} => conversations::read(&settings,conversations::ChatReadInput{repository,revision,ids:id})?,
            MacCommand::AssessChat {repository,revision,file} => {
                let lessons=serde_json::from_slice(&fs::read(file)?)?;
                conversations::assess_lessons(&settings,conversations::RememberChatInput{repository,revision,lessons,skip_reason:None}).await?
            }
            MacCommand::PreviewChat {repository,revision,file} => {
                let drafts: Vec<conversations::ChatDraft>=serde_json::from_slice(&fs::read(file)?)?;
                serde_json::to_value(conversations::build(&settings,&repository,&revision,&drafts)?)?
            }
            MacCommand::RememberChat {repository,revision,file,skip_reason} => {
                let lessons=match file {Some(file)=>serde_json::from_slice(&fs::read(file)?)?,None=>Vec::new()};
                let context=serde_json::json!({"repository":repository,"revision":revision});
                activity::track(&settings,"remember_chat_lessons",&context,async {conversations::remember(&settings,conversations::RememberChatInput{repository,revision,lessons,skip_reason}).await}).await?
            }
            MacCommand::ReconcileChat {repository,revision,file} => {
                let changes=match file {Some(file)=>serde_json::from_slice(&fs::read(file)?)?,None=>Vec::new()};
                let context=serde_json::json!({"repository":repository,"revision":revision});
                activity::track(&settings,"reconcile_chat_lessons",&context,async {conversations::reconcile(&settings,conversations::ReconcileChatInput{repository,revision,changes}).await}).await?
            }
            MacCommand::Activity {sync}=>{
                let mut value=activity::local_summary(&settings);
                if sync {value["activity_reporting"]=serde_json::json!(activity::flush(&settings).await);}
                value
            }
            MacCommand::Repositories => serde_json::json!({"default_repository":settings.repository,"repositories":settings.repositories()}),
            MacCommand::Prepare { pr, force } => activity::track(&settings,"prepare_pr_learning",&serde_json::json!({"pr":pr,"force":force}),async {evidence::prepare_with_force(&settings,&GhApi,&pr,force)}).await?,
            MacCommand::Read {
                revision,
                id,
                offset,
            } => evidence::read(
                &settings,
                &ReadInput {
                    revision,
                    ids: id,
                    offset,
                    kind: None,
                    path: None,
                },
            )?,
            MacCommand::Recall { query, repository, limit, component, path } => remote::recall(&settings, repository.as_deref(), query, limit, component, path).await?,
            MacCommand::Get { repository, id } => remote::get(&settings, repository.as_deref(), &id).await?,
            MacCommand::Reconcile { revision, file } => {
                let changes=match file {Some(file)=>serde_json::from_slice(&fs::read(file)?)?,None=>Vec::new()};
                activity::track(&settings,"reconcile_pr_lessons",&serde_json::json!({"revision":revision,"changes":changes}),async {reconciliation::reconcile(&settings,&GhApi,reconciliation::ReconcileInput {revision,changes}).await}).await?
            }
            MacCommand::Remember { revision, file, skip_reason } => {
                let lessons: Vec<Draft> = match file {
                    Some(file) => serde_json::from_slice(&fs::read(file)?)?,
                    None => Vec::new(),
                };
                activity::track(&settings,"remember_pr_lessons",&serde_json::json!({"revision":revision,"lessons":lessons,"skip_reason":skip_reason}),async {evidence::remember(&settings,&GhApi,RememberInput {revision,lessons,skip_reason}).await}).await?
            }
            MacCommand::Mcp => unreachable!(),
        };
        println!("{}", serde_json::to_string_pretty(&value)?);
        Ok(())
    }
    .await;
    result.map_err(|error| error.to_string().into())
}
