//! Always-on HTTP transport. Credentials are checked on every request, not sessions.

mod dashboard;

use super::DreamerMcp;
use crate::{config::Config, store::sqlite::SqliteStore};
use axum::{
    Json, Router,
    body::Body,
    extract::{Request, State},
    http::{HeaderMap, StatusCode, header},
    middleware::{self, Next},
    response::{IntoResponse, Response},
    routing::{any, get},
};
use rmcp::transport::streamable_http_server::{
    StreamableHttpServerConfig, StreamableHttpService, session::local::LocalSessionManager,
};
use std::{error::Error, sync::Arc};
use subtle::ConstantTimeEq;
use tokio_util::sync::CancellationToken;
use tower::ServiceExt;
use tower_http::limit::RequestBodyLimitLayer;

pub const READ_TOKEN_ENV: &str = "DREAMER_MCP_TOKEN";
pub const WRITE_TOKEN_ENV: &str = "DREAMER_MCP_WRITE_TOKEN";

/// Intentionally not Debug/Serialize: credentials must never enter logs or output.
pub struct Credentials {
    read: String,
    write: Option<String>,
}

impl Credentials {
    pub fn new(read: String, write: Option<String>) -> Result<Self, &'static str> {
        let valid = |value: &str| {
            value.len() >= 32
                && value
                    .bytes()
                    .all(|c| c.is_ascii_alphanumeric() || b"-_".contains(&c))
        };
        if !valid(&read) || write.as_deref().is_some_and(|value| !valid(value)) {
            return Err("MCP tokens must contain at least 32 ASCII letters/digits, '-' or '_'");
        }
        if write
            .as_deref()
            .is_some_and(|value| bool::from(read.as_bytes().ct_eq(value.as_bytes())))
        {
            return Err("read and write MCP tokens must be different");
        }
        Ok(Self { read, write })
    }

    pub fn from_env(allow_writes: bool) -> Result<Self, Box<dyn Error>> {
        let read = std::env::var(READ_TOKEN_ENV).map_err(|_| "DREAMER_MCP_TOKEN is required")?;
        let write = if allow_writes {
            Some(
                std::env::var(WRITE_TOKEN_ENV)
                    .map_err(|_| "--allow-writes requires DREAMER_MCP_WRITE_TOKEN")?,
            )
        } else {
            None
        };
        Ok(Self::new(read, write)?)
    }

    fn role(&self, headers: &HeaderMap) -> Option<bool> {
        if headers.get_all(header::AUTHORIZATION).iter().count() != 1 {
            return None;
        }
        let (scheme, token) = headers
            .get(header::AUTHORIZATION)?
            .to_str()
            .ok()?
            .split_once(' ')?;
        if !scheme.eq_ignore_ascii_case("Bearer") {
            return None;
        }
        if self
            .write
            .as_deref()
            .is_some_and(|value| bool::from(value.as_bytes().ct_eq(token.as_bytes())))
        {
            return Some(true);
        }
        bool::from(self.read.as_bytes().ct_eq(token.as_bytes())).then_some(false)
    }
}

type McpService = StreamableHttpService<DreamerMcp, LocalSessionManager>;

#[derive(Clone)]
struct HttpState {
    credentials: Arc<Credentials>,
    read: McpService,
    write: Option<McpService>,
    database: std::path::PathBuf,
    repository: String,
    repositories: Vec<String>,
    allowed_hosts: Vec<String>,
    database_slots: Arc<tokio::sync::Semaphore>,
}

fn service(handler: DreamerMcp, config: &Config, shutdown: &CancellationToken) -> McpService {
    let mut transport_config = StreamableHttpServerConfig::default()
        .with_json_response(true)
        .with_cancellation_token(shutdown.child_token())
        .enforce_origin_validation();
    transport_config.allowed_hosts = config.http.allowed_hosts.clone();
    // Each role owns a separate legacy session manager. A read token cannot
    // borrow a writer's session; modern requests are stateless in the SDK.
    StreamableHttpService::new(
        move || Ok(handler.clone()),
        Default::default(),
        transport_config,
    )
}

