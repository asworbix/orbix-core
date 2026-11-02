//! Commands split out of main.rs (W0.3). Mechanical move, no behavior change.

#[allow(unused_imports)]
use crate::prelude::*;
#[allow(unused_imports)]
use crate::*;

// ─────────────────────────────────────────────
//  Event Triggers (Epic V)
// ─────────────────────────────────────────────

#[tauri::command]
pub(crate) fn list_email_accounts(state: State<'_, AppState>) -> Vec<triggers::EmailAccount> {
    state.automation.trigger_store.lock().unwrap_or_else(|e| e.into_inner()).email_accounts.clone()
}

#[tauri::command]
pub(crate) fn add_email_account(
    state: State<'_, AppState>,
    provider: triggers::EmailProvider,
    email: String,
    display_name: String,
    access_token: String,
    refresh_token: Option<String>,
) -> triggers::EmailAccount {
    let mut account = triggers::EmailAccount::new(provider, email, display_name);
    account.access_token = Some(access_token);
    account.refresh_token = refresh_token;
    account.status = triggers::EmailAccountStatus::Connected;

    let mut store = state.automation.trigger_store.lock().unwrap_or_else(|e| e.into_inner());
    store.email_accounts.push(account.clone());
    store.save();
    account
}

#[tauri::command]
pub(crate) fn update_email_account(
    state: State<'_, AppState>,
    account_id: String,
    poll_interval_secs: Option<u64>,
    enabled: Option<bool>,
) -> Result<triggers::EmailAccount, String> {
    let mut store = state.automation.trigger_store.lock().unwrap_or_else(|e| e.into_inner());
    let acct = store.email_accounts.iter_mut().find(|a| a.id == account_id)
        .ok_or("Account not found")?;
    if let Some(interval) = poll_interval_secs {
        acct.poll_interval_secs = interval;
    }
    if let Some(en) = enabled {
        acct.status = if en {
            triggers::EmailAccountStatus::Connected
        } else {
            triggers::EmailAccountStatus::Disconnected
        };
    }
    let result = acct.clone();
    store.save();
    Ok(result)
}

#[tauri::command]
pub(crate) fn remove_email_account(state: State<'_, AppState>, account_id: String) {
    let mut store = state.automation.trigger_store.lock().unwrap_or_else(|e| e.into_inner());
    store.email_accounts.retain(|a| a.id != account_id);
    store.save();
}

#[tauri::command]
pub(crate) fn list_trigger_rules(state: State<'_, AppState>) -> Vec<triggers::TriggerRule> {
    state.automation.trigger_store.lock().unwrap_or_else(|e| e.into_inner()).rules.clone()
}

#[tauri::command]
pub(crate) fn create_trigger_rule(
    state: State<'_, AppState>,
    name: String,
    source: triggers::TriggerSource,
    workflow_id: String,
    conditions: Vec<triggers::RuleCondition>,
    logic: triggers::RuleLogic,
    priority: Option<usize>,
    extract_template: Option<String>,
) -> triggers::TriggerRule {
    let mut rule = triggers::TriggerRule::new(name, source, workflow_id);
    rule.conditions = conditions;
    rule.logic = logic;
    if let Some(p) = priority {
        rule.priority = p;
    }
    rule.extract_template = extract_template;

    let mut store = state.automation.trigger_store.lock().unwrap_or_else(|e| e.into_inner());
    store.rules.push(rule.clone());
    // Sort by priority
    store.rules.sort_by_key(|r| r.priority);
    store.save();
    rule
}

#[tauri::command]
pub(crate) fn update_trigger_rule(
    state: State<'_, AppState>,
    rule_id: String,
    name: Option<String>,
    enabled: Option<bool>,
    conditions: Option<Vec<triggers::RuleCondition>>,
    logic: Option<triggers::RuleLogic>,
    priority: Option<usize>,
    workflow_id: Option<String>,
    extract_template: Option<String>,
) -> Result<triggers::TriggerRule, String> {
    let mut store = state.automation.trigger_store.lock().unwrap_or_else(|e| e.into_inner());
    let rule = store.rules.iter_mut().find(|r| r.id == rule_id)
        .ok_or("Rule not found")?;
    if let Some(n) = name { rule.name = n; }
    if let Some(e) = enabled { rule.enabled = e; }
    if let Some(c) = conditions { rule.conditions = c; }
    if let Some(l) = logic { rule.logic = l; }
    if let Some(p) = priority { rule.priority = p; }
    if let Some(w) = workflow_id { rule.workflow_id = w; }
    if let Some(t) = extract_template { rule.extract_template = Some(t); }
    rule.updated_at = chrono::Utc::now().to_rfc3339();
    let result = rule.clone();
    store.rules.sort_by_key(|r| r.priority);
    store.save();
    Ok(result)
}

