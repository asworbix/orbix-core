use serde::{Deserialize, Serialize};
use std::collections::HashMap;

// ─────────────────────────────────────────────
//  Connected Service - stored credential record
// ─────────────────────────────────────────────

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ConnectedService {
    pub provider: String,
    pub user_name: Option<String>,
    pub user_email: Option<String>,
    pub access_token_encrypted: String,
    pub refresh_token_encrypted: Option<String>,
    pub expires_at: Option<String>,
    pub scopes: Vec<String>,
    pub connected_at: String,
    pub auth_method: String, // "device_flow" | "pkce"
}

const SERVICES_FILE: &str = "connected_services.json";

pub fn load_services() -> HashMap<String, ConnectedService> {
    crate::storage::load_json::<HashMap<String, ConnectedService>>(SERVICES_FILE)
        .unwrap_or_default()
}

pub fn save_services(services: &HashMap<String, ConnectedService>) -> Result<(), String> {
    crate::storage::save_json(SERVICES_FILE, services)
}

pub fn get_service(provider: &str) -> Option<ConnectedService> {
    load_services().remove(provider)
}

pub fn remove_service(provider: &str) -> Result<(), String> {
    let mut services = load_services();
    services.remove(provider);
    save_services(&services)
}

/// Store a successful auth result as a connected service
fn store_service(
    provider: &str,
    access_token: &str,
    refresh_token: Option<&str>,
    user_name: Option<String>,
    user_email: Option<String>,
    scopes: &[String],
    auth_method: &str,
    expires_in: Option<u64>,
) -> Result<(), String> {
    let encrypted_access = crate::storage::encrypt_string(access_token)?;
    let encrypted_refresh = match refresh_token {
        Some(rt) => Some(crate::storage::encrypt_string(rt)?),
        None => None,
    };

    let expires_at = expires_in.map(|secs| {
        let expiry = chrono::Utc::now() + chrono::Duration::seconds(secs as i64);
        expiry.to_rfc3339()
    });

    let service = ConnectedService {
        provider: provider.to_string(),
        user_name,
        user_email,
        access_token_encrypted: encrypted_access,
        refresh_token_encrypted: encrypted_refresh,
        expires_at,
        scopes: scopes.to_vec(),
        connected_at: chrono::Utc::now().to_rfc3339(),
        auth_method: auth_method.to_string(),
    };

    let mut services = load_services();
    services.insert(provider.to_string(), service);
    save_services(&services)
}

/// Decrypt and return a live access token for a connected service
pub fn get_access_token(provider: &str) -> Result<String, String> {
    let service = get_service(provider)
        .ok_or_else(|| format!("Not logged in to {}", provider))?;
    crate::storage::decrypt_string(&service.access_token_encrypted)
}

// ─────────────────────────────────────────────
//  Provider registry
// ─────────────────────────────────────────────

#[derive(Clone, Debug)]
pub struct ProviderConfig {
    pub name: &'static str,
    pub display_name: &'static str,
    pub supports_device_flow: bool,
    // Device flow endpoints (GitHub, Microsoft)
    pub device_auth_url: Option<&'static str>,
    pub device_token_url: Option<&'static str>,
    // PKCE endpoints (all)
    pub auth_url: &'static str,
    pub token_url: &'static str,
    // Placeholder client ID - replaced by user config
    pub default_client_id: &'static str,
    pub scopes: &'static [&'static str],
    pub user_info_url: &'static str,
}

