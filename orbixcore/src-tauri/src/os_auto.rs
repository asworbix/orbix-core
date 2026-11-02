use serde::{Deserialize, Serialize};
use tokio::process::Command;

// ─────────────────────────────────────────────
//  Data Types
// ─────────────────────────────────────────────

#[derive(Clone, Serialize, Deserialize, Debug)]
pub struct SystemInfo {
    pub os_version: String,
    pub hostname: String,
    pub cpu_model: String,
    pub cpu_cores: String,
    pub memory_total: String,
    pub memory_used: String,
    pub disk_total: String,
    pub disk_used: String,
    pub battery_percent: Option<String>,
    pub battery_charging: Option<bool>,
    pub uptime: String,
}

#[derive(Clone, Serialize, Deserialize, Debug)]
pub struct RunningApp {
    pub name: String,
    pub pid: String,
}

#[derive(Clone, Serialize, Deserialize, Debug)]
pub struct WindowInfo {
    pub app_name: String,
    pub title: String,
}

#[derive(Clone, Serialize, Deserialize, Debug)]
pub struct OsActionResult {
    pub success: bool,
    pub output: String,
    pub error: Option<String>,
}

// ─────────────────────────────────────────────
//  Cross-platform helpers
// ─────────────────────────────────────────────

async fn run_cmd(cmd: &str, args: &[&str]) -> Result<String, String> {
    let output = Command::new(cmd)
        .args(args)
        .output()
        .await
        .map_err(|e| format!("Failed to run {}: {}", cmd, e))?;

    if output.status.success() {
        Ok(String::from_utf8_lossy(&output.stdout).trim().to_string())
    } else {
        let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();
        Err(if stderr.is_empty() {
            format!("{} exited with code {}", cmd, output.status.code().unwrap_or(-1))
        } else {
            stderr
        })
    }
}

pub(crate) async fn run_applescript_raw(script: &str) -> Result<String, String> {
    run_cmd("osascript", &["-e", script]).await
}

/// Escape a string for safe embedding inside AppleScript double quotes.
/// Prevents injection by escaping backslashes, double quotes, and control chars.
pub(crate) fn escape_applescript_string(input: &str) -> String {
    input
        .replace('\\', "\\\\")
        .replace('"', "\\\"")
        .replace('\n', "\\n")
        .replace('\r', "\\r")
        .replace('\t', "\\t")
}

fn ok_result(output: String) -> OsActionResult {
    OsActionResult { success: true, output, error: None }
}

fn err_result(error: String) -> OsActionResult {
    OsActionResult { success: false, output: String::new(), error: Some(error) }
}

// ─────────────────────────────────────────────
//  PowerShell helpers (Windows)
// ─────────────────────────────────────────────

#[cfg(target_os = "windows")]
fn encode_powershell(script: &str) -> String {
    use base64::Engine;
    let utf16: Vec<u16> = script.encode_utf16().collect();
    let mut bytes = Vec::with_capacity(utf16.len() * 2);
    for u in utf16 {
        bytes.extend_from_slice(&u.to_le_bytes());
    }
    base64::engine::general_purpose::STANDARD.encode(bytes)
}

#[cfg(target_os = "windows")]
pub(crate) async fn run_powershell_raw(script: &str) -> Result<String, String> {
    let encoded = encode_powershell(script);
    // kill_on_drop: a caller that times out (app bridges do) must not
    // leave a PowerShell blocked on an app dialog behind.
    let output = crate::process_ext::tokio_command("powershell.exe")
        .args(["-NoProfile", "-NonInteractive", "-EncodedCommand", &encoded])
        .kill_on_drop(true)
        .output()
        .await
        .map_err(|e| format!("powershell spawn failed: {}", e))?;
    if output.status.success() {
        Ok(String::from_utf8_lossy(&output.stdout).trim().to_string())
    } else {
        let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();
        Err(if stderr.is_empty() {
            format!("powershell exited {}", output.status.code().unwrap_or(-1))
        } else {
            stderr
        })
    }
}

