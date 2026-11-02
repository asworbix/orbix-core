//! Door one of pair mode: the service behind the app.
//!
//! Slack, Teams, Outlook, GitHub Desktop and friends are windows onto a
//! cloud service with a real API. Orbix already signs the user into those
//! services (OAuth connectors); this module makes that sign-in usable:
//! which app maps to which service, whether it is connected, the user's
//! token for it, and an `api_call` that sends that token so Bob works with
//! the real data instead of a picture of it. Cross-platform: recipes carry
//! bundle ids for macOS and process names for Windows, and everything
//! below is HTTP.

use serde_json::Value;

use crate::connectors::{self, ConnectionStatus, ConnectorId, ConnectorInstance};
use crate::AppState;

pub struct ServiceInfo {
    pub slug: &'static str,
    pub name: &'static str,
    pub api_base: &'static str,
    /// Endpoints Bob reaches for first, relative to api_base.
    pub hint: &'static str,
}

pub const SERVICES: &[ServiceInfo] = &[
    ServiceInfo {
        slug: "microsoft",
        name: "Microsoft 365",
        api_base: "https://graph.microsoft.com/v1.0",
        hint: "/me, /me/messages?$top=10, /me/events, /me/chats, /me/drive/root/children, /me/joinedTeams",
    },
    ServiceInfo {
        slug: "google",
        name: "Google",
        api_base: "https://www.googleapis.com",
        hint: "/gmail/v1/users/me/messages?maxResults=10, /calendar/v3/calendars/primary/events, /drive/v3/files",
    },
    ServiceInfo {
        slug: "github",
        name: "GitHub",
        api_base: "https://api.github.com",
        hint: "/user, /user/repos?sort=updated, /notifications, /repos/{owner}/{repo}/issues, /search/issues?q=",
    },
    ServiceInfo {
        slug: "slack",
        name: "Slack",
        api_base: "https://slack.com/api",
        hint: "/conversations.list?types=public_channel,private_channel, /conversations.history?channel=ID&limit=20, /search.messages?query=, /users.list, POST /chat.postMessage {channel, text}",
    },
    ServiceInfo {
        slug: "linkedin",
        name: "LinkedIn",
        api_base: "https://api.linkedin.com/v2",
        hint: "/userinfo, POST /ugcPosts",
    },
];

pub fn info(slug: &str) -> Option<&'static ServiceInfo> {
    let s = slug.trim().to_lowercase();
    SERVICES.iter().find(|i| i.slug == s)
}

pub fn connector_id(slug: &str) -> Option<ConnectorId> {
    match slug.trim().to_lowercase().as_str() {
        "microsoft" => Some(ConnectorId::Microsoft),
        "google" => Some(ConnectorId::Google),
        "github" => Some(ConnectorId::Github),
        "slack" => Some(ConnectorId::Slack),
        "linkedin" => Some(ConnectorId::LinkedIn),
        _ => None,
    }
}

/// The connected instance for a service, if the user signed in.
pub fn connected(state: &AppState, slug: &str) -> Option<ConnectorInstance> {
    let want = slug.trim().to_lowercase();
    let instances = state.connect.connector_instances.lock().unwrap_or_else(|e| e.into_inner());
    instances
        .iter()
        .find(|i| i.status == ConnectionStatus::Connected && i.connector_id.slug() == want)
        .cloned()
}

/// A URL for api_call: absolute stays as is; a path is joined to the
/// service's API base.
pub fn resolve_url(slug: &str, url: &str) -> String {
    let url = url.trim();
    if url.starts_with("http://") || url.starts_with("https://") {
        return url.to_string();
    }
    match info(slug) {
        Some(i) => format!("{}/{}", i.api_base.trim_end_matches('/'), url.trim_start_matches('/')),
        None => url.to_string(),
    }
}

/// The user's access token for a service. Microsoft goes through the
/// Outlook account's refreshing path when one exists.
pub async fn bearer_token(state: &AppState, slug: &str) -> Result<String, String> {
    if slug == "microsoft" {
        if let Ok(t) = crate::graph_outlook::access_token().await {
            if !t.is_empty() {
                return Ok(t);
            }
        }
    }
    let inst = connected(state, slug).ok_or_else(|| {
        let name = info(slug).map(|i| i.name).unwrap_or(slug);
        format!("{} is not connected. connect_oauth \"{}\" signs the user in once.", name, slug)
    })?;
    connectors::load_connector_credential(&inst.id, "access_token")
        .map_err(|e| format!("could not read the {} token: {}", slug, e))
}

