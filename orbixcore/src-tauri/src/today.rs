// ─────────────────────────────────────────────
//  Today context - Bob always knows the day
//
//  "What should I do today" kept failing model-side: one model built a
//  workflow to interrogate the user, another opened Calendar and told
//  the user to look, a third gave generic productivity advice with no
//  tool calls at all (field failures 2026-08-03). Prompt rules moved
//  the needle but never closed it - so the day is now read
//  MECHANICALLY: a background task refreshes a digest of today's
//  events from both Calendar and Outlook every few minutes, and the
//  digest rides in the system prompt. Any model can answer a day
//  question from context; no tool-calling competence required.
// ─────────────────────────────────────────────

use std::sync::RwLock;

static SNAPSHOT: RwLock<Option<(i64, String)>> = RwLock::new(None);

/// How stale a snapshot may get before refresh() rebuilds it.
const TTL_SECS: i64 = 300;

/// Current digest for the system prompt; empty string when nothing has
/// been read yet (first minutes after launch, or bridges unavailable).
pub fn snapshot() -> String {
    SNAPSHOT
        .read()
        .ok()
        .and_then(|g| g.as_ref().map(|(_, s)| s.clone()))
        .unwrap_or_default()
}

fn set_snapshot(s: String) {
    if let Ok(mut g) = SNAPSHOT.write() {
        *g = Some((chrono::Utc::now().timestamp(), s));
    }
}

fn is_fresh() -> bool {
    SNAPSHOT
        .read()
        .ok()
        .and_then(|g| g.as_ref().map(|(at, _)| chrono::Utc::now().timestamp() - at < TTL_SECS))
        .unwrap_or(false)
}

/// Read today's events from one bridge app into compact lines.
async fn read_source(app: &str) -> Option<Vec<String>> {
    let args = serde_json::json!({ "days": 1 });
    let fut = crate::app_bridge::run_bridge_action(app, "upcoming_events", &args);
    let v = tokio::time::timeout(std::time::Duration::from_secs(25), fut)
        .await
        .ok()?
        .ok()?;
    let items = v.get("items")?.as_array()?;
    let mut lines = Vec::new();
    for it in items.iter().take(20) {
        let get = |k: &str| it.get(k).and_then(|x| x.as_str()).unwrap_or("").trim().to_string();
        let title = get("title");
        if title.is_empty() {
            continue;
        }
        // ISO timestamps -> HH:MM for prompt compactness.
        let hhmm = |s: &str| -> String {
            s.get(11..16).map(|t| t.to_string()).unwrap_or_else(|| s.to_string())
        };
        let mut line = format!("- {} {}-{} {}", app, hhmm(&get("start")), hhmm(&get("end")), title);
        let loc = get("location");
        if !loc.is_empty() {
            line.push_str(&format!(" @ {}", loc));
        }
        lines.push(line);
    }
    Some(lines)
}

/// Rebuild the digest if stale. Failures degrade to an honest line per
/// source rather than poisoning the whole snapshot.
pub async fn refresh(force: bool) {
    if !force && is_fresh() {
        return;
    }
    let mut sections: Vec<String> = Vec::new();
    let mut any_source_ok = false;
    // Graph-first: with a connected Microsoft account, the work calendar
    // comes from Graph and the AppleScript Outlook read (vestigial on
    // new Outlook) is skipped entirely.
    let graph_available = crate::graph_outlook::access_token().await.is_ok();
    let local_sources: &[&str] = if graph_available { &["Calendar"] } else { &["Calendar", "Outlook"] };
    for app in local_sources {
        match read_source(app).await {
            Some(lines) if !lines.is_empty() => {
                any_source_ok = true;
                sections.extend(lines);
            }
            Some(_) => {
                any_source_ok = true; // read fine, empty day
            }
            None => {}
        }
    }

    // The REAL work calendar: new Outlook hides its data from
    // AppleScript (the scriptable calendar is a vestigial empty one),
    // but a connected Microsoft account reads it via Graph - same
    // authenticated channel the mail triggers use. Token refresh is
    // persisted so the next read is warm.
    match graph_work_calendar().await {
        Ok(lines) => {
            any_source_ok = true;
            sections.extend(lines);
        }
        Err(e) if e.contains("AADSTS700082") || e.contains("invalid_grant") => {
            // The Microsoft sign-in lapsed (refresh tokens die after 90
            // days of disuse). Surface it IN the day digest so Bob tells
            // the user the actionable truth instead of showing a
            // mysteriously empty work calendar.
            any_source_ok = true;
            sections.push(
                "- Outlook work calendar unavailable: the Microsoft sign-in expired. Reconnect the Outlook account under Build > Triggers > Email accounts to see work events again.".to_string(),
            );
        }
        Err(e) if !e.is_empty() => {
            eprintln!("[today] outlook graph calendar: {}", e);
        }
        _ => {}
    }
    let digest = if sections.is_empty() {
        if any_source_ok {
            "No events today in Calendar or Outlook.".to_string()
        } else {
            // Bridges unreachable (permissions not granted yet, apps
            // missing). Say nothing rather than claiming an empty day.
            String::new()
        }
    } else {
        sections.join("\n")
    };
    set_snapshot(digest);
}

/// Today's events from connected Microsoft accounts via Graph. Empty
/// error string = no account connected (not worth logging).
async fn graph_work_calendar() -> Result<Vec<String>, String> {
    let token = crate::graph_outlook::access_token().await?;
    match crate::graph_outlook::calendar(&token, 1).await {
        Err(e) if e == "401" => {
            // Stored expiry lied; force one refresh through the token
            // helper (it persists rotations) and retry.
            let token = crate::graph_outlook::access_token().await?;
            crate::graph_outlook::calendar(&token, 1).await
        }
        other => other,
    }
}

/// Launch-time background loop: first read shortly after startup, then
/// a refresh on a fixed cadence. Spawned once from main setup.
pub fn spawn_refresher() {
    tauri::async_runtime::spawn(async {
        tokio::time::sleep(std::time::Duration::from_secs(20)).await;
        loop {
            refresh(true).await;
            tokio::time::sleep(std::time::Duration::from_secs(TTL_SECS as u64)).await;
        }
    });
}

/// The prompt block. Empty when nothing is known (costs no tokens).
pub fn prompt_block() -> String {
    let snap = snapshot();
    if snap.is_empty() {
        return String::new();
    }
    format!(
        "=== TODAY (auto-read from Calendar and Outlook, refreshed every few minutes) ===\n{}\n\
        When the user asks about their day, schedule, meetings, or what to do, answer DIRECTLY from this data in the user's language - do not tell them to check an app, and do not ask them for it. If more detail is needed, app_bridge can read deeper.\n\n",
        snap
    )
}
