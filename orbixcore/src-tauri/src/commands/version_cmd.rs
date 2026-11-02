//! Commands split out of main.rs (W0.3). Mechanical move, no behavior change.

#[allow(unused_imports)]
use crate::prelude::*;
#[allow(unused_imports)]
use crate::*;

// ─────────────────────────────────────────────
//  Commands - App version
// ─────────────────────────────────────────────

#[tauri::command]
pub(crate) async fn get_app_version() -> Result<app_version::AppVersion, String> {
    Ok(app_version::current())
}