/// Trade a stored refresh token for a new access token, where the
/// service supports it. Returns the new token or why not.
pub async fn refresh_access_token(state: &AppState, slug: &str) -> Result<String, String> {
    let inst = connected(state, slug).ok_or("not connected")?;
    let refresh = connectors::load_connector_credential(&inst.id, "refresh_token")
        .map_err(|_| format!("{} gave no refresh token; sign in again", slug))?;
    let config = crate::oauth::get_oauth_config(slug).ok_or("no OAuth config")?;
    let mut form = vec![
        ("grant_type", "refresh_token".to_string()),
        ("refresh_token", refresh),
        ("client_id", config.client_id.clone()),
    ];
    if let Some(secret) = config.client_secret.clone() {
        form.push(("client_secret", secret));
    }
    let resp = reqwest::Client::new()
        .post(&config.token_url)
        .form(&form)
        .send()
        .await
        .map_err(|e| format!("refresh: {}", e))?;
    let body: Value = resp.json().await.map_err(|e| format!("refresh: {}", e))?;
    let token = body["access_token"]
        .as_str()
        .filter(|t| !t.is_empty())
        .ok_or_else(|| format!("refresh failed: {}", body["error_description"].as_str().or(body["error"].as_str()).unwrap_or("no token returned")))?
        .to_string();
    connectors::save_connector_credential(&inst.id, "access_token", &token)?;
    if let Some(rt) = body["refresh_token"].as_str() {
        let _ = connectors::save_connector_credential(&inst.id, "refresh_token", rt);
    }
    Ok(token)
}

/// Sign the user into a service and save the connection. One place for
/// the chat tool, the pairing follow-up and the "needs you" bar.
pub async fn connect(state: &AppState, app: &tauri::AppHandle, slug: &str) -> Result<(ConnectorInstance, String), String> {
    let cid = connector_id(slug).ok_or_else(|| format!("no sign-in flow for \"{}\"", slug))?;
    let tokens = if crate::oauth::needs_proxy(slug) {
        crate::oauth::start_proxy_oauth_flow(slug).await?
    } else {
        crate::oauth::start_oauth_flow(slug).await?
    };
    let (user_name, user_email) = crate::oauth::fetch_user_info(slug, &tokens.access_token).await;
    let user_name = user_name.unwrap_or_default();
    let user_email = user_email.unwrap_or_default();

    let instance_id = uuid::Uuid::new_v4().to_string();
    connectors::save_connector_credential(&instance_id, "access_token", &tokens.access_token)?;
    let mut cred_keys = vec!["access_token".to_string()];
    if let Some(rt) = &tokens.refresh_token {
        connectors::save_connector_credential(&instance_id, "refresh_token", rt).ok();
        cred_keys.push("refresh_token".to_string());
    }
    if !user_email.is_empty() {
        connectors::save_connector_credential(&instance_id, "email", &user_email).ok();
        cred_keys.push("email".to_string());
    }
    let display = if !user_name.is_empty() {
        user_name.clone()
    } else if !user_email.is_empty() {
        user_email.clone()
    } else {
        "user".to_string()
    };
    let service_name = info(slug).map(|i| i.name).unwrap_or(slug);
    let instance = ConnectorInstance {
        id: instance_id,
        connector_id: cid,
        name: format!("{} ({})", service_name, display),
        status: ConnectionStatus::Connected,
        created_at: chrono::Utc::now().to_rfc3339(),
        last_tested: Some(chrono::Utc::now().to_rfc3339()),
        error_message: None,
        credential_keys: cred_keys,
    };
    {
        let mut instances = state.connect.connector_instances.lock().unwrap_or_else(|e| e.into_inner());
        // One live connection per service: replace an older one.
        instances.retain(|i| !(i.connector_id.slug() == instance.connector_id.slug() && i.status == ConnectionStatus::Connected));
        instances.push(instance.clone());
        connectors::save_connector_instances(&instances).ok();
    }
    let _ = crate::event_bus::emit_ui(app, "data-changed", "connector-oauth");
    Ok((instance, display))
}

