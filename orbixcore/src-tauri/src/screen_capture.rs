use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use chrono::Utc;
use uuid::Uuid;

// ─────────────────────────────────────────────
//  Permission check
// ─────────────────────────────────────────────

/// Open System Settings to the Screen & System Audio Recording privacy pane
pub async fn open_screen_recording_settings() {
    // macOS 15+ uses this path
    let _ = tokio::process::Command::new("open")
        .arg("x-apple.systempreferences:com.apple.preference.security?Privacy_ScreenCapture")
        .output()
        .await;
}

// ─────────────────────────────────────────────
//  Types
// ─────────────────────────────────────────────

#[derive(Clone, Serialize, Deserialize, Debug)]
pub struct Recording {
    pub id: String,
    pub name: String,
    pub frames: Vec<CapturedFrame>,
    pub audio_path: Option<String>,
    pub duration_secs: f64,
    pub created_at: String,
}

#[derive(Clone, Serialize, Deserialize, Debug)]
pub struct CapturedFrame {
    pub index: usize,
    pub path: String,
    pub timestamp_ms: u64,
    pub active_app: Option<String>,
    pub window_title: Option<String>,
    pub browser_url: Option<String>,
    #[serde(default)]
    pub is_browser: bool,
}

#[derive(Clone, Serialize, Deserialize, Debug)]
pub struct RecordingState {
    pub is_recording: bool,
    pub recording_id: Option<String>,
    pub frame_count: usize,
    pub elapsed_secs: f64,
    pub audio_levels: Vec<f32>, // recent audio amplitude samples for waveform
}

// ─────────────────────────────────────────────
//  Shared state for active recording
// ─────────────────────────────────────────────

pub struct ScreenRecorder {
    pub active: AtomicBool,
    pub recording_id: Mutex<Option<String>>,
    pub frames: Mutex<Vec<CapturedFrame>>,
    pub start_time: Mutex<Option<std::time::Instant>>,
    pub audio_levels: Mutex<Vec<f32>>,
    pub recordings: Mutex<Vec<Recording>>,
}

impl Default for ScreenRecorder {
    fn default() -> Self {
        Self {
            active: AtomicBool::new(false),
            recording_id: Mutex::new(None),
            frames: Mutex::new(Vec::new()),
            start_time: Mutex::new(None),
            audio_levels: Mutex::new(Vec::new()),
            recordings: Mutex::new(Vec::new()),
        }
    }
}

// ─────────────────────────────────────────────
//  Directory helpers
// ─────────────────────────────────────────────

pub fn recordings_dir() -> PathBuf {
    let base = crate::storage::base_dir().join("recordings");
    std::fs::create_dir_all(&base).ok();
    base
}

fn recording_dir(id: &str) -> PathBuf {
    let dir = recordings_dir().join(id);
    std::fs::create_dir_all(&dir).ok();
    dir
}

// ─────────────────────────────────────────────
//  Process context capture (active app, window)
// ─────────────────────────────────────────────

#[derive(Debug, Default)]
pub struct FrameContext {
    pub active_app: Option<String>,
    pub window_title: Option<String>,
    pub browser_url: Option<String>,
    pub is_browser: bool,
}

// Returns (active_app, window_title).
#[cfg(target_os = "macos")]
async fn get_front_app_and_title() -> (Option<String>, Option<String>) {
    let script = r#"
        tell application "System Events"
            set frontApp to name of first application process whose frontmost is true
            set winTitle to ""
            try
                set winTitle to name of front window of (first application process whose frontmost is true)
            end try
        end tell
        return frontApp & "|||" & winTitle
    "#;

    let app_info = tokio::process::Command::new("osascript")
        .arg("-e").arg(script)
        .output().await.ok()
        .and_then(|o| if o.status.success() {
            Some(String::from_utf8_lossy(&o.stdout).trim().to_string())
        } else {
            None
        });

    if let Some(info) = app_info {
        let parts: Vec<&str> = info.splitn(2, "|||").collect();
        (
            parts.first().map(|s| s.to_string()),
            parts.get(1).filter(|s| !s.is_empty()).map(|s| s.to_string()),
        )
    } else {
        (None, None)
    }
}

