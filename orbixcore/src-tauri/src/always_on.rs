// ─────────────────────────────────────────────
//  Always on - Bob keeps running when the window goes away
//
//  Bob is the product, the window is one way to reach him. Closing the
//  window hides it and Bob stays in the menu bar (tray on Windows):
//  workflows, triggers, the terminal and the phone keep working. Quit in
//  the tray menu (or Cmd-Q) really quits. Opening the app again shows the
//  window. Optionally Bob starts at login, headless.
//
//  Settings live in base_dir()/always_on.json:
//  - keep_running (default on): close hides instead of quitting.
//  - launch_at_login (default off: a login item needs the user's yes).
//    macOS: a LaunchAgent in ~/Library/LaunchAgents that starts the app
//    with --headless and restarts it after a crash (not after a quit).
//    Windows: the per-user Run key.
//
//  One instance: a second launch finds the first on its loopback server,
//  asks it to show its window (/cli/show) and exits, so two copies never
//  fight over port 51231.
// ─────────────────────────────────────────────

use serde::{Deserialize, Serialize};
use tauri::Manager;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AlwaysOnSettings {
    #[serde(default = "yes")]
    pub keep_running: bool,
    #[serde(default)]
    pub launch_at_login: bool,
}

fn yes() -> bool {
    true
}

impl Default for AlwaysOnSettings {
    fn default() -> Self {
        Self { keep_running: true, launch_at_login: false }
    }
}

fn settings_path() -> std::path::PathBuf {
    crate::storage::base_dir().join("always_on.json")
}

pub fn settings() -> AlwaysOnSettings {
    std::fs::read_to_string(settings_path())
        .ok()
        .and_then(|s| serde_json::from_str(&s).ok())
        .unwrap_or_default()
}

/// Save the settings and make the login item match them. The login item
/// is written first so a failure leaves the saved switch truthful.
pub fn save_settings(next: AlwaysOnSettings) -> Result<AlwaysOnSettings, String> {
    set_launch_at_login(next.launch_at_login)?;
    let json = serde_json::to_string_pretty(&next).map_err(|e| e.to_string())?;
    std::fs::write(settings_path(), json).map_err(|e| e.to_string())?;
    Ok(next)
}

// ── Window ──

/// Show the main window and bring it forward (and back into the Dock on
/// macOS, where a hidden Bob runs as a menu-bar accessory).
pub fn show_main_window(app: &tauri::AppHandle) {
    #[cfg(target_os = "macos")]
    let _ = app.set_activation_policy(tauri::ActivationPolicy::Regular);
    if let Some(w) = app.get_webview_window("main") {
        let _ = w.unminimize();
        let _ = w.show();
        let _ = w.set_focus();
    }
}

/// Hide the main window and keep running: menu bar only on macOS.
pub fn hide_main_window(app: &tauri::AppHandle) {
    if let Some(w) = app.get_webview_window("main") {
        let _ = w.hide();
    }
    #[cfg(target_os = "macos")]
    let _ = app.set_activation_policy(tauri::ActivationPolicy::Accessory);
}

// ── One instance ──

/// When another Orbix Core already answers on the loopback server, return
/// true: this launch should exit. A windowed launch first asks it to show
/// its window; a headless one (login item, `bob` cold start) leaves it be.
/// Anything else on the port (not ours, or no answer) returns false and
/// the launch goes ahead as before.
pub fn hand_over_to_running_instance(show: bool) -> bool {
    use std::io::{Read, Write};
    let addr = std::net::SocketAddr::from(([127, 0, 0, 1], 51231));
    let Ok(mut s) = std::net::TcpStream::connect_timeout(&addr, std::time::Duration::from_millis(300)) else {
        return false;
    };
    let _ = s.set_read_timeout(Some(std::time::Duration::from_secs(2)));
    let req = if show {
        "POST /cli/show HTTP/1.0\r\nHost: 127.0.0.1\r\nContent-Length: 0\r\n\r\n"
    } else {
        "GET /cli/peer/status HTTP/1.0\r\nHost: 127.0.0.1\r\n\r\n"
    };
    if s.write_all(req.as_bytes()).is_err() {
        return false;
    }
    let mut resp = String::new();
    let _ = s.read_to_string(&mut resp);
    let ours = if show { "\"orbix\":true" } else { "\"app\":\"Orbix Core\"" };
    resp.starts_with("HTTP/1.") && resp.contains(" 200 ") && resp.contains(ours)
}

