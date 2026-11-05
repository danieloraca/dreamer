use dreamer::{
    config::Config,
    lesson::Lesson,
    mcp::http::{self, Credentials},
    store::{BrowseQuery, LessonStore, sqlite::SqliteStore},
};
use serde_json::Value;
use std::{fs, time::Duration};
use tempfile::TempDir;
use tokio_util::sync::CancellationToken;

const READ: &str = "rrrrrrrrrrrrrrrrrrrrrrrrrrrrrrrrrrrrrrrrrrrrrrrrrrrrrrrrrrrrrrrr";
const WRITE: &str = "wwwwwwwwwwwwwwwwwwwwwwwwwwwwwwwwwwwwwwwwwwwwwwwwwwwwwwwwwwwwwwww";
const REPO: &str = "example/dreamer-demo";

fn setup() -> (TempDir, Config) {
    let dir = tempfile::tempdir().unwrap();
    let config = Config::parse(&format!(
        "data_dir = {:?}\n[github]\nrepository = \"{REPO}\"\nbatch_size = 10\n[memory]\ndefault_search_limit = 5\n[http]\nlisten = \"127.0.0.1:0\"\nallowed_hosts = [\"127.0.0.1\"]", dir.path()
    )).unwrap();
    let mut store = SqliteStore::open(&dir.path().join("knowledge.sqlite3")).unwrap();
    for (id, repository) in [
        ("a", REPO),
        ("b", REPO),
        ("c", REPO),
        ("other", "example/other"),
    ] {
        let mut lesson: Lesson =
            serde_json::from_str(include_str!("../fixtures/sample-lesson.json")).unwrap();
        lesson.id = id.into();
        lesson.repository = repository.into();
        lesson.summary = format!("{id}: literal 100%_ <img src=x onerror=alert(1)>");
        store.put_candidate(&lesson).unwrap();
    }
    let connection = rusqlite::Connection::open(dir.path().join("knowledge.sqlite3")).unwrap();
    connection
        .execute("UPDATE lessons SET status = 'verified' WHERE id = 'b'", [])
        .unwrap();
    connection
        .execute(
            "UPDATE lessons SET status = 'deprecated' WHERE id = 'c'",
            [],
        )
        .unwrap();
    (dir, config)
}

async fn start(config: &Config) -> (String, CancellationToken, tokio::task::JoinHandle<()>) {
    let shutdown = CancellationToken::new();
    let app = http::router(
        config,
        Credentials::new(READ.into(), Some(WRITE.into())).unwrap(),
        shutdown.clone(),
    )
    .unwrap();
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let url = format!("http://{}", listener.local_addr().unwrap());
    let token = shutdown.clone();
    let task = tokio::spawn(async move {
        axum::serve(listener, app)
            .with_graceful_shutdown(token.cancelled_owned())
            .await
            .unwrap();
    });
    (url, shutdown, task)
}

async fn stop(shutdown: CancellationToken, task: tokio::task::JoinHandle<()>) {
    shutdown.cancel();
    tokio::time::timeout(Duration::from_secs(10), task)
        .await
        .unwrap()
        .unwrap();
}

#[test]
fn browsing_is_scoped_paginated_and_includes_explicit_statuses_on_a_read_connection() {
    let (dir, _) = setup();
    let mut store = SqliteStore::open_read_only(&dir.path().join("knowledge.sqlite3")).unwrap();
    let mut query = BrowseQuery {
        repository: REPO.into(),
        text: "".into(),
        status: None,
        limit: 2,
        offset: 0,
    };
    let first = store.browse(&query).unwrap();
    assert_eq!(
        (
            first.counts.total,
            first.counts.candidate,
            first.counts.verified,
            first.counts.deprecated,
            first.counts.source_prs
        ),
        (3, 1, 1, 1, 1)
    );
    assert_eq!(first.matching, 3);
    assert_eq!(first.lessons.len(), 2);
    query.offset = 2;
    let second = store.browse(&query).unwrap();
    assert_eq!(second.lessons.len(), 1);
    assert!(
        first
            .lessons
            .iter()
            .all(|item| item.lesson.id != second.lessons[0].lesson.id)
    );
    query.offset = 0;
    query.status = Some("deprecated".into());
    assert_eq!(store.browse(&query).unwrap().lessons[0].lesson.id, "c");
    query.status = None;
    query.text = "100%_".into();
    assert_eq!(store.browse(&query).unwrap().matching, 3);
    query.text = "100%X".into();
    assert_eq!(store.browse(&query).unwrap().matching, 0);
    query.text = "".into();
    query.repository = "example/empty".into();
    let empty = store.browse(&query).unwrap();
    assert_eq!(
        (empty.matching, empty.counts.total, empty.counts.source_prs),
        (0, 0, 0)
    );
    assert!(empty.counts.last_stored_at.is_none());
}

