//! Agent-driven Mac learning. GitHub stays on the Mac; SQLite stays behind Pi MCP.
pub mod evidence;
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
    /// List the configured Mac repository scopes.
    Repositories,
    /// Capture one PR and return the evidence catalogue.
    Prepare {
        #[arg(long)]
        pr: String,
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
    /// Recall lessons using a few short task/component terms.
    Recall {
        #[arg(long)]
        repository: Option<String>,
        #[arg(long, required = true)]
        query: Vec<String>,
    },
    /// Automatically publish an agent-curated, source-validated batch.
    Remember {
        #[arg(long)]
        revision: String,
        #[arg(long)]
        file: PathBuf,
    },
}

pub async fn run(path: PathBuf, command: MacCommand) -> std::result::Result<(), Box<dyn Error>> {
    let result: Result<()> = async {
        let settings = Settings::load(&path)?;
        if matches!(command, MacCommand::Mcp) {
            return server::serve(settings).await;
        }
        let value: Value = match command {
            MacCommand::Repositories => serde_json::json!({"default_repository":settings.repository,"repositories":settings.repositories()}),
            MacCommand::Prepare { pr } => evidence::prepare(&settings, &GhApi, &pr)?,
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
            MacCommand::Recall { query, repository } => remote::recall(&settings, repository.as_deref(), query).await?,
            MacCommand::Remember { revision, file } => {
                let lessons: Vec<Draft> = serde_json::from_slice(&fs::read(file)?)?;
                evidence::remember(&settings, &GhApi, RememberInput { revision, lessons }).await?
            }
            MacCommand::Mcp => unreachable!(),
        };
        println!("{}", serde_json::to_string_pretty(&value)?);
        Ok(())
    }
    .await;
    result.map_err(|error| error.to_string().into())
}
