use serde::{Deserialize, Serialize};
use uuid::Uuid;
use chrono::Utc;

// ─────────────────────────────────────────────
//  Trigger Source Types
// ─────────────────────────────────────────────

#[derive(Clone, Serialize, Deserialize, Debug, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum TriggerSource {
    Email,
    Webhook,
    FileWatch,
}

#[derive(Clone, Serialize, Deserialize, Debug, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum EventStatus {
    Triggered,
    Running,
    Completed,
    Failed,
    Skipped,
}

// ─────────────────────────────────────────────
//  Email Account
// ─────────────────────────────────────────────

#[derive(Clone, Serialize, Deserialize, Debug, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum EmailProvider {
    Outlook,
}

#[derive(Clone, Serialize, Deserialize, Debug, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum EmailAccountStatus {
    Connected,
    Disconnected,
    Expired,
    Error,
}

#[derive(Clone, Serialize, Deserialize, Debug)]
pub struct EmailAccount {
    pub id: String,
    pub provider: EmailProvider,
    pub email: String,
    pub display_name: String,
    pub status: EmailAccountStatus,
    pub access_token: Option<String>,
    pub refresh_token: Option<String>,
    pub token_expires_at: Option<String>,
    pub poll_interval_secs: u64,
    pub last_polled: Option<String>,
    pub last_message_id: Option<String>,
    pub created_at: String,
}

impl EmailAccount {
    pub fn new(provider: EmailProvider, email: String, display_name: String) -> Self {
        Self {
            id: Uuid::new_v4().to_string(),
            provider,
            email,
            display_name,
            status: EmailAccountStatus::Disconnected,
            access_token: None,
            refresh_token: None,
            token_expires_at: None,
            poll_interval_secs: 120,
            last_polled: None,
            last_message_id: None,
            created_at: Utc::now().to_rfc3339(),
        }
    }
}

// ─────────────────────────────────────────────
//  Email Message (metadata only)
// ─────────────────────────────────────────────

/// Attachment metadata (name + MIME type, no binary content)
#[derive(Clone, Serialize, Deserialize, Debug)]
pub struct AttachmentMeta {
    pub id: String,
    pub filename: String,
    pub mime_type: String,
    pub size: u64,
}

#[derive(Clone, Serialize, Deserialize, Debug)]
pub struct EmailMeta {
    pub id: String,
    pub provider_id: String,
    pub account_id: String,
    pub from: String,
    pub subject: String,
    pub snippet: String,
    pub body_text: Option<String>,
    pub labels: Vec<String>,
    pub attachments: Vec<AttachmentMeta>,
    pub received_at: String,
    pub is_read: bool,
}

// ─────────────────────────────────────────────
//  Rule Engine
// ─────────────────────────────────────────────

#[derive(Clone, Serialize, Deserialize, Debug, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum RuleConditionType {
    SenderContains,
    SubjectContains,
    BodyContains,
    HasLabel,
    SenderEquals,
    SubjectMatches,
}

#[derive(Clone, Serialize, Deserialize, Debug)]
pub struct RuleCondition {
    pub condition_type: RuleConditionType,
    pub value: String,
}

#[derive(Clone, Serialize, Deserialize, Debug, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum RuleLogic {
    And,
    Or,
}

#[derive(Clone, Serialize, Deserialize, Debug)]
pub struct TriggerRule {
    pub id: String,
    pub name: String,
    pub source: TriggerSource,
    pub enabled: bool,
    pub priority: usize,
    pub conditions: Vec<RuleCondition>,
    pub logic: RuleLogic,
    pub workflow_id: String,
    pub extract_template: Option<String>,
    pub created_at: String,
    pub updated_at: String,
}

impl TriggerRule {
    pub fn new(name: String, source: TriggerSource, workflow_id: String) -> Self {
        let now = Utc::now().to_rfc3339();
        Self {
            id: Uuid::new_v4().to_string(),
            name,
            source,
            enabled: true,
            priority: 100,
            conditions: Vec::new(),
            logic: RuleLogic::And,
            workflow_id,
            extract_template: None,
            created_at: now.clone(),
            updated_at: now,
        }
    }

