//! Commands split out of main.rs (W0.3). Mechanical move, no behavior change.

#[allow(unused_imports)]
use crate::prelude::*;
#[allow(unused_imports)]
use crate::*;

// ─────────────────────────────────────────────
//  Commands - OS Automation (Epic H)
// ─────────────────────────────────────────────

#[tauri::command]
pub(crate) async fn os_get_system_info() -> Result<os_auto::SystemInfo, String> {
    os_auto::get_system_info().await
}

#[tauri::command]
pub(crate) async fn os_open_application(name: String) -> Result<os_auto::OsActionResult, String> {
    Ok(os_auto::open_application(&name).await)
}

#[tauri::command]
pub(crate) async fn os_quit_application(name: String) -> Result<os_auto::OsActionResult, String> {
    Ok(os_auto::quit_application(&name).await)
}

#[tauri::command]
pub(crate) async fn os_list_running_apps() -> Result<Vec<os_auto::RunningApp>, String> {
    os_auto::list_running_apps().await
}

#[tauri::command]
pub(crate) async fn os_get_clipboard() -> Result<String, String> {
    os_auto::get_clipboard().await
}

#[tauri::command]
pub(crate) async fn os_set_clipboard(text: String) -> Result<os_auto::OsActionResult, String> {
    Ok(os_auto::set_clipboard(&text).await)
}

#[tauri::command]
pub(crate) async fn os_send_notification(app: tauri::AppHandle, title: String, body: String) -> Result<os_auto::OsActionResult, String> {
    Ok(os_auto::send_notification(&app, &title, &body).await)
}

#[tauri::command]
pub(crate) async fn os_open_url(url: String) -> Result<os_auto::OsActionResult, String> {
    Ok(os_auto::open_url(&url).await)
}

#[tauri::command]
pub(crate) async fn os_open_file(path: String) -> Result<os_auto::OsActionResult, String> {
    Ok(os_auto::open_file(&path).await)
}

#[tauri::command]
pub(crate) async fn os_reveal_in_finder(path: String) -> Result<os_auto::OsActionResult, String> {
    Ok(os_auto::reveal_in_finder(&path).await)
}