pub fn providers() -> Vec<ProviderConfig> {
    vec![
        ProviderConfig {
            name: "github",
            display_name: "GitHub",
            supports_device_flow: true,
            device_auth_url: Some("https://github.com/login/device/code"),
            device_token_url: Some("https://github.com/login/oauth/access_token"),
            auth_url: "https://github.com/login/oauth/authorize",
            token_url: "https://github.com/login/oauth/access_token",
            default_client_id: "ORBIXCORE_GITHUB_CLIENT_ID",
            scopes: &["read:user", "user:email", "repo"],
            user_info_url: "https://api.github.com/user",
        },
        ProviderConfig {
            name: "microsoft",
            display_name: "Microsoft 365",
            supports_device_flow: true,
            device_auth_url: Some("https://login.microsoftonline.com/common/oauth2/v2.0/devicecode"),
            device_token_url: Some("https://login.microsoftonline.com/common/oauth2/v2.0/token"),
            auth_url: "https://login.microsoftonline.com/common/oauth2/v2.0/authorize",
            token_url: "https://login.microsoftonline.com/common/oauth2/v2.0/token",
            default_client_id: "ORBIXCORE_MS_CLIENT_ID",
            scopes: &[
                "openid", "profile", "email", "offline_access",
                "User.Read", "Mail.Read", "Calendars.Read", "Files.Read",
            ],
            user_info_url: "https://graph.microsoft.com/v1.0/me",
        },
        ProviderConfig {
            name: "google",
            display_name: "Google Workspace",
            supports_device_flow: false,
            device_auth_url: None,
            device_token_url: None,
            auth_url: "https://accounts.google.com/o/oauth2/v2/auth",
            token_url: "https://oauth2.googleapis.com/token",
            default_client_id: "ORBIXCORE_GOOGLE_CLIENT_ID",
            scopes: &[
                "openid", "email", "profile",
                "https://www.googleapis.com/auth/gmail.readonly",
                "https://www.googleapis.com/auth/calendar.readonly",
                "https://www.googleapis.com/auth/drive.readonly",
            ],
            user_info_url: "https://www.googleapis.com/oauth2/v3/userinfo",
        },
        ProviderConfig {
            name: "slack",
            display_name: "Slack",
            supports_device_flow: false,
            device_auth_url: None,
            device_token_url: None,
            auth_url: "https://slack.com/oauth/v2/authorize",
            token_url: "https://slack.com/api/oauth.v2.access",
            default_client_id: "ORBIXCORE_SLACK_CLIENT_ID",
            scopes: &["channels:read", "chat:write", "users:read"],
            user_info_url: "https://slack.com/api/auth.test",
        },
    ]
}

pub fn get_provider(name: &str) -> Option<ProviderConfig> {
    providers().into_iter().find(|p| p.name == name)
}

/// Resolve the client ID: check user config first, then fall back to built-in
fn resolve_client_id(provider: &ProviderConfig) -> Result<String, String> {
    // Check user-configured IDs
    let custom_ids = crate::oauth::load_oauth_client_ids();
    if let Some(custom) = custom_ids.get(provider.name) {
        if !custom.is_empty() && !custom.starts_with("ORBIXCORE_") {
            return Ok(custom.clone());
        }
    }

    // Check if default is a placeholder
    if provider.default_client_id.starts_with("ORBIXCORE_") {
        return Err(format!(
            "No client ID configured for {}.\n\
             \n\
             To set one up:\n\
             \n\
               orbixcore auth set-client-id {} YOUR_CLIENT_ID\n\
             \n\
             Or in the app: Settings → Connectors → OAuth App IDs",
            provider.display_name, provider.name
        ));
    }

    Ok(provider.default_client_id.to_string())
}

// ─────────────────────────────────────────────
//  Device Flow (GitHub, Microsoft)
// ─────────────────────────────────────────────

#[derive(Deserialize)]
struct DeviceCodeResponse {
    device_code: String,
    user_code: String,
    verification_uri: String,
    #[serde(default = "default_interval")]
    interval: u64,
    #[serde(default = "default_expires")]
    expires_in: u64,
}

fn default_interval() -> u64 { 5 }
fn default_expires() -> u64 { 900 }

#[derive(Deserialize)]
struct DeviceTokenResponse {
    access_token: Option<String>,
    refresh_token: Option<String>,
    token_type: Option<String>,
    expires_in: Option<u64>,
    scope: Option<String>,
    error: Option<String>,
}