#[tauri::command]
pub(crate) fn delete_trigger_rule(state: State<'_, AppState>, rule_id: String) {
    let mut store = state.automation.trigger_store.lock().unwrap_or_else(|e| e.into_inner());
    store.rules.retain(|r| r.id != rule_id);
    store.save();
}

#[tauri::command]
pub(crate) fn list_webhooks(state: State<'_, AppState>) -> Vec<triggers::WebhookEndpoint> {
    state.automation.trigger_store.lock().unwrap_or_else(|e| e.into_inner()).webhooks.clone()
}

#[tauri::command]
pub(crate) fn create_webhook(
    state: State<'_, AppState>,
    name: String,
    workflow_id: String,
    secret: Option<String>,
) -> triggers::WebhookEndpoint {
    let mut wh = triggers::WebhookEndpoint::new(name, workflow_id);
    wh.secret = secret;
    let mut store = state.automation.trigger_store.lock().unwrap_or_else(|e| e.into_inner());
    store.webhooks.push(wh.clone());
    store.save();
    wh
}

#[tauri::command]
pub(crate) fn delete_webhook(state: State<'_, AppState>, webhook_id: String) {
    let mut store = state.automation.trigger_store.lock().unwrap_or_else(|e| e.into_inner());
    store.webhooks.retain(|w| w.id != webhook_id);
    store.save();
}

#[tauri::command]
pub(crate) fn list_file_watches(state: State<'_, AppState>) -> Vec<triggers::FileWatch> {
    state.automation.trigger_store.lock().unwrap_or_else(|e| e.into_inner()).file_watches.clone()
}

#[tauri::command]
pub(crate) fn create_file_watch(
    state: State<'_, AppState>,
    name: String,
    path: String,
    workflow_id: String,
    recursive: Option<bool>,
    event_filter: Option<triggers::FileWatchEvent>,
    pattern: Option<String>,
) -> triggers::FileWatch {
    let mut fw = triggers::FileWatch::new(name, path, workflow_id);
    if let Some(r) = recursive { fw.recursive = r; }
    if let Some(ef) = event_filter { fw.event_filter = ef; }
    fw.pattern = pattern;
    let mut store = state.automation.trigger_store.lock().unwrap_or_else(|e| e.into_inner());
    store.file_watches.push(fw.clone());
    store.save();
    fw
}

#[tauri::command]
pub(crate) fn delete_file_watch(state: State<'_, AppState>, watch_id: String) {
    let mut store = state.automation.trigger_store.lock().unwrap_or_else(|e| e.into_inner());
    store.file_watches.retain(|w| w.id != watch_id);
    store.save();
}

#[tauri::command]
pub(crate) fn get_event_log(
    state: State<'_, AppState>,
    source_filter: Option<triggers::TriggerSource>,
    status_filter: Option<triggers::EventStatus>,
    limit: Option<usize>,
) -> Vec<triggers::EventLogEntry> {
    let store = state.automation.trigger_store.lock().unwrap_or_else(|e| e.into_inner());
    let max = limit.unwrap_or(100);
    store.event_log.iter()
        .filter(|e| {
            if let Some(ref sf) = source_filter {
                if &e.source != sf { return false; }
            }
            if let Some(ref stf) = status_filter {
                if &e.status != stf { return false; }
            }
            true
        })
        .take(max)
        .cloned()
        .collect()
}

#[tauri::command]
pub(crate) fn clear_event_log(state: State<'_, AppState>) {
    state.automation.trigger_store.lock().unwrap_or_else(|e| e.into_inner()).event_log.clear();
}