    /// Check if an email matches this rule
    pub fn matches_email(&self, email: &EmailMeta) -> bool {
        if self.conditions.is_empty() {
            return false;
        }

        let results: Vec<bool> = self.conditions.iter().map(|cond| {
            match cond.condition_type {
                RuleConditionType::SenderContains => {
                    email.from.to_lowercase().contains(&cond.value.to_lowercase())
                }
                RuleConditionType::SenderEquals => {
                    email.from.to_lowercase() == cond.value.to_lowercase()
                }
                RuleConditionType::SubjectContains => {
                    email.subject.to_lowercase().contains(&cond.value.to_lowercase())
                }
                RuleConditionType::SubjectMatches => {
                    regex::Regex::new(&cond.value)
                        .map(|re| re.is_match(&email.subject))
                        .unwrap_or(false)
                }
                RuleConditionType::BodyContains => {
                    email.body_text.as_deref().unwrap_or("")
                        .to_lowercase().contains(&cond.value.to_lowercase())
                }
                RuleConditionType::HasLabel => {
                    email.labels.iter().any(|l| l.to_lowercase() == cond.value.to_lowercase())
                }
            }
        }).collect();

        match self.logic {
            RuleLogic::And => results.iter().all(|&r| r),
            RuleLogic::Or => results.iter().any(|&r| r),
        }
    }
}

// ─────────────────────────────────────────────
//  Webhook Endpoint
// ─────────────────────────────────────────────

#[derive(Clone, Serialize, Deserialize, Debug)]
pub struct WebhookEndpoint {
    pub id: String,
    pub name: String,
    pub path: String,
    pub secret: Option<String>,
    pub enabled: bool,
    pub workflow_id: String,
    pub received_count: usize,
    pub created_at: String,
}

impl WebhookEndpoint {
    pub fn new(name: String, workflow_id: String) -> Self {
        let path = format!("/hook/{}", Uuid::new_v4().to_string().split('-').next().unwrap_or("x"));
        Self {
            id: Uuid::new_v4().to_string(),
            name,
            path,
            secret: None,
            enabled: true,
            workflow_id,
            received_count: 0,
            created_at: Utc::now().to_rfc3339(),
        }
    }
}

// ─────────────────────────────────────────────
//  File Watch
// ─────────────────────────────────────────────

#[derive(Clone, Serialize, Deserialize, Debug, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum FileWatchEvent {
    Created,
    Modified,
    Deleted,
    Any,
}

#[derive(Clone, Serialize, Deserialize, Debug)]
pub struct FileWatch {
    pub id: String,
    pub name: String,
    pub path: String,
    pub recursive: bool,
    pub event_filter: FileWatchEvent,
    pub pattern: Option<String>,
    pub enabled: bool,
    pub workflow_id: String,
    pub created_at: String,
}

impl FileWatch {
    pub fn new(name: String, path: String, workflow_id: String) -> Self {
        Self {
            id: Uuid::new_v4().to_string(),
            name,
            path,
            recursive: true,
            event_filter: FileWatchEvent::Any,
            pattern: None,
            enabled: true,
            workflow_id,
            created_at: Utc::now().to_rfc3339(),
        }
    }
}

// ─────────────────────────────────────────────
//  Event Log
// ─────────────────────────────────────────────

#[derive(Clone, Serialize, Deserialize, Debug)]
pub struct EventLogEntry {
    pub id: String,
    pub source: TriggerSource,
    pub source_detail: String,
    pub rule_id: Option<String>,
    pub rule_name: Option<String>,
    pub workflow_id: Option<String>,
    pub workflow_name: Option<String>,
    pub status: EventStatus,
    pub message: String,
    pub data: Option<serde_json::Value>,
    pub timestamp: String,
}