/// Build the authenticated routes. Does not initialize/migrate the database.
pub fn router(
    config: &Config,
    credentials: Credentials,
    shutdown: CancellationToken,
) -> Result<Router, Box<dyn Error>> {
    drop(SqliteStore::open_read_only(
        &config.data_dir.join("knowledge.sqlite3"),
    )?);
    let reader = DreamerMcp::new(config, false);
    let writer = credentials.write.as_ref().map(|_| {
        let mut writer = DreamerMcp::new(config, true);
        writer.database_slots = reader.database_slots.clone();
        service(writer, config, &shutdown)
    });
    let state = HttpState {
        credentials: Arc::new(credentials),
        database_slots: reader.database_slots.clone(),
        read: service(reader, config, &shutdown),
        write: writer,
        database: config.data_dir.join("knowledge.sqlite3"),
        repository: config.github.repository.clone(),
        repositories: crate::repository::configured(
            &config.github.repository,
            &config.github.repositories,
        ),
        allowed_hosts: config.http.allowed_hosts.clone(),
    };
    Ok(Router::new()
        .route("/", get(dashboard::index))
        .route("/dashboard.css", get(dashboard::css))
        .route("/dashboard.js", get(dashboard::javascript))
        .route("/api/knowledge", get(dashboard::knowledge))
        .route("/mcp", any(endpoint))
        .route("/health", any(endpoint))
        .layer(middleware::from_fn_with_state(state.clone(), allowed_host))
        .layer(RequestBodyLimitLayer::new(1024 * 1024))
        .with_state(state))
}

async fn allowed_host(State(state): State<HttpState>, request: Request, next: Next) -> Response {
    let host = request
        .headers()
        .get(header::HOST)
        .and_then(|value| value.to_str().ok())
        .and_then(|value| value.parse::<axum::http::uri::Authority>().ok());
    if request.headers().get_all(header::HOST).iter().count() != 1
        || !host.is_some_and(|value| {
            state
                .allowed_hosts
                .iter()
                .any(|host| host.eq_ignore_ascii_case(value.host()))
        })
    {
        return StatusCode::FORBIDDEN.into_response();
    }
    next.run(request).await
}

async fn endpoint(State(state): State<HttpState>, mut request: Request) -> Response {
    // Native MCP clients send no Origin. Reject browser-origin requests, including
    // null, until a browser client/CORS policy is deliberately supported.
    if request.headers().contains_key(header::ORIGIN) {
        return StatusCode::FORBIDDEN.into_response();
    }
    let Some(write) = state.credentials.role(request.headers()) else {
        return (
            StatusCode::UNAUTHORIZED,
            [(header::WWW_AUTHENTICATE, "Bearer realm=\"dreamer\"")],
        )
            .into_response();
    };
    // Authentication is complete; do not pass credentials to protocol handlers.
    request.headers_mut().remove(header::AUTHORIZATION);
    if request.uri().path() == "/health" {
        if request.method() != axum::http::Method::GET {
            return StatusCode::METHOD_NOT_ALLOWED.into_response();
        }
        let Ok(permit) = state.database_slots.clone().acquire_owned().await else {
            return StatusCode::SERVICE_UNAVAILABLE.into_response();
        };
        let healthy = tokio::task::spawn_blocking(move || {
            let _permit = permit;
            SqliteStore::open_read_only(&state.database).is_ok()
        })
        .await
        .unwrap_or(false);
        return if healthy {
            Json(serde_json::json!({"status": "ok"})).into_response()
        } else {
            StatusCode::SERVICE_UNAVAILABLE.into_response()
        };
    }
    let service = if write {
        state.write.expect("write token has a write service")
    } else {
        state.read
    };
    let response = match service.oneshot(request).await {
        Ok(response) => response,
        Err(never) => match never {},
    };
    response.map(Body::new).into_response()
}

pub async fn serve(config: &Config, allow_writes: bool) -> Result<(), Box<dyn Error>> {
    let credentials = Credentials::from_env(allow_writes)?;
    let shutdown = CancellationToken::new();
    let app = router(config, credentials, shutdown.clone())?;
    let listener = tokio::net::TcpListener::bind(config.http.listen).await?;
    eprintln!(
        "Dreamer HTTP MCP listening on http://{}/mcp",
        listener.local_addr()?
    );
    axum::serve(listener, app)
        .with_graceful_shutdown(async move {
            shutdown_signal().await;
            shutdown.cancel();
        })
        .await?;
    Ok(())
}

async fn shutdown_signal() {
    #[cfg(unix)]
    {
        match tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate()) {
            Ok(mut terminate) => {
                tokio::select! { _ = tokio::signal::ctrl_c() => {}, _ = terminate.recv() => {} }
            }
            Err(_) => {
                let _ = tokio::signal::ctrl_c().await;
            }
        }
    }
    #[cfg(not(unix))]
    {
        let _ = tokio::signal::ctrl_c().await;
    }
}
