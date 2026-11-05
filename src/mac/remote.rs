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

pub async fn supports(client: &RunningService<RoleClient, ()>, name: &str) -> Result<bool> {
    let tools = tokio::time::timeout(Duration::from_secs(30), client.list_all_tools())
        .await
        .map_err(|_| "Pi tool discovery timed out")?
        .map_err(|_| "Pi tool discovery failed")?;
    Ok(tools.iter().any(|tool| tool.name == name))
}

/// A small context reply; the Pi API and full stored evidence remain unchanged.
pub async fn recall(
    settings: &Settings,
    requested: Option<&str>,
    queries: Vec<String>,
    limit: usize,
    components: Vec<String>,
    paths: Vec<String>,
) -> Result<Value> {
    let repository = settings.select_repository(requested)?;
    let mut query = crate::retrieval::RecallQuery {
        repository: repository.clone(),
        queries,
        components,
        paths,
        limit,
    };
    query.validate()?;
    if limit > 10 {
        return Err("Mac recall limit must be 1–10".into());
    }
    query.queries = query
        .queries
        .into_iter()
        // Preserve identifier case until the shared tokenizer splits CamelCase.
        .map(|q| q.trim().to_owned())
        .collect::<BTreeSet<_>>()
        .into_iter()
        .collect();
    query.limit = 20;
    let client = connect(settings).await?;
    let result=async {
        let tools=tokio::time::timeout(Duration::from_secs(30),client.list_all_tools()).await.map_err(|_|"Pi tool discovery timed out")?.map_err(|_|"Pi tool discovery failed")?;
        let native=tools.iter().any(|tool|tool.name=="recall_lessons");
        let mut candidates=BTreeMap::new();let mut limited=false;let mut server_count=None;
        if native {
            let reply=call(&client,"recall_lessons",json!({"repository":repository,"queries":query.queries,"components":query.components,"paths":query.paths,"limit":20})).await?;
            limited=reply["truncated"]==true || reply["candidate_limit_reached"]==true;
            server_count=reply["matched_count"].as_u64();
            for stored in reply["lessons"].as_array().ok_or("missing Pi ranked lesson list")? {
                let stored:crate::lesson::StoredLesson=serde_json::from_value(stored.clone())?;
                if stored.lesson.repository!=repository {return Err("Pi returned a different repository".into());}
                candidates.insert(stored.lesson.id.clone(),stored);
            }
        } else {
            // Older Pis can still use the shared ranker over bounded literal results.
            // They cannot discover a context-only match that literal search omitted.
            for term in &query.queries {
                let reply=call(&client,"search_lessons",json!({"repository":repository,"query":term,"limit":100})).await?;
                let lessons=reply["lessons"].as_array().ok_or("missing Pi lesson list")?;
                limited|=lessons.len()==100;
                for stored in lessons {
                    let stored:crate::lesson::StoredLesson=serde_json::from_value(stored.clone())?;
                    if stored.lesson.repository!=repository {return Err("Pi returned a different repository".into());}
                    candidates.insert(stored.lesson.id.clone(),stored);
                }
            }
        }
        let ranked=crate::retrieval::rank(&query,candidates.into_values().collect(),limited)?;
        let matched_count=server_count.unwrap_or(ranked.matched_count as u64);
        let truncated=ranked.truncated;
        let mut records=BTreeMap::new();
        for hit in ranked.lessons {
            records.insert(hit.stored.lesson.id.clone(),(serde_json::to_value(&hit)?,hit.relevance.matched_queries.len()));
        }
        let mut reply=compact_recall(&repository,&query.queries,limit,records,truncated)?;
        reply["matched_count"]=json!(matched_count);
        reply["retrieval_mode"]=json!(if native {"ranked"} else {"legacy_fallback"});
        Ok(reply)
    }.await;
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
    ranked.sort_by(|(a_id, (a, a_matches)), (b_id, (b, b_matches))| {
        let a_score = a["relevance"]["score"]
            .as_u64()
            .unwrap_or(*a_matches as u64);
        let b_score = b["relevance"]["score"]
            .as_u64()
            .unwrap_or(*b_matches as u64);
        b_score.cmp(&a_score).then_with(|| a_id.cmp(b_id))
    });
    let mut reply = json!({"repository":repository,"queries":queries,"matched_count":total,"search_limited":search_limited,"truncated":total>limit || search_limited,"lessons":[],
        "guidance":"Task-word/context matches, not confidence scores. matching_queries counts word groups, not exact phrases; zero means context only. Fetch get_knowledge for selected IDs before applying; inspect conflict history, full applicability and evidence. Shortened text can omit exclusions. Zero results does not establish absence of knowledge."});
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
        let chats = lesson["conversation_evidence"].as_array();
        let chat_threads: BTreeSet<_> = chats
            .into_iter()
            .flatten()
            .filter_map(|e| e["thread_id"].as_str())
            .collect();
        let chat_claims: BTreeSet<_> = chats
            .into_iter()
            .flatten()
            .filter_map(|e| e["claim"].as_str())
            .collect();
        let prs: BTreeSet<_> = evidence
            .iter()
            .filter_map(|e| e["pull_request"]["number"].as_u64())
            .collect();
        let lifecycle = stored.get("lifecycle").map(|state| {
            json!({
                "conflict_count":state["conflicts"].as_array().map_or(0,Vec::len),
                "conflicts_truncated":state["conflicts_truncated"],
                "has_successor":state.get("superseded_by").is_some(),
            })
        });
        let scope=lesson.get("scope").map(|scope|json!({"components":scope["components"].as_array().map(|v|v.iter().take(3).collect::<Vec<_>>()),"paths":scope["paths"].as_array().map(|v|v.iter().take(3).collect::<Vec<_>>()),"has_exclusions":scope["excluded_terms"].as_array().is_some_and(|v|!v.is_empty()),"preview_only":true}));
        let context=stored.get("relevance").map(|r|json!({"score":r["score"],"matched_tokens":r["matched_tokens"].as_array().map(|v|v.iter().take(6).collect::<Vec<_>>()),"matched_components":r["matched_components"].as_array().map(|v|v.iter().take(3).collect::<Vec<_>>()),"matched_paths":r["matched_paths"].as_array().map(|v|v.iter().take(3).collect::<Vec<_>>())}));
        let preview = json!({"lesson":{"id":id,"summary":summary,"applies_when":applies_when,"status":lesson["status"]},"lifecycle":lifecycle,"scope":scope,"relevance":context,"matching_queries":matches,"evidence_count":evidence.len()+chats.map_or(0,Vec::len),"source_prs":prs.into_iter().take(4).collect::<Vec<_>>(),"source_chats":chat_threads.into_iter().take(3).collect::<Vec<_>>(),"conversation_claims":chat_claims.into_iter().take(3).collect::<Vec<_>>(),"shortened":{"summary":summary_shortened,"applies_when":applicability_shortened},"needs_details":true});
        reply["lessons"].as_array_mut().unwrap().push(preview);
        if serde_json::to_vec(&reply)?.len() > 11_700 {
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
            records.insert(format!("lesson-{n:02}"), (json!({"lesson":{"summary":"\"".repeat(1900),"applies_when":format!("{} Do not use for deletes.", "界".repeat(900)),"status":"candidate","evidence":[{"pull_request":{"number":123},"description":"full source".repeat(300)}],"provenance":{"producer":"agent"},"scope":{"components":["component".repeat(8)],"paths":vec![format!("src/{}/", "\"".repeat(140));3],"excluded_terms":["excluded"]}},"relevance":{"score":if n==19 {2}else{1},"matched_components":["component".repeat(8)],"matched_paths":vec![format!("src/{}/", "\"".repeat(140));3]}}), if n==19 {2} else {1}));
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
