// ─────────────────────────────────────────────
//  Research mode - chained searches, read pages, one dossier
//
//  Small local models will not reliably chain five searches on their
//  own, so depth is a DETERMINISTIC pipeline behind a single action:
//  plan diverse query angles (one quick model call, with a derived
//  fallback so planning can never sink the run), run every search,
//  read the top pages, and return a source-numbered dossier. The
//  agentic loop's model then writes from real material.
//
//  Every fetch rides the gated egress path (compliance + redaction +
//  audit) like any other search.
// ─────────────────────────────────────────────

use crate::AppState;

const MAX_QUERIES: usize = 4;
const RESULTS_PER_QUERY: usize = 8;
const MAX_PAGES_READ: usize = 6;
const PER_PAGE_CHARS: usize = 2500;
const DOSSIER_CAP: usize = 22_000;

/// Derived fallback angles when the planning call fails or returns
/// junk - research must never die on the planner.
pub(crate) fn fallback_queries(topic: &str) -> Vec<String> {
    let t = topic.trim();
    vec![
        t.to_string(),
        format!("{} overview facts", t),
        format!("{} statistics numbers", t),
        format!("{} problems criticism", t),
    ]
}

/// Ask the local model for diverse search angles; strict JSON array.
async fn plan_queries(topic: &str) -> Vec<String> {
    let system = "You plan web research. Reply with STRICT JSON only - a flat array of 3-4 short, DIVERSE search queries (different angles: fundamentals, numbers, recent developments, counterpoints). No prose, no fences.";
    let history = vec![
        crate::Message {
            id: uuid::Uuid::new_v4().to_string(),
            role: "system".into(),
            content: system.into(),
            context: None,
            agent_id: None,
            timestamp: chrono::Utc::now().to_rfc3339(),
            tool_turns: None,
        },
        crate::Message {
            id: uuid::Uuid::new_v4().to_string(),
            role: "user".into(),
            content: format!("Topic: {}", topic),
            context: None,
            agent_id: None,
            timestamp: chrono::Utc::now().to_rfc3339(),
            tool_turns: None,
        },
    ];
    let client = reqwest::Client::new();
    let raw = match crate::call_local_llm(&client, &history).await {
        Ok(r) => r,
        Err(_) => return fallback_queries(topic),
    };
    let cleaned = crate::strip_think_blocks(&raw);
    let parsed: Option<Vec<String>> = serde_json::from_str(cleaned.trim())
        .ok()
        .or_else(|| {
            let s = cleaned.find('[')?;
            let e = cleaned.rfind(']')?;
            serde_json::from_str(&cleaned[s..=e]).ok()
        });
    match parsed {
        Some(qs) => {
            let qs: Vec<String> = qs
                .into_iter()
                .map(|q| q.trim().to_string())
                .filter(|q| !q.is_empty() && q.len() < 120)
                .take(MAX_QUERIES)
                .collect();
            if qs.is_empty() { fallback_queries(topic) } else { qs }
        }
        None => fallback_queries(topic),
    }
}

/// Assemble the dossier from (query, results) pairs and page extracts.
/// Pure, so it is testable; the numbering ties extracts to sources.
pub(crate) fn assemble_dossier(
    topic: &str,
    per_query: &[(String, Vec<serde_json::Value>)],
    extracts: &[(usize, String, String, String)], // (source_no, title, url, text)
) -> String {
    let mut out = format!("# Research dossier: {}\n", topic);
    let mut source_no = 0;
    let mut seen: std::collections::HashSet<String> = std::collections::HashSet::new();
    for (query, results) in per_query {
        out.push_str(&format!("\n## Search: {}\n", query));
        for r in results {
            let url = r["url"].as_str().unwrap_or("");
            if url.is_empty() || !seen.insert(url.to_string()) {
                continue;
            }
            source_no += 1;
            out.push_str(&format!(
                "[{}] {} - {}\n    {}\n",
                source_no,
                r["title"].as_str().unwrap_or(""),
                url,
                r["snippet"].as_str().unwrap_or(""),
            ));
        }
    }
    if !extracts.is_empty() {
        out.push_str("\n## Page extracts (read in full)\n");
        for (no, title, url, text) in extracts {
            out.push_str(&format!("\n### [{}] {} ({})\n{}\n", no, title, url, text));
        }
    }
    out.push_str(
        "\nWrite from this material. Cite claims against the numbered sources where it matters.\n",
    );
    if out.len() > DOSSIER_CAP {
        let mut end = DOSSIER_CAP;
        while !out.is_char_boundary(end) {
            end -= 1;
        }
        out.truncate(end);
        out.push_str("\n[dossier truncated]");
    }
    out
}