/// The incoming email as a structured payload, so a triggered workflow can
/// read {{trigger.from}} / {{trigger.subject}} / {{trigger.body}} and the AI
/// mapping step can process the real message.
fn email_to_trigger(email: &triggers::EmailMeta) -> serde_json::Value {
    serde_json::json!({
        "source": "email",
        "from": email.from,
        "subject": email.subject,
        "snippet": email.snippet,
        "body": email.body_text.clone().unwrap_or_default(),
        "labels": email.labels,
        "received_at": email.received_at,
        "is_read": email.is_read,
        "account_id": email.account_id,
        "message_id": email.provider_id,
    })
}

/// True when an account is due to poll: never polled, or at least
/// poll_interval_secs (floored at 30s) has elapsed since last_polled.
fn account_due(acct: &triggers::EmailAccount, now: chrono::DateTime<chrono::Utc>) -> bool {
    match &acct.last_polled {
        None => true,
        Some(ts) => match chrono::DateTime::parse_from_rfc3339(ts) {
            Ok(last) => {
                (now - last.with_timezone(&chrono::Utc)).num_seconds()
                    >= acct.poll_interval_secs.max(30) as i64
            }
            Err(_) => true,
        },
    }
}

#[tauri::command]
pub(crate) async fn poll_emails_now(
    state: State<'_, AppState>,
    app: tauri::AppHandle,
) -> Result<Vec<triggers::EventLogEntry>, String> {
    // Manual poll: force every connected account regardless of interval.
    Ok(poll_email_accounts(&state, &app, true).await)
}

/// Poll connected email accounts, run any matched trigger rule's workflow
/// with the email as its {{trigger.*}} payload. `force` ignores each
/// account's poll interval (used by the manual command); the autonomous
/// dispatcher passes `false` so each account is polled on its own cadence.
pub(crate) async fn poll_email_accounts(
    state: &AppState,
    app: &tauri::AppHandle,
    force: bool,
) -> Vec<triggers::EventLogEntry> {
    let now = chrono::Utc::now();
    let (accounts, rules, processed_ids) = {
        let store = state.automation.trigger_store.lock().unwrap_or_else(|e| e.into_inner());
        (
            store.email_accounts.clone(),
            store.rules.clone(),
            store.processed_email_ids.clone(),
        )
    };

    let mut new_events = Vec::new();

    for account in &accounts {
        if account.status != triggers::EmailAccountStatus::Connected {
            continue;
        }
        if !force && !account_due(account, now) {
            continue;
        }
        match triggers::poll_email_account(account, &rules, &processed_ids).await {
            Ok(matches) => {
                for (email, matched_rules) in matches {
                    // Mark as processed
                    {
                        let mut store = state.automation.trigger_store.lock().unwrap_or_else(|e| e.into_inner());
                        store.processed_email_ids.push(email.provider_id.clone());
                        // Keep last 1000 processed IDs
                        if store.processed_email_ids.len() > 1000 {
                            let drain = store.processed_email_ids.len() - 1000;
                            store.processed_email_ids.drain(0..drain);
                        }
                    }

                    for rule in &matched_rules {
                        let mut entry = triggers::EventLogEntry::new(
                            triggers::TriggerSource::Email,
                            format!("{} - {}", email.from, email.subject),
                            format!("Email matched rule \"{}\"", rule.name),
                        );
                        entry.rule_id = Some(rule.id.clone());
                        entry.rule_name = Some(rule.name.clone());
                        entry.workflow_id = Some(rule.workflow_id.clone());

                        // Extract data if template exists
                        if let Some(ref template) = rule.extract_template {
                            if let Ok(data) = triggers::extract_email_data(&email, template).await {
                                entry.data = Some(data);
                            }
                        }

                        entry.status = triggers::EventStatus::Triggered;
                        crate::event_bus::emit_ui(app, "trigger-event", &entry).ok();
                        new_events.push(entry.clone());

                        {
                            let mut store = state.automation.trigger_store.lock().unwrap_or_else(|e| e.into_inner());
                            store.add_log(entry);
                            store.save();
                        }

                        // D3: a matched trigger must actually RUN the
                        // workflow. Previously it only logged "Triggered" and
                        // emitted an event - nothing ever executed.
                        let wf_id = rule.workflow_id.clone();
                        if !wf_id.is_empty() {
                            let trigger_payload = email_to_trigger(&email);
                            let (status, msg) =
                                match run_workflow_with_trigger(&wf_id, trigger_payload, state, app).await {
                                    Ok(rs)
                                        if rs.status
                                            == workflow::WorkflowRunStatus::Failed =>
                                    {
                                        (
                                            triggers::EventStatus::Failed,
                                            format!(
                                                "Workflow failed: {}",
                                                rs.error.unwrap_or_default()
                                            ),
                                        )
                                    }
                                    Ok(_) => (
                                        triggers::EventStatus::Completed,
                                        "Workflow completed".to_string(),
                                    ),
                                    Err(e) => (
                                        triggers::EventStatus::Failed,
                                        format!("Workflow error: {}", e),
                                    ),
                                };
                            let mut done = triggers::EventLogEntry::new(
                                triggers::TriggerSource::Email,
                                format!("{} - {}", email.from, email.subject),
                                msg,
                            );
                            done.rule_id = Some(rule.id.clone());
                            done.rule_name = Some(rule.name.clone());
                            done.workflow_id = Some(wf_id);
                            done.status = status;
                            crate::event_bus::emit_ui(app, "trigger-event", &done).ok();
                            new_events.push(done.clone());
                            let mut store = state
                                .automation.trigger_store
                                .lock()
                                .unwrap_or_else(|e| e.into_inner());
                            store.add_log(done);
                            store.save();
                        }
                    }
                }
            }
            Err(err) => {
                let entry = triggers::EventLogEntry {
                    id: uuid::Uuid::new_v4().to_string(),
                    source: triggers::TriggerSource::Email,
                    source_detail: account.email.clone(),
                    rule_id: None,
                    rule_name: None,
                    workflow_id: None,
                    workflow_name: None,
                    status: triggers::EventStatus::Failed,
                    message: format!("Poll failed: {}", err),
                    data: None,
                    timestamp: chrono::Utc::now().to_rfc3339(),
                };
                let mut store = state.automation.trigger_store.lock().unwrap_or_else(|e| e.into_inner());
                store.add_log(entry);
            }
        }

        // Update last_polled
        {
            let mut store = state.automation.trigger_store.lock().unwrap_or_else(|e| e.into_inner());
            if let Some(acct) = store.email_accounts.iter_mut().find(|a| a.id == account.id) {
                acct.last_polled = Some(chrono::Utc::now().to_rfc3339());
            }
            store.save();
        }
    }

    new_events
}

