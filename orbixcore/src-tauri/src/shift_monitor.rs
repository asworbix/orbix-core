// ─────────────────────────────────────────────
//  Shift monitor - live window into a running agent
//
//  While an agent works a shift the user can watch it, steer it, and
//  stop it - without touching Bob. Each running shift registers a
//  session here carrying its own control channel:
//
//    cancel  - per-run stop flag. Deliberately NOT BOB_CANCEL_FLAG:
//              stopping Bob must never kill a background shift and
//              stopping a shift must never kill Bob's reply.
//    inbox   - user messages typed into the monitor window, drained
//              into the shift's history between agentic turns.
//
//  The monitor UI is the Run monitor window ("monitor", see
//  open_window below), opened beside the main window when a shift
//  starts. It renders the run log (run_log.rs) plus the shift-* events
//  and the message-chunk stream (shift ids are "agent-shift-…"), so
//  nothing here duplicates the streaming pipeline.
// ─────────────────────────────────────────────

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};

use serde::Serialize;

/// Control channel a running shift polls between turns.
#[derive(Clone, Default)]
pub struct ShiftControl {
    pub cancel: Arc<AtomicBool>,
    pub inbox: Arc<Mutex<Vec<String>>>,
}

tokio::task_local! {
    /// The shift id of the run executing in this task, if any. bob_step
    /// consults it to redirect step events (including the ones emitted
    /// deep inside the tool executors) from Bob's chat ribbon to the
    /// monitor window, without threading a channel through every
    /// executor signature.
    pub static STEP_SCOPE: String;
}

/// The shift id of the current task, when inside a shift's STEP_SCOPE.
pub fn current_scope() -> Option<String> {
    STEP_SCOPE.try_with(|id| id.clone()).ok()
}

struct ShiftSession {
    shift_id: String,
    agent_id: String,
    agent_name: String,
    started_at: String,
    control: ShiftControl,
}

/// Wire view of a session (control stays server-side).
#[derive(Clone, Serialize)]
pub struct ShiftSessionWire {
    pub shift_id: String,
    pub agent_id: String,
    pub agent_name: String,
    pub started_at: String,
}

static SESSIONS: Mutex<Vec<ShiftSession>> = Mutex::new(Vec::new());

fn sessions() -> std::sync::MutexGuard<'static, Vec<ShiftSession>> {
    SESSIONS.lock().unwrap_or_else(|e| e.into_inner())
}

/// Register a starting shift and hand back its control channel.
pub fn register(shift_id: &str, agent_id: &str, agent_name: &str) -> ShiftControl {
    let control = ShiftControl::default();
    sessions().push(ShiftSession {
        shift_id: shift_id.to_string(),
        agent_id: agent_id.to_string(),
        agent_name: agent_name.to_string(),
        started_at: chrono::Utc::now().to_rfc3339(),
        control: control.clone(),
    });
    control
}

pub fn unregister(shift_id: &str) {
    sessions().retain(|s| s.shift_id != shift_id);
}

pub fn list_wire() -> Vec<ShiftSessionWire> {
    sessions()
        .iter()
        .map(|s| ShiftSessionWire {
            shift_id: s.shift_id.clone(),
            agent_id: s.agent_id.clone(),
            agent_name: s.agent_name.clone(),
            started_at: s.started_at.clone(),
        })
        .collect()
}

/// Queue a user message for a running shift. The loop drains it into
/// the model's history before its next turn.
pub fn push_user_message(shift_id: &str, content: &str) -> Result<(), String> {
    let guard = sessions();
    let s = guard
        .iter()
        .find(|s| s.shift_id == shift_id)
        .ok_or_else(|| "That shift is no longer running.".to_string())?;
    s.control
        .inbox
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .push(content.to_string());
    Ok(())
}

/// Request a running shift to stop after its current step.
pub fn request_stop(shift_id: &str) -> Result<(), String> {
    let guard = sessions();
    let s = guard
        .iter()
        .find(|s| s.shift_id == shift_id)
        .ok_or_else(|| "That shift is no longer running.".to_string())?;
    s.control.cancel.store(true, Ordering::SeqCst);
    Ok(())
}

/// The Run monitor: a small square free-standing window, a glance face
/// like a watch (what is running, the step it is on, the last few
/// events, Stop) that expands to the full log on demand. Not attached
/// to the main window, so it can sit anywhere and stays put while the
/// main window moves. First opens beside the main window; after that it
/// remembers where the user left it. Never steals focus: a scheduled
/// shift popping it up mid-typing must not grab the keyboard.
pub const MONITOR_LABEL: &str = "monitor";
pub const MONITOR_FACE: (f64, f64) = (380.0, 380.0);

#[derive(Clone, Copy, serde::Serialize, serde::Deserialize)]
struct Frame {
    x: f64,
    y: f64,
    w: f64,
    h: f64,
}

fn frame_path() -> std::path::PathBuf {
    crate::storage::base_dir().join("monitor_frame.json")
}