pub async fn device_flow_login(provider_name: &str) -> Result<(), String> {
    let provider = get_provider(provider_name)
        .ok_or_else(|| format!("Unknown provider: {}", provider_name))?;

    if !provider.supports_device_flow {
        return Err(format!("{} doesn't support device flow. Use browser login instead.", provider.display_name));
    }

    let client_id = resolve_client_id(&provider)?;
    let device_auth_url = provider.device_auth_url.unwrap();
    let device_token_url = provider.device_token_url.unwrap();
    let scopes: Vec<String> = provider.scopes.iter().map(|s| s.to_string()).collect();
    let scope_str = scopes.join(" ");

    let client = reqwest::Client::new();

    // Step 1: Request device code
    eprintln!("\x1b[90m→ Requesting device code from {}...\x1b[0m", provider.display_name);

    let mut form = HashMap::new();
    form.insert("client_id", client_id.as_str());
    form.insert("scope", &scope_str);

    let resp = client
        .post(device_auth_url)
        .header("Accept", "application/json")
        .form(&form)
        .send()
        .await
        .map_err(|e| format!("Failed to request device code: {}", e))?;

    if !resp.status().is_success() {
        let body = resp.text().await.unwrap_or_default();
        return Err(format!("Device code request failed: {}", body));
    }

    let device_resp: DeviceCodeResponse = resp.json().await
        .map_err(|e| format!("Failed to parse device code response: {}", e))?;

    // Step 2: Display the code to the user
    eprintln!();
    eprintln!("  \x1b[1;97m┌─────────────────────────────────────────┐\x1b[0m");
    eprintln!("  \x1b[1;97m│\x1b[0m                                         \x1b[1;97m│\x1b[0m");
    eprintln!("  \x1b[1;97m│\x1b[0m   Open: \x1b[4;36m{:<31}\x1b[0m \x1b[1;97m│\x1b[0m", device_resp.verification_uri);
    eprintln!("  \x1b[1;97m│\x1b[0m   Code: \x1b[1;93m{:<31}\x1b[0m \x1b[1;97m│\x1b[0m", device_resp.user_code);
    eprintln!("  \x1b[1;97m│\x1b[0m                                         \x1b[1;97m│\x1b[0m");
    eprintln!("  \x1b[1;97m└─────────────────────────────────────────┘\x1b[0m");
    eprintln!();

    // Try to open the browser automatically
    if open::that(&device_resp.verification_uri).is_ok() {
        eprintln!("  \x1b[90mBrowser opened. Enter the code above, then approve.\x1b[0m");
    } else {
        eprintln!("  \x1b[90mOpen the URL above in your browser and enter the code.\x1b[0m");
    }
    eprintln!("  \x1b[90mWaiting for authorization...\x1b[0m");
    eprintln!();

    // Step 3: Poll for the token
    let interval = std::time::Duration::from_secs(device_resp.interval);
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(device_resp.expires_in);

    loop {
        if std::time::Instant::now() > deadline {
            return Err("Login timed out. Please try again.".to_string());
        }

        tokio::time::sleep(interval).await;

        let mut token_form = HashMap::new();
        token_form.insert("client_id", client_id.as_str());
        token_form.insert("device_code", &device_resp.device_code);

        let grant_type = if provider_name == "github" {
            "urn:ietf:params:oauth:grant-type:device_code"
        } else {
            "urn:ietf:params:oauth:grant-type:device_code"
        };
        token_form.insert("grant_type", grant_type);

        let resp = client
            .post(device_token_url)
            .header("Accept", "application/json")
            .form(&token_form)
            .send()
            .await
            .map_err(|e| format!("Token poll failed: {}", e))?;

        let token_resp: DeviceTokenResponse = resp.json().await
            .map_err(|e| format!("Failed to parse token response: {}", e))?;

        if let Some(ref error) = token_resp.error {
            match error.as_str() {
                "authorization_pending" => continue,
                "slow_down" => {
                    tokio::time::sleep(std::time::Duration::from_secs(5)).await;
                    continue;
                }
                "expired_token" => return Err("Code expired. Please try again.".to_string()),
                "access_denied" => return Err("Authorization denied.".to_string()),
                other => return Err(format!("OAuth error: {}", other)),
            }
        }

        if let Some(access_token) = token_resp.access_token {
            // Fetch user info
            let (user_name, user_email) = crate::oauth::fetch_user_info(provider_name, &access_token).await;

            // Store the tokens
            store_service(
                provider_name,
                &access_token,
                token_resp.refresh_token.as_deref(),
                user_name.clone(),
                user_email.clone(),
                &scopes,
                "device_flow",
                token_resp.expires_in,
            )?;

            // Also create a connector instance so the GUI sees it
            sync_to_connector(provider_name, &access_token, token_resp.refresh_token.as_deref(), &user_name, &user_email)?;

            let display = user_name
                .or(user_email)
                .unwrap_or_else(|| provider.display_name.to_string());

            eprintln!("  \x1b[1;32mOK\x1b[0m Logged in to \x1b[1m{}\x1b[0m as \x1b[1;36m{}\x1b[0m", provider.display_name, display);
            eprintln!();
            return Ok(());
        }
    }
}