#[cfg(target_os = "windows")]
async fn get_front_app_and_title() -> (Option<String>, Option<String>) {
    // Win32 GetForegroundWindow + window text + process name. Driven through
    // PowerShell so we don't need extra Rust-side state for this single call.
    let script = r#"
$ErrorActionPreference = 'SilentlyContinue'
Add-Type @"
using System;
using System.Runtime.InteropServices;
using System.Text;
public static class Sc {
    [DllImport("user32.dll")] public static extern IntPtr GetForegroundWindow();
    [DllImport("user32.dll")] public static extern int GetWindowText(IntPtr hWnd, StringBuilder s, int n);
    [DllImport("user32.dll")] public static extern int GetWindowThreadProcessId(IntPtr hWnd, out uint procId);
}
"@
$hwnd = [Sc]::GetForegroundWindow()
$sb = New-Object System.Text.StringBuilder 1024
[void][Sc]::GetWindowText($hwnd, $sb, $sb.Capacity)
$pid = 0
[void][Sc]::GetWindowThreadProcessId($hwnd, [ref]$pid)
$proc = Get-Process -Id $pid -ErrorAction SilentlyContinue
$appName = if ($proc) { $proc.ProcessName } else { '' }
"$appName|||$($sb.ToString())"
"#;
    let encoded = encode_powershell(script);
    let out = crate::process_ext::tokio_command("powershell.exe")
        .args(["-NoProfile", "-NonInteractive", "-EncodedCommand", &encoded])
        .output().await.ok();
    if let Some(o) = out {
        if o.status.success() {
            let info = String::from_utf8_lossy(&o.stdout).trim().to_string();
            let parts: Vec<&str> = info.splitn(2, "|||").collect();
            return (
                parts.first().filter(|s| !s.is_empty()).map(|s| s.to_string()),
                parts.get(1).filter(|s| !s.is_empty()).map(|s| s.to_string()),
            );
        }
    }
    (None, None)
}

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

#[cfg(target_os = "macos")]
async fn get_browser_url(active_app: &str) -> Option<String> {
    let app_lower = active_app.to_lowercase();
    let url_script = if app_lower.contains("safari") {
        r#"tell application "Safari" to return URL of front document"#.to_string()
    } else if app_lower.contains("chrome") {
        r#"tell application "Google Chrome" to return URL of active tab of front window"#.to_string()
    } else if app_lower.contains("arc") {
        r#"tell application "Arc" to return URL of active tab of front window"#.to_string()
    } else if app_lower.contains("brave") {
        r#"tell application "Brave Browser" to return URL of active tab of front window"#.to_string()
    } else if app_lower.contains("edge") {
        r#"tell application "Microsoft Edge" to return URL of active tab of front window"#.to_string()
    } else if app_lower.contains("vivaldi") {
        r#"tell application "Vivaldi" to return URL of active tab of front window"#.to_string()
    } else if app_lower.contains("opera") {
        r#"tell application "Opera" to return URL of active tab of front window"#.to_string()
    } else {
        return None;
    };

    tokio::process::Command::new("osascript")
        .arg("-e").arg(&url_script)
        .output().await.ok()
        .and_then(|o| if o.status.success() {
            let url = String::from_utf8_lossy(&o.stdout).trim().to_string();
            if url.starts_with("http") { Some(url) } else { None }
        } else {
            None
        })
}

