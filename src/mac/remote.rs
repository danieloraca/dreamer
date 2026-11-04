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
use std::{
    collections::{BTreeMap, BTreeSet},
    fs,
    time::Duration,
};

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

/// A small context reply; the Pi API and full stored evidence remain unchanged.
pub async fn recall(
    settings: &Settings,
    requested: Option<&str>,
    queries: Vec<String>,
    limit: usize,
) -> Result<Value> {
    if queries.is_empty()
        || queries.len() > 6
        || queries.iter().any(|q| q.trim().is_empty() || q.len() > 200)
        || !(1..=10).contains(&limit)
    {
        return Err("recall needs 1–6 short query terms and a limit of 1–10".into());
    }
    let repository = settings.select_repository(requested)?;
    let queries: Vec<_> = queries
        .into_iter()
        .map(|q| q.trim().to_ascii_lowercase())
        .collect::<BTreeSet<_>>()
        .into_iter()
        .collect();
    let client = connect(settings).await?;
    let result = async {
        let mut lessons = BTreeMap::new();
        let mut search_limited = false;
        for query in &queries {
            let value = call(
                &client,
                "search_lessons",
                json!({"repository":repository,"query":query,"limit":20}),
            )
            .await?;
            let matches = value["lessons"]
                .as_array()
                .ok_or("missing Pi lesson list")?;
            search_limited |= matches.len() == 20;
            let mut seen = BTreeSet::new();
            for stored in matches {
                let id = stored["lesson"]["id"]
                    .as_str()
                    .ok_or("missing lesson identity")?;
                if stored["lesson"]["repository"] != repository {
                    return Err("Pi returned a different repository".into());
                }
                if seen.insert(id) {
                    let entry = lessons
                        .entry(id.to_string())
                        .or_insert((stored.clone(), 0usize));
                    entry.1 += 1;
                }
            }
        }
        compact_recall(&repository, &queries, limit, lessons, search_limited)
    }
    .await;
    let _ = client.cancel().await;
    result
}

pub async fn get(settings: &Settings, requested: Option<&str>, id: &str) -> Result<Value> {
    let repository = settings.select_repository(requested)?;
    if id.trim().is_empty() || id.len() > 1000 {
        return Err("supply a lesson ID from recall".into());
    }
    let client = connect(settings).await?;
    let result = call(
        &client,
        "get_lesson",
        json!({"repository":repository,"id":id}),
    )
    .await;
    let _ = client.cancel().await;
    let result = result?;
    if result["found"] == true
        && (result["lesson"]["lesson"]["repository"] != repository
            || result["lesson"]["lesson"]["id"] != id)
    {
        return Err("Pi returned a different lesson identity".into());
    }
    Ok(result)
}

fn shortened(text: &str, maximum: usize) -> (&str, bool) {
    let mut end = text.len().min(maximum);
    while !text.is_char_boundary(end) {
        end -= 1;
    }
    (&text[..end], end < text.len())
}

fn compact_recall(
    repository: &str,
    queries: &[String],
    limit: usize,
    lessons: BTreeMap<String, (Value, usize)>,
    search_limited: bool,
) -> Result<Value> {
    let total = lessons.len();
    let mut ranked: Vec<_> = lessons.into_iter().collect();
    // Deterministic lexical overlap, not a semantic confidence score.
    ranked.sort_by(|(a_id, (_, a_matches)), (b_id, (_, b_matches))| {
        b_matches.cmp(a_matches).then_with(|| a_id.cmp(b_id))
    });
    let mut reply = json!({"repository":repository,"queries":queries,"matched_count":total,"search_limited":search_limited,"truncated":total>limit || search_limited,"lessons":[],
        "guidance":"Compact lexical matches, not confidence scores. Fetch get_knowledge for selected IDs before applying; inspect full applicability and evidence. Shortened text can omit exclusions. Zero matches does not establish absence of knowledge."});
    for (id, (stored, matches)) in ranked.into_iter().take(limit) {
        let lesson = &stored["lesson"];
        let (summary, summary_shortened) = shortened(
            lesson["summary"].as_str().ok_or("missing lesson summary")?,
            320,
        );
        let (applies_when, applicability_shortened) = shortened(
            lesson["applies_when"]
                .as_str()
                .ok_or("missing lesson applicability")?,
            640,
        );
        let evidence = lesson["evidence"].as_array().ok_or("missing evidence")?;
        let prs: BTreeSet<_> = evidence
            .iter()
            .filter_map(|e| e["pull_request"]["number"].as_u64())
            .collect();
        let preview = json!({"lesson":{"id":id,"summary":summary,"applies_when":applies_when,"status":lesson["status"]},"matching_queries":matches,"evidence_count":evidence.len(),"source_prs":prs.into_iter().take(4).collect::<Vec<_>>(),"shortened":{"summary":summary_shortened,"applies_when":applicability_shortened},"needs_details":true});
        reply["lessons"].as_array_mut().unwrap().push(preview);
        if serde_json::to_vec(&reply)?.len() > 12_000 {
            reply["lessons"].as_array_mut().unwrap().pop();
            reply["truncated"] = json!(true);
            break;
        }
    }
    Ok(reply)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn recall_bounds_context_without_silently_dropping_exclusions() {
        let mut records = BTreeMap::new();
        for n in 0..20 {
            records.insert(format!("lesson-{n:02}"), (json!({"lesson":{"summary":"\"".repeat(1900),"applies_when":format!("{} Do not use for deletes.", "界".repeat(900)),"status":"candidate","evidence":[{"pull_request":{"number":123},"description":"full source".repeat(300)}],"provenance":{"producer":"agent"}}}), if n==19 {2} else {1}));
        }
        let reply =
            compact_recall("example/project", &["account".into()], 10, records, true).unwrap();
        assert!(serde_json::to_vec(&reply).unwrap().len() <= 12_000);
        assert_eq!(reply["lessons"][0]["lesson"]["id"], "lesson-19");
        assert_eq!(reply["truncated"], true);
        let previews = reply["lessons"].as_array().unwrap();
        assert!(!previews.is_empty());
        for preview in previews {
            assert_eq!(preview["shortened"]["applies_when"], true);
            assert_eq!(preview["needs_details"], true);
            assert!(preview["lesson"].get("evidence").is_none());
        }
    }
    #[test]
    fn normal_recall_defaults_to_five_and_retains_full_short_applicability() {
        let records = (0..8).map(|n| (format!("lesson-{n}"), (json!({"lesson":{"summary":"Apply account scope before pagination.","applies_when":"For account lists; exclude public endpoints.","status":"candidate","evidence":[]}}),1))).collect();
        let reply = compact_recall(
            "example/project",
            &["account".into()],
            super::super::evidence::default_recall_limit(),
            records,
            false,
        )
        .unwrap();
        assert_eq!(reply["lessons"].as_array().unwrap().len(), 5);
        assert_eq!(reply["matched_count"], 8);
        assert_eq!(reply["lessons"][0]["shortened"]["applies_when"], false);
        assert_eq!(
            reply["lessons"][0]["lesson"]["applies_when"],
            "For account lists; exclude public endpoints."
        );
    }
}
