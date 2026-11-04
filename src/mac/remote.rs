use super::{Result, Settings};
use rmcp::{
    RoleClient, ServiceExt,
    model::CallToolRequestParams,
    service::RunningService,
    transport::{
        StreamableHttpClientTransport, streamable_http_client::StreamableHttpClientTransportConfig,
    },
};
use serde_json::{Value, json};
use std::{collections::BTreeMap, fs, time::Duration};

pub async fn connect(settings: &Settings) -> Result<RunningService<RoleClient, ()>> {
    let meta = fs::symlink_metadata(&settings.writer_token_file)?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        if !meta.is_file() || meta.permissions().mode() & 0o077 != 0 {
            return Err("writer token file must be a regular private file (0600)".into());
        }
    }
    if meta.len() > 4096 {
        return Err("unexpected writer credential file size".into());
    }
    let source = fs::read_to_string(&settings.writer_token_file)?;
    let lines: Vec<_> = source
        .lines()
        .filter(|line| !line.trim().is_empty() && !line.trim_start().starts_with('#'))
        .collect();
    let token = if lines.len() == 1 {
        lines[0].strip_prefix("DREAMER_MCP_WRITE_TOKEN=")
    } else {
        None
    }
    .ok_or("writer file must contain one DREAMER_MCP_WRITE_TOKEN assignment")?;
    if token.len() < 32
        || !token
            .bytes()
            .all(|c| c.is_ascii_alphanumeric() || b"-_".contains(&c))
    {
        return Err("invalid writer credential format".into());
    }
    let client = reqwest::Client::builder()
        .redirect(reqwest::redirect::Policy::none())
        .no_proxy()
        .build()?;
    let transport = StreamableHttpClientTransport::with_client(
        client,
        StreamableHttpClientTransportConfig::with_uri(settings.endpoint.clone()).auth_header(token),
    );
    tokio::time::timeout(Duration::from_secs(20), ().serve(transport))
        .await
        .map_err(|_| "Pi MCP connection timed out")?
        .map_err(|_| "Pi MCP connection failed; check service and credentials".into())
}

pub async fn call(
    client: &RunningService<RoleClient, ()>,
    name: &str,
    args: Value,
) -> Result<Value> {
    let value = tokio::time::timeout(
        Duration::from_secs(30),
        client.call_tool(
            CallToolRequestParams::new(name.to_string()).with_arguments(
                args.as_object()
                    .ok_or("arguments must be an object")?
                    .clone(),
            ),
        ),
    )
    .await
    .map_err(|_| "Pi MCP call timed out; a write may have completed, retry the same input")?
    .map_err(|_| "Pi MCP call failed; a write may have completed, retry the same input")?;
    if value.is_error == Some(true) {
        return Err("Pi rejected the operation (scope, schema, conflict or storage error); no server content logged".into());
    }
    value
        .structured_content
        .ok_or_else(|| "Pi returned no structured result".into())
}

pub async fn recall(settings: &Settings, queries: Vec<String>) -> Result<Value> {
    if queries.is_empty()
        || queries.len() > 6
        || queries.iter().any(|q| q.trim().is_empty() || q.len() > 200)
    {
        return Err("recall needs 1–6 short, nonempty query terms".into());
    }
    let client = connect(settings).await?;
    let result = async {
        let mut lessons = BTreeMap::new();
        for query in &queries {
            let value = call(&client, "search_lessons", json!({"repository":settings.repository,"query":query,"limit":20})).await?;
            for stored in value["lessons"].as_array().ok_or("missing Pi lesson list")? {
                let id = stored["lesson"]["id"].as_str().ok_or("missing lesson identity")?;
                lessons.insert(id.to_string(), stored.clone());
            }
        }
        Ok(json!({"repository":settings.repository,"queries":queries,"truncated":lessons.len()>20,"lessons":lessons.into_values().take(20).collect::<Vec<_>>(),
            "guidance":"Use only applicable lessons after inspecting their evidence. Candidates are unverified; lexical matches can include exclusions. Zero matches is not proof that no relevant knowledge exists."}))
    }.await;
    let _ = client.cancel().await;
    result
}