#[cfg(target_os = "windows")]
async fn get_browser_url(_active_app: &str) -> Option<String> {
    // Read the URL via UI Automation: every major Chromium/Gecko browser
    // exposes the address bar as a focusable Edit-control descendant of the
    // foreground window with a recognisable Name. We grab the ValuePattern's
    // current value, which is the live URL string the user sees.
    let script = r#"
$ErrorActionPreference = 'SilentlyContinue'
Add-Type -AssemblyName UIAutomationClient
Add-Type -AssemblyName UIAutomationTypes
Add-Type @"
using System;
using System.Runtime.InteropServices;
public static class Sc {
    [DllImport("user32.dll")] public static extern IntPtr GetForegroundWindow();
}
"@
$hwnd = [Sc]::GetForegroundWindow()
if ($hwnd -eq [IntPtr]::Zero) { return }
$window = [System.Windows.Automation.AutomationElement]::FromHandle($hwnd)
if (-not $window) { return }
$editType = [System.Windows.Automation.PropertyCondition]::new(
    [System.Windows.Automation.AutomationElement]::ControlTypeProperty,
    [System.Windows.Automation.ControlType]::Edit
)
$matches = $window.FindAll([System.Windows.Automation.TreeScope]::Descendants, $editType)
foreach ($el in $matches) {
    $name = $el.Current.Name
    if ($name -match 'address|url|location|search') {
        try {
            $vp = $el.GetCurrentPattern([System.Windows.Automation.ValuePattern]::Pattern)
            $val = $vp.Current.Value
            if ($val) {
                Write-Output $val
                return
            }
        } catch {}
    }
}
# Fallback: first Edit with a value at all
foreach ($el in $matches) {
    try {
        $vp = $el.GetCurrentPattern([System.Windows.Automation.ValuePattern]::Pattern)
        $val = $vp.Current.Value
        if ($val) {
            Write-Output $val
            return
        }
    } catch {}
}
"#;
    let encoded = encode_powershell(script);
    let out = tokio::process::Command::new("powershell.exe")
        .args(["-NoProfile", "-NonInteractive", "-EncodedCommand", &encoded])
        .output().await.ok()?;
    if !out.status.success() {
        return None;
    }
    let raw = String::from_utf8_lossy(&out.stdout).trim().to_string();
    if raw.is_empty() {
        return None;
    }
    // The address-bar value omits the scheme on Chrome/Edge ("github.com/foo")
    // - reattach https:// so callers see a navigable URL.
    if raw.starts_with("http://") || raw.starts_with("https://") {
        Some(raw)
    } else if raw.contains('.') && !raw.contains(' ') {
        Some(format!("https://{}", raw))
    } else {
        None
    }
}

pub async fn capture_context() -> FrameContext {
    let (active_app, window_title) = get_front_app_and_title().await;

    let is_browser = if let Some(ref app) = active_app {
        let app_lower = app.to_lowercase();
        app_lower.contains("safari") || app_lower.contains("chrome")
            || app_lower.contains("firefox") || app_lower.contains("arc")
            || app_lower.contains("brave") || app_lower.contains("edge")
            || app_lower.contains("zen") || app_lower.contains("opera")
            || app_lower.contains("vivaldi") || app_lower.contains("waterfox")
            || app_lower.contains("orion") || app_lower.contains("sigmaos")
            || app_lower.contains("msedge")
    } else {
        false
    };

    let browser_url = if is_browser {
        let app = active_app.as_ref().unwrap();
        get_browser_url(app).await
    } else {
        None
    };

    // For browsers, mark the window title so the timeline knows it's browser activity
    // even when we can't get the URL (e.g. Zen/Firefox)
    let is_browser_without_url = is_browser && browser_url.is_none();

    FrameContext {
        active_app,
        window_title,
        browser_url,
        is_browser: is_browser_without_url,
    }
}

// ─────────────────────────────────────────────
//  Frame capture (context + optional screenshot)
// ─────────────────────────────────────────────

/// Try to take a screenshot. Returns the path if successful, None if permission denied.
#[cfg(target_os = "macos")]
async fn try_screenshot(path: &std::path::Path) -> Option<String> {
    let result = tokio::process::Command::new("screencapture")
        .args(["-x", "-C", "-t", "png"])
        .arg(path)
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .output()
        .await;

    match result {
        Ok(output) if output.status.success() && path.exists() => {
            if let Ok(meta) = std::fs::metadata(path) {
                if meta.len() > 1000 {
                    return Some(path.to_string_lossy().to_string());
                }
            }
            None
        }
        _ => None,
    }
}

