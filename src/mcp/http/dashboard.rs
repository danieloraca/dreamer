//! Browser-facing read-only dashboard. Separate from the native-only MCP routes.
use super::HttpState;
use crate::{
    activity::RunQuery,
    lifecycle::HistoryQuery,
    store::{BrowseQuery, sqlite::SqliteStore},
};
use axum::{
    Json,
    extract::{Query, State},
    http::{HeaderMap, StatusCode, header},
    response::{IntoResponse, Response},
};
use serde::Deserialize;

fn asset(content_type: &'static str, body: &'static str) -> Response {
    ([
        (header::CONTENT_TYPE, content_type),
        (header::CACHE_CONTROL, "no-store"),
        (header::CONTENT_SECURITY_POLICY, "default-src 'none'; script-src 'self'; style-src 'self'; connect-src 'self'; base-uri 'none'; frame-ancestors 'none'; form-action 'none'"),
        (header::X_CONTENT_TYPE_OPTIONS, "nosniff"),
        (header::REFERRER_POLICY, "no-referrer"),
    ], body).into_response()
}

pub(super) async fn index() -> Response {
    asset(
        "text/html; charset=utf-8",
        include_str!("../../../web/dashboard.html"),
    )
}

pub(super) async fn css() -> Response {
    asset(
        "text/css; charset=utf-8",
        include_str!("../../../web/dashboard.css"),
    )
}

pub(super) async fn javascript() -> Response {
    asset(
        "text/javascript; charset=utf-8",
        include_str!("../../../web/dashboard.js"),
    )
}

#[derive(Debug, Default, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub(super) struct BrowseInput {
    q: String,
    repository: Option<String>,
    status: Option<String>,
    limit: Option<usize>,
    offset: usize,
}

pub(super) async fn knowledge(
    State(state): State<HttpState>,
    headers: HeaderMap,
    input: Result<Query<BrowseInput>, axum::extract::rejection::QueryRejection>,
) -> Response {
    if let Err(response) = authorize(&state, &headers) {
        return auth_error(response);
    }
    let Ok(Query(input)) = input else {
        return StatusCode::BAD_REQUEST.into_response();
    };
    let repository = match crate::repository::select(
        &state.repository,
        &state.repositories,
        input.repository.as_deref(),
    ) {
        Ok(repository) => repository,
        Err(_) => return StatusCode::FORBIDDEN.into_response(),
    };
    let query = BrowseQuery {
        repository,
        text: input.q,
        status: input.status,
        limit: input.limit.unwrap_or(25),
        offset: input.offset,
    };
    let Ok(permit) = state.database_slots.acquire_owned().await else {
        return StatusCode::SERVICE_UNAVAILABLE.into_response();
    };
    let repositories = state.repositories;
    let default_repository = state.repository;
    let result = tokio::task::spawn_blocking(move || {
        let _permit = permit;
        SqliteStore::open_read_only(&state.database)?.browse(&query)
    })
    .await;
    let mut response = match result {
        Ok(Ok(page)) => {
            let mut value = serde_json::to_value(page).expect("knowledge page serialization");
            value["repositories"] = serde_json::json!(repositories);
            value["default_repository"] = serde_json::json!(default_repository);
            Json(value).into_response()
        }
        Ok(Err(crate::store::sqlite::StoreError::Invalid(_))) => {
            StatusCode::BAD_REQUEST.into_response()
        }
        _ => StatusCode::SERVICE_UNAVAILABLE.into_response(),
    };
    response
        .headers_mut()
        .insert(header::CACHE_CONTROL, "no-store".parse().unwrap());
    response
        .headers_mut()
        .insert(header::X_CONTENT_TYPE_OPTIONS, "nosniff".parse().unwrap());
    response
}

fn auth_error(status: StatusCode) -> Response {
    let mut response = status.into_response();
    if status == StatusCode::UNAUTHORIZED {
        response.headers_mut().insert(
            header::WWW_AUTHENTICATE,
            "Bearer realm=\"dreamer-dashboard\"".parse().unwrap(),
        );
    }
    response
}
fn authorize(state: &HttpState, headers: &HeaderMap) -> Result<(), StatusCode> {
    // The browser only needs the read token. No cookies, sessions or write routes.
    if headers
        .get("sec-fetch-site")
        .is_some_and(|value| value == "cross-site")
    {
        return Err(StatusCode::FORBIDDEN);
    }
    if let Some(origin) = headers.get(header::ORIGIN) {
        let host = headers
            .get(header::HOST)
            .and_then(|host| host.to_str().ok())
            .unwrap_or("");
        if origin != format!("http://{host}").as_str()
            && origin != format!("https://{host}").as_str()
        {
            return Err(StatusCode::FORBIDDEN);
        }
    }
    if state.credentials.role(headers) != Some(false) {
        return Err(StatusCode::UNAUTHORIZED);
    }
    Ok(())
}

fn json_response(
    result: Result<
        Result<serde_json::Value, crate::store::sqlite::StoreError>,
        tokio::task::JoinError,
    >,
) -> Response {
    let mut response = match result {
        Ok(Ok(value)) => Json(value).into_response(),
        Ok(Err(crate::store::sqlite::StoreError::Invalid(_))) => {
            StatusCode::BAD_REQUEST.into_response()
        }
        _ => StatusCode::SERVICE_UNAVAILABLE.into_response(),
    };
    response
        .headers_mut()
        .insert(header::CACHE_CONTROL, "no-store".parse().unwrap());
    response
        .headers_mut()
        .insert(header::X_CONTENT_TYPE_OPTIONS, "nosniff".parse().unwrap());
    response
}
#[derive(Default, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub(super) struct ActivityInput {
    repository: Option<String>,
    limit: Option<usize>,
    offset: usize,
}

