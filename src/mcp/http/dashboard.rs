//! Browser-facing read-only dashboard. Separate from the native-only MCP routes.
use super::HttpState;
use crate::store::{BrowseQuery, sqlite::SqliteStore};
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
    // The browser only needs the read token. No cookies, sessions or write routes.
    if headers
        .get("sec-fetch-site")
        .is_some_and(|value| value == "cross-site")
    {
        return StatusCode::FORBIDDEN.into_response();
    }
    if let Some(origin) = headers.get(header::ORIGIN) {
        let host = headers
            .get(header::HOST)
            .and_then(|host| host.to_str().ok())
            .unwrap_or("");
        if origin != format!("http://{host}").as_str()
            && origin != format!("https://{host}").as_str()
        {
            return StatusCode::FORBIDDEN.into_response();
        }
    }
    if state.credentials.role(&headers) != Some(false) {
        return (
            StatusCode::UNAUTHORIZED,
            [(
                header::WWW_AUTHENTICATE,
                "Bearer realm=\"dreamer-dashboard\"",
            )],
        )
            .into_response();
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
