//! Commands split out of main.rs (W0.3). Mechanical move, no behavior change.

#[allow(unused_imports)]
use crate::prelude::*;
#[allow(unused_imports)]
use crate::*;

// ─────────────────────────────────────────────
//  Commands - Role Profiles (Epic T)
// ─────────────────────────────────────────────

#[tauri::command]
pub(crate) fn get_onboarding(state: State<'_, AppState>) -> UserOnboarding {
    state.profile.onboarding.lock().unwrap_or_else(|e| e.into_inner()).clone()
}

#[tauri::command]
pub(crate) fn complete_onboarding(
    user_name: String,
    state: State<'_, AppState>,
) -> Result<UserOnboarding, String> {
    let result = {
        let mut onb = state.profile.onboarding.lock().unwrap_or_else(|e| e.into_inner());
        onb.completed = true;
        onb.user_name = user_name.clone();
        storage::save_onboarding(&onb).ok();
        onb.clone()
    };

    // Seed Bob's first conversation after onboarding
    let bob_welcome = Message {
        id: Uuid::new_v4().to_string(),
        role: "assistant".to_string(),
        content: format!(
            "Hey {}. Bob here.\n\n\
            Before we start - what apps and platforms do you use day-to-day?\n\n\
            Just tell me naturally, like:\n\
            *\"I use LinkedIn, Gmail, Slack, and GitHub\"*\n\n\
            I'll connect everything for you right now. You can also add more later - just say \"connect [service]\" anytime.",
            user_name
        ),
        context: None,
        agent_id: None,
        timestamp: Utc::now().to_rfc3339(),
        tool_turns: None,
    };
    {
        let mut msgs = state.chat.conversations.lock().unwrap_or_else(|e| e.into_inner());
        msgs.clear();
        msgs.push(bob_welcome);
    }

    Ok(result)
}

// ── Agent Activity Log ──────────────────────

#[tauri::command]
pub(crate) fn get_activity_log(
    agent_name: Option<String>,
    limit: Option<usize>,
    state: State<'_, AppState>,
) -> Vec<AgentActivity> {
    let log = state.chat.activity_log.lock().unwrap_or_else(|e| e.into_inner());
    let max = limit.unwrap_or(200);
    let iter = log.iter().rev();
    match agent_name {
        Some(name) => iter.filter(|a| a.agent_name == name).take(max).cloned().collect::<Vec<_>>().into_iter().rev().collect(),
        None => iter.take(max).cloned().collect::<Vec<_>>().into_iter().rev().collect(),
    }
}

#[tauri::command]
pub(crate) fn clear_activity_log(state: State<'_, AppState>) {
    state.chat.activity_log.lock().unwrap_or_else(|e| e.into_inner()).clear();
}

// ── Connectors (Epic U) ─────────────────────

#[tauri::command]
pub(crate) fn list_connector_catalog() -> Vec<connectors::ConnectorDefinition> {
    connectors::connector_catalog()
}

#[tauri::command]
pub(crate) fn list_connector_instances(state: State<'_, AppState>) -> Vec<connectors::ConnectorInstance> {
    state.connect.connector_instances.lock().unwrap_or_else(|e| e.into_inner()).clone()
}

#[tauri::command]
pub(crate) fn create_connector_instance(
    connector_id: connectors::ConnectorId,
    name: String,
    credentials: std::collections::HashMap<String, String>,
    state: State<'_, AppState>,
) -> Result<connectors::ConnectorInstance, String> {
    let instance_id = Uuid::new_v4().to_string();

    // Store each credential encrypted
    let mut cred_keys = Vec::new();
    for (key, value) in &credentials {
        if !value.is_empty() {
            connectors::save_connector_credential(&instance_id, key, value)?;
            cred_keys.push(key.clone());
        }
    }

    let instance = connectors::ConnectorInstance {
        id: instance_id,
        connector_id,
        name,
        status: connectors::ConnectionStatus::Disconnected,
        created_at: Utc::now().to_rfc3339(),
        last_tested: None,
        error_message: None,
        credential_keys: cred_keys,
    };

    let mut instances = state.connect.connector_instances.lock().unwrap_or_else(|e| e.into_inner());
    instances.push(instance.clone());
    connectors::save_connector_instances(&instances).ok();

    Ok(instance)
}

