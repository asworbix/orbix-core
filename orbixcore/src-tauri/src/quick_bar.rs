// ─────────────────────────────────────────────
//  Quick bar - Bob from anywhere, one shortcut away
//
//  A global shortcut (default Cmd+Shift+Space on macOS, Ctrl+Alt+Space
//  on Windows) opens a small floating bar over whatever app is in front.
//  Type, Enter: the same Bob turn as the chat (the message and reply show
//  up there too), with his progress in the bar while he works. Esc or
//  clicking away hides it; the shortcut again brings it back with the
//  last answer still there.
//
//  The bar is one more Tauri window (label "bar") loading the same bundle
//  with ?window=bar, like the run monitor. It is created once and then
//  shown and hidden, so a turn keeps running while it is out of sight.
//
//  Settings in base_dir()/quick_bar.json: enabled + the shortcut. The
//  defaults avoid what Microsoft Copilot takes (Option+Space on macOS,
//  Alt+Space on Windows) and what the OS uses. On Windows a shortcut
//  another app owns fails to register and Settings says so; macOS lets
//  two apps register the same one, so there the choice list is the guard.
// ─────────────────────────────────────────────

use serde::{Deserialize, Serialize};
use std::sync::Mutex;
use tauri::Manager;
use tauri_plugin_global_shortcut::{GlobalShortcutExt, ShortcutState};

pub const BAR_LABEL: &str = "bar";
const BAR_WIDTH: f64 = 680.0;
const BAR_HEIGHT: f64 = 72.0;

#[cfg(target_os = "macos")]
pub const DEFAULT_SHORTCUT: &str = "Super+Shift+Space";
#[cfg(not(target_os = "macos"))]
pub const DEFAULT_SHORTCUT: &str = "Ctrl+Alt+Space";

/// The shortcuts Settings offers. The first is the default.
#[cfg(target_os = "macos")]
pub const CHOICES: &[&str] = &["Super+Shift+Space", "Ctrl+Alt+Space", "Ctrl+Shift+B"];
#[cfg(not(target_os = "macos"))]
pub const CHOICES: &[&str] = &["Ctrl+Alt+Space", "Ctrl+Shift+Space", "Alt+Shift+B", "Ctrl+Shift+B"];

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct QuickBarSettings {
    #[serde(default = "yes")]
    pub enabled: bool,
    #[serde(default = "default_shortcut")]
    pub shortcut: String,
}

fn yes() -> bool {
    true
}

fn default_shortcut() -> String {
    DEFAULT_SHORTCUT.to_string()
}

impl Default for QuickBarSettings {
    fn default() -> Self {
        Self { enabled: true, shortcut: default_shortcut() }
    }
}

/// What Settings shows: the saved settings, whether the shortcut is live,
/// and why not when it is not.
#[derive(Debug, Clone, Serialize)]
pub struct QuickBarStatus {
    pub settings: QuickBarSettings,
    pub registered: bool,
    pub error: Option<String>,
    pub choices: Vec<String>,
}

static LAST_ERROR: Mutex<Option<String>> = Mutex::new(None);

fn settings_path() -> std::path::PathBuf {
    crate::storage::base_dir().join("quick_bar.json")
}

pub fn settings() -> QuickBarSettings {
    std::fs::read_to_string(settings_path())
        .ok()
        .and_then(|s| serde_json::from_str(&s).ok())
        .unwrap_or_default()
}

pub fn status(app: &tauri::AppHandle) -> QuickBarStatus {
    let s = settings();
    let registered = s.enabled && app.global_shortcut().is_registered(s.shortcut.as_str());
    QuickBarStatus {
        settings: s,
        registered,
        error: LAST_ERROR.lock().unwrap_or_else(|e| e.into_inner()).clone(),
        choices: CHOICES.iter().map(|c| c.to_string()).collect(),
    }
}

/// Save and re-register. The new shortcut is tried before it is saved, so
/// one that another app owns never replaces a working one.
pub fn save_settings(app: &tauri::AppHandle, next: QuickBarSettings) -> QuickBarStatus {
    let previous = settings();
    match apply(app, &next) {
        Ok(()) => {
            if let Ok(json) = serde_json::to_string_pretty(&next) {
                let _ = std::fs::write(settings_path(), json);
            }
        }
        Err(e) => {
            let _ = apply(app, &previous);
            // Re-applying the old shortcut cleared the error; Settings
            // needs the failed attempt's reason.
            *LAST_ERROR.lock().unwrap_or_else(|e| e.into_inner()) = Some(e);
        }
    }
    status(app)
}

/// Register the saved shortcut at startup. Never fails the launch: a
/// shortcut that cannot be registered is reported in Settings instead.
pub fn init(app: &tauri::AppHandle) {
    let s = settings();
    if let Err(e) = apply(app, &s) {
        eprintln!("[quick_bar] shortcut {} not registered: {}", s.shortcut, e);
    }
}