impl EventLogEntry {
    pub fn new(source: TriggerSource, source_detail: String, message: String) -> Self {
        Self {
            id: Uuid::new_v4().to_string(),
            source,
            source_detail,
            rule_id: None,
            rule_name: None,
            workflow_id: None,
            workflow_name: None,
            status: EventStatus::Triggered,
            message,
            data: None,
            timestamp: Utc::now().to_rfc3339(),
        }
    }
}

// ─────────────────────────────────────────────
//  Trigger Store
// ─────────────────────────────────────────────

pub struct TriggerStore {
    pub email_accounts: Vec<EmailAccount>,
    pub rules: Vec<TriggerRule>,
    pub webhooks: Vec<WebhookEndpoint>,
    pub file_watches: Vec<FileWatch>,
    pub event_log: Vec<EventLogEntry>,
    pub processed_email_ids: Vec<String>,
}

impl Default for TriggerStore {
    fn default() -> Self {
        Self {
            email_accounts: Vec::new(),
            rules: Vec::new(),
            webhooks: Vec::new(),
            file_watches: Vec::new(),
            event_log: Vec::new(),
            processed_email_ids: Vec::new(),
        }
    }
}

impl TriggerStore {
    pub fn load() -> Self {
        let data_dir = crate::storage::data_dir();
        let path = data_dir.join("triggers.json");
        if let Ok(data) = std::fs::read_to_string(&path) {
            let saved: SavedTriggerState = serde_json::from_str(&data).unwrap_or_default();
            Self::from(saved)
        } else {
            Self::default()
        }
    }

    pub fn save(&self) {
        let data_dir = crate::storage::data_dir();
        let _ = std::fs::create_dir_all(&data_dir);
        let path = data_dir.join("triggers.json");
        if let Ok(json) = serde_json::to_string_pretty(&SavedTriggerState {
            email_accounts: self.email_accounts.clone(),
            rules: self.rules.clone(),
            webhooks: self.webhooks.clone(),
            file_watches: self.file_watches.clone(),
            processed_email_ids: self.processed_email_ids.clone(),
        }) {
            let _ = crate::storage::atomic_write(&path, json);
        }
    }

    pub fn add_log(&mut self, entry: EventLogEntry) {
        self.event_log.insert(0, entry);
        // Keep last 500 entries
        if self.event_log.len() > 500 {
            self.event_log.truncate(500);
        }
    }
}

// For serialization (event_log is not persisted)
#[derive(Serialize, Deserialize)]
struct SavedTriggerState {
    email_accounts: Vec<EmailAccount>,
    rules: Vec<TriggerRule>,
    webhooks: Vec<WebhookEndpoint>,
    file_watches: Vec<FileWatch>,
    processed_email_ids: Vec<String>,
}

impl Default for SavedTriggerState {
    fn default() -> Self {
        Self {
            email_accounts: Vec::new(),
            rules: Vec::new(),
            webhooks: Vec::new(),
            file_watches: Vec::new(),
            processed_email_ids: Vec::new(),
        }
    }
}

// Allow loading from the saved format
impl From<SavedTriggerState> for TriggerStore {
    fn from(s: SavedTriggerState) -> Self {
        Self {
            email_accounts: s.email_accounts,
            rules: s.rules,
            webhooks: s.webhooks,
            file_watches: s.file_watches,
            event_log: Vec::new(),
            processed_email_ids: s.processed_email_ids,
        }
    }
}

// ─────────────────────────────────────────────
//  Email Polling
// ─────────────────────────────────────────────