// ── Launch at login ──

#[cfg(target_os = "macos")]
const AGENT_LABEL: &str = "ai.orbixcore.bob";

#[cfg(target_os = "macos")]
fn agent_path() -> Result<std::path::PathBuf, String> {
    dirs::home_dir()
        .map(|h| h.join("Library/LaunchAgents").join(format!("{AGENT_LABEL}.plist")))
        .ok_or_else(|| "no home folder".to_string())
}

#[cfg(target_os = "macos")]
fn agent_plist(exe: &std::path::Path) -> String {
    let exe = exe.to_string_lossy().replace('&', "&amp;").replace('<', "&lt;").replace('>', "&gt;");
    format!(
        r#"<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
    <key>Label</key>
    <string>{AGENT_LABEL}</string>
    <key>ProgramArguments</key>
    <array>
        <string>{exe}</string>
        <string>--headless</string>
    </array>
    <key>RunAtLoad</key>
    <true/>
    <key>KeepAlive</key>
    <dict>
        <key>SuccessfulExit</key>
        <false/>
    </dict>
    <key>ProcessType</key>
    <string>Interactive</string>
</dict>
</plist>
"#
    )
}

/// Turning it off removes the item for the next login only: the running
/// Bob (possibly started by it) keeps running.
#[cfg(target_os = "macos")]
fn set_launch_at_login(enabled: bool) -> Result<(), String> {
    let path = agent_path()?;
    if !enabled {
        return match std::fs::remove_file(&path) {
            Err(e) if e.kind() != std::io::ErrorKind::NotFound => Err(format!("could not remove {}: {}", path.display(), e)),
            _ => Ok(()),
        };
    }
    let exe = std::env::current_exe().map_err(|e| e.to_string())?;
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir).map_err(|e| e.to_string())?;
    }
    std::fs::write(&path, agent_plist(&exe)).map_err(|e| format!("could not write {}: {}", path.display(), e))
}

#[cfg(target_os = "windows")]
fn set_launch_at_login(enabled: bool) -> Result<(), String> {
    const KEY: &str = r"HKCU\Software\Microsoft\Windows\CurrentVersion\Run";
    let mut cmd = crate::process_ext::std_command("reg.exe");
    if enabled {
        let exe = std::env::current_exe().map_err(|e| e.to_string())?;
        cmd.args(["add", KEY, "/v", "Orbix Core", "/t", "REG_SZ", "/d"])
            .arg(format!("\"{}\" --headless", exe.display()))
            .arg("/f");
    } else {
        cmd.args(["delete", KEY, "/v", "Orbix Core", "/f"]);
    }
    let out = cmd.output().map_err(|e| e.to_string())?;
    // Deleting a value that is not there is fine.
    if out.status.success() || !enabled {
        Ok(())
    } else {
        Err(format!("could not update the login item: {}", String::from_utf8_lossy(&out.stderr).trim()))
    }
}

#[cfg(not(any(target_os = "macos", target_os = "windows")))]
fn set_launch_at_login(enabled: bool) -> Result<(), String> {
    if enabled { Err("start at login is not supported on this platform yet".into()) } else { Ok(()) }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn defaults_keep_bob_running_but_add_no_login_item() {
        let s: AlwaysOnSettings = serde_json::from_str("{}").unwrap();
        assert!(s.keep_running);
        assert!(!s.launch_at_login);
    }

    #[cfg(target_os = "macos")]
    #[test]
    fn launch_agent_starts_headless_and_survives_crashes_not_quits() {
        let p = agent_plist(std::path::Path::new("/Applications/Orbix Core.app/Contents/MacOS/orbixcore"));
        assert!(p.contains("<string>/Applications/Orbix Core.app/Contents/MacOS/orbixcore</string>"));
        assert!(p.contains("<string>--headless</string>"));
        assert!(p.contains("<key>SuccessfulExit</key>\n        <false/>"));
    }
}