#[tokio::test]
async fn dashboard_assets_are_embedded_and_locked_while_hosts_and_methods_are_restricted() {
    let (_dir, config) = setup();
    let (url, shutdown, task) = start(&config).await;
    let client = reqwest::Client::new();
    for path in ["/", "/dashboard.js", "/dashboard.css"] {
        let response = client.get(format!("{url}{path}")).send().await.unwrap();
        assert_eq!(response.status(), 200);
        assert_eq!(response.headers()["cache-control"], "no-store");
        assert_eq!(response.headers()["x-content-type-options"], "nosniff");
        assert!(
            response.headers()["content-security-policy"]
                .to_str()
                .unwrap()
                .contains("frame-ancestors 'none'")
        );
        let body = response.text().await.unwrap();
        assert!(!body.contains(READ));
        assert!(!body.contains("literal 100%_"));
        assert_eq!(
            client
                .get(format!("{url}{path}"))
                .header("Host", "untrusted.invalid")
                .send()
                .await
                .unwrap()
                .status(),
            403
        );
    }
    assert_eq!(
        client
            .post(format!("{url}/api/knowledge"))
            .bearer_auth(READ)
            .send()
            .await
            .unwrap()
            .status(),
        405
    );
    stop(shutdown, task).await;
}

#[tokio::test]
async fn dashboard_requires_read_token_and_rejects_cross_origin_requests() {
    let (_dir, config) = setup();
    let (url, shutdown, task) = start(&config).await;
    let client = reqwest::Client::new();
    for token in ["", "incorrect", WRITE] {
        let response = client
            .get(format!("{url}/api/knowledge"))
            .bearer_auth(token)
            .send()
            .await
            .unwrap();
        assert_eq!(response.status(), 401);
        assert!(!response.text().await.unwrap().contains(REPO));
    }
    for origin in ["https://untrusted.invalid", "null", "http://127.0.0.1:1"] {
        assert_eq!(
            client
                .get(format!("{url}/api/knowledge"))
                .bearer_auth(READ)
                .header("Origin", origin)
                .send()
                .await
                .unwrap()
                .status(),
            403
        );
    }
    assert_eq!(
        client
            .get(format!("{url}/api/knowledge"))
            .bearer_auth(READ)
            .header("Sec-Fetch-Site", "cross-site")
            .send()
            .await
            .unwrap()
            .status(),
        403
    );
    let response = client
        .get(format!("{url}/api/knowledge"))
        .bearer_auth(READ)
        .header("Origin", &url)
        .send()
        .await
        .unwrap();
    assert_eq!(response.status(), 200);
    assert_eq!(response.headers()["cache-control"], "no-store");
    assert!(
        !response
            .headers()
            .contains_key("access-control-allow-origin")
    );
    let page: Value = response.json().await.unwrap();
    assert_eq!(page["repository"], REPO);
    assert_eq!(page["counts"]["total"], 3);
    assert_eq!(page["lessons"].as_array().unwrap().len(), 3);
    stop(shutdown, task).await;
}

