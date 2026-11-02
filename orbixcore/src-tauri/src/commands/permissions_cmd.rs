//! Commands split out of main.rs (W0.3). Mechanical move, no behavior change.

#[allow(unused_imports)]
use crate::prelude::*;
#[allow(unused_imports)]
use crate::*;

// ─────────────────────────────────────────────
//  Commands - Window Intelligence (Epic H)
// ─────────────────────────────────────────────


// ─────────────────────────────────────────────
//  Commands - Permission Check (Onboarding)
// ─────────────────────────────────────────────


#[tauri::command]
pub(crate) async fn check_all_permissions(state: State<'_, AppState>) -> Result<PermissionStatus, String> {
    // Accessibility - check via AXIsProcessTrusted
    let accessibility = {
        let store = state.connect.window_intelligence_store.lock().map_err(|e| e.to_string())?;
        let status = store.check_accessibility();
        status.trusted
    };

    // Full Disk Access - try to read a protected path
    let full_disk_access = std::fs::metadata(
        dirs::home_dir().unwrap_or_default().join("Library/Mail")
    ).is_ok();

    // Notifications - check via UNUserNotificationCenter (approximate: check if we can create dir in app support)
    let notifications = true; // macOS apps don't need explicit notification permission for local notifications

    // Local network - check if llama-server port is bindable
    let local_network = std::net::TcpStream::connect(("127.0.0.1", local_llm::llama_port()))
        .map(|_| true)
        .unwrap_or(true); // Not running is fine, just needs network access

    Ok(PermissionStatus {
        accessibility,
        full_disk_access,
        notifications,
        local_network,
    })
}

#[tauri::command]
pub(crate) async fn open_system_preferences(pane: String) -> Result<(), String> {
    let url = match pane.as_str() {
        "accessibility" => "x-apple.systempreferences:com.apple.preference.security?Privacy_Accessibility",
        "full_disk_access" => "x-apple.systempreferences:com.apple.preference.security?Privacy_AllFiles",
        "notifications" => "x-apple.systempreferences:com.apple.preference.notifications",
        "automation" => "x-apple.systempreferences:com.apple.preference.security?Privacy_Automation",
        _ => "x-apple.systempreferences:com.apple.preference.security",
    };
    tokio::process::Command::new("open")
        .arg(url)
        .output()
        .await
        .map_err(|e| e.to_string())?;
    Ok(())
}

#[tauri::command]
pub(crate) fn wi_check_accessibility(state: State<'_, AppState>) -> Result<window_intelligence::AccessibilityStatus, String> {
    let store = state.connect.window_intelligence_store.lock().map_err(|e| e.to_string())?;
    Ok(store.check_accessibility())
}

#[tauri::command]
pub(crate) fn wi_request_accessibility(state: State<'_, AppState>) -> Result<window_intelligence::AccessibilityStatus, String> {
    let store = state.connect.window_intelligence_store.lock().map_err(|e| e.to_string())?;
    Ok(store.request_accessibility())
}

#[tauri::command]
pub(crate) fn wi_list_running_apps(state: State<'_, AppState>) -> Result<Vec<window_intelligence::RunningApp>, String> {
    let store = state.connect.window_intelligence_store.lock().map_err(|e| e.to_string())?;
    Ok(store.list_running_apps())
}

#[tauri::command]
pub(crate) async fn wi_profile_window(pid: i64, app_name: String, state: State<'_, AppState>) -> Result<window_intelligence::WindowProfile, String> {
    // Run in a blocking task since profile_window spawns a subprocess
    let profile = tokio::task::spawn_blocking(move || {
        // We can't hold the lock across await, so do it all in the blocking closure
        let exe = std::env::current_exe().map_err(|e| e.to_string())?;
        let output = std::process::Command::new(&exe)
            .arg("--wi-profile")
            .arg(pid.to_string())
            .arg(&app_name)
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::piped())
            .output()
            .map_err(|e| format!("Failed to spawn profiler subprocess: {}", e))?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            return Err(format!(
                "Profiling {} failed (the application may have an incompatible accessibility tree). {}",
                app_name,
                if stderr.len() > 200 { &stderr[..200] } else { &stderr }
            ));
        }

        let stdout = String::from_utf8_lossy(&output.stdout);
        let profile: window_intelligence::WindowProfile = serde_json::from_str(&stdout)
            .map_err(|e| format!("Failed to parse profile result: {}", e))?;
        Ok(profile)
    }).await.map_err(|e| e.to_string())??;

    // Now update the store with the result
    let mut store = state.connect.window_intelligence_store.lock().map_err(|e| e.to_string())?;
    store.profiles.retain(|p| p.pid != pid);
    store.profiles.push(profile.clone());
    store.events.push(window_intelligence::WindowTwinEvent {
        id: uuid::Uuid::new_v4().to_string(),
        window_id: profile.window_id.clone(),
        event_type: window_intelligence::TwinEventType::ProfileCreated,
        field_id: None,
        old_value: None,
        new_value: Some(format!("{} fields, {} actions", profile.field_count, profile.action_count)),
        timestamp: chrono::Utc::now().to_rfc3339(),
    });
    store.save().ok();
    Ok(profile)
}

