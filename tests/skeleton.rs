use dreamer::{
    config::Config,
    lesson::{Evidence, Lesson, LessonStatus},
};
use std::process::Command;

const EXAMPLE: &str = include_str!("../dreamer.example.toml");

#[test]
fn example_config_is_valid() {
    let config = Config::parse(EXAMPLE).unwrap();
    assert_eq!(config.github.repository, "geckolabs/Gecko-API");
    assert_eq!(config.memory.default_search_limit, 5);
}

#[test]
fn rejects_mistyped_config_and_unsafe_batch_sizes() {
    assert!(Config::parse(&EXAMPLE.replace("batch_size", "batch_sze")).is_err());
    for size in [0, 101] {
        assert!(
            Config::parse(&EXAMPLE.replace("batch_size = 25", &format!("batch_size = {size}")))
                .is_err()
        );
    }
    assert!(
        Config::parse(&EXAMPLE.replace("default_search_limit = 5", "default_search_limit = 0"))
            .is_err()
    );
}

#[test]
fn rejects_missing_storage_path_and_malformed_repositories() {
    assert!(Config::parse(&EXAMPLE.replace("data_dir = \"data\"", "data_dir = \"\"")).is_err());
    for repository in [
        "",
        "Gecko-API",
        "owner/repo/extra",
        "../repo",
        "owner/repo name",
    ] {
        assert!(Config::parse(&EXAMPLE.replace("geckolabs/Gecko-API", repository)).is_err());
    }
}

#[test]
fn lessons_require_evidence_and_applicability() {
    let mut lesson = Lesson {
        id: "example-1".into(),
        repository: "example/project".into(),
        summary: "Check access scope before pagination".into(),
        applies_when: "Reviewing a paginated list endpoint".into(),
        status: LessonStatus::Candidate,
        evidence: vec![],
    };
    assert!(lesson.validate().is_err());
    lesson.evidence.push(Evidence {
        source_url: "https://example.invalid/pull/1#review-comment".into(),
        commit_sha: None,
        description: "Synthetic test fixture; not a real Gecko finding".into(),
    });
    assert!(lesson.validate().is_ok());
    assert_eq!(lesson.status, LessonStatus::Candidate);
    lesson.applies_when.clear();
    assert!(lesson.validate().is_err());
}

#[test]
fn status_reads_example_config_and_reports_pending_adapters() {
    let output = Command::new(env!("CARGO_BIN_EXE_dreamer"))
        .args(["status", "--config"])
        .arg(concat!(env!("CARGO_MANIFEST_DIR"), "/dreamer.example.toml"))
        .output()
        .unwrap();
    assert!(output.status.success());
    let stdout = String::from_utf8(output.stdout).unwrap();
    assert!(stdout.contains("Configuration: valid"));
    assert!(stdout.contains("GitHub ingestion: not implemented"));
    assert!(stdout.contains("MCP: not implemented"));
    assert!(stdout.contains(concat!(env!("CARGO_MANIFEST_DIR"), "/data")));
}

#[test]
fn missing_config_fails_and_reserved_commands_do_not_claim_success() {
    let output = Command::new(env!("CARGO_BIN_EXE_dreamer"))
        .args(["status", "--config", "does-not-exist.toml"])
        .output()
        .unwrap();
    assert_eq!(output.status.code(), Some(1));
    for command in ["ingest", "consolidate", "mcp"] {
        let output = Command::new(env!("CARGO_BIN_EXE_dreamer"))
            .arg(command)
            .output()
            .unwrap();
        assert_eq!(output.status.code(), Some(2));
        assert!(output.stdout.is_empty());
        assert!(
            String::from_utf8(output.stderr)
                .unwrap()
                .contains("not implemented")
        );
    }
}
