use dreamer::{
    config::Config,
    lesson::Lesson,
    mcp::http::{self, Credentials},
    store::{LessonStore, sqlite::SqliteStore},
};
use rmcp::{
    RoleClient, ServiceExt,
    model::CallToolRequestParams,
    service::RunningService,
    transport::{
        StreamableHttpClientTransport, streamable_http_client::StreamableHttpClientTransportConfig,
    },
};
use serde_json::{Value, json};
use std::{fs, path::PathBuf, time::Duration};
use tempfile::TempDir;
use tokio_util::sync::CancellationToken;

const READ: &str = "rrrrrrrrrrrrrrrrrrrrrrrrrrrrrrrrrrrrrrrrrrrrrrrrrrrrrrrrrrrrrrrr";
const WRITE: &str = "wwwwwwwwwwwwwwwwwwwwwwwwwwwwwwwwwwwwwwwwwwwwwwwwwwwwwwwwwwwwwwww";
const REPO: &str = "example/dreamer-demo";
const FIXTURE: &str = include_str!("../fixtures/sample-lesson.json");

fn setup() -> (TempDir, PathBuf, Config) {
    let directory = tempfile::tempdir().unwrap();
    let config_path = directory.path().join("demo.toml");
    let contents = include_str!("../fixtures/demo.toml").replace("../data/demo", "data")
        + "\n[http]\nlisten = \"127.0.0.1:0\"\nallowed_hosts = [\"localhost\", \"127.0.0.1\"]\n";
    fs::write(&config_path, contents).unwrap();
    let config = Config::load(&config_path).unwrap();
    fs::create_dir(&config.data_dir).unwrap();
    drop(SqliteStore::open(&config.data_dir.join("knowledge.sqlite3")).unwrap());
    (directory, config_path, config)
}

async fn start(
    config: &Config,
    writes: bool,
) -> (String, CancellationToken, tokio::task::JoinHandle<()>) {
    let shutdown = CancellationToken::new();
    let credentials = Credentials::new(READ.into(), writes.then(|| WRITE.into())).unwrap();
    let router = http::router(config, credentials, shutdown.clone()).unwrap();
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let url = format!("http://{}/mcp", listener.local_addr().unwrap());
    let token = shutdown.clone();
    let task = tokio::spawn(async move {
        axum::serve(listener, router)
            .with_graceful_shutdown(token.cancelled_owned())
            .await
            .unwrap();
    });
    (url, shutdown, task)
}

async fn connect(url: &str, token: &str) -> RunningService<RoleClient, ()> {
    let transport = StreamableHttpClientTransport::with_client(
        reqwest::Client::new(),
        StreamableHttpClientTransportConfig::with_uri(url.to_string()).auth_header(token),
    );
    tokio::time::timeout(Duration::from_secs(10), ().serve(transport))
        .await
        .unwrap()
        .unwrap()
}