// ─────────────────────────────────────────────
//  System Info - macOS
// ─────────────────────────────────────────────

#[cfg(target_os = "macos")]
pub async fn get_system_info() -> Result<SystemInfo, String> {
    let os_version = run_cmd("sw_vers", &["-productVersion"]).await.unwrap_or_default();
    let hostname = run_cmd("hostname", &[]).await.unwrap_or_default();
    let cpu_model = run_cmd("sysctl", &["-n", "machdep.cpu.brand_string"]).await.unwrap_or_default();
    let cpu_cores = run_cmd("sysctl", &["-n", "hw.ncpu"]).await.unwrap_or_default();
    let uptime = run_cmd("uptime", &[]).await.unwrap_or_default();

    let mem_total = run_cmd("sysctl", &["-n", "hw.memsize"]).await
        .unwrap_or_default()
        .parse::<u64>()
        .unwrap_or(0);
    let mem_total_gb = format!("{:.1} GB", mem_total as f64 / 1_073_741_824.0);

    let vm_stat = run_cmd("vm_stat", &[]).await.unwrap_or_default();
    let pages_active = parse_vm_stat_value(&vm_stat, "Pages active");
    let pages_wired = parse_vm_stat_value(&vm_stat, "Pages wired down");
    let pages_compressed = parse_vm_stat_value(&vm_stat, "Pages occupied by compressor");
    let used_pages = pages_active + pages_wired + pages_compressed;
    let mem_used_gb = format!("{:.1} GB", (used_pages * 4096) as f64 / 1_073_741_824.0);

    let df_output = run_cmd("df", &["-H", "/"]).await.unwrap_or_default();
    let (disk_total, disk_used) = parse_df_output(&df_output);

    let battery_raw = run_cmd("pmset", &["-g", "batt"]).await.unwrap_or_default();
    let battery_percent = extract_battery_percent(&battery_raw);
    let battery_charging = if battery_raw.contains("AC Power") { Some(true) }
        else if battery_raw.contains("Battery Power") { Some(false) }
        else { None };

    Ok(SystemInfo {
        os_version,
        hostname,
        cpu_model,
        cpu_cores,
        memory_total: mem_total_gb,
        memory_used: mem_used_gb,
        disk_total,
        disk_used,
        battery_percent,
        battery_charging,
        uptime: uptime.split("up").last().unwrap_or("").trim().to_string(),
    })
}

#[cfg(target_os = "macos")]
fn parse_vm_stat_value(output: &str, key: &str) -> u64 {
    output.lines()
        .find(|line| line.contains(key))
        .and_then(|line| line.split(':').last())
        .and_then(|val| val.trim().trim_end_matches('.').parse::<u64>().ok())
        .unwrap_or(0)
}

#[cfg(target_os = "macos")]
fn parse_df_output(output: &str) -> (String, String) {
    let parts: Vec<&str> = output.lines()
        .nth(1)
        .unwrap_or("")
        .split_whitespace()
        .collect();
    let total = parts.get(1).unwrap_or(&"?").to_string();
    let used = parts.get(2).unwrap_or(&"?").to_string();
    (total, used)
}

#[cfg(target_os = "macos")]
fn extract_battery_percent(output: &str) -> Option<String> {
    output.find('%').map(|pos| {
        let start = output[..pos].rfind(|c: char| !c.is_ascii_digit())
            .map(|i| i + 1)
            .unwrap_or(0);
        format!("{}%", &output[start..pos])
    })
}

// ─────────────────────────────────────────────
//  System Info - Windows
// ─────────────────────────────────────────────