#[tauri::command]
pub(crate) async fn test_email_connection(
    state: State<'_, AppState>,
    account_id: String,
) -> Result<String, String> {
    let account = {
        let store = state.automation.trigger_store.lock().unwrap_or_else(|e| e.into_inner());
        store.email_accounts.iter().find(|a| a.id == account_id).cloned()
    }.ok_or("Account not found")?;

    let token = account.access_token.as_deref().ok_or("No access token")?;

    match account.provider {
        triggers::EmailProvider::Outlook => {
            let emails = triggers::fetch_outlook_emails(token, None, 1).await?;
            Ok(format!("Connected! Found {} messages", emails.len()))
        }
    }
}


// ─────────────────────────────────────────────
//  Mail send + calendar (W5.5)
// ─────────────────────────────────────────────

/// Resolve (provider, access_token) for a connected email account.
fn resolve_account(state: &AppState, account_id: &str) -> Result<(productivity::Provider, String), String> {
    let store = state.automation.trigger_store.lock().unwrap_or_else(|e| e.into_inner());
    let account = store
        .email_accounts
        .iter()
        .find(|a| a.id == account_id)
        .ok_or("Account not found")?;
    let token = account.access_token.clone().ok_or("Account has no access token; reconnect it")?;
    let provider = match account.provider {
        triggers::EmailProvider::Outlook => productivity::Provider::Microsoft,
    };
    Ok((provider, token))
}

#[tauri::command]
pub(crate) async fn send_email(
    account_id: String,
    to: String,
    subject: String,
    body: String,
    state: State<'_, AppState>,
) -> Result<String, String> {
    let (provider, token) = resolve_account(&state, &account_id)?;
    productivity::send_mail(provider, &token, &to, &subject, &body).await?;
    Ok(format!("Sent to {}", to))
}