/// Fetch emails from Outlook/Microsoft Graph
pub async fn fetch_outlook_emails(
    access_token: &str,
    _after_id: Option<&str>,
    max_results: u32,
) -> Result<Vec<EmailMeta>, String> {
    let client = reqwest::Client::new();
    let url = format!(
        "https://graph.microsoft.com/v1.0/me/messages?$top={}&$select=id,from,subject,bodyPreview,body,receivedDateTime,isRead,categories&$orderby=receivedDateTime desc",
        max_results
    );

    let resp = client.get(&url)
        .header("Authorization", format!("Bearer {}", access_token))
        .send().await
        .map_err(|e| format!("Outlook API error: {}", e))?;

    if !resp.status().is_success() {
        let status = resp.status();
        let body = resp.text().await.unwrap_or_default();
        return Err(format!("Outlook API {}: {}", status, body));
    }

    let data: serde_json::Value = resp.json().await
        .map_err(|e| format!("Parse error: {}", e))?;

    let messages = data["value"].as_array().cloned().unwrap_or_default();
    let mut emails = Vec::new();

    for msg in messages {
        let from = msg["from"]["emailAddress"]["address"].as_str().unwrap_or("").to_string();
        let subject = msg["subject"].as_str().unwrap_or("").to_string();
        let snippet = msg["bodyPreview"].as_str().unwrap_or("").to_string();
        let provider_id = msg["id"].as_str().unwrap_or("").to_string();
        let received_at = msg["receivedDateTime"].as_str().unwrap_or("").to_string();
        let is_read = msg["isRead"].as_bool().unwrap_or(true);
        let labels: Vec<String> = msg["categories"].as_array()
            .unwrap_or(&Vec::new())
            .iter()
            .filter_map(|c| c.as_str().map(String::from))
            .collect();

        // Get full body content (HTML or text)
        let body_content = msg["body"]["content"].as_str().unwrap_or("").to_string();

        emails.push(EmailMeta {
            id: Uuid::new_v4().to_string(),
            provider_id,
            account_id: String::new(),
            from,
            subject,
            snippet,
            body_text: if body_content.is_empty() { None } else { Some(body_content) },
            labels,
            attachments: Vec::new(),
            received_at,
            is_read,
        });
    }

    Ok(emails)
}

/// Refresh a Microsoft (Outlook) token via the public-client flow -
/// same endpoint the device-code sign-in used. Returns (access_token,
/// new_refresh_token_if_rotated, expires_at).
pub async fn refresh_outlook_token(
    client_id: &str,
    refresh_token: &str,
) -> Result<(String, Option<String>, String), String> {
    let client = reqwest::Client::new();
    let resp = client
        .post("https://login.microsoftonline.com/common/oauth2/v2.0/token")
        .form(&[
            ("client_id", client_id),
            ("refresh_token", refresh_token),
            ("grant_type", "refresh_token"),
            ("scope", "openid profile email offline_access User.Read Mail.Read Mail.Send Calendars.Read Files.Read"),
        ])
        .send()
        .await
        .map_err(|e| format!("Token refresh error: {}", e))?;
    let data: serde_json::Value = resp.json().await.map_err(|e| format!("Parse error: {}", e))?;
    let access_token = data["access_token"]
        .as_str()
        .ok_or_else(|| format!(
            "No access_token in refresh response: {}",
            data["error_description"].as_str().unwrap_or("unknown")
        ))?
        .to_string();
    let new_refresh = data["refresh_token"].as_str().map(|s| s.to_string());
    let expires_in = data["expires_in"].as_u64().unwrap_or(3600);
    let expires_at = (Utc::now() + chrono::Duration::seconds(expires_in as i64)).to_rfc3339();
    Ok((access_token, new_refresh, expires_at))
}

