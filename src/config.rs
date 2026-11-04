use serde::Deserialize;
use std::{error::Error, fs, path::Path, path::PathBuf};

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Config {
    pub data_dir: PathBuf,
    pub github: GithubConfig,
    pub memory: MemoryConfig,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct GithubConfig {
    pub repository: String,
    pub batch_size: usize,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MemoryConfig {
    pub default_search_limit: usize,
}

impl Config {
    /// Reads configuration only; does not create directories or contact services.
    pub fn load(path: &Path) -> Result<Self, Box<dyn Error>> {
        let contents = fs::read_to_string(path)?;
        let mut config = Self::parse(&contents)?;
        if config.data_dir.is_relative() {
            config.data_dir = path
                .parent()
                .unwrap_or(Path::new("."))
                .join(config.data_dir);
        }
        Ok(config)
    }

    pub fn parse(contents: &str) -> Result<Self, Box<dyn Error>> {
        let config: Self = toml::from_str(contents)?;
        if config.data_dir.as_os_str().is_empty() {
            return Err("data_dir must not be empty".into());
        }
        let parts: Vec<_> = config.github.repository.split('/').collect();
        let valid_component = |part: &&str| {
            !part.is_empty()
                && *part != "."
                && *part != ".."
                && part
                    .bytes()
                    .all(|c| c.is_ascii_alphanumeric() || b"-_.".contains(&c))
        };
        if parts.len() != 2 || !parts.iter().all(valid_component) {
            return Err("github.repository must use owner/repository syntax".into());
        }
        if !(1..=100).contains(&config.github.batch_size) {
            return Err("github.batch_size must be between 1 and 100".into());
        }
        if !(1..=100).contains(&config.memory.default_search_limit) {
            return Err("memory.default_search_limit must be between 1 and 100".into());
        }
        Ok(config)
    }
}
