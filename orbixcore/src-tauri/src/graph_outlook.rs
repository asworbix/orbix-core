// ─────────────────────────────────────────────
//  Graph-first Outlook - the user's own channel
//
//  When a Microsoft account is connected, Outlook data comes from
//  Microsoft Graph with the user's own token: it reads the REAL
//  mailbox and work calendar (new Outlook hides both from
//  AppleScript), needs no macOS Automation grant, and the constant
//  use keeps the refresh token alive (the March token died of 90-day
//  inactivity precisely because nothing exercised it). AppleScript
//  remains the fallback when no account is connected.
//
//  Device -> Microsoft directly. Token stored locally; no middleman.
// ─────────────────────────────────────────────

use chrono::Utc;

/// A fresh access token for the first connected Outlook account,
/// refreshing (and persisting rotated tokens) when needed.
/// Err("") = no account connected.
pub async fn access_token() -> Result<String, String> {
    let acct = {
        let store = crate::triggers::TriggerStore::load();
        store
            .email_accounts
            .iter()
            .find(|a| matches!(a.provider, crate::triggers::EmailProvider::Outlook))
            .cloned()
    };
    let Some(acct) = acct else { return Err(String::new()) };
    let token = acct.access_token.clone().ok_or("no token")?;

    // Expiry check by stored timestamp; unknown expiry = try and let a
    // 401 force the refresh at the call site.
    let expired = acct
        .token_expires_at
        .as_deref()
        .and_then(|s| chrono::DateTime::parse_from_rfc3339(s).ok())
        .map(|t| t.with_timezone(&Utc) <= Utc::now() + chrono::Duration::seconds(60))
        .unwrap_or(true);
    if !expired {
        return Ok(token);
    }
    refresh_and_persist(&acct).await
}

async fn refresh_and_persist(acct: &crate::triggers::EmailAccount) -> Result<String, String> {
    let refresh = acct
        .refresh_token
        .clone()
        .ok_or("Microsoft sign-in expired and no refresh token is stored - reconnect the Outlook account")?;
    let ids = crate::oauth::load_oauth_client_ids();
    let client_id = ids
        .get("microsoft")
        .cloned()
        .ok_or("no Microsoft client id configured")?;
    let (access, new_refresh, expires_at) =
        crate::triggers::refresh_outlook_token(&client_id, &refresh).await?;
    let mut store = crate::triggers::TriggerStore::load();
    if let Some(a) = store.email_accounts.iter_mut().find(|a| a.id == acct.id) {
        a.access_token = Some(access.clone());
        if let Some(r) = new_refresh {
            a.refresh_token = Some(r);
        }
        a.token_expires_at = Some(expires_at);
    }
    store.save();
    Ok(access)
}

/// Serve an Outlook bridge action from Graph. Returns Ok(None) when the
/// action has no Graph equivalent (caller falls back to AppleScript).
pub async fn bridge_action(
    token: &str,
    action_id: &str,
    args: &serde_json::Value,
) -> Result<Option<serde_json::Value>, String> {
    match action_id {
        "recent_messages" => {
            let limit = args["limit"].as_u64().unwrap_or(10).min(50) as u32;
            let emails = crate::triggers::fetch_outlook_emails(token, None, limit).await?;
            let items: Vec<serde_json::Value> = emails
                .iter()
                .map(|e| {
                    serde_json::json!({
                        "subject": e.subject,
                        "from_name": e.from,
                        "from_address": e.from,
                        "received": e.received_at,
                        "read": "",
                    })
                })
                .collect();
            Ok(Some(serde_json::json!({
                "count": items.len(),
                "items": items,
                "source": "Microsoft Graph (work account)",
            })))
        }
        "unread_count" => {
            let client = reqwest::Client::new();
            let resp = client
                .get("https://graph.microsoft.com/v1.0/me/mailFolders/inbox?$select=unreadItemCount")
                .header("Authorization", format!("Bearer {}", token))
                .send()
                .await
                .map_err(|e| format!("Graph error: {}", e))?;
            let data: serde_json::Value =
                resp.json().await.map_err(|e| format!("Parse error: {}", e))?;
            Ok(Some(serde_json::json!({
                "text": data["unreadItemCount"].as_u64().unwrap_or(0).to_string(),
                "source": "Microsoft Graph (work account)",
            })))
        }
        "upcoming_events" => {
            let days = args["days"].as_u64().unwrap_or(1).clamp(1, 31);
            let lines = calendar(token, days as i64).await?;
            let items: Vec<serde_json::Value> = lines
                .iter()
                .map(|l| serde_json::json!({ "title": l.clone(), "start": "", "end": "", "calendar": "Outlook (work)", "location": "" }))
                .collect();
            Ok(Some(serde_json::json!({
                "count": items.len(),
                "items": items,
                "source": "Microsoft Graph (work account)",
            })))
        }
        _ => Ok(None),
    }
}

/// Events for the next `days` days as compact local-time lines.
pub async fn calendar(token: &str, days: i64) -> Result<Vec<String>, String> {
    let now = chrono::Local::now();
    let day_start = now.date_naive().and_hms_opt(0, 0, 0).unwrap();
    let start_utc = day_start
        .and_local_timezone(chrono::Local)
        .single()
        .map(|t| t.with_timezone(&Utc))
        .unwrap_or_else(Utc::now);
    let end_utc = start_utc + chrono::Duration::days(days.max(1));
    let url = format!(
        "https://graph.microsoft.com/v1.0/me/calendarview?startDateTime={}&endDateTime={}&$select=subject,start,end,location&$orderby=start/dateTime&$top=30",
        start_utc.format("%Y-%m-%dT%H:%M:%SZ"),
        end_utc.format("%Y-%m-%dT%H:%M:%SZ"),
    );
    let client = reqwest::Client::new();
    let resp = client
        .get(&url)
        .header("Authorization", format!("Bearer {}", token))
        .send()
        .await
        .map_err(|e| format!("Graph calendar error: {}", e))?;
    if resp.status().as_u16() == 401 {
        return Err("401".to_string());
    }
    let data: serde_json::Value = resp.json().await.map_err(|e| format!("Parse error: {}", e))?;
    let mut out = Vec::new();
    for ev in data["value"].as_array().unwrap_or(&Vec::new()).iter().take(30) {
        let subject = ev["subject"].as_str().unwrap_or("(no title)");
        let hhmm = |v: &serde_json::Value| -> String {
            v["dateTime"]
                .as_str()
                .and_then(|s| {
                    chrono::NaiveDateTime::parse_from_str(&s[..19.min(s.len())], "%Y-%m-%dT%H:%M:%S").ok()
                })
                .map(|n| {
                    chrono::DateTime::<Utc>::from_naive_utc_and_offset(n, Utc)
                        .with_timezone(&chrono::Local)
                        .format("%H:%M")
                        .to_string()
                })
                .unwrap_or_default()
        };
        let mut line = format!("- Outlook (work) {}-{} {}", hhmm(&ev["start"]), hhmm(&ev["end"]), subject);
        if let Some(loc) = ev["location"]["displayName"].as_str() {
            if !loc.is_empty() {
                line.push_str(&format!(" @ {}", loc));
            }
        }
        out.push(line);
    }
    Ok(out)
}