/// Today's events from the account's REAL Outlook calendar via Microsoft
/// Graph - the data new Outlook hides from AppleScript. Returns compact
/// "HH:MM-HH:MM title @ location" lines in local time.
pub async fn fetch_outlook_calendar_today(access_token: &str) -> Result<Vec<String>, String> {
    let now = chrono::Local::now();
    let day_start = now.date_naive().and_hms_opt(0, 0, 0).unwrap();
    let day_end = day_start + chrono::Duration::days(1);
    let start_utc = day_start
        .and_local_timezone(chrono::Local)
        .single()
        .map(|t| t.with_timezone(&Utc))
        .unwrap_or_else(Utc::now);
    let end_utc = start_utc + chrono::Duration::days(1);
    let _ = day_end;
    let url = format!(
        "https://graph.microsoft.com/v1.0/me/calendarview?startDateTime={}&endDateTime={}&$select=subject,start,end,location&$orderby=start/dateTime&$top=20",
        start_utc.format("%Y-%m-%dT%H:%M:%SZ"),
        end_utc.format("%Y-%m-%dT%H:%M:%SZ"),
    );
    let client = reqwest::Client::new();
    let resp = client
        .get(&url)
        .header("Authorization", format!("Bearer {}", access_token))
        .send()
        .await
        .map_err(|e| format!("Graph calendar error: {}", e))?;
    if resp.status().as_u16() == 401 {
        return Err("401".to_string());
    }
    let data: serde_json::Value = resp.json().await.map_err(|e| format!("Parse error: {}", e))?;
    let mut out = Vec::new();
    for ev in data["value"].as_array().unwrap_or(&Vec::new()).iter().take(20) {
        let subject = ev["subject"].as_str().unwrap_or("(no title)");
        let to_local_hhmm = |v: &serde_json::Value| -> String {
            v["dateTime"]
                .as_str()
                .and_then(|s| chrono::NaiveDateTime::parse_from_str(&s[..19.min(s.len())], "%Y-%m-%dT%H:%M:%S").ok())
                .map(|n| {
                    chrono::DateTime::<Utc>::from_naive_utc_and_offset(n, Utc)
                        .with_timezone(&chrono::Local)
                        .format("%H:%M")
                        .to_string()
                })
                .unwrap_or_default()
        };
        let mut line = format!(
            "- Outlook (work) {}-{} {}",
            to_local_hhmm(&ev["start"]),
            to_local_hhmm(&ev["end"]),
            subject
        );
        if let Some(loc) = ev["location"]["displayName"].as_str() {
            if !loc.is_empty() {
                line.push_str(&format!(" @ {}", loc));
            }
        }
        out.push(line);
    }
    Ok(out)
}

/// Poll a single email account for new messages and match against rules
pub async fn poll_email_account(
    account: &EmailAccount,
    rules: &[TriggerRule],
    processed_ids: &[String],
) -> Result<Vec<(EmailMeta, Vec<TriggerRule>)>, String> {
    let token = account.access_token.as_deref()
        .ok_or("No access token")?;

    let emails = match account.provider {
        EmailProvider::Outlook => fetch_outlook_emails(token, account.last_message_id.as_deref(), 10).await?,
    };

    let email_rules: Vec<TriggerRule> = rules.iter()
        .filter(|r| r.source == TriggerSource::Email && r.enabled)
        .cloned()
        .collect();

    let mut matched = Vec::new();
    for mut email in emails {
        email.account_id = account.id.clone();
        // Skip already processed
        if processed_ids.contains(&email.provider_id) {
            continue;
        }
        let matching_rules: Vec<TriggerRule> = email_rules.iter()
            .filter(|r| r.matches_email(&email))
            .cloned()
            .collect();
        if !matching_rules.is_empty() {
            matched.push((email, matching_rules));
        }
    }

    Ok(matched)
}

// ─────────────────────────────────────────────
//  AI Data Extraction
// ─────────────────────────────────────────────

/// Use the local LLM to extract structured data from email text
pub async fn extract_email_data(
    email: &EmailMeta,
    template: &str,
) -> Result<serde_json::Value, String> {
    let prompt = format!(
        "Extract the following data from this email. Return ONLY valid JSON.\n\n\
        Template: {}\n\n\
        From: {}\n\
        Subject: {}\n\
        Body: {}\n\n\
        Return a JSON object with the extracted fields:",
        template,
        email.from,
        email.subject,
        email.body_text.as_deref().unwrap_or(&email.snippet),
    );

    let client = reqwest::Client::new();
    let resp = client.post("http://127.0.0.1:8787/v1/chat/completions")
        .json(&serde_json::json!({
            "model": "local",
            "messages": [{"role": "user", "content": prompt}],
            "temperature": 0.1,
            "max_tokens": 500,
        }))
        .send().await
        .map_err(|e| format!("LLM error: {}", e))?;

    let data: serde_json::Value = resp.json().await
        .map_err(|e| format!("Parse error: {}", e))?;

    let content = data["choices"][0]["message"]["content"].as_str().unwrap_or("{}");

    // Try to parse the JSON from the response
    serde_json::from_str(content)
        .or_else(|_| {
            // Try to find JSON in the response
            if let Some(start) = content.find('{') {
                if let Some(end) = content.rfind('}') {
                    return serde_json::from_str(&content[start..=end]);
                }
            }
            Ok(serde_json::json!({"raw": content}))
        })
        .map_err(|e| format!("JSON parse error: {}", e))
}

