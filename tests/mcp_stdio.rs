use dreamer::{
    lesson::Lesson,
    store::{LessonStore, sqlite::SqliteStore},
};
use rmcp::{
    RoleClient, ServiceExt,
    model::{CallToolRequestParams, CallToolResult},
    service::RunningService,
    transport::TokioChildProcess,
};
use serde_json::{Value, json};
use std::{fs, path::PathBuf, process::Stdio, time::Duration};
use tempfile::TempDir;
use tokio::{
    io::{AsyncBufReadExt, AsyncWriteExt, BufReader, Lines},
    process::{ChildStdin, ChildStdout, Command},
    time::timeout,
};

const REPOSITORY: &str = "example/dreamer-demo";
const LESSON_ID: &str = "pagination-scope-001";
const FIXTURE: &str = include_str!("../fixtures/sample-lesson.json");

fn setup(seed: bool) -> (TempDir, PathBuf) {
    let directory = tempfile::tempdir().unwrap();
    let config = directory.path().join("demo.toml");
    fs::write(
        &config,
        include_str!("../fixtures/demo.toml").replace("../data/demo", "data"),
    )
    .unwrap();
    fs::create_dir(directory.path().join("data")).unwrap();
    let mut store = SqliteStore::open(&directory.path().join("data/knowledge.sqlite3")).unwrap();
    if seed {
        store
            .put_candidate(&serde_json::from_str::<Lesson>(FIXTURE).unwrap())
            .unwrap();
    }
    (directory, config)
}

async fn connect(config: &std::path::Path, writes: bool) -> RunningService<RoleClient, ()> {
    let mut command = Command::new(env!("CARGO_BIN_EXE_dreamer"));
    command
        .arg("--config")
        .arg(config)
        .arg("mcp")
        .stderr(Stdio::inherit());
    if writes {
        command.arg("--allow-writes");
    }
    timeout(
        Duration::from_secs(10),
        ().serve(TokioChildProcess::new(command).unwrap()),
    )
    .await
    .unwrap()
    .unwrap()
}

async fn call(
    client: &RunningService<RoleClient, ()>,
    name: &str,
    arguments: Value,
) -> CallToolResult {
    timeout(
        Duration::from_secs(10),
        client.call_tool(
            CallToolRequestParams::new(name.to_string())
                .with_arguments(arguments.as_object().unwrap().clone()),
        ),
    )
    .await
    .unwrap()
    .unwrap()
}

fn data(result: CallToolResult) -> Value {
    assert_ne!(result.is_error, Some(true), "{result:?}");
    let structured = result.structured_content.unwrap();
    // Text-only clients see exactly the same JSON as structured-content clients.
    assert_eq!(
        serde_json::from_str::<Value>(&result.content[0].as_text().unwrap().text).unwrap(),
        structured
    );
    structured
}

#[tokio::test]
async fn sdk_client_discovers_reads_and_enforces_read_only_repository_scope() {
    let (_directory, config) = setup(true);
    let client = connect(&config, false).await;
    let tools = client.list_all_tools().await.unwrap();
    let names: Vec<_> = tools.iter().map(|tool| tool.name.as_ref()).collect();
    assert_eq!(
        names,
        [
            "assess_lesson_change",
            "assess_conversation_candidate",
            "list_repositories",
            "search_lessons",
            "get_lesson",
            "recall_lessons",
            "get_lesson_history",
            "get_learning_runs"
        ]
    );
    for tool in tools {
        let annotations = tool.annotations.unwrap();
        assert_eq!(annotations.read_only_hint, Some(true));
        assert_eq!(annotations.open_world_hint, Some(false));
        if tool.name != "list_repositories" && !tool.name.starts_with("assess_") {
            assert!(
                tool.input_schema["required"]
                    .as_array()
                    .unwrap()
                    .contains(&json!("repository"))
            );
        }
    }
    let results = data(
        call(
            &client,
            "search_lessons",
            json!({"repository": REPOSITORY, "query": "PAGINATION", "limit": 1}),
        )
        .await,
    );
    assert_eq!(results["lessons"].as_array().unwrap().len(), 1);
    assert_eq!(results["lessons"][0]["lesson"]["status"], "candidate");
    assert_eq!(
        results["lessons"][0]["lesson"]["evidence"][0]["pull_request"]["number"],
        42
    );
    let found = data(
        call(
            &client,
            "get_lesson",
            json!({"repository": REPOSITORY, "id": LESSON_ID}),
        )
        .await,
    );
    assert_eq!(found["found"], true);
    assert_eq!(found["lesson"], results["lessons"][0]);
    let absent = data(
        call(
            &client,
            "get_lesson",
            json!({"repository": REPOSITORY, "id": "absent"}),
        )
        .await,
    );
    assert_eq!(absent["found"], false);
    assert_eq!(absent["lesson"], Value::Null);
    let denied = call(
        &client,
        "search_lessons",
        json!({"repository": "example/other", "query": "pagination"}),
    )
    .await;
    assert_eq!(denied.is_error, Some(true));
    assert_eq!(
        denied.structured_content.unwrap()["code"],
        "repository_scope"
    );
    for arguments in [
        json!({"repository": REPOSITORY, "query": " "}),
        json!({"repository": REPOSITORY, "query": "pagination", "limit": 0}),
        json!({"repository": REPOSITORY, "query": "pagination", "unexpected": true}),
    ] {
        assert_eq!(
            call(&client, "search_lessons", arguments).await.is_error,
            Some(true)
        );
    }
    // Hiding the write tool is insufficient; direct calls must also fail.
    assert!(
        client
            .call_tool(
                CallToolRequestParams::new("submit_candidate").with_arguments(
                    json!({"lesson": serde_json::from_str::<Value>(FIXTURE).unwrap()})
                        .as_object()
                        .unwrap()
                        .clone()
                )
            )
            .await
            .is_err()
    );
    assert!(
        client
            .call_tool(CallToolRequestParams::new("unknown"))
            .await
            .is_err()
    );
    client.cancel().await.unwrap();
}