fn load_frame() -> Option<Frame> {
    let f: Frame = serde_json::from_str(&std::fs::read_to_string(frame_path()).ok()?).ok()?;
    (f.w >= 300.0 && f.h >= 300.0).then_some(f)
}

/// Save the window's logical frame, throttled so a drag does not write
/// on every pixel.
fn remember(win: &tauri::WebviewWindow) {
    use std::sync::atomic::{AtomicU64, Ordering};
    static LAST: AtomicU64 = AtomicU64::new(0);
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0);
    if now.saturating_sub(LAST.load(Ordering::Relaxed)) < 400 {
        return;
    }
    LAST.store(now, Ordering::Relaxed);
    let scale = win.scale_factor().unwrap_or(1.0);
    let (Ok(pos), Ok(size)) = (win.outer_position(), win.inner_size()) else { return };
    let frame = Frame {
        x: pos.x as f64 / scale,
        y: pos.y as f64 / scale,
        w: size.width as f64 / scale,
        h: size.height as f64 / scale,
    };
    if let Ok(json) = serde_json::to_string(&frame) {
        std::fs::write(frame_path(), json).ok();
    }
}

/// Whether runs open the monitor on their own (Settings > Run monitor).
/// A manual open (the palette, the Agents panel, "open the monitor")
/// always works; this only governs the automatic pop-up.
#[derive(Clone, Copy, serde::Serialize, serde::Deserialize)]
pub struct MonitorSettings {
    pub auto_open: bool,
}

impl Default for MonitorSettings {
    fn default() -> Self {
        Self { auto_open: true }
    }
}

fn settings_path() -> std::path::PathBuf {
    crate::storage::base_dir().join("monitor_settings.json")
}

pub fn settings() -> MonitorSettings {
    std::fs::read_to_string(settings_path())
        .ok()
        .and_then(|s| serde_json::from_str(&s).ok())
        .unwrap_or_default()
}

pub fn save_settings(settings: MonitorSettings) -> Result<(), String> {
    let json = serde_json::to_string_pretty(&settings).map_err(|e| e.to_string())?;
    std::fs::write(settings_path(), json).map_err(|e| e.to_string())
}

/// Open the monitor because a run started: honours the auto-open setting.
pub fn open_window_for_run(app: &tauri::AppHandle) {
    if settings().auto_open {
        open_window(app);
    }
}

pub fn open_window(app: &tauri::AppHandle) {
    let app = app.clone();
    // Window creation must happen on the main thread on macOS.
    let _ = app.clone().run_on_main_thread(move || {
        use tauri::{WebviewUrl, WebviewWindowBuilder, WindowEvent};
        if let Some(win) = tauri::Manager::get_webview_window(&app, MONITOR_LABEL) {
            win.show().ok();
            return;
        }
        let remembered = load_frame();
        let beside_main = tauri::Manager::get_webview_window(&app, "main").and_then(|main| {
            let scale = main.scale_factor().unwrap_or(1.0);
            let pos = main.outer_position().ok()?;
            let size = main.outer_size().ok()?;
            Some((
                (pos.x as f64 + size.width as f64) / scale + 12.0,
                pos.y as f64 / scale,
            ))
        });
        let position = remembered.map(|f| (f.x, f.y)).or(beside_main);
        let (w, h) = remembered.map(|f| (f.w, f.h)).unwrap_or(MONITOR_FACE);
        let mut b = WebviewWindowBuilder::new(
            &app,
            MONITOR_LABEL,
            WebviewUrl::App("index.html?window=monitor".into()),
        )
        .title("Run monitor")
        .inner_size(w, h)
        .min_inner_size(300.0, 300.0)
        .focused(false);
        // hidden_title / TitleBarStyle::Overlay are macOS-only builder
        // methods; Windows keeps the standard window chrome here.
        #[cfg(target_os = "macos")]
        {
            b = b.hidden_title(true).title_bar_style(tauri::TitleBarStyle::Overlay);
        }
        if let Some((x, y)) = position {
            b = b.position(x, y);
        }
        match b.build() {
            Ok(win) => {
                let tracked = win.clone();
                win.on_window_event(move |e| {
                    if matches!(e, WindowEvent::Moved(_) | WindowEvent::Resized(_)) {
                        remember(&tracked);
                    }
                });
            }
            Err(e) => eprintln!("[monitor] could not open the run monitor window: {}", e),
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn register_control_and_stop_roundtrip() {
        let control = register("agent-shift-t1", "a1", "Watcher");
        assert!(!control.cancel.load(Ordering::SeqCst));
        push_user_message("agent-shift-t1", "focus on invoices").unwrap();
        assert_eq!(
            control.inbox.lock().unwrap().as_slice(),
            &["focus on invoices".to_string()]
        );
        request_stop("agent-shift-t1").unwrap();
        assert!(control.cancel.load(Ordering::SeqCst));
        assert_eq!(list_wire().iter().filter(|s| s.shift_id == "agent-shift-t1").count(), 1);
        unregister("agent-shift-t1");
        assert!(push_user_message("agent-shift-t1", "x").is_err());
        assert!(request_stop("agent-shift-t1").is_err());
    }
}