#[tauri::command]
pub(crate) async fn wi_refresh_profile(window_id: String, state: State<'_, AppState>) -> Result<window_intelligence::WindowProfile, String> {
    let (pid, app_name, captured_at) = {
        let store = state.connect.window_intelligence_store.lock().map_err(|e| e.to_string())?;
        store.get_profile_info(&window_id)?
    };

    let profile = tokio::task::spawn_blocking(move || {
        let exe = std::env::current_exe().map_err(|e| e.to_string())?;
        let output = std::process::Command::new(&exe)
            .arg("--wi-profile")
            .arg(pid.to_string())
            .arg(&app_name)
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::piped())
            .output()
            .map_err(|e| format!("Failed to spawn profiler subprocess: {}", e))?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            return Err(format!("Refresh failed: {}", if stderr.len() > 200 { &stderr[..200] } else { &stderr }));
        }

        let stdout = String::from_utf8_lossy(&output.stdout);
        serde_json::from_str::<window_intelligence::WindowProfile>(&stdout)
            .map_err(|e| format!("Failed to parse profile: {}", e))
    }).await.map_err(|e| e.to_string())??;

    let mut store = state.connect.window_intelligence_store.lock().map_err(|e| e.to_string())?;
    store.store_refreshed_profile(&window_id, profile.clone(), captured_at);
    Ok(profile)
}

#[tauri::command]
pub(crate) fn wi_get_library(state: State<'_, AppState>) -> Result<window_intelligence::WindowLibrary, String> {
    let store = state.connect.window_intelligence_store.lock().map_err(|e| e.to_string())?;
    Ok(store.get_library())
}

#[tauri::command]
pub(crate) fn wi_get_profile(window_id: String, state: State<'_, AppState>) -> Result<Option<window_intelligence::WindowProfile>, String> {
    let store = state.connect.window_intelligence_store.lock().map_err(|e| e.to_string())?;
    Ok(store.get_profile(&window_id).cloned())
}

#[tauri::command]
pub(crate) fn wi_remove_profile(window_id: String, state: State<'_, AppState>) -> Result<(), String> {
    let mut store = state.connect.window_intelligence_store.lock().map_err(|e| e.to_string())?;
    store.remove_profile(&window_id)
}

#[tauri::command]
pub(crate) fn wi_read_fields(window_id: String, fields: Vec<String>, state: State<'_, AppState>) -> Result<window_intelligence::WindowReadResult, String> {
    let store = state.connect.window_intelligence_store.lock().map_err(|e| e.to_string())?;
    store.read_fields(&window_id, fields)
}

#[tauri::command]
pub(crate) fn wi_write_fields(window_id: String, writes: Vec<window_intelligence::WindowWriteRequest>, state: State<'_, AppState>) -> Result<window_intelligence::WindowWriteResult, String> {
    let mut store = state.connect.window_intelligence_store.lock().map_err(|e| e.to_string())?;
    store.write_fields(&window_id, writes)
}

#[tauri::command]
pub(crate) fn wi_trigger_action(window_id: String, action_id: String, state: State<'_, AppState>) -> Result<window_intelligence::WindowActionResult, String> {
    let mut store = state.connect.window_intelligence_store.lock().map_err(|e| e.to_string())?;
    store.trigger_action(&window_id, &action_id)
}

#[tauri::command]
pub(crate) fn wi_add_watched(watched: window_intelligence::WatchedWindow, state: State<'_, AppState>) -> Result<window_intelligence::WatchedWindow, String> {
    let mut store = state.connect.window_intelligence_store.lock().map_err(|e| e.to_string())?;
    store.add_watched(watched)
}

#[tauri::command]
pub(crate) fn wi_remove_watched(id: String, state: State<'_, AppState>) -> Result<(), String> {
    let mut store = state.connect.window_intelligence_store.lock().map_err(|e| e.to_string())?;
    store.remove_watched(&id)
}

#[tauri::command]
pub(crate) fn wi_toggle_watched(id: String, state: State<'_, AppState>) -> Result<window_intelligence::WatchedWindow, String> {
    let mut store = state.connect.window_intelligence_store.lock().map_err(|e| e.to_string())?;
    store.toggle_watched(&id)
}

#[tauri::command]
pub(crate) fn wi_get_events(window_id: Option<String>, limit: Option<usize>, state: State<'_, AppState>) -> Result<Vec<window_intelligence::WindowTwinEvent>, String> {
    let store = state.connect.window_intelligence_store.lock().map_err(|e| e.to_string())?;
    Ok(store.get_events(window_id.as_deref(), limit.unwrap_or(50)))
}


// ─────────────────────────────────────────────
//  Commands - App bridges + Automation permission (FABLE A4)
// ─────────────────────────────────────────────

/// Status of every app bridge for the "Work on top of apps" settings
/// section. Probes only apps that are already running - never launches.
#[tauri::command]
pub(crate) async fn app_bridge_list() -> Result<Vec<app_bridge::BridgeStatus>, String> {
    Ok(app_bridge::list_bridges().await)
}

/// Silent Automation permission check (no OS prompt).
#[tauri::command]
pub(crate) async fn automation_check() -> Result<app_bridge::AutomationStatus, String> {
    Ok(app_bridge::automation_status(false).await)
}

/// Request Automation - shows the one-time OS consent prompt when the
/// state is undetermined.
#[tauri::command]
pub(crate) async fn automation_request() -> Result<app_bridge::AutomationStatus, String> {
    Ok(app_bridge::automation_status(true).await)
}