async fn call(
    client: &RunningService<RoleClient, ()>,
    name: &str,
    arguments: Value,
) -> rmcp::model::CallToolResult {
    tokio::time::timeout(
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

#[tokio::test]
async fn authentication_origin_host_and_request_size_are_enforced() {
    let (_directory, _path, config) = setup();
    let (url, shutdown, task) = start(&config, false).await;
    let client = reqwest::Client::new();
    for token in [None, Some("bad-token"), Some(WRITE)] {
        let mut request = client.post(&url);
        if let Some(token) = token {
            request = request.bearer_auth(token);
        }
        let response = request.body("{}").send().await.unwrap();
        assert_eq!(response.status(), 401);
        assert!(response.headers().contains_key("www-authenticate"));
    }
    for origin in ["https://untrusted.invalid", "null", "http://localhost"] {
        assert_eq!(
            client
                .post(&url)
                .bearer_auth(READ)
                .header("Origin", origin)
                .body("{}")
                .send()
                .await
                .unwrap()
                .status(),
            403
        );
    }
    assert_eq!(
        client
            .post(&url)
            .bearer_auth(READ)
            .header("Host", "untrusted.invalid")
            .body("{}")
            .send()
            .await
            .unwrap()
            .status(),
        403
    );
    // Exercise the real limit layer directly: an early 413 can reset a large
    // unread TCP upload on macOS, before reqwest can observe its status.
    let router = http::router(
        &config,
        Credentials::new(READ.into(), None).unwrap(),
        CancellationToken::new(),
    )
    .unwrap();
    let oversized = axum::http::Request::builder()
        .method("POST")
        .uri("/mcp")
        .header("Host", "localhost")
        .header("Authorization", format!("Bearer {READ}"))
        .header("Content-Length", 1024 * 1024 + 1)
        .body(axum::body::Body::from("x".repeat(1024 * 1024 + 1)))
        .unwrap();
    assert_eq!(
        tower::ServiceExt::oneshot(router, oversized)
            .await
            .unwrap()
            .status(),
        413
    );
    let health = url.replace("/mcp", "/health");
    assert_eq!(client.get(&health).send().await.unwrap().status(), 401);
    assert_eq!(
        client
            .get(&health)
            .bearer_auth(READ)
            .send()
            .await
            .unwrap()
            .json::<Value>()
            .await
            .unwrap(),
        json!({"status": "ok"})
    );
    shutdown.cancel();
    tokio::time::timeout(Duration::from_secs(10), task)
        .await
        .unwrap()
        .unwrap();
}

#[tokio::test]
async fn sdk_clients_use_read_and_write_tokens_against_the_same_endpoint() {
    let (_directory, _path, config) = setup();
    let (url, shutdown, task) = start(&config, true).await;
    let reader = connect(&url, READ).await;
    let writer = connect(&url, WRITE).await;
    assert_eq!(reader.list_all_tools().await.unwrap().len(), 6);
    assert_eq!(writer.list_all_tools().await.unwrap().len(), 9);
    let report = json!({"repository":REPO,"id":"a".repeat(64),"phase":"capture","state":"failed","client_started_at":"2026-11-05T12:00:00Z","client_finished_at":"2026-11-05T12:01:00Z","pr":12});
    assert!(
        reader
            .call_tool(
                CallToolRequestParams::new("record_learning_run")
                    .with_arguments(report.as_object().unwrap().clone())
            )
            .await
            .is_err()
    );
    for outcome in ["recorded", "unchanged"] {
        let result = call(&writer, "record_learning_run", report.clone()).await;
        assert_ne!(result.is_error, Some(true), "{result:?}");
        assert_eq!(result.structured_content.unwrap()["outcome"], outcome);
    }
    let activity = call(&reader, "get_learning_runs", json!({"repository":REPO})).await;
    assert_eq!(
        activity.structured_content.unwrap()["summary"]["failures"],
        1
    );
    let mut invalid = report.clone();
    invalid["raw_error"] = json!("withheld");
    assert_eq!(
        call(&writer, "record_learning_run", invalid).await.is_error,
        Some(true)
    );
    let mut outside = report;
    outside["repository"] = json!("outside/repo");
    assert_eq!(
        call(&writer, "record_learning_run", outside).await.is_error,
        Some(true)
    );
    let lesson: Value = serde_json::from_str(FIXTURE).unwrap();
    assert!(
        reader
            .call_tool(
                CallToolRequestParams::new("submit_candidate")
                    .with_arguments(json!({"lesson": lesson}).as_object().unwrap().clone())
            )
            .await
            .is_err()
    );
    for outcome in ["inserted", "unchanged"] {
        let result = call(&writer, "submit_candidate", json!({"lesson": lesson})).await;
        assert_ne!(result.is_error, Some(true));
        assert_eq!(result.structured_content.unwrap()["outcome"], outcome);
    }
    let result = call(
        &reader,
        "search_lessons",
        json!({"repository": REPO, "query": "pagination"}),
    )
    .await;
    assert_eq!(
        result.structured_content.unwrap()["lessons"][0]["lesson"],
        lesson
    );
    assert_eq!(
        call(
            &writer,
            "get_lesson",
            json!({"repository": "example/other", "id": "pagination-scope-001"})
        )
        .await
        .is_error,
        Some(true)
    );
    reader.cancel().await.unwrap();
    writer.cancel().await.unwrap();
    shutdown.cancel();
    tokio::time::timeout(Duration::from_secs(10), task)
        .await
        .unwrap()
        .unwrap();
    let store = SqliteStore::open_read_only(&config.data_dir.join("knowledge.sqlite3")).unwrap();
    assert_eq!(
        store
            .get(REPO, "pagination-scope-001")
            .unwrap()
            .unwrap()
            .lesson,
        serde_json::from_str::<Lesson>(FIXTURE).unwrap()
    );
}

#[tokio::test]
async fn a_read_token_cannot_borrow_a_legacy_writer_session() {
    let (_directory, _path, config) = setup();
    let (url, shutdown, task) = start(&config, true).await;
    let client = reqwest::Client::new();
    let initialized = client.post(&url).bearer_auth(WRITE).header("Accept", "application/json, text/event-stream")
        .json(&json!({"jsonrpc":"2.0","id":1,"method":"initialize","params":{"protocolVersion":"2025-06-18","capabilities":{},"clientInfo":{"name":"legacy-test","version":"1"}}}))
        .send().await.unwrap();
    assert!(initialized.status().is_success());
    let session = initialized.headers()["mcp-session-id"]
        .to_str()
        .unwrap()
        .to_string();
    drop(initialized);
    let response = client.post(&url).bearer_auth(READ).header("Accept", "application/json, text/event-stream")
        .header("MCP-Session-Id", session).header("MCP-Protocol-Version", "2025-06-18")
        .json(&json!({"jsonrpc":"2.0","id":2,"method":"tools/call","params":{"name":"submit_candidate","arguments":{"lesson":serde_json::from_str::<Value>(FIXTURE).unwrap()}}}))
        .send().await.unwrap();
    assert!(!response.status().is_success());
    let store = SqliteStore::open_read_only(&config.data_dir.join("knowledge.sqlite3")).unwrap();
    assert!(store.get(REPO, "pagination-scope-001").unwrap().is_none());
    shutdown.cancel();
    tokio::time::timeout(Duration::from_secs(10), task)
        .await
        .unwrap()
        .unwrap();
}

#[test]
fn credentials_and_http_configuration_fail_closed_without_revealing_tokens() {
    assert!(Credentials::new("short".into(), None).is_err());
    assert!(Credentials::new(READ.into(), Some(READ.into())).is_err());
    assert!(Credentials::new(format!("{READ}\n"), None).is_err());
    let source = include_str!("../fixtures/demo.toml");
    for hosts in ["[]", "[\"*\"]", "[\"host:3445\"]", "[\"user@host\"]"] {
        assert!(Config::parse(&format!("{source}\n[http]\nallowed_hosts = {hosts}\n")).is_err());
    }
    let (_directory, path, _config) = setup();
    for writes in [false, true] {
        let mut command = std::process::Command::new(env!("CARGO_BIN_EXE_dreamer"));
        command
            .arg("--config")
            .arg(&path)
            .arg("serve")
            .env_remove("DREAMER_MCP_TOKEN")
            .env_remove("DREAMER_MCP_WRITE_TOKEN");
        if writes {
            command.arg("--allow-writes").env("DREAMER_MCP_TOKEN", READ);
        }
        let output = command.output().unwrap();
        assert_eq!(output.status.code(), Some(1));
        assert!(output.stdout.is_empty());
        assert!(!String::from_utf8_lossy(&output.stderr).contains(READ));
    }
}

#[cfg(unix)]
#[tokio::test]
async fn service_process_starts_and_exits_cleanly_on_sigterm() {
    use std::process::Stdio;
    use tokio::io::{AsyncBufReadExt, AsyncReadExt, BufReader};
    let (_directory, path, _config) = setup();
    let mut child = tokio::process::Command::new(env!("CARGO_BIN_EXE_dreamer"))
        .arg("--config")
        .arg(path)
        .arg("serve")
        .env("DREAMER_MCP_TOKEN", READ)
        .env_remove("DREAMER_MCP_WRITE_TOKEN")
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .kill_on_drop(true)
        .spawn()
        .unwrap();
    let mut stderr = BufReader::new(child.stderr.take().unwrap()).lines();
    let line = tokio::time::timeout(Duration::from_secs(10), stderr.next_line())
        .await
        .unwrap()
        .unwrap()
        .unwrap();
    assert!(!line.contains(READ));
    let url = line
        .strip_prefix("Dreamer HTTP MCP listening on ")
        .unwrap()
        .replace("/mcp", "/health");
    assert_eq!(
        reqwest::Client::new()
            .get(&url)
            .bearer_auth(READ)
            .send()
            .await
            .unwrap()
            .status(),
        200
    );
    assert!(
        std::process::Command::new("kill")
            .args(["-TERM", &child.id().unwrap().to_string()])
            .status()
            .unwrap()
            .success()
    );
    assert!(
        tokio::time::timeout(Duration::from_secs(10), child.wait())
            .await
            .unwrap()
            .unwrap()
            .success()
    );
    let mut stdout = String::new();
    child
        .stdout
        .take()
        .unwrap()
        .read_to_string(&mut stdout)
        .await
        .unwrap();
    assert!(stdout.is_empty());
}

#[tokio::test]
async fn lifecycle_http_enforces_writer_role_scope_and_exposes_audit_after_restart() {
    use dreamer::lifecycle::{ChangeAction, LessonChange};
    let (_directory, _path, config) = setup();
    let mut store = SqliteStore::open_existing(&config.data_dir.join("knowledge.sqlite3")).unwrap();
    let original: Lesson = serde_json::from_str(FIXTURE).unwrap();
    let mut replacement = original.clone();
    replacement.id = "replacement".into();
    store.put_candidate(&original).unwrap();
    store.put_candidate(&replacement).unwrap();
    drop(store);
    let change = LessonChange {
        repository: REPO.into(),
        source_id: original.id.clone(),
        action: ChangeAction::Supersede,
        target_id: Some(replacement.id.clone()),
        reason: "Inspected code corrects the earlier account scope advice.".into(),
        producer: "fixture agent".into(),
        evidence: replacement.evidence.clone(),
    };
    let (url, shutdown, task) = start(&config, true).await;
    let reader = connect(&url, READ).await;
    let writer = connect(&url, WRITE).await;
    assert!(
        reader
            .call_tool(
                CallToolRequestParams::new("change_lesson")
                    .with_arguments(json!({"change":change}).as_object().unwrap().clone())
            )
            .await
            .is_err()
    );
    let mut outside = change.clone();
    outside.repository = "unconfigured/repo".into();
    assert_eq!(
        call(&writer, "change_lesson", json!({"change":outside}))
            .await
            .is_error,
        Some(true)
    );
    for expected in ["applied", "unchanged"] {
        let applied = call(&writer, "change_lesson", json!({"change":change})).await;
        assert_ne!(applied.is_error, Some(true));
        assert_eq!(applied.structured_content.unwrap()["outcome"], expected);
    }
    let history = call(
        &reader,
        "get_lesson_history",
        json!({"repository":REPO,"id":original.id}),
    )
    .await
    .structured_content
    .unwrap();
    assert_eq!(
        history["changes"][0]["change"],
        serde_json::to_value(&change).unwrap()
    );
    let stored = call(
        &reader,
        "get_lesson",
        json!({"repository":REPO,"id":original.id}),
    )
    .await
    .structured_content
    .unwrap();
    assert_eq!(stored["lesson"]["lesson"]["status"], "deprecated");
    assert_eq!(
        stored["lesson"]["lifecycle"]["superseded_by"],
        replacement.id
    );
    reader.cancel().await.unwrap();
    writer.cancel().await.unwrap();
    shutdown.cancel();
    task.await.unwrap();
    let (url, shutdown, task) = start(&config, false).await;
    let reader = connect(&url, READ).await;
    assert_eq!(
        call(
            &reader,
            "get_lesson_history",
            json!({"repository":REPO,"id":original.id,"change_id":change.id()})
        )
        .await
        .structured_content
        .unwrap()["changes"],
        history["changes"]
    );
    reader.cancel().await.unwrap();
    shutdown.cancel();
    task.await.unwrap();
}