// Expose a fixed metadata allowlist, never arbitrary file contents or error text.
fn backup_status(database: &std::path::Path) -> serde_json::Value {
    use serde_json::{Value, json};
    use std::io::Read;
    let missing = || json!({"state":"not_yet_reported"});
    let Some(parent) = database.parent() else {
        return missing();
    };
    let path = parent.join("backup-status.json");
    let Ok(metadata) = path.symlink_metadata() else {
        return missing();
    };
    if !metadata.is_file() || metadata.len() > 16384 {
        return json!({"state":"unreadable"});
    }
    let read = || -> Option<Value> {
        let mut text = String::new();
        std::fs::File::open(path)
            .ok()?
            .take(16385)
            .read_to_string(&mut text)
            .ok()?;
        serde_json::from_str(&text).ok()
    };
    let Some(value) = read() else {
        return json!({"state":"unreadable"});
    };
    if value["manager"] != "dreamer-backup-v1"
        || !matches!(value["state"].as_str(), Some("ok" | "failed"))
    {
        return json!({"state":"unreadable"});
    }
    let timestamp = |v: &Value| {
        v.as_str()
            .filter(|s| s.len() <= 64 && chrono::DateTime::parse_from_rfc3339(s).is_ok())
            .map(str::to_owned)
    };
    let mut status =
        json!({"state":value["state"],"last_attempt_at":timestamp(&value["last_attempt_at"])});
    let success = &value["last_success"];
    if let (Some(completed), Some(schema), Some(bytes), Some(hash)) = (
        timestamp(&success["completed_at"]),
        success["schema"].as_u64(),
        success["bytes"].as_u64(),
        success["sha256"]
            .as_str()
            .filter(|s| s.len() == 64 && s.bytes().all(|c| c.is_ascii_hexdigit())),
    ) {
        let stale = chrono::DateTime::parse_from_rfc3339(&completed)
            .is_ok_and(|date| chrono::Utc::now().signed_duration_since(date).num_hours() > 48);
        status["last_success"] = json!({"completed_at":completed,"schema":schema,"bytes":bytes,"sha256":hash,"retained_count":success["retained_count"].as_u64(),"stale":stale});
    }
    if value["state"] == "failed" {
        status["error_code"] = json!("backup_failed");
    }
    status
}
pub(super) async fn activity(
    State(state): State<HttpState>,
    headers: HeaderMap,
    input: Result<Query<ActivityInput>, axum::extract::rejection::QueryRejection>,
) -> Response {
    if let Err(response) = authorize(&state, &headers) {
        return auth_error(response);
    }
    let Ok(Query(input)) = input else {
        return StatusCode::BAD_REQUEST.into_response();
    };
    let Ok(repository) = crate::repository::select(
        &state.repository,
        &state.repositories,
        input.repository.as_deref(),
    ) else {
        return StatusCode::FORBIDDEN.into_response();
    };
    let Ok(permit) = state.database_slots.acquire_owned().await else {
        return StatusCode::SERVICE_UNAVAILABLE.into_response();
    };
    json_response(
        tokio::task::spawn_blocking(move || {
            let _permit = permit;
            let page = SqliteStore::open_read_only(&state.database)?.runs(&RunQuery {
                repository,
                limit: input.limit.unwrap_or(10),
                offset: input.offset,
            })?;
            let mut value = serde_json::to_value(page).expect("activity serialization");
            value["backup"] = backup_status(&state.database);
            Ok(value)
        })
        .await,
    )
}
#[derive(Default, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub(super) struct LessonInput {
    repository: Option<String>,
    id: String,
    offset: usize,
    limit: Option<usize>,
}
pub(super) async fn lesson(
    State(state): State<HttpState>,
    headers: HeaderMap,
    input: Result<Query<LessonInput>, axum::extract::rejection::QueryRejection>,
) -> Response {
    if let Err(response) = authorize(&state, &headers) {
        return auth_error(response);
    }
    let Ok(Query(input)) = input else {
        return StatusCode::BAD_REQUEST.into_response();
    };
    let Ok(repository) = crate::repository::select(
        &state.repository,
        &state.repositories,
        input.repository.as_deref(),
    ) else {
        return StatusCode::FORBIDDEN.into_response();
    };
    let Ok(permit) = state.database_slots.acquire_owned().await else {
        return StatusCode::SERVICE_UNAVAILABLE.into_response();
    };
    let result = tokio::task::spawn_blocking(move || {
        let _permit = permit;
        let store = SqliteStore::open_read_only(&state.database)?;
        // Validate pagination/identity even when there is no matching lesson.
        let (stored, history) = store.lesson_details(&HistoryQuery {
            repository: repository.clone(),
            id: input.id.clone(),
            change_id: None,
            offset: input.offset,
            limit: input.limit.unwrap_or(20),
        })?;
        Ok(serde_json::json!({"repository":repository,"stored":stored,"history":history}))
    })
    .await;
    let mut response = json_response(result);
    if response.status() == StatusCode::OK {
        // The response schema represents a missing lesson with stored:null.
        response
            .headers_mut()
            .insert(header::CACHE_CONTROL, "no-store".parse().unwrap());
    }
    response
}