#[tauri::command]
pub(crate) async fn test_connector(
    instance_id: String,
    state: State<'_, AppState>,
    app: tauri::AppHandle,
) -> Result<connectors::ConnectorTestResult, String> {
    let (instance, def) = {
        let instances = state.connect.connector_instances.lock().unwrap_or_else(|e| e.into_inner());
        let inst = instances.iter().find(|i| i.id == instance_id)
            .ok_or("Connector not found")?
            .clone();
        let catalog = connectors::connector_catalog();
        let d = catalog.into_iter().find(|c| c.id == inst.connector_id)
            .ok_or("Connector definition not found")?;
        (inst, d)
    };

    // Set status to connecting
    {
        let mut instances = state.connect.connector_instances.lock().unwrap_or_else(|e| e.into_inner());
        if let Some(inst) = instances.iter_mut().find(|i| i.id == instance_id) {
            inst.status = connectors::ConnectionStatus::Connecting;
        }
    }

    let result = connectors::test_connection(&instance, &def).await;

    // Update status based on result
    {
        let mut instances = state.connect.connector_instances.lock().unwrap_or_else(|e| e.into_inner());
        if let Some(inst) = instances.iter_mut().find(|i| i.id == instance_id) {
            inst.status = if result.success {
                connectors::ConnectionStatus::Connected
            } else {
                connectors::ConnectionStatus::Error
            };
            inst.last_tested = Some(Utc::now().to_rfc3339());
            inst.error_message = if result.success { None } else { Some(result.message.clone()) };
        }
        connectors::save_connector_instances(&instances).ok();
    }

    // Send OS notification
    let notif_title = if result.success {
        format!("{} connected!", instance.name)
    } else {
        format!("{} connection failed", instance.name)
    };
    crate::event_bus::emit_ui(&app, "connector-status", serde_json::json!({
        "instance_id": instance_id,
        "success": result.success,
        "message": result.message,
    })).ok();

    // Also send OS notification
    use tauri_plugin_notification::NotificationExt;
    app.notification()
        .builder()
        .title(&notif_title)
        .body(&result.message)
        .show()
        .ok();

    Ok(result)
}

#[tauri::command]
pub(crate) fn remove_connector(instance_id: String, state: State<'_, AppState>) -> Result<(), String> {
    let mut instances = state.connect.connector_instances.lock().unwrap_or_else(|e| e.into_inner());
    connectors::delete_connector_credentials(&instance_id);
    instances.retain(|i| i.id != instance_id);
    connectors::save_connector_instances(&instances).ok();
    Ok(())
}

#[tauri::command]
pub(crate) fn get_login_hints(connector_id: connectors::ConnectorId) -> Vec<connectors::LoginAssistantMessage> {
    connectors::get_login_hints(&connector_id)
}

#[tauri::command]
pub(crate) async fn ask_connector_assistant(
    connector_id: connectors::ConnectorId,
    field_key: String,
    user_input: String,
) -> String {
    connectors::ask_login_assistant(&connector_id, &field_key, &user_input).await
}

#[tauri::command]
pub(crate) fn validate_connector_field(
    connector_id: connectors::ConnectorId,
    field_key: String,
    value: String,
) -> connectors::ValidationResult {
    let catalog = connectors::connector_catalog();
    let def = catalog.iter().find(|c| c.id == connector_id);
    match def {
        Some(d) => connectors::validate_field(d, &field_key, &value),
        None => connectors::ValidationResult {
            field_key,
            valid: false,
            message: "Unknown connector".to_string(),
        },
    }
}

// ── Universal Connectors ─────────────────────

