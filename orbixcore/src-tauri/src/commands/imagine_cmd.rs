//! Imagine commands: start Bob's free-rein run, steer it, stop it, and
//! ask what is running so the overlay can re-attach.

use crate::imagine::ImagineRun;

/// Start a run with an optional hint. Returns at once; the run reports
/// through imagine-started, shift-step, message-chunk, and imagine-ended.
#[tauri::command]
pub(crate) async fn imagine_start(seed: Option<String>, app: tauri::AppHandle) -> Result<ImagineRun, String> {
    crate::imagine::start(app, seed)
}

/// The run in progress, if any.
#[tauri::command]
pub(crate) async fn imagine_status() -> Result<Option<ImagineRun>, String> {
    Ok(crate::imagine::current())
}

/// Queue a hint for the running Imagine; the model reads it before its
/// next turn.
#[tauri::command]
pub(crate) async fn imagine_nudge(content: String) -> Result<(), String> {
    let content = content.trim().to_string();
    if content.is_empty() {
        return Err("Message is empty.".into());
    }
    crate::imagine::nudge(&content)
}

/// Stop the running Imagine after its current step.
#[tauri::command]
pub(crate) async fn imagine_stop(app: tauri::AppHandle) -> Result<(), String> {
    crate::imagine::request_stop()?;
    if let Some(run) = crate::imagine::current() {
        crate::event_bus::emit_ui(&app, "shift-step", serde_json::json!({
            "shift_id": run.run_id,
            "label": "Stopping…",
        })).ok();
    }
    Ok(())
}
