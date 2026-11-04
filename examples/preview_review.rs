//! Exercise curated local lessons through the real stdio MCP tools. No Pi writes.
use dreamer::{
    collector::write_private,
    lesson::Lesson,
    store::{LessonStore, sqlite::SqliteStore},
};
use rmcp::{ServiceExt, model::CallToolRequestParams, transport::TokioChildProcess};
use serde_json::{Value, json};
use std::{error::Error, path::PathBuf};

#[tokio::main]
async fn main() -> Result<(), Box<dyn Error>> {
    let mut args = std::env::args().skip(1);
    let binary = PathBuf::from(
        args.next()
            .ok_or("usage: preview_review BINARY LESSONS_JSON OUTPUT_JSON [QUERY ...]")?,
    );
    let input = PathBuf::from(args.next().ok_or("missing lessons JSON")?);
    let output = PathBuf::from(args.next().ok_or("missing output JSON")?);
    let mut queries: Vec<String> = args.collect();
    if queries.is_empty() {
        queries = vec![
            "export".into(),
            "event".into(),
            "callback".into(),
            "rollout".into(),
        ];
    }
    let lessons: Vec<Lesson> = serde_json::from_slice(&std::fs::read(input)?)?;
    let repository = lessons
        .first()
        .ok_or("lesson preview is empty")?
        .repository
        .clone();
    if lessons.iter().any(|lesson| lesson.repository != repository) {
        return Err("preview must contain one repository".into());
    }
    if output.exists() {
        return Err("output exists; preserve the previous retrieval log".into());
    }
    let directory = tempfile::tempdir()?;
    let mut store = SqliteStore::open(&directory.path().join("knowledge.sqlite3"))?;
    for lesson in &lessons {
        store.put_candidate(lesson)?;
    }
    drop(store);
    let config = directory.path().join("preview.toml");
    let content = toml::to_string(
        &json!({"data_dir":directory.path(),"github":{"repository":repository,"batch_size":10},"memory":{"default_search_limit":5}}),
    )?;
    write_private(&config, content.as_bytes())?;
    let mut command = tokio::process::Command::new(binary);
    command.arg("--config").arg(config).arg("mcp");
    let client = ().serve(TokioChildProcess::new(command)?).await?;
    let mut results = Vec::new();
    for query in queries {
        let result = client
            .call_tool(
                CallToolRequestParams::new("search_lessons").with_arguments(
                    json!({"repository":repository,"query":query,"limit":10})
                        .as_object()
                        .unwrap()
                        .clone(),
                ),
            )
            .await?;
        if result.is_error == Some(true) {
            return Err("preview MCP search failed".into());
        }
        let search = result
            .structured_content
            .ok_or("missing structured search result")?;
        let mut details = Vec::new();
        for stored in search["lessons"]
            .as_array()
            .ok_or("search returned no lesson list")?
        {
            let detail = client
                .call_tool(
                    CallToolRequestParams::new("get_lesson").with_arguments(
                        json!({"repository":repository,"id":stored["lesson"]["id"]})
                            .as_object()
                            .unwrap()
                            .clone(),
                    ),
                )
                .await?;
            if detail.is_error == Some(true) {
                return Err("preview MCP get failed".into());
            }
            details.push(
                detail
                    .structured_content
                    .ok_or("missing structured lesson")?,
            );
        }
        results.push(json!({"query":query,"search":search,"details":details}));
    }
    client.cancel().await?;
    let record: Value = json!({"transport":"real stdio MCP, temporary local SQLite","repository":repository,"lesson_count":lessons.len(),"published_to_pi":false,"results":results});
    write_private(&output, &serde_json::to_vec_pretty(&record)?)?;
    println!(
        "Preview MCP retrieval saved to {} (Pi unchanged)",
        output.display()
    );
    Ok(())
}
