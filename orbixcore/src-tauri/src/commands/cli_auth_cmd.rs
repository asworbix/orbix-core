//! Commands split out of main.rs (W0.3). Mechanical move, no behavior change.

#[allow(unused_imports)]
use crate::prelude::*;
#[allow(unused_imports)]
use crate::*;

// ─────────────────────────────────────────────
//  Connected Services (CLI Auth)
// ─────────────────────────────────────────────

#[tauri::command]
pub(crate) fn list_connected_services() -> std::collections::HashMap<String, cli_auth::ConnectedService> {
    cli_auth::load_services()
}

#[tauri::command]
pub(crate) fn get_connected_service(provider: String) -> Option<cli_auth::ConnectedService> {
    cli_auth::get_service(&provider)
}

#[tauri::command]
pub(crate) fn list_auth_providers() -> Vec<serde_json::Value> {
    cli_auth::providers().iter().map(|p| {
        serde_json::json!({
            "name": p.name,
            "display_name": p.display_name,
            "supports_device_flow": p.supports_device_flow,
            "connected": cli_auth::get_service(p.name).is_some(),
        })
    }).collect()
}

#[tauri::command]
pub(crate) async fn quick_auth_login(provider: String) -> Result<String, String> {
    cli_auth::login(&provider).await?;
    Ok(format!("Connected to {}", provider))
}

#[tauri::command]
pub(crate) async fn quick_auth_logout(provider: String) -> Result<String, String> {
    cli_auth::logout(&provider).await?;
    Ok(format!("Disconnected from {}", provider))
}