#[cfg(target_os = "windows")]
pub async fn get_system_info() -> Result<SystemInfo, String> {
    // Single PowerShell call returning JSON for atomic snapshot.
    let script = r#"
$ErrorActionPreference = 'SilentlyContinue'
$os = Get-CimInstance Win32_OperatingSystem
$cpu = Get-CimInstance Win32_Processor | Select-Object -First 1
$disk = Get-CimInstance Win32_LogicalDisk -Filter "DeviceID='C:'"
$battery = Get-CimInstance Win32_Battery | Select-Object -First 1
$uptimeTs = (Get-Date) - $os.LastBootUpTime
$uptimeStr = "{0}d {1}h {2}m" -f [int]$uptimeTs.TotalDays, $uptimeTs.Hours, $uptimeTs.Minutes
$memTotalGb = [Math]::Round($os.TotalVisibleMemorySize / 1MB, 1)
$memUsedGb = [Math]::Round(($os.TotalVisibleMemorySize - $os.FreePhysicalMemory) / 1MB, 1)
$diskTotalGb = if ($disk) { [Math]::Round($disk.Size / 1GB, 1) } else { 0 }
$diskUsedGb = if ($disk) { [Math]::Round(($disk.Size - $disk.FreeSpace) / 1GB, 1) } else { 0 }
$batteryPct = if ($battery) { "$($battery.EstimatedChargeRemaining)%" } else { $null }
# BatteryStatus: 1=Discharging, 2=AC, 3-5=Charging variants, 6+ = various charging states
$batteryCharging = if ($battery) { ($battery.BatteryStatus -ne 1) } else { $null }
$result = @{
    os_version = $os.Caption + " " + $os.Version
    hostname = [System.Net.Dns]::GetHostName()
    cpu_model = $cpu.Name
    cpu_cores = "$($cpu.NumberOfLogicalProcessors)"
    memory_total = "$memTotalGb GB"
    memory_used = "$memUsedGb GB"
    disk_total = "$diskTotalGb GB"
    disk_used = "$diskUsedGb GB"
    battery_percent = $batteryPct
    battery_charging = $batteryCharging
    uptime = $uptimeStr
}
$result | ConvertTo-Json -Compress
"#;
    let json = run_powershell_raw(script).await?;
    serde_json::from_str(&json).map_err(|e| format!("Failed to parse system info JSON: {} (raw: {})", e, json))
}

// ─────────────────────────────────────────────
//  Application Control
// ─────────────────────────────────────────────

#[cfg(target_os = "macos")]
pub async fn open_application(name: &str) -> OsActionResult {
    match run_cmd("open", &["-a", name]).await {
        Ok(_) => ok_result(format!("Opened {}", name)),
        Err(e) => err_result(e),
    }
}

#[cfg(target_os = "windows")]
pub async fn open_application(name: &str) -> OsActionResult {
    // Try Start-Process with the name (works for apps registered with App Paths
    // in the registry - e.g. "notepad", "chrome", "code"). If that fails, fall
    // back to launching the literal path.
    let escaped = name.replace('\'', "''");
    let script = format!("Start-Process -FilePath '{}'", escaped);
    match run_powershell_raw(&script).await {
        Ok(_) => ok_result(format!("Opened {}", name)),
        Err(e) => err_result(e),
    }
}