/// `api_call` with a connector: the user's token goes on the request, a
/// relative url is joined to the service's API base, and one expired
/// token is refreshed and retried. Returns Bob's formatted result.
pub async fn api_call(state: &AppState, action: &Value) -> String {
    let raw_url = action["url"].as_str().unwrap_or("").trim().to_string();
    let method = action["method"].as_str().unwrap_or("GET").to_uppercase();
    let body_data = action["body"].clone();
    let connector = action["connector"]
        .as_str()
        .map(|s| s.trim().to_lowercase())
        .filter(|s| !s.is_empty() && s != "none");
    if raw_url.is_empty() {
        return "**[ERR] api_call:** missing url".to_string();
    }
    let url = match &connector {
        Some(slug) => resolve_url(slug, &raw_url),
        None => raw_url.clone(),
    };
    let mut token = match &connector {
        Some(slug) => match bearer_token(state, slug).await {
            Ok(t) => Some(t),
            Err(e) => return format!("**[ERR] api_call:** {}", e),
        },
        None => None,
    };
    let client = reqwest::Client::new();
    for attempt in 0..2 {
        let req = match method.as_str() {
            "POST" => client.post(&url),
            "PUT" => client.put(&url),
            "PATCH" => client.patch(&url),
            "DELETE" => client.delete(&url),
            _ => client.get(&url),
        };
        let mut req = req
            .header("Content-Type", "application/json")
            .header("Accept", "application/json")
            .header("User-Agent", "OrbixCore")
            .timeout(std::time::Duration::from_secs(30));
        if let Some(t) = &token {
            req = req.bearer_auth(t);
        }
        if !body_data.is_null() {
            req = req.json(&body_data);
        }
        match req.send().await {
            Ok(resp) => {
                let status = resp.status();
                let text = resp.text().await.unwrap_or_default();
                let unauthorized = status.as_u16() == 401 || (status.as_u16() == 403 && text.contains("expired"));
                // Slack answers 200 with ok:false on a bad token.
                let slack_expired = connector.as_deref() == Some("slack")
                    && text.contains("\"ok\":false")
                    && (text.contains("token_expired") || text.contains("invalid_auth") || text.contains("not_authed"));
                if attempt == 0 && token.is_some() && (unauthorized || slack_expired) {
                    if let Some(slug) = &connector {
                        if let Ok(fresh) = refresh_access_token(state, slug).await {
                            token = Some(fresh);
                            continue;
                        }
                    }
                }
                if status.is_success() {
                    return crate::support::format_api_result(&method, &url, status.as_u16(), &text);
                }
                let hint = if unauthorized || slack_expired {
                    connector
                        .as_deref()
                        .map(|s| format!("\nThe {} sign-in is no longer valid. connect_oauth \"{}\" signs the user in again.", s, s))
                        .unwrap_or_default()
                } else {
                    String::new()
                };
                return format!(
                    "**[ERR] {} {} ({})**\n```\n{}\n```{}",
                    method,
                    url,
                    status.as_u16(),
                    crate::truncate_str(&text, 2000),
                    hint
                );
            }
            Err(e) => return format!("**[ERR] api_call failed:** {}", e),
        }
    }
    "**[ERR] api_call:** gave up after a token refresh".to_string()
}

/// Prompt lines for connected services: how Bob reaches each one.
pub fn prompt_lines(state: &AppState) -> Vec<String> {
    let instances = state.connect.connector_instances.lock().unwrap_or_else(|e| e.into_inner());
    instances
        .iter()
        .filter(|i| i.status == ConnectionStatus::Connected)
        .map(|i| {
            let slug = i.connector_id.slug();
            match info(&slug) {
                Some(s) => format!(
                    "- {} (connector \"{}\"): api_call with connector \"{}\" sends the user's token; base {}; try {}",
                    i.name, slug, slug, s.api_base, s.hint
                ),
                None => format!("- {} ({})", i.name, slug),
            }
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn urls_join_to_the_service_base() {
        assert_eq!(resolve_url("slack", "/conversations.list"), "https://slack.com/api/conversations.list");
        assert_eq!(resolve_url("slack", "conversations.list"), "https://slack.com/api/conversations.list");
        assert_eq!(resolve_url("github", "https://api.github.com/user"), "https://api.github.com/user");
        assert_eq!(resolve_url("nope", "/x"), "/x");
    }

    #[test]
    fn known_services_have_ids_and_bases() {
        for s in SERVICES {
            assert!(connector_id(s.slug).is_some(), "{}", s.slug);
            assert!(s.api_base.starts_with("https://"));
            assert!(info(s.slug).is_some());
        }
        assert!(connector_id("notion").is_none());
    }
}
