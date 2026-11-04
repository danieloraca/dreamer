use crate::repository;
use serde::Deserialize;
use std::{error::Error, fs, net::SocketAddr, path::Path, path::PathBuf};

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Config {
    pub data_dir: PathBuf,
    pub github: GithubConfig,
    pub memory: MemoryConfig,
    #[serde(default)]
    pub http: HttpConfig,
}

#[derive(Debug, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct HttpConfig {
    pub listen: SocketAddr,
    /// Explicit DNS names or IP addresses; bracket IPv6 literals.
    pub allowed_hosts: Vec<String>,
}

impl Default for HttpConfig {
    fn default() -> Self {
        Self {
            listen: "127.0.0.1:3445".parse().expect("static listen address"),
            allowed_hosts: vec!["localhost".into(), "127.0.0.1".into(), "[::1]".into()],
        }
    }
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct GithubConfig {
    pub repository: String,
    /// Complete allowlist; omission preserves the legacy single-repository scope.
    #[serde(default)]
    pub repositories: Vec<String>,
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
        repository::validate(&config.github.repository, &config.github.repositories)?;
        if !(1..=100).contains(&config.github.batch_size) {
            return Err("github.batch_size must be between 1 and 100".into());
        }
        if !(1..=100).contains(&config.memory.default_search_limit) {
            return Err("memory.default_search_limit must be between 1 and 100".into());
        }
        if config.http.allowed_hosts.is_empty() {
            return Err("http.allowed_hosts must contain an explicit hostname or IP".into());
        }
        for host in &config.http.allowed_hosts {
            let authority: axum::http::uri::Authority = host
                .parse()
                .map_err(|_| "http.allowed_hosts entries must be hostnames or IPs")?;
            if authority.port().is_some()
                || authority.host() != host
                || host.contains('*')
                || host.contains('@')
                || host.trim() != host
            {
                return Err(
                    "http.allowed_hosts entries must be exact hosts without ports or wildcards"
                        .into(),
                );
            }
        }
        Ok(config)
    }
}
