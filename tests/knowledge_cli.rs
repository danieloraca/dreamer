use serde_json::Value;
use std::{
    fs,
    path::Path,
    process::{Command, Output},
};
use tempfile::TempDir;

fn setup() -> (TempDir, std::path::PathBuf) {
    let directory = tempfile::tempdir().unwrap();
    let config = directory.path().join("demo.toml");
    fs::write(
        &config,
        include_str!("../fixtures/demo.toml").replace("../data/demo", "data"),
    )
    .unwrap();
    (directory, config)
}

fn run(config: &Path, arguments: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_dreamer"))
        .arg("--config")
        .arg(config)
        .args(arguments)
        .output()
        .unwrap()
}

fn success(output: Output) -> String {
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    String::from_utf8(output.stdout).unwrap()
}

const FIXTURE: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/fixtures/sample-lesson.json");

#[test]
fn local_fixture_can_be_added_searched_and_retrieved_across_cli_processes() {
    let (directory, config) = setup();
    success(run(&config, &["status"]));
    assert!(!directory.path().join("data").exists());
    assert!(success(run(&config, &["db", "init"])).contains("schema 2"));
    assert!(
        success(run(&config, &["lesson", "add", "--file", FIXTURE])).contains("Inserted candidate")
    );
    assert!(
        success(run(&config, &["lesson", "add", "--file", FIXTURE]))
            .contains("Unchanged candidate")
    );
    let results: Value = serde_json::from_str(&success(run(
        &config,
        &["lesson", "search", "--query", "pagination", "--limit", "1"],
    )))
    .unwrap();
    assert_eq!(results.as_array().unwrap().len(), 1);
    assert_eq!(results[0]["lesson"]["status"], "candidate");
    assert_eq!(results[0]["lesson"]["provenance"]["method"], "fixture");
    assert_eq!(
        results[0]["lesson"]["evidence"][0]["pull_request"]["number"],
        42
    );
    let record: Value = serde_json::from_str(&success(run(
        &config,
        &["lesson", "get", "--id", "pagination-scope-001"],
    )))
    .unwrap();
    assert_eq!(record, results[0]);
    let missing = run(&config, &["lesson", "get", "--id", "absent"]);
    assert_eq!(missing.status.code(), Some(1));
    let invalid_limit = run(
        &config,
        &[
            "lesson",
            "search",
            "--query",
            "pagination",
            "--limit",
            "101",
        ],
    );
    assert_eq!(invalid_limit.status.code(), Some(2));
}

#[test]
fn invalid_and_cross_repository_imports_do_not_create_a_database() {
    let (directory, config) = setup();
    let bad_file = directory.path().join("bad.json");
    for contents in [
        "{}".to_string(),
        include_str!("../fixtures/sample-lesson.json")
            .replace("example/dreamer-demo", "example/other"),
        include_str!("../fixtures/sample-lesson.json").replace("\"candidate\"", "\"verified\""),
    ] {
        fs::write(&bad_file, contents).unwrap();
        let output = run(
            &config,
            &["lesson", "add", "--file", bad_file.to_str().unwrap()],
        );
        assert_eq!(output.status.code(), Some(1));
        assert!(!directory.path().join("data").exists());
    }
}

#[test]
fn reading_a_missing_database_does_not_create_it() {
    let (directory, config) = setup();
    for arguments in [
        vec!["lesson", "search", "--query", "pagination"],
        vec!["lesson", "get", "--id", "absent"],
    ] {
        assert_eq!(run(&config, &arguments).status.code(), Some(1));
        assert!(!directory.path().join("data").exists());
    }
}