/// Windows screenshot via System.Drawing.Graphics.CopyFromScreen, driven from
/// PowerShell. Captures the entire primary screen at native resolution.
#[cfg(target_os = "windows")]
async fn try_screenshot(path: &std::path::Path) -> Option<String> {
    let path_str = path.to_string_lossy().replace('\'', "''");
    let script = format!(r#"
$ErrorActionPreference = 'Stop'
Add-Type -AssemblyName System.Windows.Forms
Add-Type -AssemblyName System.Drawing
$screen = [System.Windows.Forms.Screen]::PrimaryScreen.Bounds
$bmp = New-Object System.Drawing.Bitmap $screen.Width, $screen.Height
$g = [System.Drawing.Graphics]::FromImage($bmp)
$g.CopyFromScreen($screen.Location, [System.Drawing.Point]::Empty, $screen.Size)
$bmp.Save('{path}', [System.Drawing.Imaging.ImageFormat]::Png)
$g.Dispose()
$bmp.Dispose()
"#, path = path_str);
    let encoded = encode_powershell(&script);
    let result = crate::process_ext::tokio_command("powershell.exe")
        .args(["-NoProfile", "-NonInteractive", "-EncodedCommand", &encoded])
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .output()
        .await;

    match result {
        Ok(output) if output.status.success() && path.exists() => {
            if let Ok(meta) = std::fs::metadata(path) {
                if meta.len() > 1000 {
                    return Some(path.to_string_lossy().to_string());
                }
            }
            None
        }
        _ => None,
    }
}

pub async fn capture_frame(recording_id: &str, index: usize, try_screen: bool) -> Result<CapturedFrame, String> {
    let dir = recording_dir(recording_id);

    // Always capture process context (no permission needed)
    let context = capture_context().await;

    // Skip frames where OrbixCore itself is the active app - it pollutes the recording
    if let Some(ref app) = context.active_app {
        let app_lower = app.to_lowercase();
        if app_lower.contains("orbixcore") || app_lower.contains("orbix") {
            return Err("Skipping OrbixCore frame".into());
        }
    }

    // Optionally try screenshot
    let screenshot_path = if try_screen {
        let filename = format!("frame_{:05}.png", index);
        let path = dir.join(&filename);
        try_screenshot(&path).await
    } else {
        None
    };

    Ok(CapturedFrame {
        index,
        path: screenshot_path.unwrap_or_default(),
        timestamp_ms: std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_millis() as u64,
        active_app: context.active_app,
        window_title: context.window_title,
        browser_url: context.browser_url,
        is_browser: context.is_browser,
    })
}

/// Take a single quick screenshot and return its base64-encoded PNG data.
/// Returns empty string if screenshot permission not available.
pub async fn capture_preview() -> Result<String, String> {
    let tmp = std::env::temp_dir().join(format!("orbix_preview_{}.png", std::process::id()));

    if let Some(_path) = try_screenshot(&tmp).await {
        let data = tokio::fs::read(&tmp).await
            .map_err(|e| format!("Failed to read screenshot: {}", e))?;
        tokio::fs::remove_file(&tmp).await.ok();
        use base64::Engine;
        Ok(base64::engine::general_purpose::STANDARD.encode(&data))
    } else {
        Ok(String::new()) // No screenshot available
    }
}

// ─────────────────────────────────────────────
//  Recording lifecycle
// ─────────────────────────────────────────────

impl ScreenRecorder {
    pub fn start(&self) -> String {
        let id = Uuid::new_v4().to_string();
        *self.recording_id.lock().unwrap() = Some(id.clone());
        *self.frames.lock().unwrap() = Vec::new();
        *self.start_time.lock().unwrap() = Some(std::time::Instant::now());
        *self.audio_levels.lock().unwrap() = Vec::new();
        self.active.store(true, Ordering::SeqCst);
        recording_dir(&id); // ensure directory exists
        id
    }

    pub fn stop(&self) -> Option<Recording> {
        if !self.active.load(Ordering::SeqCst) {
            return None;
        }
        self.active.store(false, Ordering::SeqCst);

        let id = self.recording_id.lock().unwrap().take()?;
        let frames = std::mem::take(&mut *self.frames.lock().unwrap());
        let start = self.start_time.lock().unwrap().take();
        let duration = start.map(|s| s.elapsed().as_secs_f64()).unwrap_or(0.0);

        let recording = Recording {
            id: id.clone(),
            name: format!("Recording {}", Utc::now().format("%Y-%m-%d %H:%M")),
            frames,
            audio_path: None,
            duration_secs: duration,
            created_at: Utc::now().to_rfc3339(),
        };

        self.recordings.lock().unwrap().push(recording.clone());
        Some(recording)
    }

    pub fn add_frame(&self, frame: CapturedFrame) {
        self.frames.lock().unwrap().push(frame);
    }

    pub fn push_audio_level(&self, level: f32) {
        let mut levels = self.audio_levels.lock().unwrap();
        levels.push(level);
        // Keep last 100 samples for waveform display
        if levels.len() > 100 {
            let drain_to = levels.len() - 100;
            levels.drain(..drain_to);
        }
    }

    pub fn get_state(&self) -> RecordingState {
        let elapsed = if self.active.load(Ordering::SeqCst) {
            self.start_time.lock().unwrap()
                .map(|s| s.elapsed().as_secs_f64())
                .unwrap_or(0.0)
        } else {
            0.0
        };

        RecordingState {
            is_recording: self.active.load(Ordering::SeqCst),
            recording_id: self.recording_id.lock().unwrap().clone(),
            frame_count: self.frames.lock().unwrap().len(),
            elapsed_secs: elapsed,
            audio_levels: self.audio_levels.lock().unwrap().clone(),
        }
    }

    pub fn list_recordings(&self) -> Vec<Recording> {
        self.recordings.lock().unwrap().clone()
    }

    pub fn delete_recording(&self, id: &str) -> bool {
        let mut recordings = self.recordings.lock().unwrap();
        let before = recordings.len();
        recordings.retain(|r| r.id != id);
        // Clean up files
        let dir = recording_dir(id);
        std::fs::remove_dir_all(&dir).ok();
        recordings.len() < before
    }
}

// ─────────────────────────────────────────────
//  Audio level monitoring (reuses cpal)
// ─────────────────────────────────────────────

pub fn start_audio_monitor(recorder: Arc<ScreenRecorder>) -> Result<cpal::Stream, String> {
    use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};

    let host = cpal::default_host();
    let device = host.default_input_device()
        .ok_or("No audio input device found")?;
    let config = device.default_input_config()
        .map_err(|e| format!("No input config: {}", e))?;

    let stream = match config.sample_format() {
        cpal::SampleFormat::F32 => build_monitor_stream::<f32>(&device, &config.into(), recorder)?,
        cpal::SampleFormat::I16 => build_monitor_stream::<i16>(&device, &config.into(), recorder)?,
        cpal::SampleFormat::U16 => build_monitor_stream::<u16>(&device, &config.into(), recorder)?,
        _ => return Err("Unsupported audio format".into()),
    };

    stream.play().map_err(|e| format!("Failed to start audio monitor: {}", e))?;
    Ok(stream)
}