// ─────────────────────────────────────────────
//  Browser PKCE Flow (all providers)
// ─────────────────────────────────────────────

pub async fn browser_login(provider_name: &str) -> Result<(), String> {
    let provider = get_provider(provider_name)
        .ok_or_else(|| format!("Unknown provider: {}", provider_name))?;

    let _client_id = resolve_client_id(&provider)?;
    let scopes: Vec<String> = provider.scopes.iter().map(|s| s.to_string()).collect();

    eprintln!("\x1b[90m→ Starting browser login for {}...\x1b[0m", provider.display_name);

    // Use the existing PKCE flow from oauth.rs
    let token_resp = crate::oauth::start_oauth_flow(provider_name).await?;

    // Fetch user info
    let (user_name, user_email) = crate::oauth::fetch_user_info(provider_name, &token_resp.access_token).await;

    // Store as connected service
    store_service(
        provider_name,
        &token_resp.access_token,
        token_resp.refresh_token.as_deref(),
        user_name.clone(),
        user_email.clone(),
        &scopes,
        "pkce",
        token_resp.expires_in,
    )?;

    // Sync to connector store
    sync_to_connector(provider_name, &token_resp.access_token, token_resp.refresh_token.as_deref(), &user_name, &user_email)?;

    let display = user_name
        .or(user_email)
        .unwrap_or_else(|| provider.display_name.to_string());

    eprintln!("  \x1b[1;32mOK\x1b[0m Logged in to \x1b[1m{}\x1b[0m as \x1b[1;36m{}\x1b[0m", provider.display_name, display);
    eprintln!();
    Ok(())
}

// ─────────────────────────────────────────────
//  Unified login - picks best method
// ─────────────────────────────────────────────

pub async fn login(provider_name: &str) -> Result<(), String> {
    let provider = get_provider(provider_name)
        .ok_or_else(|| format!(
            "Unknown provider: '{}'\n\nAvailable: {}",
            provider_name,
            providers().iter().map(|p| p.name).collect::<Vec<_>>().join(", ")
        ))?;

    // Prefer device flow when available (simpler, no redirect URI)
    if provider.supports_device_flow {
        device_flow_login(provider_name).await
    } else {
        browser_login(provider_name).await
    }
}

// ─────────────────────────────────────────────
//  Sync to connector system (so GUI sees it)
// ─────────────────────────────────────────────

fn sync_to_connector(
    provider_name: &str,
    access_token: &str,
    refresh_token: Option<&str>,
    user_name: &Option<String>,
    user_email: &Option<String>,
) -> Result<(), String> {
    use crate::connectors;

    let connector_id = match provider_name {
        "microsoft" => connectors::ConnectorId::Microsoft,
        "google" => connectors::ConnectorId::Google,
        "github" => connectors::ConnectorId::Github,
        "slack" => connectors::ConnectorId::Slack,
        _ => return Ok(()), // unknown, skip
    };

    let display_name = user_name.clone()
        .or_else(|| user_email.clone())
        .unwrap_or_else(|| provider_name.to_string());

    let instance_name = format!("{} ({})",
        get_provider(provider_name).map(|p| p.display_name).unwrap_or(provider_name),
        display_name
    );

    let instance_id = uuid::Uuid::new_v4().to_string();

    // Store encrypted credentials
    connectors::save_connector_credential(&instance_id, "access_token", access_token)?;
    if let Some(rt) = refresh_token {
        connectors::save_connector_credential(&instance_id, "refresh_token", rt)?;
    }
    if let Some(ref email) = user_email {
        connectors::save_connector_credential(&instance_id, "email", email)?;
    }

    let mut cred_keys = vec!["access_token".to_string()];
    if refresh_token.is_some() { cred_keys.push("refresh_token".to_string()); }
    if user_email.is_some() { cred_keys.push("email".to_string()); }

    let instance = connectors::ConnectorInstance {
        id: instance_id,
        connector_id,
        name: instance_name,
        status: connectors::ConnectionStatus::Connected,
        created_at: chrono::Utc::now().to_rfc3339(),
        last_tested: Some(chrono::Utc::now().to_rfc3339()),
        error_message: None,
        credential_keys: cred_keys,
    };

    // Load existing instances, remove any previous for same provider, add new
    let mut instances = connectors::load_connector_instances().unwrap_or_default();
    instances.retain(|i| i.connector_id != instance.connector_id);
    instances.push(instance);
    connectors::save_connector_instances(&instances)?;

    Ok(())
}

