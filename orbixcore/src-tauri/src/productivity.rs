//! Mail and calendar as generic, provider-agnostic capabilities (W5.5).
//!
//! Inbox reads already existed (triggers::fetch_*_emails); sending mail and
//! creating/reading calendar events did not. This module adds them once, at
//! the capability level, so a provider inherits the same public API. The
//! public functions take a `Provider` + access token; per-provider request
//! shaping is the only thing that branches. Microsoft is the only provider
//! today; the enum keeps the seam for future ones.

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Provider {
    Microsoft,
}

impl Provider {
    pub fn from_str(s: &str) -> Option<Provider> {
        match s.trim().to_lowercase().as_str() {
            "microsoft" | "outlook" | "office365" | "o365" => Some(Provider::Microsoft),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CalendarEvent {
    pub id: Option<String>,
    pub subject: String,
    pub start: String,
    pub end: String,
    #[serde(default)]
    pub location: Option<String>,
    #[serde(default)]
    pub attendees: Vec<String>,
}

fn client() -> reqwest::Client {
    reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(30))
        .build()
        .unwrap_or_default()
}

// ─────────────────────────────────────────────
//  Send mail
// ─────────────────────────────────────────────

/// Send an email. `to` is a comma/space separated recipient list; `body` is
/// treated as HTML. Provider-agnostic entry point.
pub async fn send_mail(
    provider: Provider,
    access_token: &str,
    to: &str,
    subject: &str,
    body: &str,
) -> Result<(), String> {
    let recipients: Vec<String> = to
        .split([',', ';', ' '])
        .map(|s| s.trim())
        .filter(|s| !s.is_empty())
        .map(|s| s.to_string())
        .collect();
    if recipients.is_empty() {
        return Err("no recipients".to_string());
    }
    match provider {
        Provider::Microsoft => send_mail_microsoft(access_token, &recipients, subject, body).await,
    }
}

async fn send_mail_microsoft(token: &str, to: &[String], subject: &str, body: &str) -> Result<(), String> {
    let payload = serde_json::json!({
        "message": {
            "subject": subject,
            "body": { "contentType": "HTML", "content": body },
            "toRecipients": to.iter().map(|a| serde_json::json!({ "emailAddress": { "address": a } })).collect::<Vec<_>>(),
        },
        "saveToSentItems": true,
    });
    let resp = client()
        .post("https://graph.microsoft.com/v1.0/me/sendMail")
        .header("Authorization", format!("Bearer {}", token))
        .header("Content-Type", "application/json")
        .json(&payload)
        .send()
        .await
        .map_err(|e| format!("sendMail request failed: {}", e))?;
    // Graph returns 202 Accepted with an empty body on success.
    if resp.status().is_success() {
        Ok(())
    } else {
        let code = resp.status().as_u16();
        let msg = resp.text().await.unwrap_or_default();
        Err(format!("Graph sendMail HTTP {}: {}", code, msg))
    }
}

// ─────────────────────────────────────────────
//  Calendar: create
// ─────────────────────────────────────────────

/// Create a calendar event. `start`/`end` are RFC 3339 timestamps.
pub async fn calendar_create(
    provider: Provider,
    access_token: &str,
    subject: &str,
    start: &str,
    end: &str,
    attendees: &[String],
) -> Result<String, String> {
    match provider {
        Provider::Microsoft => calendar_create_microsoft(access_token, subject, start, end, attendees).await,
    }
}

async fn calendar_create_microsoft(token: &str, subject: &str, start: &str, end: &str, attendees: &[String]) -> Result<String, String> {
    let payload = serde_json::json!({
        "subject": subject,
        "start": { "dateTime": start, "timeZone": "UTC" },
        "end": { "dateTime": end, "timeZone": "UTC" },
        "attendees": attendees.iter().map(|a| serde_json::json!({
            "emailAddress": { "address": a }, "type": "required"
        })).collect::<Vec<_>>(),
    });
    let resp = client()
        .post("https://graph.microsoft.com/v1.0/me/events")
        .header("Authorization", format!("Bearer {}", token))
        .json(&payload)
        .send()
        .await
        .map_err(|e| format!("create event request failed: {}", e))?;
    if resp.status().is_success() {
        let v: serde_json::Value = resp.json().await.map_err(|e| format!("parse event: {}", e))?;
        Ok(v.get("id").and_then(|i| i.as_str()).unwrap_or("").to_string())
    } else {
        let code = resp.status().as_u16();
        Err(format!("Graph create event HTTP {}: {}", code, resp.text().await.unwrap_or_default()))
    }
}

// ─────────────────────────────────────────────
//  Calendar: list
// ─────────────────────────────────────────────

/// List upcoming calendar events (soonest first), capped at `max`.
pub async fn calendar_list(
    provider: Provider,
    access_token: &str,
    max: usize,
) -> Result<Vec<CalendarEvent>, String> {
    match provider {
        Provider::Microsoft => calendar_list_microsoft(access_token, max).await,
    }
}

async fn calendar_list_microsoft(token: &str, max: usize) -> Result<Vec<CalendarEvent>, String> {
    let url = format!(
        "https://graph.microsoft.com/v1.0/me/events?$top={}&$select=id,subject,start,end,location,attendees&$orderby=start/dateTime",
        max.min(50)
    );
    let resp = client()
        .get(&url)
        .header("Authorization", format!("Bearer {}", token))
        .send()
        .await
        .map_err(|e| format!("list events request failed: {}", e))?;
    if !resp.status().is_success() {
        return Err(format!("Graph list events HTTP {}", resp.status().as_u16()));
    }
    let v: serde_json::Value = resp.json().await.map_err(|e| format!("parse events: {}", e))?;
    let out = v.get("value").and_then(|a| a.as_array()).map(|arr| {
        arr.iter().map(|e| CalendarEvent {
            id: e.get("id").and_then(|x| x.as_str()).map(String::from),
            subject: e.get("subject").and_then(|x| x.as_str()).unwrap_or("").to_string(),
            start: e.pointer("/start/dateTime").and_then(|x| x.as_str()).unwrap_or("").to_string(),
            end: e.pointer("/end/dateTime").and_then(|x| x.as_str()).unwrap_or("").to_string(),
            location: e.pointer("/location/displayName").and_then(|x| x.as_str()).filter(|s| !s.is_empty()).map(String::from),
            attendees: e.get("attendees").and_then(|a| a.as_array()).map(|arr| {
                arr.iter().filter_map(|a| a.pointer("/emailAddress/address").and_then(|x| x.as_str()).map(String::from)).collect()
            }).unwrap_or_default(),
        }).collect()
    }).unwrap_or_default();
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn provider_parsing() {
        assert_eq!(Provider::from_str("Outlook"), Some(Provider::Microsoft));
        assert_eq!(Provider::from_str("office365"), Some(Provider::Microsoft));
        assert_eq!(Provider::from_str("gmail"), None);
        assert_eq!(Provider::from_str("nope"), None);
    }
}