fn build_monitor_stream<T: cpal::Sample + cpal::SizedSample + Send + 'static>(
    device: &cpal::Device,
    config: &cpal::StreamConfig,
    recorder: Arc<ScreenRecorder>,
) -> Result<cpal::Stream, String>
where
    f32: cpal::FromSample<T>,
{
    use cpal::traits::DeviceTrait;

    let channels = config.channels as usize;
    let sample_count = std::sync::Arc::new(std::sync::atomic::AtomicUsize::new(0));
    let sample_count_clone = sample_count.clone();
    let sample_rate = config.sample_rate.0 as usize;
    // Compute RMS every ~100ms
    let chunk_size = sample_rate / 10;
    let rms_accum = std::sync::Arc::new(Mutex::new(0.0f64));
    let rms_accum_clone = rms_accum.clone();

    let stream = device.build_input_stream(
        config,
        move |data: &[T], _: &cpal::InputCallbackInfo| {
            let mut sum = 0.0f64;
            for sample in data.iter().step_by(channels) {
                let val: f32 = cpal::Sample::from_sample(*sample);
                sum += (val as f64) * (val as f64);
            }
            let n = data.len() / channels;
            let mut accum = rms_accum_clone.lock().unwrap();
            *accum += sum;
            let count = sample_count_clone.fetch_add(n, Ordering::Relaxed) + n;

            if count >= chunk_size {
                let rms = (*accum / count as f64).sqrt() as f32;
                recorder.push_audio_level(rms);
                *accum = 0.0;
                sample_count_clone.store(0, Ordering::Relaxed);
            }
        },
        |err| eprintln!("Audio monitor error: {}", err),
        None,
    ).map_err(|e| format!("Failed to build audio stream: {}", e))?;

    Ok(stream)
}

// ─────────────────────────────────────────────
//  Process timeline - extract steps from recording
// ─────────────────────────────────────────────

#[derive(Clone, Serialize, Deserialize, Debug)]
pub struct ProcessStep {
    pub description: String,
    pub app: Option<String>,
    pub url: Option<String>,
    pub action_type: String, // "app_switch", "browser_nav", "app_action", "wait"
    pub timestamp_ms: u64,
}