/// Run the pipeline. Progress lands on the step ribbon throughout.
pub async fn run(state: &AppState, app: &tauri::AppHandle, topic: &str) -> Result<String, String> {
    let topic = topic.trim();
    if topic.is_empty() {
        return Err("research needs a topic".to_string());
    }

    crate::bob_core::bob_step(app, &format!("Planning research on '{}'…", topic));
    let queries = plan_queries(topic).await;

    let mut per_query: Vec<(String, Vec<serde_json::Value>)> = Vec::new();
    for q in &queries {
        crate::bob_core::bob_step(app, &format!("Searching: {}…", q));
        match crate::egress::web_search(state, q, RESULTS_PER_QUERY).await {
            Ok(results) => per_query.push((q.clone(), results)),
            Err(_) => per_query.push((q.clone(), Vec::new())),
        }
    }
    let total_hits: usize = per_query.iter().map(|(_, r)| r.len()).sum();
    if total_hits == 0 {
        return Err(
            "Research found nothing - the search provider may be rate-limiting. Try again in a minute."
                .to_string(),
        );
    }

    // Read the top pages across all queries: first hit of each query
    // first (angle diversity), then second hits, until the budget.
    let mut to_read: Vec<(usize, String, String)> = Vec::new(); // (source_no, title, url)
    {
        let mut seen = std::collections::HashSet::new();
        let mut numbered: Vec<(usize, String, String)> = Vec::new();
        let mut n = 0;
        for (_, results) in &per_query {
            for r in results {
                let url = r["url"].as_str().unwrap_or("").to_string();
                if url.is_empty() || !seen.insert(url.clone()) {
                    continue;
                }
                n += 1;
                numbered.push((n, r["title"].as_str().unwrap_or("").to_string(), url));
            }
        }
        for rank in 0..RESULTS_PER_QUERY {
            for (qi, (_, results)) in per_query.iter().enumerate() {
                if to_read.len() >= MAX_PAGES_READ {
                    break;
                }
                if let Some(r) = results.get(rank) {
                    let url = r["url"].as_str().unwrap_or("");
                    if let Some(entry) = numbered.iter().find(|(_, _, u)| u == url) {
                        if !to_read.iter().any(|(_, _, u)| u == url) {
                            to_read.push(entry.clone());
                        }
                    }
                    let _ = qi;
                }
            }
        }
    }

    let mut extracts: Vec<(usize, String, String, String)> = Vec::new();
    for (no, title, url) in &to_read {
        let domain = url.split('/').nth(2).unwrap_or(url);
        crate::bob_core::bob_step(app, &format!("Reading {}…", domain));
        if let Ok((status, body)) = crate::egress::web_fetch(state, url).await {
            if status < 400 {
                let text = crate::tools::html_readable_text(&body, PER_PAGE_CHARS);
                if text.len() > 200 {
                    extracts.push((*no, title.clone(), url.clone(), text));
                }
            }
        }
    }

    crate::bob_core::bob_step(app, "Compiling the dossier…");
    Ok(assemble_dossier(topic, &per_query, &extracts))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fallback_queries_are_diverse_and_nonempty() {
        let qs = fallback_queries("local llms");
        assert_eq!(qs.len(), 4);
        assert!(qs.iter().all(|q| q.contains("local llms")));
        let unique: std::collections::HashSet<&String> = qs.iter().collect();
        assert_eq!(unique.len(), 4);
    }

    #[test]
    fn dossier_numbers_sources_and_dedupes_urls() {
        let per_query = vec![
            (
                "q1".to_string(),
                vec![
                    serde_json::json!({"title":"A","url":"https://a.com","snippet":"sa"}),
                    serde_json::json!({"title":"B","url":"https://b.com","snippet":"sb"}),
                ],
            ),
            (
                "q2".to_string(),
                vec![serde_json::json!({"title":"A again","url":"https://a.com","snippet":"dup"})],
            ),
        ];
        let extracts = vec![(1usize, "A".to_string(), "https://a.com".to_string(), "body text".to_string())];
        let d = assemble_dossier("cheese", &per_query, &extracts);
        assert!(d.contains("[1] A - https://a.com"));
        assert!(d.contains("[2] B - https://b.com"));
        assert_eq!(d.matches("https://a.com").count(), 2, "dup url listed once + extract");
        assert!(d.contains("### [1] A"));
        assert!(d.contains("Write from this material"));
    }

    #[test]
    fn dossier_respects_cap() {
        let big = "x".repeat(30_000);
        let per_query = vec![("q".to_string(), vec![serde_json::json!({"title":"T","url":"https://t.com","snippet": big})])];
        let d = assemble_dossier("t", &per_query, &[]);
        assert!(d.len() <= DOSSIER_CAP + 32);
        assert!(d.ends_with("[dossier truncated]"));
    }
}
