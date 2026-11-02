//! Commands split out of main.rs (W0.3). Mechanical move, no behavior change.

#[allow(unused_imports)]
use crate::prelude::*;
#[allow(unused_imports)]
use crate::*;

// ─────────────────────────────────────────────
//  Commands - Orb State
// ─────────────────────────────────────────────

#[tauri::command]
pub(crate) async fn get_orb_state(state: State<'_, AppState>) -> Result<OrbState, String> {
    let orb = state.chat.orb_state.lock().unwrap_or_else(|e| e.into_inner());
    Ok(orb.clone())
}

#[tauri::command]
pub(crate) async fn set_orb_state(
    new_state: OrbState,
    state: State<'_, AppState>,
    app: tauri::AppHandle,
) -> Result<(), String> {
    {
        let mut orb = state.chat.orb_state.lock().unwrap_or_else(|e| e.into_inner());
        *orb = new_state.clone();
    }
    crate::event_bus::emit_ui(&app, "orb-state-change", new_state).ok();
    Ok(())
}