/// Analyze frames and deduplicate into meaningful process steps.
/// Filters out OrbixCore frames and adds "open app" steps for apps used.
pub fn build_process_timeline(recording: &Recording) -> Vec<ProcessStep> {
    let mut steps: Vec<ProcessStep> = Vec::new();
    let mut last_app: Option<String> = None;
    let mut last_url: Option<String> = None;
    let mut last_title: Option<String> = None;
    let mut seen_apps: std::collections::HashSet<String> = std::collections::HashSet::new();

    for frame in &recording.frames {
        let app = frame.active_app.as_deref();
        let url = frame.browser_url.as_deref();
        let title = frame.window_title.as_deref();

        // Skip OrbixCore frames
        if let Some(a) = app {
            let a_lower = a.to_lowercase();
            if a_lower.contains("orbixcore") || a_lower.contains("orbix") {
                continue;
            }
        }

        // Detect app switch
        if app != last_app.as_deref() {
            if let Some(app_name) = app {
                // First time seeing this app → add an "open_app" step
                if seen_apps.insert(app_name.to_string()) {
                    steps.push(ProcessStep {
                        description: format!("Open {}", app_name),
                        app: Some(app_name.to_string()),
                        url: None,
                        action_type: "open_app".into(),
                        timestamp_ms: frame.timestamp_ms,
                    });
                }

                steps.push(ProcessStep {
                    description: format!("Switched to {}", app_name),
                    app: Some(app_name.to_string()),
                    url: url.map(|s| s.to_string()),
                    action_type: "app_switch".into(),
                    timestamp_ms: frame.timestamp_ms,
                });
            }
            last_app = app.map(|s| s.to_string());
            last_url = url.map(|s| s.to_string());
            last_title = title.map(|s| s.to_string());
            continue;
        }

        // Detect URL change (browser navigation)
        if url != last_url.as_deref() && url.is_some() {
            steps.push(ProcessStep {
                description: format!("Navigated to {}", title.unwrap_or(url.unwrap_or("page"))),
                app: app.map(|s| s.to_string()),
                url: url.map(|s| s.to_string()),
                action_type: "browser_nav".into(),
                timestamp_ms: frame.timestamp_ms,
            });
            last_url = url.map(|s| s.to_string());
            last_title = title.map(|s| s.to_string());
            continue;
        }

        // Detect window title change (different document/page within same app)
        if title != last_title.as_deref() && title.is_some() {
            // For browsers without URL access, treat title changes as navigations
            let (desc, action) = if frame.is_browser || url.is_some() {
                (format!("Navigated to \"{}\"", title.unwrap_or("page")), "browser_nav")
            } else {
                (format!("Opened \"{}\"", title.unwrap_or("document")), "app_action")
            };
            steps.push(ProcessStep {
                description: desc,
                app: app.map(|s| s.to_string()),
                url: url.map(|s| s.to_string()),
                action_type: action.into(),
                timestamp_ms: frame.timestamp_ms,
            });
            last_title = title.map(|s| s.to_string());
        }
    }

    steps
}

/// Build a text summary of the process for the LLM
pub fn build_process_summary(recording: &Recording) -> String {
    let steps = build_process_timeline(recording);
    if steps.is_empty() {
        return format!(
            "Screen recording '{}' with {} frames over {:.0} seconds. No app context was captured.",
            recording.name, recording.frames.len(), recording.duration_secs
        );
    }

    // Collect unique apps that need to be opened
    let apps_used: Vec<&str> = steps.iter()
        .filter(|s| s.action_type == "open_app")
        .filter_map(|s| s.app.as_deref())
        .collect();

    let mut summary = format!(
        "Process recording '{}' ({:.0}s, {} frames):\n\n",
        recording.name, recording.duration_secs, recording.frames.len()
    );

    if !apps_used.is_empty() {
        summary.push_str(&format!("Applications used (MUST be opened first): {}\n\n", apps_used.join(", ")));
    }

    for (i, step) in steps.iter().enumerate() {
        summary.push_str(&format!("{}. [{}] {}", i + 1, step.action_type, step.description));
        if let Some(ref url) = step.url {
            summary.push_str(&format!(" ({})", url));
        }
        summary.push('\n');
    }

    summary
}