#[tauri::command]
pub(crate) async fn calendar_create_event(
    account_id: String,
    subject: String,
    start: String,
    end: String,
    attendees: Option<Vec<String>>,
    state: State<'_, AppState>,
) -> Result<String, String> {
    let (provider, token) = resolve_account(&state, &account_id)?;
    let att = attendees.unwrap_or_default();
    let id = productivity::calendar_create(provider, &token, &subject, &start, &end, &att).await?;
    Ok(id)
}

#[tauri::command]
pub(crate) async fn calendar_list_events(
    account_id: String,
    max: Option<usize>,
    state: State<'_, AppState>,
) -> Result<Vec<productivity::CalendarEvent>, String> {
    let (provider, token) = resolve_account(&state, &account_id)?;
    productivity::calendar_list(provider, &token, max.unwrap_or(10)).await
}

/// Start the Microsoft device-code sign-in: returns the code the
/// user types plus the verification URL (browser is opened for them).
#[tauri::command]
pub(crate) async fn email_device_start(provider: String) -> Result<serde_json::Value, String> {
    let connector_id = match provider.as_str() {
        "gmail" | "google" => {
            return Err("Google mail is not supported yet - connect a Microsoft account instead.".to_string())
        }
        _ => "microsoft",
    };
    let dcr = crate::oauth::start_device_code_flow(connector_id).await?;
    open::that(&dcr.verification_uri).ok();
    Ok(serde_json::json!({
        "user_code": dcr.user_code,
        "verification_uri": dcr.verification_uri,
        "device_code": dcr.device_code,
        "expires_in": dcr.expires_in,
        "connector_id": connector_id,
    }))
}

/// Wait for the user to finish signing in, then save (or update) the
/// account. Blocks until sign-in completes or the code expires - the
/// frontend awaits with a spinner.
#[tauri::command]
pub(crate) async fn email_device_complete(
    connector_id: String,
    device_code: String,
    state: State<'_, AppState>,
) -> Result<crate::triggers::EmailAccount, String> {
    let tokens = crate::oauth::poll_device_code(&connector_id, &device_code)
        .await
        .map_err(|e| {
            // Translate registration-side AADSTS errors into something an
            // end user can act on (or forward) - they did nothing wrong.
            if e.contains("AADSTS900971") || e.contains("AADSTS500113") {
                "Microsoft rejected the sign-in because of the app's Azure registration (missing platform/reply URL). This is a vendor-side configuration issue, not something you did - report it to Orbix support.".to_string()
            } else if e.contains("AADSTS65001") || e.contains("consent") {
                "Your organization requires admin consent for this app. Ask your Microsoft 365 admin to approve Orbix Core, then sign in again.".to_string()
            } else {
                e
            }
        })?;
    let (name, email) = crate::oauth::fetch_user_info(&connector_id, &tokens.access_token).await;
    let email = email.unwrap_or_default();
    let display_name = name.unwrap_or_else(|| email.clone());
    if connector_id == "google" {
        return Err("Google mail is not supported yet - connect a Microsoft account instead.".to_string());
    }
    let provider = crate::triggers::EmailProvider::Outlook;
    let expires_at = tokens
        .expires_in
        .map(|e| (chrono::Utc::now() + chrono::Duration::seconds(e as i64)).to_rfc3339());

    let mut store = state.automation.trigger_store.lock().map_err(|e| e.to_string())?;
    // Re-signing in must UPDATE the existing account, never duplicate it
    // (that is how stale-token ghosts accumulate).
    let account = if let Some(a) = store
        .email_accounts
        .iter_mut()
        .find(|a| a.email.eq_ignore_ascii_case(&email) && a.provider == provider)
    {
        a.access_token = Some(tokens.access_token);
        if tokens.refresh_token.is_some() {
            a.refresh_token = tokens.refresh_token;
        }
        a.token_expires_at = expires_at;
        a.status = crate::triggers::EmailAccountStatus::Connected;
        a.clone()
    } else {
        let mut account = crate::triggers::EmailAccount::new(provider, email, display_name);
        account.access_token = Some(tokens.access_token);
        account.refresh_token = tokens.refresh_token;
        account.token_expires_at = expires_at;
        account.status = crate::triggers::EmailAccountStatus::Connected;
        store.email_accounts.push(account.clone());
        account
    };
    store.save();
    Ok(account)
}