#[tauri::command]
pub(crate) async fn list_universal_connectors(state: State<'_, AppState>) -> Result<Vec<connectors::UniversalConnector>, String> {
    let store = state.connect.universal_connectors.lock().unwrap_or_else(|e| e.into_inner());
    Ok(store.clone())
}

#[tauri::command]
pub(crate) async fn create_universal_connector(
    name: String,
    base_url: String,
    auth_type: connectors::UniversalAuthType,
    auth_config: connectors::AuthConfig,
    default_headers: std::collections::HashMap<String, String>,
    state: State<'_, AppState>,
    app: tauri::AppHandle,
) -> Result<connectors::UniversalConnector, String> {
    let connector = connectors::UniversalConnector {
        id: Uuid::new_v4().to_string(),
        name,
        description: String::new(),
        base_url,
        auth_type,
        auth_config,
        default_headers,
        endpoints: Vec::new(),
        status: connectors::ConnectionStatus::Disconnected,
        created_at: Utc::now().to_rfc3339(),
        updated_at: Utc::now().to_rfc3339(),
    };
    let mut store = state.connect.universal_connectors.lock().unwrap_or_else(|e| e.into_inner());
    store.push(connector.clone());
    connectors::save_universal_connectors(&store).ok();
    drop(store);
    crate::event_bus::emit_ui(&app, "data-changed", "connector").ok();
    Ok(connector)
}

#[tauri::command]
pub(crate) async fn update_universal_connector(
    id: String,
    name: Option<String>,
    description: Option<String>,
    base_url: Option<String>,
    auth_type: Option<connectors::UniversalAuthType>,
    auth_config: Option<connectors::AuthConfig>,
    default_headers: Option<std::collections::HashMap<String, String>>,
    state: State<'_, AppState>,
    app: tauri::AppHandle,
) -> Result<connectors::UniversalConnector, String> {
    let mut store = state.connect.universal_connectors.lock().unwrap_or_else(|e| e.into_inner());
    let conn = store.iter_mut().find(|c| c.id == id).ok_or("Connector not found")?;
    if let Some(n) = name { conn.name = n; }
    if let Some(d) = description { conn.description = d; }
    if let Some(u) = base_url { conn.base_url = u; }
    if let Some(a) = auth_type { conn.auth_type = a; }
    if let Some(ac) = auth_config { conn.auth_config = ac; }
    if let Some(h) = default_headers { conn.default_headers = h; }
    conn.updated_at = Utc::now().to_rfc3339();
    let result = conn.clone();
    connectors::save_universal_connectors(&store).ok();
    drop(store);
    crate::event_bus::emit_ui(&app, "data-changed", "connector").ok();
    Ok(result)
}

#[tauri::command]
pub(crate) async fn delete_universal_connector(
    id: String,
    state: State<'_, AppState>,
    app: tauri::AppHandle,
) -> Result<(), String> {
    let mut store = state.connect.universal_connectors.lock().unwrap_or_else(|e| e.into_inner());
    store.retain(|c| c.id != id);
    connectors::save_universal_connectors(&store).ok();
    drop(store);
    crate::event_bus::emit_ui(&app, "data-changed", "connector").ok();
    Ok(())
}

#[tauri::command]
pub(crate) async fn discover_connector_endpoints(base_url: String) -> Result<connectors::DiscoveryResult, String> {
    Ok(connectors::discover_endpoints(&base_url).await)
}

#[tauri::command]
pub(crate) async fn add_connector_endpoint(
    connector_id: String,
    endpoint: connectors::ConnectorEndpoint,
    state: State<'_, AppState>,
) -> Result<(), String> {
    let mut store = state.connect.universal_connectors.lock().unwrap_or_else(|e| e.into_inner());
    let conn = store.iter_mut().find(|c| c.id == connector_id).ok_or("Connector not found")?;
    // Remove existing endpoint with same key
    conn.endpoints.retain(|e| e.key != endpoint.key);
    conn.endpoints.push(endpoint);
    conn.updated_at = Utc::now().to_rfc3339();
    connectors::save_universal_connectors(&store).ok();
    Ok(())
}

