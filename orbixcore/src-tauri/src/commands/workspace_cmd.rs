//! Commands split out of main.rs (W0.3). Mechanical move, no behavior change.

#[allow(unused_imports)]
use crate::prelude::*;
#[allow(unused_imports)]
use crate::*;

// ─────────────────────────────────────────────
//  Commands - Workspace setting (ORBIX_HANDOFF.md Step 2 #1)
// ─────────────────────────────────────────────

#[tauri::command]
pub(crate) fn bob_cli_install(app: tauri::AppHandle) -> Result<bob_cli_install::InstallReport, String> {
    bob_cli_install::install(&app)
}

#[tauri::command]
pub(crate) fn workspace_get(state: State<'_, AppState>) -> Result<workspace::WorkspaceSettings, String> {
    let ws = state.engine.workspace.lock().map_err(|e| e.to_string())?;
    Ok(ws.clone())
}

/// Git repos the user can bind chat actions to (/repo palette). Scans
/// the workspace plus common project roots; runs on a blocking thread
/// so a large disk walk never stalls the async runtime.
#[tauri::command]
pub(crate) async fn list_git_repos(state: State<'_, AppState>) -> Result<Vec<crate::repos::RepoInfo>, String> {
    let workspace = {
        let ws = state.engine.workspace.lock().map_err(|e| e.to_string())?;
        ws.usable_path()
    };
    tokio::task::spawn_blocking(move || crate::repos::scan(workspace))
        .await
        .map_err(|e| format!("Repo scan failed: {}", e))
}

/// User-installed skills in the active workspace (for the "/" palette).
#[tauri::command]
pub(crate) async fn list_skills() -> Result<Vec<crate::skills::SkillMeta>, String> {
    Ok(tokio::task::spawn_blocking(crate::skills::list)
        .await
        .map_err(|e| e.to_string())?)
}

#[tauri::command]
pub(crate) fn workspace_set(
    path: Option<String>,
    state: State<'_, AppState>,
) -> Result<workspace::WorkspaceSettings, String> {
    let mut ws = state.engine.workspace.lock().map_err(|e| e.to_string())?;
    // Empty string is the wire-level "unset" signal; treat it as None
    // so the frontend can clear the field without a separate command.
    let trimmed = path.as_deref().map(|s| s.trim().to_string()).filter(|s| !s.is_empty());
    if let Some(p) = &trimmed {
        let pb = std::path::PathBuf::from(p);
        if !pb.is_dir() {
            return Err(format!(
                "'{}' is not an existing directory - pick a real folder",
                p,
            ));
        }
    }
    ws.path = trimmed;
    ws.save()?;
    Ok(ws.clone())
}

#[tauri::command]
pub(crate) async fn open_claude_terminal(state: State<'_, AppState>) -> Result<String, String> {
    // Prefer the user-set workspace; fall back to the historical hardcode
    // only if they have not set one yet, so existing users do not lose
    // the "open this repo" affordance the moment they install this build.
    let project_dir = {
        let ws = state.engine.workspace.lock().map_err(|e| e.to_string())?;
        ws.usable_path()
            .map(|p| p.to_string_lossy().into_owned())
            .unwrap_or_else(|| {
                let home = dirs::home_dir().unwrap_or_default().display().to_string();
                format!("{}/Documents/GitHub/orbixcoreapp.mac", home)
            })
    };
    let script = format!(r#"tell application "Terminal"
        activate
        do script "cd '{}' && claude"
    end tell"#, project_dir);
    tokio::process::Command::new("osascript")
        .arg("-e")
        .arg(&script)
        .output()
        .await
        .map(|_| "ok".to_string())
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub(crate) async fn os_get_active_window() -> Result<os_auto::WindowInfo, String> {
    os_auto::get_active_window().await
}

#[tauri::command]
pub(crate) async fn os_list_windows() -> Result<Vec<os_auto::WindowInfo>, String> {
    os_auto::list_windows().await
}

#[tauri::command]
pub(crate) async fn os_run_applescript(script: String) -> Result<os_auto::OsActionResult, String> {
    Ok(os_auto::run_applescript(&script).await)
}

