//! Explicit, canonical repository allowlists shared by Pi and Mac configuration.
use crate::lesson::validate_repository;

pub fn validate(default: &str, repositories: &[String]) -> Result<(), &'static str> {
    validate_repository(default)?;
    let mut unique = std::collections::BTreeSet::new();
    for repository in repositories {
        validate_repository(repository)?;
        if !unique.insert(repository.to_ascii_lowercase()) {
            return Err("repositories must not contain duplicates or case aliases");
        }
    }
    if !repositories.is_empty() && !repositories.iter().any(|repo| repo == default) {
        return Err("repositories must include the default repository with its canonical spelling");
    }
    Ok(())
}

pub fn configured(default: &str, repositories: &[String]) -> Vec<String> {
    if repositories.is_empty() {
        vec![default.into()]
    } else {
        repositories.to_vec()
    }
}

pub fn select(
    default: &str,
    repositories: &[String],
    requested: Option<&str>,
) -> Result<String, &'static str> {
    let requested = requested.unwrap_or(default);
    validate_repository(requested)?;
    configured(default, repositories)
        .into_iter()
        .find(|repo| repo.eq_ignore_ascii_case(requested))
        .ok_or("repository is not in the configured allowlist")
}
