//! Commands split out of main.rs (W0.3). Mechanical move, no behavior change.

#[allow(unused_imports)]
use crate::prelude::*;
#[allow(unused_imports)]
use crate::*;

// ─────────────────────────────────────────────
//  Commands - Browser (Epic F)
// ─────────────────────────────────────────────

#[tauri::command]
pub(crate) async fn browser_navigate(
    url: String,
    state: State<'_, AppState>,
    app: tauri::AppHandle,
) -> Result<BrowserActionResult, String> {
    let result = {
        let mut engine = state.engine.browser.lock().unwrap_or_else(|e| e.into_inner());
        engine.navigate(&url)
    };
    if result.is_ok() {
        let session = state.engine.browser.lock().unwrap_or_else(|e| e.into_inner()).get_session();
        crate::event_bus::emit_ui(&app, "browser-status-change", session).ok();
    }
    result
}

#[tauri::command]
pub(crate) async fn browser_click(
    selector: String,
    state: State<'_, AppState>,
) -> Result<BrowserActionResult, String> {
    let mut engine = state.engine.browser.lock().unwrap_or_else(|e| e.into_inner());
    engine.click(&selector)
}

#[tauri::command]
pub(crate) async fn browser_type_text(
    selector: String,
    text: String,
    state: State<'_, AppState>,
) -> Result<BrowserActionResult, String> {
    let mut engine = state.engine.browser.lock().unwrap_or_else(|e| e.into_inner());
    engine.type_text(&selector, &text)
}

#[tauri::command]
pub(crate) async fn browser_screenshot(
    state: State<'_, AppState>,
) -> Result<BrowserActionResult, String> {
    let mut engine = state.engine.browser.lock().unwrap_or_else(|e| e.into_inner());
    engine.screenshot()
}

#[tauri::command]
pub(crate) async fn browser_get_content(
    state: State<'_, AppState>,
) -> Result<BrowserActionResult, String> {
    let mut engine = state.engine.browser.lock().unwrap_or_else(|e| e.into_inner());
    engine.get_content()
}

#[tauri::command]
pub(crate) async fn browser_evaluate(
    script: String,
    state: State<'_, AppState>,
) -> Result<BrowserActionResult, String> {
    let mut engine = state.engine.browser.lock().unwrap_or_else(|e| e.into_inner());
    engine.evaluate_js(&script)
}

#[tauri::command]
pub(crate) async fn browser_get_session(
    state: State<'_, AppState>,
) -> Result<BrowserSession, String> {
    let engine = state.engine.browser.lock().unwrap_or_else(|e| e.into_inner());
    Ok(engine.get_session())
}

#[tauri::command]
pub(crate) async fn browser_close(
    state: State<'_, AppState>,
    app: tauri::AppHandle,
) -> Result<(), String> {
    let mut engine = state.engine.browser.lock().unwrap_or_else(|e| e.into_inner());
    engine.close();
    crate::event_bus::emit_ui(&app, "browser-status-change", engine.get_session()).ok();
    Ok(())
}


// ─────────────────────────────────────────────
//  Live Chrome driver (W5.4) - drives the user's real browser via CDP
//  attach (port 9222). Distinct from the feature-gated headless engine
//  above; these work at runtime with no rebuild.
// ─────────────────────────────────────────────

const CDP_PORT: u16 = 9222;

#[tauri::command]
pub(crate) async fn chrome_probe() -> Result<bool, String> {
    Ok(crate::browser_attach::probe(CDP_PORT).await)
}

#[tauri::command]
pub(crate) async fn chrome_tabs() -> Result<Vec<crate::browser_attach::TabInfo>, String> {
    crate::browser_attach::list_tabs(CDP_PORT).await
}

#[tauri::command]
pub(crate) async fn chrome_navigate(url: String, tab_id: Option<String>) -> Result<String, String> {
    crate::browser_attach::drive_navigate(CDP_PORT, tab_id.as_deref(), &url).await
}

#[tauri::command]
pub(crate) async fn chrome_click(selector: String, tab_id: Option<String>) -> Result<(), String> {
    crate::browser_attach::drive_click(CDP_PORT, tab_id.as_deref(), &selector).await
}

#[tauri::command]
pub(crate) async fn chrome_type(selector: String, text: String, tab_id: Option<String>) -> Result<(), String> {
    crate::browser_attach::drive_type(CDP_PORT, tab_id.as_deref(), &selector, &text).await
}

#[tauri::command]
pub(crate) async fn chrome_read(selector: Option<String>, tab_id: Option<String>) -> Result<String, String> {
    crate::browser_attach::drive_read_text(CDP_PORT, tab_id.as_deref(), selector.as_deref().unwrap_or("")).await
}