#[cfg(target_os = "macos")]
pub async fn quit_application(name: &str) -> OsActionResult {
    let safe_name = escape_applescript_string(name);
    let script = format!(r#"tell application "{}" to quit"#, safe_name);
    match run_applescript_raw(&script).await {
        Ok(_) => ok_result(format!("Quit {}", name)),
        Err(e) => err_result(e),
    }
}

#[cfg(target_os = "windows")]
pub async fn quit_application(name: &str) -> OsActionResult {
    // Strip a trailing .exe if the caller passed one - Stop-Process uses the
    // image name without the extension.
    let process_name = name.trim_end_matches(".exe").trim_end_matches(".EXE");
    let escaped = process_name.replace('\'', "''");
    let script = format!(
        "Stop-Process -Name '{}' -Force -ErrorAction Stop",
        escaped
    );
    match run_powershell_raw(&script).await {
        Ok(_) => ok_result(format!("Quit {}", name)),
        Err(e) => err_result(e),
    }
}

#[cfg(target_os = "macos")]
pub async fn list_running_apps() -> Result<Vec<RunningApp>, String> {
    let script = r#"tell application "System Events" to get the name of every process whose background only is false"#;
    let output = run_applescript_raw(script).await?;
    let apps: Vec<RunningApp> = output.split(", ")
        .map(|name| RunningApp {
            name: name.trim().to_string(),
            pid: String::new(),
        })
        .collect();
    Ok(apps)
}

#[cfg(target_os = "windows")]
pub async fn list_running_apps() -> Result<Vec<RunningApp>, String> {
    // Foreground-style apps = processes that own a top-level window with a title.
    let script = r#"
Get-Process | Where-Object { $_.MainWindowTitle -ne '' -and $_.MainWindowHandle -ne 0 } |
    Select-Object @{N='name';E={$_.ProcessName}}, @{N='pid';E={"$($_.Id)"}} |
    ConvertTo-Json -Compress -AsArray
"#;
    let json = run_powershell_raw(script).await?;
    if json.is_empty() {
        return Ok(Vec::new());
    }
    serde_json::from_str(&json).map_err(|e| format!("Failed to parse process list: {}", e))
}

// ─────────────────────────────────────────────
//  Clipboard
// ─────────────────────────────────────────────

#[cfg(target_os = "macos")]
pub async fn get_clipboard() -> Result<String, String> {
    run_cmd("pbpaste", &[]).await
}

#[cfg(target_os = "windows")]
pub async fn get_clipboard() -> Result<String, String> {
    run_powershell_raw("Get-Clipboard -Raw").await
}

#[cfg(target_os = "macos")]
pub async fn set_clipboard(text: &str) -> OsActionResult {
    let child = Command::new("pbcopy")
        .stdin(std::process::Stdio::piped())
        .spawn();

    match child {
        Ok(mut child) => {
            use tokio::io::AsyncWriteExt;
            if let Some(ref mut stdin) = child.stdin {
                let _ = stdin.write_all(text.as_bytes()).await;
                let _ = stdin.shutdown().await;
            }
            let _ = child.wait().await;
            ok_result("Copied to clipboard".into())
        }
        Err(e) => err_result(format!("Failed to set clipboard: {}", e)),
    }
}

#[cfg(target_os = "windows")]
pub async fn set_clipboard(text: &str) -> OsActionResult {
    // Embed text as a single-quoted PS literal with '' escaping for '.
    let escaped = text.replace('\'', "''");
    let script = format!("Set-Clipboard -Value '{}'", escaped);
    match run_powershell_raw(&script).await {
        Ok(_) => ok_result("Copied to clipboard".into()),
        Err(e) => err_result(format!("Failed to set clipboard: {}", e)),
    }
}

// ─────────────────────────────────────────────
//  Notifications
// ─────────────────────────────────────────────

pub async fn send_notification(app: &tauri::AppHandle, title: &str, body: &str) -> OsActionResult {
    // Via the notification plugin so the alert shows the Orbix Core app icon;
    // `osascript display notification` is always attributed to Script Editor,
    // and on Windows the plugin's WinRT toast picks up our registered
    // AppUserModelID (see register_toast_identity).
    crate::notifications::os_notify(app, title, body);
    ok_result("Notification sent".into())
}

/// AppUserModelID for toasts + taskbar identity. Matches the Tauri bundle
/// identifier so the installed Start-menu shortcut and our registry
/// registration agree on who "Orbix Core" is.
#[cfg(target_os = "windows")]
pub const APP_USER_MODEL_ID: &str = "ai.orbixcore.app";

/// Register the AUMID under HKCU so toasts show the Orbix Core name and
/// icon instead of a bare string label. Idempotent; runs once at startup.
/// The icon is embedded at compile time and written to the data dir
/// because IconUri needs an on-disk image the shell can read any time.
#[cfg(target_os = "windows")]
pub async fn register_toast_identity() -> Result<(), String> {
    let icon_path = crate::storage::data_dir().join("toast-icon.png");
    if !icon_path.exists() {
        let _ = std::fs::create_dir_all(crate::storage::data_dir());
        std::fs::write(&icon_path, include_bytes!("../icons/128x128.png"))
            .map_err(|e| format!("write toast icon: {}", e))?;
    }
    let icon_esc = icon_path.to_string_lossy().replace('\'', "''");
    let script = format!(
        r#"
$ErrorActionPreference = 'Stop'
$key = 'HKCU:\Software\Classes\AppUserModelId\{aumid}'
if (-not (Test-Path $key)) {{ New-Item -Path $key -Force | Out-Null }}
Set-ItemProperty -Path $key -Name DisplayName -Value 'Orbix Core'
Set-ItemProperty -Path $key -Name IconUri -Value '{icon}'
"#,
        aumid = APP_USER_MODEL_ID,
        icon = icon_esc,
    );
    run_powershell_raw(&script).await.map(|_| ())
}

// ─────────────────────────────────────────────
//  Files & URLs
// ─────────────────────────────────────────────

#[cfg(target_os = "macos")]
pub async fn open_url(url: &str) -> OsActionResult {
    match run_cmd("open", &[url]).await {
        Ok(_) => ok_result(format!("Opened {}", url)),
        Err(e) => err_result(e),
    }
}

#[cfg(target_os = "windows")]
pub async fn open_url(url: &str) -> OsActionResult {
    let escaped = url.replace('\'', "''");
    let script = format!("Start-Process '{}'", escaped);
    match run_powershell_raw(&script).await {
        Ok(_) => ok_result(format!("Opened {}", url)),
        Err(e) => err_result(e),
    }
}

#[cfg(target_os = "macos")]
pub async fn open_file(path: &str) -> OsActionResult {
    match run_cmd("open", &[path]).await {
        Ok(_) => ok_result(format!("Opened {}", path)),
        Err(e) => err_result(e),
    }
}

#[cfg(target_os = "windows")]
pub async fn open_file(path: &str) -> OsActionResult {
    let escaped = path.replace('\'', "''");
    let script = format!("Start-Process '{}'", escaped);
    match run_powershell_raw(&script).await {
        Ok(_) => ok_result(format!("Opened {}", path)),
        Err(e) => err_result(e),
    }
}

#[cfg(target_os = "macos")]
pub async fn reveal_in_finder(path: &str) -> OsActionResult {
    match run_cmd("open", &["-R", path]).await {
        Ok(_) => ok_result(format!("Revealed {} in Finder", path)),
        Err(e) => err_result(e),
    }
}

#[cfg(target_os = "windows")]
pub async fn reveal_in_finder(path: &str) -> OsActionResult {
    // explorer.exe /select,"<path>" highlights the item in its parent folder.
    // explorer.exe is unusual - it returns 1 even on success, so we trust the spawn.
    let result = crate::process_ext::tokio_command("explorer.exe")
        .args([format!("/select,{}", path).as_str()])
        .status()
        .await;
    match result {
        Ok(_) => ok_result(format!("Revealed {} in Explorer", path)),
        Err(e) => err_result(format!("Failed to reveal: {}", e)),
    }
}

// ─────────────────────────────────────────────
//  Window Management
// ─────────────────────────────────────────────

#[cfg(target_os = "macos")]
pub async fn get_active_window() -> Result<WindowInfo, String> {
    let script = r#"
        tell application "System Events"
            set frontApp to name of first application process whose frontmost is true
        end tell
        tell application frontApp
            if (count of windows) > 0 then
                set winTitle to name of front window
            else
                set winTitle to ""
            end if
        end tell
        return frontApp & "|" & winTitle
    "#;
    let output = run_applescript_raw(script).await?;
    let parts: Vec<&str> = output.splitn(2, '|').collect();
    Ok(WindowInfo {
        app_name: parts.first().unwrap_or(&"").to_string(),
        title: parts.get(1).unwrap_or(&"").to_string(),
    })
}

#[cfg(target_os = "windows")]
pub async fn get_active_window() -> Result<WindowInfo, String> {
    // Win32 GetForegroundWindow + process lookup. Done via PowerShell P/Invoke
    // to avoid pulling in extra Rust-side state for a single call.
    let script = r#"
$ErrorActionPreference = 'Stop'
Add-Type @"
using System;
using System.Runtime.InteropServices;
using System.Text;
public static class Win32 {
    [DllImport("user32.dll")] public static extern IntPtr GetForegroundWindow();
    [DllImport("user32.dll")] public static extern int GetWindowText(IntPtr hWnd, StringBuilder s, int n);
    [DllImport("user32.dll")] public static extern int GetWindowThreadProcessId(IntPtr hWnd, out uint procId);
}
"@
$hwnd = [Win32]::GetForegroundWindow()
$sb = New-Object System.Text.StringBuilder 1024
[void][Win32]::GetWindowText($hwnd, $sb, $sb.Capacity)
$pid = 0
[void][Win32]::GetWindowThreadProcessId($hwnd, [ref]$pid)
$proc = Get-Process -Id $pid -ErrorAction SilentlyContinue
$appName = if ($proc) { $proc.ProcessName } else { '' }
@{ app_name = $appName; title = $sb.ToString() } | ConvertTo-Json -Compress
"#;
    let json = run_powershell_raw(script).await?;
    serde_json::from_str(&json).map_err(|e| format!("Failed to parse active window: {}", e))
}

#[cfg(target_os = "macos")]
pub async fn list_windows() -> Result<Vec<WindowInfo>, String> {
    let script = r#"
        set windowList to ""
        tell application "System Events"
            repeat with proc in (every process whose background only is false)
                set appName to name of proc
                try
                    repeat with w in (every window of proc)
                        set windowList to windowList & appName & "|" & name of w & linefeed
                    end repeat
                end try
            end repeat
        end tell
        return windowList
    "#;
    let output = run_applescript_raw(script).await?;
    let windows: Vec<WindowInfo> = output.lines()
        .filter(|l| !l.is_empty())
        .map(|line| {
            let parts: Vec<&str> = line.splitn(2, '|').collect();
            WindowInfo {
                app_name: parts.first().unwrap_or(&"").to_string(),
                title: parts.get(1).unwrap_or(&"").to_string(),
            }
        })
        .collect();
    Ok(windows)
}

#[cfg(target_os = "windows")]
pub async fn list_windows() -> Result<Vec<WindowInfo>, String> {
    let script = r#"
Get-Process | Where-Object { $_.MainWindowTitle -ne '' -and $_.MainWindowHandle -ne 0 } |
    Select-Object @{N='app_name';E={$_.ProcessName}}, @{N='title';E={$_.MainWindowTitle}} |
    ConvertTo-Json -Compress -AsArray
"#;
    let json = run_powershell_raw(script).await?;
    if json.is_empty() {
        return Ok(Vec::new());
    }
    serde_json::from_str(&json).map_err(|e| format!("Failed to parse window list: {}", e))
}

// ─────────────────────────────────────────────
//  Script Runners
// ─────────────────────────────────────────────

#[cfg(target_os = "macos")]
pub async fn run_applescript(script: &str) -> OsActionResult {
    match run_applescript_raw(script).await {
        Ok(output) => ok_result(output),
        Err(e) => err_result(e),
    }
}

#[cfg(target_os = "windows")]
pub async fn run_applescript(_script: &str) -> OsActionResult {
    // AppleScript is macOS-only. Frontends that target both platforms should
    // detect the OS and call run_powershell instead.
    err_result("AppleScript is not supported on Windows. Use run_powershell.".into())
}

/// Cross-platform script runner. On Windows this runs the script via
/// powershell.exe -EncodedCommand. On macOS it runs via osascript (treated as
/// AppleScript). Exposed so the frontend can dispatch to whichever runtime is
/// available without branching at the Tauri-command boundary.
#[cfg(target_os = "windows")]
pub async fn run_powershell(script: &str) -> OsActionResult {
    match run_powershell_raw(script).await {
        Ok(output) => ok_result(output),
        Err(e) => err_result(e),
    }
}

#[cfg(target_os = "macos")]
pub async fn run_powershell(_script: &str) -> OsActionResult {
    err_result("PowerShell is not supported on macOS. Use run_applescript.".into())
}