fn apply(app: &tauri::AppHandle, s: &QuickBarSettings) -> Result<(), String> {
    let gs = app.global_shortcut();
    let _ = gs.unregister_all();
    let result = if s.enabled {
        gs.on_shortcut(s.shortcut.as_str(), |app, _shortcut, event| {
            if event.state() == ShortcutState::Pressed {
                toggle(app);
            }
        })
        .map_err(|e| format!("{} could not be used ({}). Another app may own it: pick another.", s.shortcut, e))
    } else {
        Ok(())
    };
    *LAST_ERROR.lock().unwrap_or_else(|e| e.into_inner()) = result.as_ref().err().cloned();
    result
}

/// The shortcut: show the bar, or hide it when it is already in front.
pub fn toggle(app: &tauri::AppHandle) {
    let app = app.clone();
    let _ = app.clone().run_on_main_thread(move || {
        if let Some(w) = app.get_webview_window(BAR_LABEL) {
            let in_front = w.is_visible().unwrap_or(false) && w.is_focused().unwrap_or(false);
            if in_front {
                let _ = w.hide();
            } else {
                place(&app, &w);
                let _ = w.show();
                let _ = w.set_focus();
            }
            return;
        }
        build(&app);
    });
}

pub fn hide(app: &tauri::AppHandle) {
    if let Some(w) = app.get_webview_window(BAR_LABEL) {
        let _ = w.hide();
    }
}

/// Centered on the monitor the main window is on (else the primary one),
/// a fifth of the way down, like a launcher.
fn place(app: &tauri::AppHandle, w: &tauri::WebviewWindow) {
    let monitor = app
        .get_webview_window("main")
        .and_then(|m| m.current_monitor().ok().flatten())
        .or_else(|| w.primary_monitor().ok().flatten());
    if let Some(m) = monitor {
        let scale = m.scale_factor();
        let size = m.size().to_logical::<f64>(scale);
        let pos = m.position().to_logical::<f64>(scale);
        let x = pos.x + (size.width - BAR_WIDTH) / 2.0;
        let y = pos.y + size.height / 5.0;
        let _ = w.set_position(tauri::LogicalPosition::new(x, y));
    }
}

fn build(app: &tauri::AppHandle) {
    use tauri::{WebviewUrl, WebviewWindowBuilder, WindowEvent};
    let built = WebviewWindowBuilder::new(app, BAR_LABEL, WebviewUrl::App("index.html?window=bar".into()))
        .title("Bob")
        .inner_size(BAR_WIDTH, BAR_HEIGHT)
        .decorations(false)
        .transparent(true)
        .always_on_top(true)
        .skip_taskbar(true)
        .visible_on_all_workspaces(true)
        .resizable(false)
        .shadow(true)
        .focused(true)
        .build();
    match built {
        Ok(w) => {
            place(app, &w);
            let hide_on_blur = w.clone();
            w.on_window_event(move |e| {
                // Clicking away hides it, like any launcher. The turn keeps
                // running; the shortcut brings back the bar and its answer.
                if let WindowEvent::Focused(false) = e {
                    let _ = hide_on_blur.hide();
                }
            });
            let _ = w.set_focus();
        }
        Err(e) => eprintln!("[quick_bar] could not open the bar: {}", e),
    }
}

/// One Bob turn from the bar. Same turn as the chat; the origin makes the
/// window show the question too.
pub async fn ask(app: tauri::AppHandle, content: String) -> Result<crate::Message, String> {
    let scope = crate::turn_scope::TurnScope { origin: Some("Quick bar".into()), ..Default::default() };
    crate::turn_scope::scoped(scope, async move {
        let state = app.state::<crate::AppState>();
        crate::bob_core::bob_send(content, None, state, app.clone()).await
    })
    .await
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn defaults_are_on_with_the_platform_shortcut() {
        let s: QuickBarSettings = serde_json::from_str("{}").unwrap();
        assert!(s.enabled);
        assert_eq!(s.shortcut, DEFAULT_SHORTCUT);
        assert_eq!(CHOICES[0], DEFAULT_SHORTCUT);
    }

    #[test]
    fn copilots_shortcuts_are_never_offered() {
        assert!(!CHOICES.contains(&"Alt+Space"), "Microsoft Copilot owns Option/Alt+Space");
    }

    #[test]
    fn every_choice_parses_as_a_shortcut() {
        for c in CHOICES {
            let parsed: Result<tauri_plugin_global_shortcut::Shortcut, _> = c.parse();
            assert!(parsed.is_ok(), "{c} does not parse");
        }
    }
}