#[tokio::test]
async fn api_validates_filters_and_does_not_create_or_migrate_storage() {
    let (dir, config) = setup();
    let (url, shutdown, task) = start(&config).await;
    let client = reqwest::Client::new();
    for query in [
        "limit=0",
        "limit=51",
        "offset=1000001",
        "status=trusted",
        "limit=no",
        "q=\u{2665}".repeat(501).as_str(),
    ] {
        let response = client
            .get(format!("{url}/api/knowledge?{query}"))
            .bearer_auth(READ)
            .send()
            .await
            .unwrap();
        assert_eq!(response.status(), 400, "{query}");
    }
    let page: Value = client
        .get(format!("{url}/api/knowledge?status=verified&limit=1"))
        .bearer_auth(READ)
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    assert_eq!(page["matching"], 1);
    assert_eq!(page["lessons"][0]["lesson"]["id"], "b");
    let database = dir.path().join("knowledge.sqlite3");
    fs::remove_file(&database).unwrap();
    assert_eq!(
        client
            .get(format!("{url}/api/knowledge"))
            .bearer_auth(READ)
            .send()
            .await
            .unwrap()
            .status(),
        503
    );
    assert!(!database.exists());
    stop(shutdown, task).await;
}

#[tokio::test]
async fn operational_apis_share_read_auth_origin_scope_and_bounded_metadata() {
    use serde_json::json;
    let (dir, config) = setup();
    fs::write(dir.path().join("backup-status.json"),serde_json::to_vec(&json!({"manager":"dreamer-backup-v1","state":"failed","error_code":"RAW_SECRET","private":"RAW_SECRET","last_attempt_at":"2026-11-05T12:00:00Z","last_success":{"completed_at":"2026-01-01T00:00:00Z","schema":5,"bytes":1234,"sha256":"a".repeat(64),"retained_count":3,"secret":"RAW_SECRET"}})).unwrap()).unwrap();
    let (url, shutdown, task) = start(&config).await;
    let client = reqwest::Client::new();
    for path in ["/api/activity", "/api/lesson?id=a"] {
        for credential in ["", WRITE] {
            assert_eq!(
                client
                    .get(format!("{url}{path}"))
                    .bearer_auth(credential)
                    .send()
                    .await
                    .unwrap()
                    .status(),
                401
            );
        }
        assert_eq!(
            client
                .get(format!("{url}{path}"))
                .bearer_auth(READ)
                .header("Origin", "https://untrusted.invalid")
                .send()
                .await
                .unwrap()
                .status(),
            403
        );
        assert_eq!(
            client
                .get(format!("{url}{path}"))
                .bearer_auth(READ)
                .header("sec-fetch-site", "cross-site")
                .send()
                .await
                .unwrap()
                .status(),
            403
        );
        assert_eq!(
            client
                .post(format!("{url}{path}"))
                .bearer_auth(READ)
                .send()
                .await
                .unwrap()
                .status(),
            405
        );
    }
    for path in [
        "/api/activity?repository=other/repo",
        "/api/lesson?id=other&repository=example/other",
    ] {
        assert_eq!(
            client
                .get(format!("{url}{path}"))
                .bearer_auth(READ)
                .send()
                .await
                .unwrap()
                .status(),
            403
        );
    }
    for path in [
        "/api/activity?limit=51",
        "/api/activity?offset=1000001",
        "/api/activity?raw=true",
        "/api/lesson?id=a&limit=0",
        "/api/lesson?id=",
    ] {
        assert_eq!(
            client
                .get(format!("{url}{path}"))
                .bearer_auth(READ)
                .send()
                .await
                .unwrap()
                .status(),
            400
        );
    }
    let response = client
        .get(format!("{url}/api/activity"))
        .bearer_auth(READ)
        .send()
        .await
        .unwrap();
    assert_eq!(response.headers()["cache-control"], "no-store");
    let value: Value = response.json().await.unwrap();
    assert_eq!(value["backup"]["state"], "failed");
    assert_eq!(value["backup"]["error_code"], "backup_failed");
    assert_eq!(value["backup"]["last_success"]["stale"], true);
    assert!(!value.to_string().contains("RAW_SECRET"));
    let value: Value = client
        .get(format!("{url}/api/lesson?id=c"))
        .bearer_auth(READ)
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    assert_eq!(value["stored"]["lesson"]["status"], "deprecated");
    assert_eq!(value["history"]["changes"], json!([]));
    let value: Value = client
        .get(format!("{url}/api/lesson?id=other"))
        .bearer_auth(READ)
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    assert_eq!(value["stored"], Value::Null);
    stop(shutdown, task).await;
}
