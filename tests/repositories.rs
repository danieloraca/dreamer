use dreamer::{
    config::Config,
    lesson::Lesson,
    mcp::http::{self, Credentials},
    repository,
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
use std::{fs, path::Path, time::Duration};
use tokio_util::sync::CancellationToken;
const A: &str = "example/first";
const B: &str = "example/second";
const OUTSIDE: &str = "example/outside";
const READ: &str = "rrrrrrrrrrrrrrrrrrrrrrrrrrrrrrrrrrrrrrrrrrrrrrrrrrrrrrrrrrrrrrrr";
const WRITE: &str = "wwwwwwwwwwwwwwwwwwwwwwwwwwwwwwwwwwwwwwwwwwwwwwwwwwwwwwwwwwwwwwww";
fn fixture(repo: &str) -> Lesson {
    let mut lesson: Lesson =
        serde_json::from_str(include_str!("../fixtures/sample-lesson.json")).unwrap();
    lesson.id = "shared-id".into();
    lesson.repository = repo.into();
    lesson.summary = format!("Scoped private fact for {repo}");
    lesson
}
fn config(root: &Path) -> String {
    format!(
        "data_dir = {:?}\n[github]\nrepository = \"{A}\"\nrepositories = [\"{A}\", \"{B}\"]\nbatch_size = 10\n[memory]\ndefault_search_limit = 5\n[http]\nlisten = \"127.0.0.1:0\"\nallowed_hosts = [\"127.0.0.1\"]",
        root
    )
}
#[test]
fn allowlists_validate_legacy_defaults_aliases_and_explicit_selection() {
    assert_eq!(repository::select(A, &[], None).unwrap(), A);
    assert_eq!(repository::configured(A, &[]), vec![A]);
    for list in [
        vec![B.into()],
        vec![A.into(), A.into()],
        vec![A.into(), "EXAMPLE/FIRST".into()],
        vec![A.into(), "example/*".into()],
    ] {
        assert!(repository::validate(A, &list).is_err());
    }
    let allowed = vec![A.into(), B.into()];
    repository::validate(A, &allowed).unwrap();
    assert_eq!(
        repository::select(A, &allowed, Some("EXAMPLE/SECOND")).unwrap(),
        B
    );
    assert!(repository::select(A, &allowed, Some(OUTSIDE)).is_err());
    let dir = tempfile::tempdir().unwrap();
    assert!(Config::parse(&config(dir.path())).is_ok());
    assert!(
        Config::parse(
            &config(dir.path()).replace(&format!("[\"{A}\", \"{B}\"]"), &format!("[\"{B}\"]"))
        )
        .is_err()
    );
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
    args: Value,
) -> rmcp::model::CallToolResult {
    tokio::time::timeout(
        Duration::from_secs(10),
        client.call_tool(
            CallToolRequestParams::new(name.to_string())
                .with_arguments(args.as_object().unwrap().clone()),
        ),
    )
    .await
    .unwrap()
    .unwrap()
}
#[tokio::test]
async fn mcp_and_dashboard_keep_identical_ids_isolated_and_hide_unconfigured_data() {
    let dir = tempfile::tempdir().unwrap();
    let database = dir.path().join("knowledge.sqlite3");
    let mut store = SqliteStore::open(&database).unwrap();
    for repo in [A, B, OUTSIDE] {
        store.put_candidate(&fixture(repo)).unwrap();
    }
    drop(store);
    let config = Config::parse(&config(dir.path())).unwrap();
    let shutdown = CancellationToken::new();
    let router = http::router(
        &config,
        Credentials::new(READ.into(), Some(WRITE.into())).unwrap(),
        shutdown.clone(),
    )
    .unwrap();
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let root = format!("http://{}", listener.local_addr().unwrap());
    let stop = shutdown.clone();
    let task = tokio::spawn(async move {
        axum::serve(listener, router)
            .with_graceful_shutdown(stop.cancelled_owned())
            .await
            .unwrap();
    });
    let reader = connect(&format!("{root}/mcp"), READ).await;
    let writer = connect(&format!("{root}/mcp"), WRITE).await;
    let scopes = call(&reader, "list_repositories", json!({}))
        .await
        .structured_content
        .unwrap();
    assert_eq!(scopes["default_repository"], A);
    assert_eq!(scopes["repositories"], json!([A, B]));
    assert!(!scopes.to_string().contains(OUTSIDE));
    for repo in [A, B] {
        let page = call(
            &reader,
            "search_lessons",
            json!({"repository":repo,"query":"private fact"}),
        )
        .await
        .structured_content
        .unwrap();
        assert_eq!(page["repository"], repo);
        assert_eq!(page["lessons"].as_array().unwrap().len(), 1);
        assert_eq!(page["lessons"][0]["lesson"]["repository"], repo);
        let found = call(
            &reader,
            "get_lesson",
            json!({"repository":repo,"id":"shared-id"}),
        )
        .await
        .structured_content
        .unwrap();
        assert_eq!(found["lesson"]["lesson"]["summary"], fixture(repo).summary);
    }
    for tool in ["get_lesson", "search_lessons"] {
        let args = if tool == "get_lesson" {
            json!({"repository":OUTSIDE,"id":"shared-id"})
        } else {
            json!({"repository":OUTSIDE,"query":"private"})
        };
        assert_eq!(call(&reader, tool, args).await.is_error, Some(true));
    }
    let mut new = fixture(B);
    new.id = "new-in-second".into();
    assert_eq!(
        call(&writer, "submit_candidate", json!({"lesson":new}))
            .await
            .structured_content
            .unwrap()["outcome"],
        "inserted"
    );
    assert_eq!(
        call(
            &reader,
            "get_lesson",
            json!({"repository":A,"id":"new-in-second"})
        )
        .await
        .structured_content
        .unwrap()["found"],
        false
    );
    let mut bad = fixture(OUTSIDE);
    bad.id = "must-not-insert".into();
    assert_eq!(
        call(&writer, "submit_candidate", json!({"lesson":bad}))
            .await
            .is_error,
        Some(true)
    );
    let client = reqwest::Client::new();
    for (repo, total) in [(A, 1), (B, 2)] {
        let page: Value = client
            .get(format!("{root}/api/knowledge?repository={repo}"))
            .bearer_auth(READ)
            .send()
            .await
            .unwrap()
            .json()
            .await
            .unwrap();
        assert_eq!(page["repository"], repo);
        assert_eq!(page["counts"]["total"], total);
        assert_eq!(page["repositories"], json!([A, B]));
        assert!(
            page["lessons"]
                .as_array()
                .unwrap()
                .iter()
                .all(|s| s["lesson"]["repository"] == repo)
        );
    }
    assert_eq!(
        client
            .get(format!("{root}/api/knowledge?repository={OUTSIDE}"))
            .bearer_auth(READ)
            .send()
            .await
            .unwrap()
            .status(),
        403
    );
    reader.cancel().await.unwrap();
    writer.cancel().await.unwrap();
    shutdown.cancel();
    tokio::time::timeout(Duration::from_secs(10), task)
        .await
        .unwrap()
        .unwrap();
}
#[test]
fn local_cli_defaults_to_old_scope_and_accepts_an_explicit_allowed_scope() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("config.toml");
    fs::write(&path, config(dir.path())).unwrap();
    let mut store = SqliteStore::open(&dir.path().join("knowledge.sqlite3")).unwrap();
    for repo in [A, B] {
        store.put_candidate(&fixture(repo)).unwrap();
    }
    drop(store);
    for (args, repo) in [
        (vec!["lesson", "get", "--id", "shared-id"], A),
        (
            vec!["lesson", "--repository", B, "get", "--id", "shared-id"],
            B,
        ),
    ] {
        let result = std::process::Command::new(env!("CARGO_BIN_EXE_dreamer"))
            .arg("--config")
            .arg(&path)
            .args(args)
            .output()
            .unwrap();
        assert!(
            result.status.success(),
            "{}",
            String::from_utf8_lossy(&result.stderr)
        );
        let value: Value = serde_json::from_slice(&result.stdout).unwrap();
        assert_eq!(value["lesson"]["repository"], repo);
    }
    let bad = std::process::Command::new(env!("CARGO_BIN_EXE_dreamer"))
        .arg("--config")
        .arg(&path)
        .args([
            "lesson",
            "--repository",
            OUTSIDE,
            "get",
            "--id",
            "shared-id",
        ])
        .output()
        .unwrap();
    assert!(!bad.status.success());
    assert!(bad.stdout.is_empty());
}