// ─────────────────────────────────────────────
//  Status - list connected services
// ─────────────────────────────────────────────

pub fn print_status() {
    let services = load_services();

    if services.is_empty() {
        eprintln!();
        eprintln!("  \x1b[90mNo connected services.\x1b[0m");
        eprintln!("  \x1b[90mRun: orbixcore auth login <provider>\x1b[0m");
        eprintln!();
        eprintln!("  \x1b[90mAvailable providers:\x1b[0m");
        for p in providers() {
            let method = if p.supports_device_flow { "device flow" } else { "browser" };
            eprintln!("    \x1b[97m{:<14}\x1b[0m \x1b[90m{} ({})\x1b[0m", p.name, p.display_name, method);
        }
        eprintln!();
        return;
    }

    eprintln!();
    eprintln!("  \x1b[1;97mConnected Services\x1b[0m");
    eprintln!("  \x1b[90m─────────────────────────────────────────\x1b[0m");

    for (provider, service) in &services {
        let display = service.user_name.as_deref()
            .or(service.user_email.as_deref())
            .unwrap_or("unknown");

        let provider_cfg = get_provider(provider);
        let display_name = provider_cfg.map(|p| p.display_name).unwrap_or(provider.as_str());

        let expired = service.expires_at.as_ref().map_or(false, |exp| {
            chrono::DateTime::parse_from_rfc3339(exp)
                .map(|dt| dt < chrono::Utc::now())
                .unwrap_or(false)
        });

        let status_icon = if expired { "\x1b[33m!\x1b[0m" } else { "\x1b[32m*\x1b[0m" };
        let status_text = if expired { "\x1b[33mexpired\x1b[0m" } else { "\x1b[32mconnected\x1b[0m" };

        eprintln!("  {} \x1b[1m{:<16}\x1b[0m \x1b[36m{}\x1b[0m  {}", status_icon, display_name, display, status_text);
    }

    eprintln!();
}

// ─────────────────────────────────────────────
//  Logout - remove a service
// ─────────────────────────────────────────────

pub async fn logout(provider_name: &str) -> Result<(), String> {
    let provider = get_provider(provider_name)
        .ok_or_else(|| format!("Unknown provider: {}", provider_name))?;

    let services = load_services();
    if !services.contains_key(provider_name) {
        return Err(format!("Not logged in to {}", provider.display_name));
    }

    // Remove from connected services
    remove_service(provider_name)?;

    // Also remove connector instance
    let connector_id = match provider_name {
        "microsoft" => Some(crate::connectors::ConnectorId::Microsoft),
        "google" => Some(crate::connectors::ConnectorId::Google),
        "github" => Some(crate::connectors::ConnectorId::Github),
        "slack" => Some(crate::connectors::ConnectorId::Slack),
        _ => None,
    };

    if let Some(cid) = connector_id {
        let mut instances = crate::connectors::load_connector_instances().unwrap_or_default();
        instances.retain(|i| i.connector_id != cid);
        crate::connectors::save_connector_instances(&instances)?;
    }

    eprintln!("  \x1b[1;31mERR\x1b[0m Logged out from \x1b[1m{}\x1b[0m", provider.display_name);
    eprintln!();
    Ok(())
}
