//! Commands split out of main.rs (W0.3). Mechanical move, no behavior change.

#[allow(unused_imports)]
use crate::prelude::*;
#[allow(unused_imports)]
use crate::*;

// ─────────────────────────────────────────────
//  Commands - MCP Servers
// ─────────────────────────────────────────────

#[tauri::command]
pub(crate) async fn list_mcp_servers(state: State<'_, AppState>) -> Result<Vec<McpServer>, String> {
    let servers = state.chat.mcp_servers.lock().unwrap_or_else(|e| e.into_inner());
    Ok(servers.clone())
}

#[tauri::command]
pub(crate) async fn add_mcp_server(
    input: McpServerInput,
    state: State<'_, AppState>,
) -> Result<McpServer, String> {
    let server = McpServer {
        id: Uuid::new_v4().to_string(),
        name: input.name,
        transport: input.transport,
        command: input.command,
        args: input.args,
        env: input.env,
        url: input.url,
        enabled: true,
        status: McpStatus::Stopped,
        created_at: Utc::now().to_rfc3339(),
        bearer_from_service: None,
    };
    let mut servers = state.chat.mcp_servers.lock().unwrap_or_else(|e| e.into_inner());
    servers.push(server.clone());
    storage::save_mcp_servers(&servers).ok();
    Ok(server)
}

#[tauri::command]
pub(crate) async fn remove_mcp_server(id: String, state: State<'_, AppState>) -> Result<(), String> {
    let mut servers = state.chat.mcp_servers.lock().unwrap_or_else(|e| e.into_inner());
    servers.retain(|s| s.id != id);
    storage::save_mcp_servers(&servers).ok();
    Ok(())
}

#[tauri::command]
pub(crate) async fn toggle_mcp_server(id: String, state: State<'_, AppState>) -> Result<McpServer, String> {
    let toggled = {
        let mut servers = state.chat.mcp_servers.lock().unwrap_or_else(|e| e.into_inner());
        let Some(server) = servers.iter_mut().find(|s| s.id == id) else {
            return Err(format!("MCP server {} not found", id));
        };
        server.enabled = !server.enabled;
        server.status = if server.enabled { McpStatus::Unknown } else { McpStatus::Stopped };
        let result = server.clone();
        storage::save_mcp_servers(&servers).ok();
        result
    };
    if !toggled.enabled {
        crate::mcp_client::stop(&id).await;
    }
    Ok(toggled)
}