#[tauri::command]
pub(crate) async fn remove_connector_endpoint(
    connector_id: String,
    endpoint_key: String,
    state: State<'_, AppState>,
) -> Result<(), String> {
    let mut store = state.connect.universal_connectors.lock().unwrap_or_else(|e| e.into_inner());
    let conn = store.iter_mut().find(|c| c.id == connector_id).ok_or("Connector not found")?;
    conn.endpoints.retain(|e| e.key != endpoint_key);
    conn.updated_at = Utc::now().to_rfc3339();
    connectors::save_universal_connectors(&store).ok();
    Ok(())
}

#[tauri::command]
pub(crate) async fn test_universal_connector(
    connector_id: String,
    state: State<'_, AppState>,
) -> Result<connectors::ConnectorTestResult, String> {
    let connector = {
        let store = state.connect.universal_connectors.lock().unwrap_or_else(|e| e.into_inner());
        store.iter().find(|c| c.id == connector_id).cloned()
    }.ok_or("Connector not found")?;

    let result = connectors::test_universal_connection(&connector).await;

    // Update status
    {
        let mut store = state.connect.universal_connectors.lock().unwrap_or_else(|e| e.into_inner());
        if let Some(conn) = store.iter_mut().find(|c| c.id == connector_id) {
            conn.status = if result.success {
                connectors::ConnectionStatus::Connected
            } else {
                connectors::ConnectionStatus::Error
            };
            connectors::save_universal_connectors(&store).ok();
        }
    }

    Ok(result)
}

#[tauri::command]
pub(crate) async fn execute_connector_endpoint(
    connector_id: String,
    endpoint_key: String,
    variables: std::collections::HashMap<String, String>,
    state: State<'_, AppState>,
) -> Result<serde_json::Value, String> {
    let connector = {
        let store = state.connect.universal_connectors.lock().unwrap_or_else(|e| e.into_inner());
        store.iter().find(|c| c.id == connector_id).cloned()
    }.ok_or("Connector not found")?;

    let (status, body) = connectors::execute_connector_request(&connector, &endpoint_key, &variables).await?;
    Ok(serde_json::json!({ "status": status, "body": body }))
}

#[tauri::command]
pub(crate) async fn update_agent_connectors(
    agent_id: String,
    connector_ids: Vec<String>,
    state: State<'_, AppState>,
    app: tauri::AppHandle,
) -> Result<(), String> {
    let mut agents = state.chat.agents.lock().unwrap_or_else(|e| e.into_inner());
    let agent = agents.iter_mut().find(|a| a.id == agent_id).ok_or("Agent not found")?;
    agent.connector_ids = connector_ids;
    crate::storage::save_agents(&agents).ok();
    drop(agents);
    crate::event_bus::emit_ui(&app, "data-changed", "agent").ok();
    Ok(())
}

#[tauri::command]
pub(crate) async fn update_agent_workflows(
    agent_id: String,
    workflow_ids: Vec<String>,
    state: State<'_, AppState>,
    app: tauri::AppHandle,
) -> Result<(), String> {
    let mut agents = state.chat.agents.lock().unwrap_or_else(|e| e.into_inner());
    let agent = agents.iter_mut().find(|a| a.id == agent_id).ok_or("Agent not found")?;
    agent.workflow_ids = workflow_ids;
    crate::storage::save_agents(&agents).ok();
    drop(agents);
    crate::event_bus::emit_ui(&app, "data-changed", "agent").ok();
    Ok(())
}

// ── OAuth Login Flow ─────────────────────────

#[tauri::command]
pub(crate) fn get_oauth_connectors() -> Vec<String> {
    vec!["microsoft", "google", "github", "slack"]
        .into_iter()
        .filter(|id| oauth::supports_oauth_login(id))
        .map(|s| s.to_string())
        .collect()
}

#[tauri::command]
pub(crate) fn get_oauth_client_ids() -> std::collections::HashMap<String, String> {
    oauth::load_oauth_client_ids()
}

