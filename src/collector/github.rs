//! Read-only GitHub CLI adapter. Credentials stay with gh on the Mac.
use super::Result;
use serde_json::Value;
use std::process::Command;

pub trait GitHubApi {
    fn json(&self, path: &str) -> Result<Value>;
    fn diff(&self, path: &str) -> Result<String>;
}

pub struct GhApi;

impl GhApi {
    fn request(&self, path: &str, accept: &str) -> Result<Vec<u8>> {
        let output = Command::new("gh")
            .args([
                "api",
                "--hostname",
                "github.com",
                "--method",
                "GET",
                "-H",
                accept,
                "-H",
                "X-GitHub-Api-Version: 2026-03-10",
                path,
            ])
            .env_remove("GH_DEBUG")
            .output()
            .map_err(|_| "could not run gh; install GitHub CLI and sign in on the Mac")?;
        if !output.status.success() {
            // Do not echo subprocess output: errors can contain private content.
            return Err(format!("GitHub GET failed for {path}; check gh auth status and rate limits (no partial batch will be marked complete)").into());
        }
        Ok(output.stdout)
    }
}

impl GitHubApi for GhApi {
    fn json(&self, path: &str) -> Result<Value> {
        Ok(serde_json::from_slice(
            &self.request(path, "Accept: application/vnd.github+json")?,
        )?)
    }
    fn diff(&self, path: &str) -> Result<String> {
        Ok(String::from_utf8(
            self.request(path, "Accept: application/vnd.github.diff")?,
        )?)
    }
}

/// GitHub's pull files/commits limits are checked against PR metadata by caller.
pub fn pages(api: &impl GitHubApi, path: &str, expected: Option<usize>) -> Result<Vec<Value>> {
    let mut all = Vec::new();
    let separator = if path.contains('?') { '&' } else { '?' };
    for page in 1..=30 {
        let value = api.json(&format!("{path}{separator}per_page=100&page={page}"))?;
        let items = value
            .as_array()
            .ok_or("expected an array from GitHub pagination")?;
        all.extend(items.iter().cloned());
        if items.len() < 100 || expected == Some(all.len()) {
            if expected.is_some_and(|count| count != all.len()) {
                return Err("GitHub item count changed or was truncated during capture".into());
            }
            return Ok(all);
        }
        if expected.is_some_and(|count| all.len() > count) {
            return Err("GitHub returned more items than its snapshot metadata".into());
        }
    }
    Err("GitHub pagination reached the 3000-item capture limit".into())
}