// ─────────────────────────────────────────────
//  File watch polling (W2.6)
// ─────────────────────────────────────────────

/// A cheap snapshot of a watched path used to detect changes between
/// polling ticks. Kept comparable and cloneable so the tick loop can hold
/// the previous snapshot per watch id in memory.
#[derive(Clone, Debug, PartialEq, Default)]
pub struct FileSnapshot {
    pub exists: bool,
    /// Modification time as a unix timestamp (seconds), when available.
    pub mtime: Option<i64>,
    pub size: Option<u64>,
}

/// Snapshot a path's current state. Missing paths return `exists: false`
/// rather than an error, so a watch on a not-yet-created file works.
pub fn snapshot_path(path: &str) -> FileSnapshot {
    match std::fs::metadata(path) {
        Ok(meta) => FileSnapshot {
            exists: true,
            mtime: meta
                .modified()
                .ok()
                .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
                .map(|d| d.as_secs() as i64),
            size: Some(meta.len()),
        },
        Err(_) => FileSnapshot { exists: false, mtime: None, size: None },
    }
}

/// Decide whether a change between two snapshots matches the watch's event
/// filter. Pure, so the firing rule is unit-testable without touching disk.
/// `prev` is `None` on the first tick (baseline), which never fires.
pub fn file_change_detected(
    filter: &FileWatchEvent,
    prev: Option<&FileSnapshot>,
    cur: &FileSnapshot,
) -> bool {
    let Some(prev) = prev else { return false }; // first observation: baseline only
    let created = !prev.exists && cur.exists;
    let deleted = prev.exists && !cur.exists;
    let modified = prev.exists && cur.exists && (prev.mtime != cur.mtime || prev.size != cur.size);
    match filter {
        FileWatchEvent::Created => created,
        FileWatchEvent::Deleted => deleted,
        FileWatchEvent::Modified => modified,
        FileWatchEvent::Any => created || deleted || modified,
    }
}

#[cfg(test)]
mod file_watch_tests {
    use super::*;

    fn snap(exists: bool, mtime: i64, size: u64) -> FileSnapshot {
        FileSnapshot { exists, mtime: Some(mtime), size: Some(size) }
    }

    #[test]
    fn first_observation_never_fires() {
        let cur = snap(true, 100, 10);
        assert!(!file_change_detected(&FileWatchEvent::Any, None, &cur));
    }

    #[test]
    fn creation_detected() {
        let prev = FileSnapshot::default(); // did not exist
        let cur = snap(true, 100, 10);
        assert!(file_change_detected(&FileWatchEvent::Created, Some(&prev), &cur));
        assert!(file_change_detected(&FileWatchEvent::Any, Some(&prev), &cur));
        assert!(!file_change_detected(&FileWatchEvent::Deleted, Some(&prev), &cur));
    }

    #[test]
    fn modification_detected_by_mtime_or_size() {
        let prev = snap(true, 100, 10);
        assert!(file_change_detected(&FileWatchEvent::Modified, Some(&prev), &snap(true, 200, 10)));
        assert!(file_change_detected(&FileWatchEvent::Modified, Some(&prev), &snap(true, 100, 20)));
        assert!(!file_change_detected(&FileWatchEvent::Modified, Some(&prev), &snap(true, 100, 10)));
    }

    #[test]
    fn deletion_detected() {
        let prev = snap(true, 100, 10);
        let cur = FileSnapshot::default();
        assert!(file_change_detected(&FileWatchEvent::Deleted, Some(&prev), &cur));
        assert!(!file_change_detected(&FileWatchEvent::Created, Some(&prev), &cur));
    }
}