#[tauri::command]
pub(crate) fn set_oauth_client_id(connector_id: String, client_id: String) -> Result<(), String> {
    let mut ids = oauth::load_oauth_client_ids();
    ids.insert(connector_id, client_id);
    oauth::save_oauth_client_ids(&ids)
}

#[tauri::command]
pub(crate) async fn start_oauth_login(
    connector_id: String,
    state: State<'_, AppState>,
    app: tauri::AppHandle,
) -> Result<oauth::OAuthResult, String> {
    // Emit that we're starting OAuth
    crate::event_bus::emit_ui(&app, "oauth-status", serde_json::json!({
        "connector_id": &connector_id,
        "status": "starting",
    })).ok();

    // Run the OAuth flow
    let token_result = oauth::start_oauth_flow(&connector_id).await;

    match token_result {
        Ok(tokens) => {
            // Fetch user info
            let (user_name, user_email) = oauth::fetch_user_info(&connector_id, &tokens.access_token).await;

            let display_name = user_name.clone().unwrap_or_else(|| {
                user_email.clone().unwrap_or_else(|| connector_id.clone())
            });

            let instance_name = format!("{} ({})",
                match connector_id.as_str() {
                    "microsoft" => "Microsoft 365",
                    "google" => "Google Workspace",
                    "github" => "GitHub",
                    "slack" => "Slack",
                    _ => &connector_id,
                },
                display_name
            );

            // Create connector instance with the OAuth tokens
            let instance_id = Uuid::new_v4().to_string();
            let connector_id_enum = match connector_id.as_str() {
                "microsoft" => connectors::ConnectorId::Microsoft,
                "google" => connectors::ConnectorId::Google,
                "github" => connectors::ConnectorId::Github,
                "slack" => connectors::ConnectorId::Slack,
                _ => return Err("Unknown connector".to_string()),
            };

            // Store tokens as encrypted credentials
            connectors::save_connector_credential(&instance_id, "access_token", &tokens.access_token)?;
            if let Some(ref refresh) = tokens.refresh_token {
                connectors::save_connector_credential(&instance_id, "refresh_token", refresh)?;
            }
            if let Some(ref email) = user_email {
                connectors::save_connector_credential(&instance_id, "email", email)?;
            }

            let mut cred_keys = vec!["access_token".to_string()];
            if tokens.refresh_token.is_some() { cred_keys.push("refresh_token".to_string()); }
            if user_email.is_some() { cred_keys.push("email".to_string()); }

            let instance = connectors::ConnectorInstance {
                id: instance_id,
                connector_id: connector_id_enum,
                name: instance_name,
                status: connectors::ConnectionStatus::Connected,
                created_at: Utc::now().to_rfc3339(),
                last_tested: Some(Utc::now().to_rfc3339()),
                error_message: None,
                credential_keys: cred_keys,
            };

            {
                let mut instances = state.connect.connector_instances.lock().unwrap_or_else(|e| e.into_inner());
                instances.push(instance.clone());
                connectors::save_connector_instances(&instances).ok();
            }

            crate::event_bus::emit_ui(&app, "oauth-status", serde_json::json!({
                "connector_id": &connector_id,
                "status": "connected",
                "instance_id": &instance.id,
            })).ok();

            // Send OS notification
            use tauri_plugin_notification::NotificationExt;
            app.notification()
                .builder()
                .title(&format!("{} connected!", instance.name))
                .body("You're all set. I can now access your data.")
                .show()
                .ok();

            Ok(oauth::OAuthResult {
                success: true,
                connector_id,
                user_email,
                user_name,
                error: None,
            })
        }
        Err(e) => {
            crate::event_bus::emit_ui(&app, "oauth-status", serde_json::json!({
                "connector_id": &connector_id,
                "status": "error",
                "error": &e,
            })).ok();

            Ok(oauth::OAuthResult {
                success: false,
                connector_id,
                user_email: None,
                user_name: None,
                error: Some(e),
            })
        }
    }
}