#[tokio::test]
async fn write_mode_submits_candidates_idempotently_and_persists_for_another_process() {
    let (_directory, config) = setup(false);
    let client = connect(&config, true).await;
    let tools = client.list_all_tools().await.unwrap();
    let submit = tools
        .iter()
        .find(|tool| tool.name == "submit_candidate")
        .unwrap();
    let annotations = submit.annotations.as_ref().unwrap();
    assert_eq!(annotations.read_only_hint, Some(false));
    assert_eq!(annotations.destructive_hint, Some(false));
    assert_eq!(annotations.idempotent_hint, Some(true));
    let lesson: Value = serde_json::from_str(FIXTURE).unwrap();
    assert_eq!(
        data(call(&client, "submit_candidate", json!({"lesson": lesson})).await)["outcome"],
        "inserted"
    );
    assert_eq!(
        data(call(&client, "submit_candidate", json!({"lesson": lesson})).await)["outcome"],
        "unchanged"
    );
    let mut conflicting = lesson.clone();
    conflicting["summary"] = json!("Conflicting replacement");
    let result = call(&client, "submit_candidate", json!({"lesson": conflicting})).await;
    assert_eq!(result.is_error, Some(true));
    assert_eq!(result.structured_content.unwrap()["code"], "conflict");
    let mut invalid = lesson.clone();
    invalid["id"] = json!("must-not-verify");
    invalid["status"] = json!("verified");
    assert_eq!(
        call(&client, "submit_candidate", json!({"lesson": invalid}))
            .await
            .is_error,
        Some(true)
    );
    let mut wrong_scope = lesson.clone();
    wrong_scope["repository"] = json!("example/other");
    assert_eq!(
        call(&client, "submit_candidate", json!({"lesson": wrong_scope}))
            .await
            .is_error,
        Some(true)
    );
    client.cancel().await.unwrap();
    let reader = connect(&config, false).await;
    let found = data(
        call(
            &reader,
            "get_lesson",
            json!({"repository": REPOSITORY, "id": LESSON_ID}),
        )
        .await,
    );
    assert_eq!(found["lesson"]["lesson"], lesson);
    reader.cancel().await.unwrap();
}

async fn raw_request(
    stdin: &mut ChildStdin,
    stdout: &mut Lines<BufReader<ChildStdout>>,
    id: u64,
    method: &str,
    params: Value,
) -> Value {
    let message = json!({"jsonrpc": "2.0", "id": id, "method": method, "params": params});
    stdin
        .write_all(format!("{message}\n").as_bytes())
        .await
        .unwrap();
    let line = timeout(Duration::from_secs(10), stdout.next_line())
        .await
        .unwrap()
        .unwrap()
        .unwrap();
    let response: Value = serde_json::from_str(&line).unwrap();
    assert_eq!(response["id"], id);
    assert!(response.get("error").is_none(), "{response}");
    response["result"].clone()
}

#[tokio::test]
async fn legacy_initialize_and_clean_eof_work_over_raw_json_rpc() {
    let (_directory, config) = setup(true);
    let mut child = Command::new(env!("CARGO_BIN_EXE_dreamer"))
        .arg("--config")
        .arg(config)
        .arg("mcp")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::inherit())
        .kill_on_drop(true)
        .spawn()
        .unwrap();
    let mut stdin = child.stdin.take().unwrap();
    let mut stdout = BufReader::new(child.stdout.take().unwrap()).lines();
    let initialized = raw_request(&mut stdin, &mut stdout, 1, "initialize", json!({"protocolVersion": "2025-06-18", "capabilities": {}, "clientInfo": {"name": "dreamer-legacy-test", "version": "1"}})).await;
    assert_eq!(initialized["serverInfo"]["name"], "dreamer");
    assert_eq!(initialized["protocolVersion"], "2025-06-18");
    stdin
        .write_all(b"{\"jsonrpc\":\"2.0\",\"method\":\"notifications/initialized\"}\n")
        .await
        .unwrap();
    let tools = raw_request(&mut stdin, &mut stdout, 2, "tools/list", json!({})).await;
    assert_eq!(tools["tools"].as_array().unwrap().len(), 8);
    let result = raw_request(&mut stdin, &mut stdout, 3, "tools/call", json!({"name": "search_lessons", "arguments": {"repository": REPOSITORY, "query": "pagination"}})).await;
    assert_eq!(
        result["structuredContent"]["lessons"]
            .as_array()
            .unwrap()
            .len(),
        1
    );
    drop(stdin);
    assert!(
        timeout(Duration::from_secs(10), child.wait())
            .await
            .unwrap()
            .unwrap()
            .success()
    );
}

#[test]
fn mcp_requires_explicit_database_initialization_and_has_protocol_only_stdout() {
    let directory = tempfile::tempdir().unwrap();
    let config = directory.path().join("demo.toml");
    fs::write(
        &config,
        include_str!("../fixtures/demo.toml").replace("../data/demo", "data"),
    )
    .unwrap();
    let output = std::process::Command::new(env!("CARGO_BIN_EXE_dreamer"))
        .arg("--config")
        .arg(config)
        .arg("mcp")
        .output()
        .unwrap();
    assert_eq!(output.status.code(), Some(1));
    assert!(output.stdout.is_empty());
    assert!(!directory.path().join("data").exists());
}
