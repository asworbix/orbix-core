//! Pair mode: drag a bridge from Orbix onto any app window and Bob works
//! out how to talk to it. The user sees one thing (a paired app and a
//! plain-language line about what Bob can do with it); the probe ladder
//! below is Bob's problem, never the user's.
//!
//! A peer is an app plus an ordered list of transports plus the
//! capabilities those transports gave us:
//!
//!   mcp            the app runs `orbix mcp` and calls Bob's tools
//!                  (Claude Desktop, Cursor, anything with an MCP config)
//!   bridge         an app_bridge manifest exists for the app
//!   accessibility  the OS accessibility tree: type into it, read it
//!   clipboard      last resort: copy for the user to paste
//!
//! Recipes (resources/peers/*.json, user overrides in <data_dir>/peers/)
//! tell the ladder what an app prefers. Unknown apps get the generic
//! order: bridge if one exists, accessibility, clipboard. Everything
//! is data; adding an app is a JSON file, never Rust.

use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::path::{Path, PathBuf};
use std::sync::{Mutex, OnceLock};

const FILE: &str = "peers.json";
/// The MCP server name written into other apps' config files.
const MCP_SERVER_NAME: &str = "orbix";

// ─────────────────────────────────────────────
//  Model
// ─────────────────────────────────────────────

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum Transport {
    /// The cloud service behind the app, through the user's own sign-in.
    Service,
    /// The app's registered URL scheme: deterministic open/create actions.
    Url,
    /// An MCP server the app or its service publishes; Orbix is the client.
    McpServer,
    /// A Chromium/Electron app relaunched with DevTools: DOM read and type.
    #[serde(rename = "devtools")]
    DevTools,
    Mcp,
    Bridge,
    Accessibility,
    Clipboard,
}

impl Transport {
    pub fn label(&self) -> &'static str {
        match self {
            Transport::Service => "api",
            Transport::Url => "links",
            Transport::McpServer => "mcp",
            Transport::DevTools => "devtools",
            Transport::Mcp => "tools",
            Transport::Bridge => "scripting",
            Transport::Accessibility => "window",
            Transport::Clipboard => "clipboard",
        }
    }
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum Capability {
    /// Bob can put text into the app.
    Send,
    /// Bob can read what the app shows.
    Read,
    /// The app can call Bob's tools (MCP).
    ToolsExposed,
    /// Bob reaches the service behind the app with the user's sign-in.
    ServiceApi,
    /// Bob can open or create things in the app through its URL scheme.
    Actions,
    /// Bob can run the tools of an MCP server the app publishes.
    Tools,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum PeerStatus {
    Connected,
    /// Connected once the app restarts (MCP config written, app running).
    NeedsRestart,
    /// Connected, but the OS still has to grant a permission.
    NeedsPermission,
    /// The service behind the app is one sign-in away.
    NeedsSignIn,
    /// Only the clipboard rung worked.
    Limited,
}

/// One human step Bob could not do himself, shown as a single button.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct FollowUp {
    pub label: String,
    /// restart_app | open_accessibility_settings
    pub action: String,
}

/// One thing Bob can do through the app's URL scheme. `template` holds
/// `{{arg}}` placeholders; values are URL-encoded on substitution.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct UrlAction {
    pub id: String,
    pub description: String,
    pub template: String,
    #[serde(default)]
    pub args: Vec<UrlArg>,
    /// True once an open of this action returned without error. Shipped
    /// recipes start verified; drafted ones do not.
    #[serde(default)]
    pub verified: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct UrlArg {
    pub name: String,
    #[serde(default)]
    pub description: String,
    #[serde(default = "default_true")]
    pub required: bool,
}

fn default_true() -> bool {
    true
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Peer {
    pub id: String,
    /// Name Bob and the user use: "ChatGPT".
    pub app: String,
    #[serde(default)]
    pub bundle_id: String,
    #[serde(default)]
    pub process_name: String,
    /// Last pid seen. Informational; peers are matched by name/bundle.
    #[serde(default)]
    pub pid: Option<i64>,
    /// Transports that worked, in the order Bob tries them.
    pub transports: Vec<Transport>,
    pub capabilities: Vec<Capability>,
    /// app_bridge manifest name when a bridge transport is present.
    #[serde(default)]
    pub bridge: Option<String>,
    #[serde(default)]
    pub mcp_config_path: Option<String>,
    /// The app publishes an AppleScript dictionary: a future bridge author
    /// can target it even though this pairing did not use it.
    #[serde(default)]
    pub scriptable: bool,
    /// App-specific URL schemes the app registered (no http/https/file).
    #[serde(default)]
    pub url_schemes: Vec<String>,
    /// Actions Bob can open through those schemes.
    #[serde(default)]
    pub url_actions: Vec<UrlAction>,
    /// Last window profile (window_intelligence) used for controls.
    #[serde(default)]
    pub window_id: Option<String>,
    /// Chromium or Electron shell: DevTools can drive it once relaunched.
    #[serde(default)]
    pub chromium: bool,
    #[serde(default)]
    pub devtools_port: Option<u16>,
    #[serde(default)]
    pub devtools: Option<DevToolsRecipe>,
    pub status: PeerStatus,
    /// Bob's plain-language line about this pairing.
    pub bob_line: String,
    #[serde(default)]
    pub follow_up: Option<FollowUp>,
    pub paired_at: String,
    #[serde(default)]
    pub last_ok_at: Option<String>,
    #[serde(default)]
    pub last_error: Option<String>,
}

// ─────────────────────────────────────────────
//  Recipes (embedded + user dir)
// ─────────────────────────────────────────────

#[derive(Debug, Clone, Deserialize)]
pub struct McpRecipe {
    /// Config file the app reads MCP servers from (macOS, `~` expanded).
    #[serde(default)]
    pub config_path: Option<String>,
    /// Same for Windows (`%VAR%` expanded).
    #[serde(default)]
    #[cfg_attr(not(target_os = "windows"), allow(dead_code))]
    pub config_path_windows: Option<String>,
    #[serde(default = "default_servers_key")]
    pub servers_key: String,
    #[serde(default)]
    pub restart_required: bool,
    /// Apps that only take MCP servers through their own UI: the step
    /// Bob tells the user about, since he cannot do it silently.
    #[serde(default)]
    pub manual: Option<String>,
}

fn default_servers_key() -> String {
    "mcpServers".to_string()
}

/// DOM selectors for a Chromium/Electron app driven over DevTools.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct DevToolsRecipe {
    /// The message input (input, textarea or contenteditable).
    #[serde(default)]
    pub composer: Option<String>,
    /// The send button; Enter is sent when absent.
    #[serde(default)]
    pub send: Option<String>,
    /// Elements holding replies; the last one is read.
    #[serde(default)]
    pub reply: Option<String>,
    #[serde(default)]
    pub port: Option<u16>,
}

/// An MCP server a recipe says the app (or its service) publishes.
#[derive(Debug, Clone, Deserialize)]
pub struct McpServerRecipe {
    #[serde(default)]
    pub name: Option<String>,
    /// "stdio" or "http"
    #[serde(default = "default_stdio")]
    pub transport: String,
    #[serde(default)]
    pub command: String,
    #[serde(default)]
    pub args: Vec<String>,
    #[serde(default)]
    pub env: std::collections::HashMap<String, String>,
    #[serde(default)]
    pub url: Option<String>,
    /// Send the user's token for this connected service as the bearer.
    #[serde(default)]
    pub bearer_from_service: Option<String>,
}

fn default_stdio() -> String {
    "stdio".to_string()
}

#[derive(Debug, Clone, Deserialize)]
pub struct PeerRecipe {
    pub app: String,
    #[serde(default)]
    pub aliases: Vec<String>,
    #[serde(default)]
    pub bundle_ids: Vec<String>,
    #[serde(default)]
    pub process_names: Vec<String>,
    /// Preferred transport order. Empty means the generic ladder.
    #[serde(default)]
    pub transports: Vec<Transport>,
    #[serde(default)]
    pub mcp: Option<McpRecipe>,
    /// app_bridge manifest name to use for the bridge transport.
    #[serde(default)]
    pub bridge: Option<String>,
    /// The cloud service behind the app (services.rs slug).
    #[serde(default)]
    pub service: Option<String>,
    /// Known URL-scheme actions for the app.
    #[serde(default)]
    pub url_actions: Vec<UrlAction>,
    /// An MCP server to consume for this app.
    #[serde(default)]
    pub mcp_server: Option<McpServerRecipe>,
    /// DevTools selectors for a Chromium/Electron app.
    #[serde(default)]
    pub devtools: Option<DevToolsRecipe>,
    #[serde(default)]
    pub bob_line: Option<String>,
}

const BUILTIN: &[&str] = &[
    include_str!("../resources/peers/claude.json"),
    include_str!("../resources/peers/chatgpt.json"),
    include_str!("../resources/peers/cursor.json"),
    include_str!("../resources/peers/slack.json"),
    include_str!("../resources/peers/teams.json"),
    include_str!("../resources/peers/outlook.json"),
    include_str!("../resources/peers/github-desktop.json"),
    include_str!("../resources/peers/obsidian.json"),
    include_str!("../resources/peers/zoom.json"),
    include_str!("../resources/peers/spotify.json"),
    include_str!("../resources/peers/vscode.json"),
    include_str!("../resources/peers/mail.json"),
];

fn builtin_recipes() -> Vec<PeerRecipe> {
    BUILTIN
        .iter()
        .filter_map(|raw| match serde_json::from_str(raw) {
            Ok(r) => Some(r),
            Err(e) => {
                log::error!("[peer] embedded recipe failed to parse: {}", e);
                None
            }
        })
        .collect()
}

/// User recipes live in <data_dir>/peers/*.json and override embedded
/// ones with the same app name.
fn user_recipes() -> Vec<PeerRecipe> {
    let dir = crate::storage::data_dir().join("peers");
    let Ok(entries) = std::fs::read_dir(&dir) else {
        return Vec::new();
    };
    let mut out = Vec::new();
    for entry in entries.flatten() {
        let path = entry.path();
        if path.extension().and_then(|e| e.to_str()) != Some("json") {
            continue;
        }
        match std::fs::read_to_string(&path)
            .map_err(|e| e.to_string())
            .and_then(|raw| serde_json::from_str::<PeerRecipe>(&raw).map_err(|e| e.to_string()))
        {
            Ok(r) => out.push(r),
            Err(e) => log::warn!("[peer] skipping {}: {}", path.display(), e),
        }
    }
    out
}

pub fn all_recipes() -> Vec<PeerRecipe> {
    let mut recipes = builtin_recipes();
    for user in user_recipes() {
        if let Some(existing) = recipes.iter_mut().find(|r| r.app.eq_ignore_ascii_case(&user.app)) {
            *existing = user;
        } else {
            recipes.push(user);
        }
    }
    recipes
}

fn find_recipe(identity: &AppIdentity) -> Option<PeerRecipe> {
    let bundle = identity.bundle_id.to_lowercase();
    let name = identity.name.to_lowercase();
    let process = identity.process_name.to_lowercase();
    all_recipes().into_iter().find(|r| {
        (!bundle.is_empty() && r.bundle_ids.iter().any(|b| b.to_lowercase() == bundle))
            || r.process_names.iter().any(|p| p.to_lowercase() == process)
            || r.app.to_lowercase() == name
            || r.aliases.iter().any(|a| a.to_lowercase() == name)
    })
}

// ─────────────────────────────────────────────
//  Store
// ─────────────────────────────────────────────

static PEERS: OnceLock<Mutex<Vec<Peer>>> = OnceLock::new();

fn store() -> &'static Mutex<Vec<Peer>> {
    PEERS.get_or_init(|| Mutex::new(crate::storage::load_json::<Vec<Peer>>(FILE).unwrap_or_default()))
}

fn save(peers: &[Peer]) {
    if let Err(e) = crate::storage::save_json(FILE, peers) {
        log::warn!("[peer] could not save peers: {}", e);
    }
}

pub fn list() -> Vec<Peer> {
    store().lock().unwrap_or_else(|e| e.into_inner()).clone()
}

/// Find a peer by name, alias-tolerant: exact, then prefix, then contains.
pub fn find(name: &str) -> Option<Peer> {
    let needle = name.trim().to_lowercase();
    if needle.is_empty() {
        return None;
    }
    let peers = list();
    peers
        .iter()
        .find(|p| p.app.to_lowercase() == needle || p.id == needle)
        .or_else(|| peers.iter().find(|p| p.app.to_lowercase().starts_with(&needle)))
        .or_else(|| peers.iter().find(|p| p.app.to_lowercase().contains(&needle)))
        .cloned()
}

fn upsert(peer: Peer) -> Peer {
    let mut peers = store().lock().unwrap_or_else(|e| e.into_inner());
    if let Some(existing) = peers.iter_mut().find(|p| {
        p.app.eq_ignore_ascii_case(&peer.app)
            || (!peer.bundle_id.is_empty() && p.bundle_id == peer.bundle_id)
    }) {
        let mut merged = peer.clone();
        merged.id = existing.id.clone();
        *existing = merged.clone();
        save(&peers);
        merged
    } else {
        peers.push(peer.clone());
        save(&peers);
        peer
    }
}

fn update<F: FnOnce(&mut Peer)>(id: &str, f: F) -> Option<Peer> {
    let mut peers = store().lock().unwrap_or_else(|e| e.into_inner());
    let peer = peers.iter_mut().find(|p| p.id == id)?;
    f(peer);
    let out = peer.clone();
    save(&peers);
    Some(out)
}

pub fn unpair(name_or_id: &str) -> Result<Peer, String> {
    let peer = find(name_or_id).ok_or_else(|| format!("no paired app called \"{}\"", name_or_id))?;
    // Leave the other app's config as we found it: remove our MCP entry.
    if let Some(path) = &peer.mcp_config_path {
        if let Err(e) = remove_mcp_entry(Path::new(path)) {
            log::warn!("[peer] could not remove the orbix entry from {}: {}", path, e);
        }
    }
    let mut peers = store().lock().unwrap_or_else(|e| e.into_inner());
    peers.retain(|p| p.id != peer.id);
    save(&peers);
    Ok(peer)
}

// ─────────────────────────────────────────────
//  App identity
// ─────────────────────────────────────────────

#[derive(Debug, Clone, Serialize)]
pub struct AppIdentity {
    pub pid: i64,
    pub name: String,
    pub process_name: String,
    pub bundle_id: String,
    pub bundle_path: String,
    pub scriptable: bool,
    /// App-specific URL schemes (generic web/file schemes filtered out).
    #[serde(default)]
    pub url_schemes: Vec<String>,
    /// Chromium or Electron shell (ships a v8 context snapshot).
    #[serde(default)]
    pub chromium: bool,
    #[serde(default)]
    pub exe_path: String,
}

/// Chromium and Electron both ship `v8_context_snapshot*.bin` inside the
/// framework; native apps never do. Bounded walk, no shelling out.
fn has_v8_snapshot(dir: &Path, depth: u32) -> bool {
    if depth > 6 {
        return false;
    }
    let Ok(entries) = std::fs::read_dir(dir) else { return false };
    for e in entries.flatten() {
        let path = e.path();
        let name = e.file_name().to_string_lossy().to_lowercase();
        if name.starts_with("v8_context_snapshot") && name.ends_with(".bin") {
            return true;
        }
        if path.is_dir() && !name.ends_with(".app") && has_v8_snapshot(&path, depth + 1) {
            return true;
        }
    }
    false
}

/// A stable DevTools port per app, away from Chrome's 9222.
pub fn devtools_port_for(key: &str) -> u16 {
    let mut h: u32 = 2166136261;
    for b in key.to_lowercase().bytes() {
        h ^= b as u32;
        h = h.wrapping_mul(16777619);
    }
    9400 + (h % 400) as u16
}

/// Schemes every app may claim that say nothing about the app itself.
const GENERIC_SCHEMES: &[&str] = &["http", "https", "file", "ftp", "mailto", "tel", "sms", "webcal", "x-apple-openurl"];

fn app_specific_schemes(all: Vec<String>) -> Vec<String> {
    let mut out: Vec<String> = all
        .into_iter()
        .map(|s| s.trim().to_lowercase())
        .filter(|s| !s.is_empty() && !GENERIC_SCHEMES.contains(&s.as_str()))
        .collect();
    out.sort();
    out.dedup();
    out
}

/// Identify a running app from its pid. macOS: executable path from ps,
/// then the enclosing .app bundle's Info.plist. No TCC prompts.
#[cfg(target_os = "macos")]
pub async fn identify_pid(pid: i64) -> Result<AppIdentity, String> {
    let out = crate::process_ext::tokio_command("ps")
        .args(["-o", "comm=", "-p", &pid.to_string()])
        .output()
        .await
        .map_err(|e| format!("ps: {}", e))?;
    let exe = String::from_utf8_lossy(&out.stdout).trim().to_string();
    if exe.is_empty() {
        return Err(format!("no process with pid {}", pid));
    }
    let exe_path = PathBuf::from(&exe);
    let process_name = exe_path
        .file_name()
        .map(|s| s.to_string_lossy().into_owned())
        .unwrap_or_else(|| exe.clone());
    let bundle = exe_path
        .ancestors()
        .find(|p| p.extension().and_then(|e| e.to_str()) == Some("app"))
        .map(|p| p.to_path_buf());

    let mut identity = AppIdentity {
        pid,
        name: process_name.clone(),
        process_name,
        bundle_id: String::new(),
        bundle_path: bundle.as_ref().map(|b| b.to_string_lossy().into_owned()).unwrap_or_default(),
        scriptable: false,
        url_schemes: Vec::new(),
        chromium: bundle.as_ref().map(|b| has_v8_snapshot(&b.join("Contents/Frameworks"), 0)).unwrap_or(false),
        exe_path: exe.clone(),
    };
    if let Some(bundle) = bundle {
        let plist = bundle.join("Contents/Info.plist");
        if let Ok(out) = crate::process_ext::tokio_command("plutil")
            .args(["-convert", "json", "-o", "-"])
            .arg(&plist)
            .output()
            .await
        {
            if let Ok(info) = serde_json::from_slice::<Value>(&out.stdout) {
                let s = |k: &str| info.get(k).and_then(|v| v.as_str()).map(|v| v.trim().to_string()).filter(|v| !v.is_empty());
                identity.bundle_id = s("CFBundleIdentifier").unwrap_or_default();
                if let Some(n) = s("CFBundleDisplayName").or_else(|| s("CFBundleName")) {
                    identity.name = n;
                }
                identity.scriptable = s("OSAScriptingDefinition").is_some()
                    || info.get("NSAppleScriptEnabled").and_then(|v| v.as_bool()).unwrap_or(false);
                let schemes: Vec<String> = info
                    .get("CFBundleURLTypes")
                    .and_then(|t| t.as_array())
                    .map(|types| {
                        types
                            .iter()
                            .filter_map(|t| t.get("CFBundleURLSchemes").and_then(|s| s.as_array()))
                            .flatten()
                            .filter_map(|v| v.as_str().map(String::from))
                            .collect()
                    })
                    .unwrap_or_default();
                identity.url_schemes = app_specific_schemes(schemes);
            }
        }
        if identity.name.is_empty() {
            identity.name = bundle
                .file_stem()
                .map(|s| s.to_string_lossy().into_owned())
                .unwrap_or_else(|| identity.process_name.clone());
        }
    }
    Ok(identity)
}

#[cfg(not(target_os = "macos"))]
pub async fn identify_pid(pid: i64) -> Result<AppIdentity, String> {
    let app = crate::window_intelligence::running_apps()
        .into_iter()
        .find(|a| a.pid == pid)
        .ok_or_else(|| format!("no running app with pid {}", pid))?;
    let mut identity = AppIdentity {
        pid,
        name: app.name.clone(),
        process_name: app.name,
        bundle_id: String::new(),
        bundle_path: String::new(),
        scriptable: false,
        url_schemes: Vec::new(),
        chromium: false,
        exe_path: String::new(),
    };
    #[cfg(target_os = "windows")]
    {
        // The exe path, then every HKCR protocol whose open command runs it.
        let script = format!(
            r#"$p = Get-Process -Id {pid} -ErrorAction SilentlyContinue
if ($null -eq $p -or -not $p.Path) {{ exit 0 }}
$exe = [IO.Path]::GetFileName($p.Path).ToLower()
$out = @("EXE=" + $p.Path)
Get-ChildItem Registry::HKEY_CLASSES_ROOT -ErrorAction SilentlyContinue | ForEach-Object {{
  $k = $_
  if ($null -ne $k.GetValue('URL Protocol', $null)) {{
    $cmd = (Get-ItemProperty -Path ("Registry::HKEY_CLASSES_ROOT" + $k.PSChildName + "\shell\open\command") -ErrorAction SilentlyContinue).'(default)'
    if ($cmd -and $cmd.ToLower().Contains($exe)) {{ $out += $k.PSChildName }}
  }}
}}
$out -join "`n""#,
            pid = pid
        );
        if let Ok(out) = crate::os_auto::run_powershell_raw(&script).await {
            let mut schemes = Vec::new();
            for line in out.lines() {
                let line = line.trim();
                if let Some(exe) = line.strip_prefix("EXE=") {
                    identity.exe_path = exe.to_string();
                } else {
                    schemes.push(line.to_string());
                }
            }
            identity.url_schemes = app_specific_schemes(schemes);
            if let Some(dir) = Path::new(&identity.exe_path).parent() {
                identity.chromium = has_v8_snapshot(dir, 4);
            }
        }
    }
    Ok(identity)
}

/// Identify a running app by the name the user or Bob used.
pub async fn identify_name(name: &str) -> Result<AppIdentity, String> {
    let app = crate::window_intelligence::find_running_app(name).ok_or_else(|| {
        format!(
            "{} is not running. Open it, then pair it (drag the pair handle onto its window, or say \"pair with {}\").",
            name, name
        )
    })?;
    let mut identity = identify_pid(app.pid).await?;
    if identity.name.is_empty() {
        identity.name = app.name;
    }
    Ok(identity)
}

// ─────────────────────────────────────────────
//  The probe ladder
// ─────────────────────────────────────────────

/// Progress a caller can show while the ladder runs.
pub struct PairProgress<'a>(pub &'a (dyn Fn(&str) + Send + Sync));

/// Pair with an identified app: run the ladder, save the peer, return it.
/// Never launches the app, never types into it; probing reads only.
pub async fn pair(identity: AppIdentity, app: Option<&tauri::AppHandle>, progress: PairProgress<'_>) -> Result<Peer, String> {
    let own_pid = std::process::id() as i64;
    if identity.pid == own_pid || identity.bundle_id == "ai.orbixcore.app" {
        return Err("that is Orbix itself. Drag the handle onto another app's window.".to_string());
    }
    let recipe = find_recipe(&identity);
    let display = recipe.as_ref().map(|r| r.app.clone()).unwrap_or_else(|| identity.name.clone());
    (progress.0)(&format!("Connecting to {}…", display));

    let bridge_name = recipe
        .as_ref()
        .and_then(|r| r.bridge.clone())
        .or_else(|| crate::app_bridge::find_manifest(&display).map(|m| m.app));
    let service_slug = recipe.as_ref().and_then(|r| r.service.clone()).filter(|s| crate::services::info(s).is_some());
    let order: Vec<Transport> = match recipe.as_ref().filter(|r| !r.transports.is_empty()) {
        Some(r) => r.transports.clone(),
        None => {
            let mut v = Vec::new();
            if service_slug.is_some() {
                v.push(Transport::Service);
            }
            if bridge_name.is_some() {
                v.push(Transport::Bridge);
            }
            if !identity.url_schemes.is_empty() {
                v.push(Transport::Url);
            }
            if identity.chromium {
                v.push(Transport::DevTools);
            }
            v.push(Transport::Accessibility);
            v.push(Transport::Clipboard);
            v
        }
    };
    // Recipes that predate the DevTools rung still get it for Chromium apps.
    let order: Vec<Transport> = if identity.chromium && !order.contains(&Transport::DevTools) {
        let mut v = order.clone();
        let at = v.iter().position(|t| matches!(t, Transport::Accessibility | Transport::Clipboard)).unwrap_or(v.len());
        v.insert(at, Transport::DevTools);
        v
    } else {
        order
    };
    // Recipes that predate the URL rung still get it when the app has a scheme.
    let order: Vec<Transport> = if !identity.url_schemes.is_empty() && !order.contains(&Transport::Url) {
        let mut v = order.clone();
        let at = v.iter().position(|t| matches!(t, Transport::Accessibility | Transport::Clipboard)).unwrap_or(v.len());
        v.insert(at, Transport::Url);
        v
    } else {
        order
    };
    let mut url_actions: Vec<UrlAction> = Vec::new();
    let mut tools_line: Option<String> = None;
    let mut devtools_recipe = recipe.as_ref().and_then(|r| r.devtools.clone());
    let devtools_port = devtools_recipe
        .as_ref()
        .and_then(|d| d.port)
        .unwrap_or_else(|| devtools_port_for(if identity.bundle_id.is_empty() { &identity.name } else { &identity.bundle_id }));
    let mut devtools_up = false;

    let mut transports = Vec::new();
    let mut capabilities: Vec<Capability> = Vec::new();
    let mut notes: Vec<String> = Vec::new();
    let mut follow_up: Option<FollowUp> = None;
    let mut status = PeerStatus::Connected;
    let mut mcp_config_path = None;
    let mut errors: Vec<String> = Vec::new();

    let mut service_line: Option<String> = None;
    for transport in order {
        match transport {
            Transport::Service => {
                let Some(slug) = service_slug.clone() else { continue };
                let Some(info) = crate::services::info(&slug) else { continue };
                let connected = app.and_then(|a| {
                    use tauri::Manager;
                    a.try_state::<crate::AppState>().and_then(|st| crate::services::connected(&st, &slug))
                });
                match connected {
                    Some(inst) => {
                        transports.push(Transport::Service);
                        push_unique(&mut capabilities, Capability::ServiceApi);
                        service_line = Some(format!(
                            "{} is connected through its API as {}, so I work with the real thing: not the window, the data behind it.",
                            display,
                            inst.name.trim_end_matches(')').rsplit('(').next().unwrap_or(&inst.name)
                        ));
                    }
                    None => {
                        if status == PeerStatus::Connected {
                            status = PeerStatus::NeedsSignIn;
                        }
                        if follow_up.is_none() {
                            follow_up = Some(FollowUp {
                                label: format!("Sign in to {}", info.name),
                                action: format!("connect_service:{}", slug),
                            });
                        }
                        notes.push(format!(
                            "{} lets me in properly through its API. Sign in to {} once and I get the real data, not just the window.",
                            display, info.name
                        ));
                    }
                }
            }
            Transport::Url => {
                if identity.url_schemes.is_empty() {
                    continue;
                }
                transports.push(Transport::Url);
                // Shipped actions arrive verified; ones the user or Bob
                // taught earlier are kept from the existing peer.
                let mut actions: Vec<UrlAction> = recipe
                    .as_ref()
                    .map(|r| r.url_actions.iter().cloned().map(|mut a| { a.verified = true; a }).collect())
                    .unwrap_or_default();
                if let Some(existing) = find(&display) {
                    for a in existing.url_actions {
                        if !actions.iter().any(|b| b.id == a.id) {
                            actions.push(a);
                        }
                    }
                }
                if !actions.is_empty() {
                    push_unique(&mut capabilities, Capability::Actions);
                }
                url_actions = actions;
            }
            Transport::DevTools => {
                if !identity.chromium && devtools_recipe.is_none() {
                    continue;
                }
                if crate::browser_attach::probe(devtools_port).await {
                    devtools_up = true;
                    transports.push(Transport::DevTools);
                    push_unique(&mut capabilities, Capability::Read);
                    // No shipped selectors, or none for the composer: look at the page.
                    if devtools_recipe.as_ref().and_then(|d| d.composer.clone()).is_none() {
                        if let Ok(found) = crate::devtools_discover::discover(devtools_port).await {
                            devtools_recipe = Some(crate::devtools_discover::merge(devtools_recipe.clone(), found, false));
                        }
                    }
                    if devtools_recipe.as_ref().and_then(|d| d.composer.clone()).is_some() {
                        push_unique(&mut capabilities, Capability::Send);
                    }
                } else {
                    notes.push(format!(
                        "{} is a Chromium app. Relaunch it with DevTools and I read it and type into it reliably instead of guessing from the window.",
                        display
                    ));
                    if follow_up.is_none() {
                        follow_up = Some(FollowUp {
                            label: format!("Relaunch {} with DevTools", display),
                            action: "devtools_relaunch".to_string(),
                        });
                    }
                }
            }
            Transport::McpServer => {
                let Some(spec) = recipe.as_ref().and_then(|r| r.mcp_server.clone()) else { continue };
                let Some(app) = app else { continue };
                use tauri::Manager;
                let state = app.state::<crate::AppState>();
                if let Some(slug) = spec.bearer_from_service.as_deref().filter(|s| !s.is_empty()) {
                    if crate::services::connected(&state, slug).is_none() {
                        // The service rung already asked for the sign-in.
                        notes.push(format!("{}'s tool server works once that sign-in is done.", display));
                        continue;
                    }
                }
                let slug: String = display.to_lowercase().chars().map(|c| if c.is_ascii_alphanumeric() { c } else { '-' }).collect();
                let server = crate::McpServer {
                    id: format!("peer-{}", slug.trim_matches('-')),
                    name: spec.name.clone().unwrap_or_else(|| format!("{} tools", display)),
                    transport: if spec.transport.eq_ignore_ascii_case("http") { crate::McpTransport::Http } else { crate::McpTransport::Stdio },
                    command: spec.command.clone(),
                    args: spec.args.clone(),
                    env: spec.env.clone(),
                    url: spec.url.clone(),
                    enabled: true,
                    status: crate::McpStatus::Unknown,
                    created_at: chrono::Utc::now().to_rfc3339(),
                    bearer_from_service: spec.bearer_from_service.clone(),
                };
                let server = crate::mcp_client::upsert_server(&state, server);
                match crate::mcp_client::list_tools(&state, &server.id).await {
                    Ok((srv, tools)) => {
                        transports.push(Transport::McpServer);
                        push_unique(&mut capabilities, Capability::Tools);
                        let names: Vec<String> = tools.iter().take(6).map(|t| t.name.clone()).collect();
                        tools_line = Some(format!(
                            "Its tool server \"{}\" is up with {} tool{}{}.",
                            srv.name,
                            tools.len(),
                            if tools.len() == 1 { "" } else { "s" },
                            if names.is_empty() { String::new() } else { format!(" ({}{})", names.join(", "), if tools.len() > 6 { ", …" } else { "" }) }
                        ));
                    }
                    Err(e) => errors.push(format!("tool server: {}", e)),
                }
            }
            Transport::Mcp => {
                let Some(mcp) = recipe.as_ref().and_then(|r| r.mcp.clone()) else { continue };
                match mcp_config_file(&mcp) {
                    Some(path) => match register_mcp(&path, &mcp.servers_key, app, &display) {
                        Ok(()) => {
                            transports.push(Transport::Mcp);
                            push_unique(&mut capabilities, Capability::ToolsExposed);
                            mcp_config_path = Some(path.to_string_lossy().into_owned());
                            if mcp.restart_required {
                                status = PeerStatus::NeedsRestart;
                                follow_up = Some(FollowUp {
                                    label: format!("Restart {}", display),
                                    action: "restart_app".to_string(),
                                });
                            }
                        }
                        Err(e) => errors.push(format!("tools: {}", e)),
                    },
                    None => {
                        if let Some(manual) = &mcp.manual {
                            notes.push(manual.clone());
                        }
                    }
                }
            }
            Transport::Bridge => {
                let Some(name) = bridge_name.clone() else { continue };
                let Some(m) = crate::app_bridge::find_manifest(&name) else { continue };
                // Health check: a bridge only counts if the app answers its
                // probe right now. A learned one that fails is re-learned.
                match crate::app_bridge::probe_manifest(&m).await {
                    Ok(_) => {
                        transports.push(Transport::Bridge);
                        push_unique(&mut capabilities, Capability::Read);
                        if m.actions.iter().any(|a| {
                            let id = a.id.to_lowercase();
                            id.contains("send") || id.contains("compose") || id.contains("create")
                        }) {
                            push_unique(&mut capabilities, Capability::Send);
                        }
                    }
                    Err(e) => {
                        errors.push(format!("bridge: {}", e));
                        if m.learned {
                            if let Some(app) = app {
                                if crate::bridge_author::relearn_in_background(app, &m.app) {
                                    notes.push(format!("{}'s bridge stopped answering, so I am re-learning it.", display));
                                }
                            }
                        }
                    }
                }
            }
            Transport::Accessibility => {
                if !crate::window_intelligence::accessibility_trusted() {
                    // Ask once, in the OS's own dialog. The peer stays usable
                    // for the rungs that do not need it.
                    crate::window_intelligence::request_accessibility_prompt();
                    if status == PeerStatus::Connected {
                        status = PeerStatus::NeedsPermission;
                    }
                    if follow_up.is_none() {
                        follow_up = Some(FollowUp {
                            label: "Allow Accessibility".to_string(),
                            action: "open_accessibility_settings".to_string(),
                        });
                    }
                    notes.push("macOS needs to let me see other apps' windows (Accessibility) before I can type into it or read it.".to_string());
                    continue;
                }
                match crate::window_intelligence::ax_probe(identity.pid, &display).await {
                    Ok(probe) => {
                        transports.push(Transport::Accessibility);
                        if probe.readable {
                            push_unique(&mut capabilities, Capability::Read);
                        }
                        if probe.text_input.is_some() {
                            push_unique(&mut capabilities, Capability::Send);
                        }
                    }
                    Err(e) if e.contains("no window open") => {
                        // Running without a window: the rung is real, just
                        // untested. Keep it so send/read work once a window
                        // exists, and leave the user one step: check again.
                        transports.push(Transport::Accessibility);
                        notes.push(format!(
                            "{} has no window open right now. Open one and I will see what it lets me do.",
                            display
                        ));
                        if follow_up.is_none() {
                            follow_up = Some(FollowUp {
                                label: "Check again".to_string(),
                                action: "refresh".to_string(),
                            });
                        }
                    }
                    Err(e) => errors.push(format!("window: {}", e)),
                }
            }
            Transport::Clipboard => {
                transports.push(Transport::Clipboard);
                if !capabilities.contains(&Capability::Send) && !capabilities.contains(&Capability::ToolsExposed) {
                    if status == PeerStatus::Connected {
                        status = PeerStatus::Limited;
                    }
                }
            }
        }
    }

    let mut bob_line = compose_line(
        &display,
        &transports,
        &capabilities,
        status,
        recipe.as_ref().and_then(|r| r.bob_line.clone()),
        &notes,
    );
    if let Some(line) = service_line {
        bob_line = format!("{} {}", line, bob_line);
    }
    if let Some(line) = tools_line {
        bob_line.push(' ');
        bob_line.push_str(&line);
    }
    if devtools_up {
        bob_line.push_str(" It is running with DevTools, so I read it and type into it reliably.");
    } else if transports.contains(&Transport::Accessibility) && status != PeerStatus::NeedsPermission {
        bob_line.push_str(" I can also press its buttons and fill its fields.");
    }
    if !identity.url_schemes.is_empty() {
        if url_actions.is_empty() {
            bob_line.push_str(&format!(
                " It answers {}:// links, but I do not know its commands yet; tell me one and I will keep it.",
                identity.url_schemes[0]
            ));
        } else {
            let names: Vec<String> = url_actions.iter().take(4).map(|a| a.id.replace('_', " ")).collect();
            bob_line.push_str(&format!(" Through its links I can {}.", names.join(", ")));
        }
    }

    let prior_window_id = find(&display).and_then(|p| p.window_id);
    let peer = Peer {
        id: uuid::Uuid::new_v4().to_string(),
        app: display,
        bundle_id: identity.bundle_id.clone(),
        process_name: identity.process_name.clone(),
        pid: Some(identity.pid),
        transports,
        capabilities,
        bridge: bridge_name,
        mcp_config_path,
        scriptable: identity.scriptable,
        url_schemes: identity.url_schemes.clone(),
        url_actions,
        window_id: prior_window_id,
        chromium: identity.chromium,
        devtools_port: Some(devtools_port),
        devtools: devtools_recipe,
        status,
        bob_line,
        follow_up,
        paired_at: chrono::Utc::now().to_rfc3339(),
        last_ok_at: Some(chrono::Utc::now().to_rfc3339()),
        last_error: if errors.is_empty() { None } else { Some(errors.join("; ")) },
    };
    Ok(upsert(peer))
}

/// An app with a scripting dictionary and no bridge yet: learn it in the
/// background so the pairing result is instant. bridge_author refreshes
/// the peer and speaks when it is done. Callers of `pair` decide to call
/// this; `refresh` never does, so learning cannot re-trigger itself.
pub fn learn_in_background(app: &tauri::AppHandle, peer: &Peer, identity: &AppIdentity) -> bool {
    if !identity.scriptable || peer.bridge.is_some() || identity.bundle_path.is_empty() {
        return false;
    }
    let app = app.clone();
    let (name, display, process, bundle) = (peer.app.clone(), peer.app.clone(), identity.process_name.clone(), identity.bundle_path.clone());
    tauri::async_runtime::spawn(async move {
        crate::bridge_author::author_for_peer(app, name, display, process, bundle).await;
    });
    true
}

fn push_unique(v: &mut Vec<Capability>, c: Capability) {
    if !v.contains(&c) {
        v.push(c);
    }
}

/// Bob's one line. Honest about what actually worked; a weak pairing
/// says so rather than promising a full connection.
fn compose_line(
    app: &str,
    transports: &[Transport],
    caps: &[Capability],
    status: PeerStatus,
    recipe_line: Option<String>,
    notes: &[String],
) -> String {
    let tools = caps.contains(&Capability::ToolsExposed);
    let send = caps.contains(&Capability::Send);
    let read = caps.contains(&Capability::Read);
    let via_bridge = transports.contains(&Transport::Bridge);
    let mut parts: Vec<String> = Vec::new();

    if tools {
        parts.push(recipe_line.unwrap_or_else(|| format!("{} can now call me for anything on this Mac.", app)));
        if status == PeerStatus::NeedsRestart {
            parts.push(format!("{} needs a restart to see me.", app));
        }
    }
    match (send, read) {
        (true, true) => parts.push(if via_bridge {
            format!("I can read {} directly and put things into it.", app)
        } else {
            format!("I can type messages into {} and read what it shows.", app)
        }),
        (false, true) => parts.push(format!("I can read what {} shows. It does not let me type into it, so I will copy things for you to paste.", app)),
        (true, false) => parts.push(format!("I can put text into {}, but I cannot read it back.", app)),
        (false, false) => {
            if status == PeerStatus::NeedsPermission {
                parts.push(format!("Once Accessibility is allowed I can type into {} and read it.", app));
            } else if !tools {
                parts.push(format!("{} does not let me in. I can copy things for you to paste. That is all it allows.", app));
            }
        }
    }
    for n in notes {
        if !parts.iter().any(|p| p == n) {
            parts.push(n.clone());
        }
    }
    parts.join(" ")
}

// ─────────────────────────────────────────────
//  MCP registration: Orbix as a tool server inside other apps
// ─────────────────────────────────────────────

fn expand_home(p: &str) -> PathBuf {
    if let Some(rest) = p.strip_prefix("~/") {
        if let Some(home) = dirs::home_dir() {
            return home.join(rest);
        }
    }
    PathBuf::from(p)
}

#[cfg(target_os = "windows")]
fn expand_env(p: &str) -> PathBuf {
    let mut out = p.to_string();
    for var in ["APPDATA", "LOCALAPPDATA", "USERPROFILE"] {
        if let Ok(v) = std::env::var(var) {
            out = out.replace(&format!("%{}%", var), &v);
        }
    }
    PathBuf::from(out)
}

fn mcp_config_file(mcp: &McpRecipe) -> Option<PathBuf> {
    #[cfg(target_os = "windows")]
    {
        return mcp.config_path_windows.as_deref().map(expand_env);
    }
    #[allow(unreachable_code)]
    mcp.config_path.as_deref().map(expand_home)
}

/// The command other apps spawn to reach Bob. Every candidate is asked to
/// actually serve `mcp` before it is written into anyone's config: an
/// installed CLI from before pair mode parses `mcp` as a prompt and dies,
/// which reads as "server disconnected" in the other app. When the
/// installed copy is stale and the bundle carries a capable one, the
/// installed copy is refreshed so the config keeps the stable path.
pub fn cli_command_path(app: Option<&tauri::AppHandle>) -> Result<PathBuf, String> {
    let exe = if cfg!(windows) { ".exe" } else { "" };
    let installed = dirs::home_dir().map(|h| h.join(".local").join("bin").join(format!("orbix{exe}")));
    let mut candidates: Vec<PathBuf> = Vec::new();
    if let Some(p) = installed.clone() {
        candidates.push(p);
    }
    if let Some(app) = app {
        if let Ok(p) = crate::bob_cli_install::locate_bundled_cli(app) {
            candidates.push(p);
        }
    }
    if let Some(dir) = std::env::current_exe().ok().and_then(|p| p.parent().map(|d| d.to_path_buf())) {
        candidates.push(dir.join(format!("orbix{exe}")));
    }
    let capable = candidates
        .iter()
        .find(|p| is_real_binary(p) && supports_mcp(p))
        .cloned();
    let Some(capable) = capable else {
        let stale = installed.as_ref().map(|p| is_real_binary(p)).unwrap_or(false);
        return Err(if stale {
            "the orbix command on this computer is older than pair mode. Open Orbix Settings and choose Install orbix CLI, then pair again.".to_string()
        } else {
            "the orbix command is not installed yet. Open Orbix Settings and choose Install orbix CLI, then pair again.".to_string()
        });
    };
    if let (Some(installed), Some(app)) = (installed, app) {
        if capable != installed {
            // Same binary the Settings button installs; refreshing it keeps
            // the other app's config on a path that survives app updates.
            if crate::bob_cli_install::install(app).is_ok() && supports_mcp(&installed) {
                return Ok(installed);
            }
        }
    }
    Ok(capable)
}

/// Does this binary serve MCP? `orbix mcp` with a closed stdin exits 0 at
/// once; older builds reject the arguments and exit non-zero.
fn supports_mcp(path: &Path) -> bool {
    std::process::Command::new(path)
        .args(["mcp", "--client", "probe"])
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .status()
        .map(|s| s.success())
        .unwrap_or(false)
}

fn is_real_binary(p: &Path) -> bool {
    std::fs::metadata(p).map(|m| m.is_file() && m.len() > 100_000).unwrap_or(false)
}

fn read_config(path: &Path) -> Result<Value, String> {
    match std::fs::read_to_string(path) {
        Ok(raw) if raw.trim().is_empty() => Ok(json!({})),
        Ok(raw) => serde_json::from_str::<Value>(&raw)
            .map_err(|e| format!("{} is not valid JSON ({}); fix or remove it and pair again", path.display(), e)),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(json!({})),
        Err(e) => Err(format!("read {}: {}", path.display(), e)),
    }
}

fn write_config(path: &Path, value: &Value) -> Result<(), String> {
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir).map_err(|e| format!("create {}: {}", dir.display(), e))?;
    }
    // One backup of the file as we first found it, never overwritten.
    let backup = path.with_extension("json.orbix-backup");
    if path.exists() && !backup.exists() {
        let _ = std::fs::copy(path, &backup);
    }
    let pretty = serde_json::to_string_pretty(value).map_err(|e| e.to_string())?;
    crate::storage::atomic_write(path, pretty).map_err(|e| format!("write {}: {}", path.display(), e))
}

/// Add (or refresh) the `orbix` server entry in an app's MCP config.
/// Other entries are left byte-for-byte as the user had them.
fn register_mcp(path: &Path, servers_key: &str, app: Option<&tauri::AppHandle>, client: &str) -> Result<(), String> {
    let command = cli_command_path(app)?;
    let mut config = read_config(path)?;
    if !config.is_object() {
        return Err(format!("{} is not a JSON object", path.display()));
    }
    let servers = config
        .as_object_mut()
        .unwrap()
        .entry(servers_key.to_string())
        .or_insert_with(|| json!({}));
    if !servers.is_object() {
        *servers = json!({});
    }
    servers.as_object_mut().unwrap().insert(
        MCP_SERVER_NAME.to_string(),
        json!({
            "command": command.to_string_lossy(),
            "args": ["mcp", "--client", client],
        }),
    );
    write_config(path, &config)
}

fn remove_mcp_entry(path: &Path) -> Result<(), String> {
    let mut config = read_config(path)?;
    let mut changed = false;
    if let Some(obj) = config.as_object_mut() {
        for (_, v) in obj.iter_mut() {
            if let Some(servers) = v.as_object_mut() {
                if servers.remove(MCP_SERVER_NAME).is_some() {
                    changed = true;
                }
            }
        }
    }
    if changed {
        write_config(path, &config)?;
    }
    Ok(())
}

// ─────────────────────────────────────────────
//  Using a peer: send, read, follow-up
// ─────────────────────────────────────────────

#[derive(Debug, Clone, Serialize)]
pub struct SendReport {
    pub app: String,
    pub via: Transport,
    /// What happened, in Bob's words.
    pub detail: String,
    /// True when the text reached the app itself (not just the clipboard).
    pub delivered: bool,
}

pub async fn send(name: &str, text: &str) -> Result<SendReport, String> {
    let peer = find(name).ok_or_else(|| not_paired(name))?;
    let text = text.trim();
    if text.is_empty() {
        return Err("nothing to send".to_string());
    }
    let mut last_err: Option<String> = None;
    for transport in &peer.transports {
        match transport {
            Transport::Service | Transport::Url | Transport::McpServer | Transport::Mcp | Transport::Bridge => continue,
            Transport::DevTools => {
                let mut result = devtools_send(&peer, text).await;
                if result.is_err() {
                    // The selectors may have drifted: look at the page again,
                    // keep what it shows, and try once more.
                    if let Some(healed) = rediscover(&peer, true).await {
                        result = devtools_send(&healed, text).await;
                    }
                }
                match result {
                    Ok(detail) => {
                        mark_ok(&peer.id, None);
                        return Ok(SendReport { app: peer.app.clone(), via: Transport::DevTools, detail, delivered: true });
                    }
                    Err(e) => last_err = Some(e),
                }
            }
            Transport::Accessibility => {
                let pid = match live_pid(&peer).await {
                    Some(p) => p,
                    None => {
                        last_err = Some(format!("{} is not running", peer.app));
                        continue;
                    }
                };
                match crate::window_intelligence::ax_send_text(pid, &peer.app, text).await {
                    Ok(report) => {
                        mark_ok(&peer.id, Some(pid));
                        return Ok(SendReport {
                            app: peer.app.clone(),
                            via: Transport::Accessibility,
                            detail: report.detail,
                            delivered: true,
                        });
                    }
                    Err(e) => last_err = Some(e),
                }
            }
            Transport::Clipboard => {
                let r = crate::os_auto::set_clipboard(text).await;
                if r.success {
                    mark_ok(&peer.id, None);
                    return Ok(SendReport {
                        app: peer.app.clone(),
                        via: Transport::Clipboard,
                        detail: format!(
                            "{} does not let me type into it, so I copied the text. Paste it into {} (Cmd+V).{}",
                            peer.app,
                            peer.app,
                            last_err.as_ref().map(|e| format!(" (window route failed: {})", e)).unwrap_or_default()
                        ),
                        delivered: false,
                    });
                }
                last_err = r.error.or(last_err);
            }
        }
    }
    let e = last_err.unwrap_or_else(|| format!("{} has no way to receive text", peer.app));
    mark_err(&peer.id, &e);
    Err(e)
}

#[derive(Debug, Clone, Serialize)]
pub struct ReadReport {
    pub app: String,
    pub via: Transport,
    pub title: String,
    pub text: String,
    pub truncated: bool,
}

pub async fn read(name: &str) -> Result<ReadReport, String> {
    let peer = find(name).ok_or_else(|| not_paired(name))?;
    let mut last_err: Option<String> = None;
    for transport in &peer.transports {
        match transport {
            Transport::Service | Transport::Url | Transport::McpServer | Transport::Mcp | Transport::Bridge => continue,
            Transport::DevTools => match {
                let mut r = devtools_read(&peer).await;
                if r.is_err() || peer.devtools.as_ref().and_then(|d| d.reply.clone()).is_none() {
                    if let Some(healed) = rediscover(&peer, false).await {
                        if healed.devtools.as_ref().and_then(|d| d.reply.clone()).is_some() {
                            r = devtools_read(&healed).await;
                        }
                    }
                }
                r
            } {
                Ok((title, text)) => {
                    mark_ok(&peer.id, None);
                    let truncated = text.len() > 12_000;
                    let text = if truncated { text.chars().rev().take(12_000).collect::<Vec<_>>().into_iter().rev().collect() } else { text };
                    return Ok(ReadReport { app: peer.app.clone(), via: Transport::DevTools, title, text, truncated });
                }
                Err(e) => last_err = Some(e),
            },
            Transport::Accessibility => {
                let pid = match live_pid(&peer).await {
                    Some(p) => p,
                    None => {
                        last_err = Some(format!("{} is not running", peer.app));
                        continue;
                    }
                };
                match crate::window_intelligence::read_window_by_pid(pid, &peer.app).await {
                    Ok(win) => {
                        mark_ok(&peer.id, Some(pid));
                        return Ok(ReadReport {
                            app: peer.app.clone(),
                            via: Transport::Accessibility,
                            title: win.window_title,
                            text: win.text,
                            truncated: win.truncated,
                        });
                    }
                    Err(e) => last_err = Some(e),
                }
            }
            Transport::Clipboard => {
                if let Ok(text) = crate::os_auto::get_clipboard().await {
                    return Ok(ReadReport {
                        app: peer.app.clone(),
                        via: Transport::Clipboard,
                        title: "clipboard".to_string(),
                        text,
                        truncated: false,
                    });
                }
            }
        }
    }
    let e = last_err.unwrap_or_else(|| format!("{} has no readable surface", peer.app));
    mark_err(&peer.id, &e);
    Err(e)
}

/// Build a URL from an action template: every `{{arg}}` is replaced by the
/// URL-encoded value; a missing required arg is an error, a missing
/// optional one becomes empty.
pub fn render_url(action: &UrlAction, args: &Value) -> Result<String, String> {
    let mut url = action.template.clone();
    for arg in &action.args {
        let raw = args.get(&arg.name).map(|v| match v {
            Value::String(s) => s.clone(),
            other => other.to_string(),
        });
        let value = match raw {
            Some(v) if !v.trim().is_empty() => v,
            _ if arg.required => return Err(format!("{} needs \"{}\": {}", action.id, arg.name, arg.description)),
            _ => String::new(),
        };
        url = url.replace(&format!("{{{{{}}}}}", arg.name), &urlencoding::encode(&value));
    }
    if url.contains("{{") {
        return Err(format!("{} has a placeholder no arg fills: {}", action.id, url));
    }
    Ok(url)
}

/// Open one of a paired app's URL actions. The URL's scheme must be one
/// the app registered (or mailto/tel, which the recipe opted into), so a
/// template can never be pointed at another app.
pub async fn open_action(app_name: &str, action_id: &str, args: &Value) -> Result<String, String> {
    let peer = find(app_name).ok_or_else(|| not_paired(app_name))?;
    let action = peer
        .url_actions
        .iter()
        .find(|a| a.id.eq_ignore_ascii_case(action_id.trim()))
        .cloned()
        .ok_or_else(|| {
            let known: Vec<&str> = peer.url_actions.iter().map(|a| a.id.as_str()).collect();
            if known.is_empty() {
                format!("{} has no link actions yet. add_url_action teaches one.", peer.app)
            } else {
                format!("{} has no action \"{}\". Its actions: {}", peer.app, action_id, known.join(", "))
            }
        })?;
    let url = render_url(&action, args)?;
    let scheme = url.split(':').next().unwrap_or("").to_lowercase();
    let allowed = peer.url_schemes.iter().any(|s| *s == scheme) || matches!(scheme.as_str(), "mailto" | "tel");
    if !allowed {
        return Err(format!("{} does not own the {}:// scheme; refusing to open {}", peer.app, scheme, url));
    }
    let r = crate::os_auto::open_url(&url).await;
    if !r.success {
        mark_err(&peer.id, r.error.as_deref().unwrap_or("open failed"));
        return Err(r.error.unwrap_or_else(|| format!("could not open {}", url)));
    }
    update(&peer.id, |p| {
        if let Some(a) = p.url_actions.iter_mut().find(|a| a.id == action.id) {
            a.verified = true;
        }
        p.last_ok_at = Some(chrono::Utc::now().to_rfc3339());
        p.last_error = None;
    });
    Ok(format!("Opened {} in {}: {}", action.id.replace('_', " "), peer.app, url))
}

/// Teach a paired app a URL action (from the user, or from Bob's own
/// knowledge of the app). Stored unverified until it opens cleanly.
pub fn add_url_action(app_name: &str, action: UrlAction) -> Result<Peer, String> {
    let peer = find(app_name).ok_or_else(|| not_paired(app_name))?;
    let id = action.id.trim().to_lowercase().replace(' ', "_");
    if id.is_empty() || action.template.trim().is_empty() {
        return Err("an action needs an id and a template".to_string());
    }
    let scheme = action.template.split(':').next().unwrap_or("").to_lowercase();
    let allowed = peer.url_schemes.iter().any(|s| *s == scheme) || matches!(scheme.as_str(), "mailto" | "tel");
    if !allowed {
        return Err(format!(
            "{} registered {}, not {}://",
            peer.app,
            if peer.url_schemes.is_empty() { "no URL scheme".to_string() } else { peer.url_schemes.iter().map(|s| format!("{}://", s)).collect::<Vec<_>>().join(", ") },
            scheme
        ));
    }
    // Every placeholder must be a declared arg.
    let placeholders: Vec<String> = regex::Regex::new(r"\{\{([a-zA-Z0-9_]+)\}\}")
        .unwrap()
        .captures_iter(&action.template)
        .map(|c| c[1].to_string())
        .collect();
    let mut action = UrlAction { id, verified: false, ..action };
    for ph in placeholders {
        if !action.args.iter().any(|a| a.name == ph) {
            action.args.push(UrlArg { name: ph, description: String::new(), required: true });
        }
    }
    update(&peer.id, |p| {
        p.url_actions.retain(|a| a.id != action.id);
        p.url_actions.push(action.clone());
        if !p.capabilities.contains(&Capability::Actions) {
            p.capabilities.push(Capability::Actions);
        }
        if !p.transports.contains(&Transport::Url) {
            p.transports.push(Transport::Url);
        }
    })
    .ok_or_else(|| "peer vanished".to_string())
}

#[cfg(target_os = "macos")]
async fn relaunch_with_devtools(peer: &Peer, port: u16) -> Result<(), String> {
    let out = crate::process_ext::tokio_command("open")
        .args(["-a", &peer.app, "--args", &format!("--remote-debugging-port={}", port)])
        .output()
        .await
        .map_err(|e| format!("open: {}", e))?;
    if out.status.success() {
        Ok(())
    } else {
        Err(String::from_utf8_lossy(&out.stderr).trim().to_string())
    }
}

#[cfg(target_os = "windows")]
async fn relaunch_with_devtools(peer: &Peer, port: u16) -> Result<(), String> {
    let exe = crate::window_intelligence::find_running_app(&peer.app)
        .map(|_| String::new())
        .unwrap_or_default();
    let _ = exe;
    let script = format!(
        "Start-Process -FilePath '{}' -ArgumentList '--remote-debugging-port={}'",
        peer.process_name.replace('\'', "''"),
        port
    );
    crate::os_auto::run_powershell_raw(&script).await.map(|_| ())
}

#[cfg(not(any(target_os = "macos", target_os = "windows")))]
async fn relaunch_with_devtools(_peer: &Peer, _port: u16) -> Result<(), String> {
    Err("DevTools relaunch is not available on this platform".to_string())
}

fn devtools_port(peer: &Peer) -> Result<u16, String> {
    let port = peer.devtools_port.unwrap_or_else(|| devtools_port_for(&peer.app));
    Ok(port)
}

/// Look at the page again and keep what it shows on the peer. `replace`
/// swaps a failed composer/send for the discovered ones.
async fn rediscover(peer: &Peer, replace: bool) -> Option<Peer> {
    let port = peer.devtools_port.unwrap_or_else(|| devtools_port_for(&peer.app));
    if !crate::browser_attach::probe(port).await {
        return None;
    }
    let found = crate::devtools_discover::discover(port).await.ok()?;
    let merged = crate::devtools_discover::merge(peer.devtools.clone(), found, replace);
    let mut healed = peer.clone();
    healed.devtools = Some(merged.clone());
    update(&peer.id, |p| {
        p.devtools = Some(merged.clone());
        if merged.composer.is_some() && !p.capabilities.contains(&Capability::Send) {
            p.capabilities.push(Capability::Send);
        }
    });
    Some(healed)
}

/// Type into the app's composer over DevTools and send. Works for inputs,
/// textareas and contenteditable editors (insertText keeps the editor's
/// own model in sync, unlike setting a value).
pub(crate) async fn devtools_send(peer: &Peer, text: &str) -> Result<String, String> {
    let port = devtools_port(peer)?;
    if !crate::browser_attach::probe(port).await {
        return Err(format!("{} is not running with DevTools", peer.app));
    }
    let dt = peer.devtools.clone().unwrap_or(DevToolsRecipe { composer: None, send: None, reply: None, port: None });
    let composer = dt
        .composer
        .clone()
        .unwrap_or_else(|| "textarea, [contenteditable='true'], input[type='text']".to_string());
    let js = crate::browser_attach::js_str;
    let expr = format!(
        "(() => {{ const els = document.querySelectorAll({sel}); const el = els[els.length - 1]; if (!el) return '__notfound__'; \
          el.focus(); \
          if (el.isContentEditable) {{ const s = window.getSelection(); s.selectAllChildren(el); s.collapseToEnd(); document.execCommand('insertText', false, {txt}); }} \
          else {{ const setter = Object.getOwnPropertyDescriptor(Object.getPrototypeOf(el), 'value'); if (setter && setter.set) setter.set.call(el, {txt}); else el.value = {txt}; el.dispatchEvent(new Event('input', {{bubbles: true}})); }} \
          const now = el.isContentEditable ? el.innerText : el.value; return now.includes({txt}) ? 'ok' : '__ignored__'; }})()",
        sel = js(&composer),
        txt = js(text)
    );
    match crate::browser_attach::drive_evaluate(port, None, &expr).await? {
        Value::String(s) if s == "ok" => {}
        Value::String(s) if s == "__notfound__" => return Err(format!("{} has no composer matching {}", peer.app, composer)),
        _ => return Err(format!("{} ignored the text", peer.app)),
    }
    tokio::time::sleep(std::time::Duration::from_millis(150)).await;
    let sent = match dt.send.clone() {
        Some(btn) => {
            let expr = format!(
                "(() => {{ const b = document.querySelector({sel}); if (!b) return '__notfound__'; if (b.disabled) return '__disabled__'; b.click(); return 'ok'; }})()",
                sel = js(&btn)
            );
            match crate::browser_attach::drive_evaluate(port, None, &expr).await? {
                Value::String(s) if s == "ok" => "pressed Send".to_string(),
                Value::String(s) if s == "__disabled__" => return Err(format!("{}'s Send button stayed disabled", peer.app)),
                _ => "typed it; the Send button was not where I expected, press Enter there".to_string(),
            }
        }
        None => {
            let expr = format!(
                "(() => {{ const els = document.querySelectorAll({sel}); const el = els[els.length - 1]; if (!el) return '__notfound__'; \
                  for (const type of ['keydown', 'keypress', 'keyup']) el.dispatchEvent(new KeyboardEvent(type, {{key: 'Enter', code: 'Enter', keyCode: 13, which: 13, bubbles: true}})); return 'ok'; }})()",
                sel = js(&composer)
            );
            let _ = crate::browser_attach::drive_evaluate(port, None, &expr).await?;
            "pressed Enter".to_string()
        }
    };
    Ok(format!("Typed into {} over DevTools and {}.", peer.app, sent))
}

/// Read the latest reply (recipe selector) or the whole page text.
pub(crate) async fn devtools_read(peer: &Peer) -> Result<(String, String), String> {
    let port = devtools_port(peer)?;
    if !crate::browser_attach::probe(port).await {
        return Err(format!("{} is not running with DevTools", peer.app));
    }
    let js = crate::browser_attach::js_str;
    let title = crate::browser_attach::drive_evaluate(port, None, "document.title")
        .await
        .ok()
        .and_then(|v| v.as_str().map(String::from))
        .unwrap_or_default();
    if let Some(sel) = peer.devtools.as_ref().and_then(|d| d.reply.clone()) {
        let expr = format!(
            "(() => {{ const els = document.querySelectorAll({sel}); const el = els[els.length - 1]; return el ? el.innerText : '__notfound__'; }})()",
            sel = js(&sel)
        );
        if let Ok(Value::String(t)) = crate::browser_attach::drive_evaluate(port, None, &expr).await {
            if t != "__notfound__" && !t.trim().is_empty() {
                return Ok((title, t));
            }
        }
    }
    let text = crate::browser_attach::drive_read_text(port, None, "").await?;
    Ok((title, text))
}

// ─────────────────────────────────────────────
//  Window controls: buttons and fields through the window profile
// ─────────────────────────────────────────────

#[derive(Debug, Clone, Serialize)]
pub struct ControlsReport {
    pub app: String,
    pub window: String,
    pub fields: Vec<(String, String, Option<String>)>,
    pub buttons: Vec<(String, String)>,
}

/// Profile the app's front window now and remember it on the peer.
async fn profile_window(state: &crate::AppState, peer: &Peer) -> Result<crate::window_intelligence::WindowProfile, String> {
    if !crate::window_intelligence::accessibility_trusted() {
        return Err("Accessibility access is not granted, so Bob cannot see the app's controls.".to_string());
    }
    let pid = live_pid(peer).await.ok_or_else(|| format!("{} is not running", peer.app))?;
    let name = peer.app.clone();
    let profile = tokio::task::spawn_blocking(move || {
        let exe = std::env::current_exe().map_err(|e| e.to_string())?;
        let output = std::process::Command::new(&exe)
            .arg("--wi-profile")
            .arg(pid.to_string())
            .arg(&name)
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::piped())
            .output()
            .map_err(|e| format!("Failed to spawn profiler: {}", e))?;
        if !output.status.success() {
            return Err(format!("could not read {}'s controls: {}", name, crate::truncate_str(String::from_utf8_lossy(&output.stderr).trim(), 200)));
        }
        serde_json::from_str::<crate::window_intelligence::WindowProfile>(&String::from_utf8_lossy(&output.stdout))
            .map_err(|e| format!("bad profile: {}", e))
    })
    .await
    .map_err(|e| e.to_string())??;
    {
        let mut store = state.connect.window_intelligence_store.lock().unwrap_or_else(|e| e.into_inner());
        store.profiles.retain(|p| p.pid != pid);
        store.profiles.push(profile.clone());
        store.save().ok();
    }
    update(&peer.id, |p| {
        p.window_id = Some(profile.window_id.clone());
        p.pid = Some(pid);
    });
    Ok(profile)
}

fn pick<'a, T>(items: &'a [T], wanted: &str, id: impl Fn(&T) -> &str, label: impl Fn(&T) -> &str) -> Option<&'a T> {
    let w = wanted.trim().to_lowercase();
    if w.is_empty() {
        return None;
    }
    items
        .iter()
        .find(|i| id(i).to_lowercase() == w || label(i).to_lowercase() == w)
        .or_else(|| items.iter().find(|i| label(i).to_lowercase().starts_with(&w)))
        .or_else(|| items.iter().find(|i| label(i).to_lowercase().contains(&w)))
}

/// The buttons and fields in the app's front window, fresh.
pub async fn window_controls(state: &crate::AppState, app_name: &str) -> Result<ControlsReport, String> {
    let peer = find(app_name).ok_or_else(|| not_paired(app_name))?;
    let profile = profile_window(state, &peer).await?;
    Ok(ControlsReport {
        app: peer.app.clone(),
        window: profile.title.clone(),
        fields: profile
            .profile
            .inputs
            .iter()
            .map(|i| (i.id.clone(), if i.label.is_empty() { i.input_type.clone() } else { i.label.clone() }, i.current_value.clone()))
            .collect(),
        buttons: profile
            .profile
            .actions
            .iter()
            .filter(|a| a.enabled)
            .map(|a| (a.id.clone(), if a.label.is_empty() { a.action_type.clone() } else { a.label.clone() }))
            .collect(),
    })
}

/// Press a button by label or id. Re-profiles once if the window changed.
pub async fn press(state: &crate::AppState, app_name: &str, control: &str) -> Result<String, String> {
    let peer = find(app_name).ok_or_else(|| not_paired(app_name))?;
    for attempt in 0..2 {
        let profile = profile_window(state, &peer).await?;
        let Some(action) = pick(&profile.profile.actions, control, |a| &a.id, |a| &a.label) else {
            if attempt == 1 {
                let known: Vec<&str> = profile.profile.actions.iter().map(|a| a.label.as_str()).filter(|l| !l.is_empty()).take(20).collect();
                return Err(format!("{} has no button like \"{}\". Buttons: {}", peer.app, control, known.join(", ")));
            }
            continue;
        };
        let result = {
            let mut store = state.connect.window_intelligence_store.lock().unwrap_or_else(|e| e.into_inner());
            store.trigger_action(&profile.window_id, &action.id)
        };
        match result {
            Ok(r) if r.success => return Ok(format!("Pressed \"{}\" in {}.", action.label, peer.app)),
            Ok(_) | Err(_) if attempt == 0 => continue,
            Ok(_) => return Err(format!("{} did not react to \"{}\"", peer.app, action.label)),
            Err(e) => return Err(e),
        }
    }
    Err(format!("could not press \"{}\" in {}", control, peer.app))
}

/// Fill a field by label or id.
pub async fn fill(state: &crate::AppState, app_name: &str, field: &str, value: &str) -> Result<String, String> {
    let peer = find(app_name).ok_or_else(|| not_paired(app_name))?;
    let profile = profile_window(state, &peer).await?;
    let Some(input) = pick(&profile.profile.inputs, field, |i| &i.id, |i| &i.label) else {
        let known: Vec<&str> = profile.profile.inputs.iter().map(|i| i.label.as_str()).filter(|l| !l.is_empty()).take(20).collect();
        return Err(format!("{} has no field like \"{}\". Fields: {}", peer.app, field, known.join(", ")));
    };
    let result = {
        let mut store = state.connect.window_intelligence_store.lock().unwrap_or_else(|e| e.into_inner());
        store.write_fields(
            &profile.window_id,
            vec![crate::window_intelligence::WindowWriteRequest { field: input.id.clone(), value: value.to_string() }],
        )
    }?;
    if result.written.contains(&input.id) {
        Ok(format!("Filled \"{}\" in {}.", if input.label.is_empty() { &input.id } else { &input.label }, peer.app))
    } else {
        Err(format!("{} did not accept a value for \"{}\"", peer.app, if input.label.is_empty() { &input.id } else { &input.label }))
    }
}

/// Run the one human step Bob offered as a button.
pub async fn follow_up(name_or_id: &str, app: Option<&tauri::AppHandle>) -> Result<Peer, String> {
    let peer = find(name_or_id).ok_or_else(|| not_paired(name_or_id))?;
    let Some(fu) = peer.follow_up.clone() else {
        return Ok(peer);
    };
    if let Some(slug) = fu.action.strip_prefix("connect_service:") {
        let app = app.ok_or("signing in needs the app window")?;
        use tauri::Manager;
        let state = app.state::<crate::AppState>();
        crate::services::connect(&state, app, slug).await?;
        return refresh(&peer.app, Some(app)).await;
    }
    match fu.action.as_str() {
        "restart_app" => {
            let quit = crate::os_auto::quit_application(&peer.app).await;
            if !quit.success {
                return Err(quit.error.unwrap_or_else(|| format!("could not quit {}", peer.app)));
            }
            tokio::time::sleep(std::time::Duration::from_millis(1800)).await;
            let open = crate::os_auto::open_application(&peer.app).await;
            if !open.success {
                return Err(open.error.unwrap_or_else(|| format!("could not reopen {}", peer.app)));
            }
            update(&peer.id, |p| {
                p.status = PeerStatus::Connected;
                p.follow_up = None;
                p.bob_line = p.bob_line.replace(&format!(" {} needs a restart to see me.", p.app), "");
                p.last_ok_at = Some(chrono::Utc::now().to_rfc3339());
            })
            .ok_or_else(|| "peer vanished".to_string())
        }
        "open_accessibility_settings" => {
            let _ = crate::os_auto::open_url("x-apple.systempreferences:com.apple.preference.security?Privacy_Accessibility").await;
            Ok(peer)
        }
        "refresh" => refresh(&peer.app, None).await,
        "devtools_relaunch" => {
            let port = peer.devtools_port.unwrap_or_else(|| devtools_port_for(&peer.app));
            let quit = crate::os_auto::quit_application(&peer.app).await;
            if !quit.success {
                return Err(quit.error.unwrap_or_else(|| format!("could not quit {}", peer.app)));
            }
            tokio::time::sleep(std::time::Duration::from_millis(1800)).await;
            relaunch_with_devtools(&peer, port).await?;
            // Give the shell a moment to open its debugging port.
            let deadline = std::time::Instant::now() + std::time::Duration::from_secs(20);
            while std::time::Instant::now() < deadline {
                if crate::browser_attach::probe(port).await {
                    break;
                }
                tokio::time::sleep(std::time::Duration::from_millis(500)).await;
            }
            update(&peer.id, |p| {
                if p.status == PeerStatus::NeedsRestart {
                    p.status = PeerStatus::Connected;
                }
                p.follow_up = None;
            });
            refresh(&peer.app, app).await
        }
        other => Err(format!("unknown follow-up \"{}\"", other)),
    }
}

/// Re-check a peer whose permission or restart was pending: if the app
/// is running and Accessibility now works, promote it to connected and
/// fill in the window capabilities the first pass could not probe.
pub async fn refresh(name_or_id: &str, app: Option<&tauri::AppHandle>) -> Result<Peer, String> {
    let peer = find(name_or_id).ok_or_else(|| not_paired(name_or_id))?;
    let Some(pid) = live_pid(&peer).await else {
        return Ok(peer);
    };
    let identity = identify_pid(pid).await?;
    pair(identity, app, PairProgress(&|_| {})).await
}

async fn live_pid(peer: &Peer) -> Option<i64> {
    if let Some(pid) = peer.pid {
        if crate::window_intelligence::running_apps().iter().any(|a| a.pid == pid) {
            return Some(pid);
        }
    }
    crate::window_intelligence::find_running_app(&peer.app)
        .or_else(|| crate::window_intelligence::find_running_app(&peer.process_name))
        .map(|a| a.pid)
}

fn mark_ok(id: &str, pid: Option<i64>) {
    update(id, |p| {
        p.last_ok_at = Some(chrono::Utc::now().to_rfc3339());
        p.last_error = None;
        if pid.is_some() {
            p.pid = pid;
        }
    });
}

fn mark_err(id: &str, e: &str) {
    update(id, |p| p.last_error = Some(e.to_string()));
}

fn not_paired(name: &str) -> String {
    let known: Vec<String> = list().into_iter().map(|p| p.app).collect();
    if known.is_empty() {
        format!("{} is not paired yet. Pair it first: drag the pair handle onto its window, or call pair_app.", name)
    } else {
        format!("{} is not paired. Paired apps: {}. Pair it first with pair_app.", name, known.join(", "))
    }
}

// ─────────────────────────────────────────────
//  Prompt digest + UI events
// ─────────────────────────────────────────────

/// Short block for Bob's system prompt so he knows which apps are paired
/// and how to use them without a tool round-trip.
pub fn prompt_digest() -> String {
    let peers = list();
    if peers.is_empty() {
        return "\nPAIRED APPS: none yet. The user can drag Orbix's pair handle onto any app window, or ask to pair an app (pair_app). Paired apps take peer_send / peer_read.\n".to_string();
    }
    let mut out = String::from("\nPAIRED APPS (use peer_send / peer_read with the app name; pair_app adds more):\n");
    for p in peers {
        let caps: Vec<&str> = p
            .capabilities
            .iter()
            .map(|c| match c {
                Capability::Send => "send",
                Capability::Read => "read",
                Capability::ToolsExposed => "calls Bob's tools",
                Capability::ServiceApi => "API access (api_call with its connector)",
                Capability::Actions => "link actions (peer_open)",
                Capability::Tools => "MCP tools (mcp_tools / mcp_call on its server)",
            })
            .collect();
        let status = match p.status {
            PeerStatus::Connected => "",
            PeerStatus::NeedsRestart => " (needs a restart)",
            PeerStatus::NeedsPermission => " (waiting for Accessibility)",
            PeerStatus::NeedsSignIn => " (API sign-in pending)",
            PeerStatus::Limited => " (clipboard only)",
        };
        out.push_str(&format!(
            "- {}: {}{}\n",
            p.app,
            if caps.is_empty() { "clipboard only".to_string() } else { caps.join(", ") },
            status
        ));
        for a in p.url_actions.iter().take(8) {
            let args: Vec<String> = a.args.iter().map(|x| if x.required { x.name.clone() } else { format!("{}?", x.name) }).collect();
            out.push_str(&format!("    peer_open {} ({}): {}{}\n", a.id, args.join(", "), a.description, if a.verified { "" } else { " [untested]" }));
        }
        if p.url_actions.is_empty() && !p.url_schemes.is_empty() {
            out.push_str(&format!("    (answers {}:// links; add_url_action teaches a command)\n", p.url_schemes.join(":// and ")));
        }
        if p.transports.contains(&Transport::Accessibility) && p.status != PeerStatus::NeedsPermission {
            out.push_str("    window controls: peer_controls lists its buttons and fields; peer_press / peer_fill use them\n");
        }
        if p.chromium && !p.transports.contains(&Transport::DevTools) {
            out.push_str(&format!("    (Chromium app; a DevTools relaunch makes send/read reliable: run_user_action \"{}\")\n", p.app));
        }
    }
    out
}

/// A pairing lifecycle event for the UI: toast plus chip refresh.
pub fn emit_event(app: &tauri::AppHandle, stage: &str, text: &str, peer: Option<&Peer>) {
    let _ = crate::event_bus::emit_ui(
        app,
        "pair-event",
        json!({ "stage": stage, "text": text, "peer": peer }),
    );
    if matches!(stage, "connected" | "failed" | "unpaired" | "updated") {
        let _ = crate::event_bus::emit_ui(app, "data-changed", "peers");
        crate::user_actions::emit_changed(app);
    }
}

/// Bob says one line in the chat, as himself. Used after a pairing so the
/// result lives in the conversation, not only in a toast.
pub fn bob_says(app: &tauri::AppHandle, text: &str) {
    use tauri::Manager;
    let msg = crate::Message {
        id: uuid::Uuid::new_v4().to_string(),
        role: "assistant".to_string(),
        content: text.to_string(),
        context: None,
        agent_id: None,
        timestamp: chrono::Utc::now().to_rfc3339(),
        tool_turns: None,
    };
    if let Some(state) = app.try_state::<crate::AppState>() {
        if let Ok(mut convos) = state.chat.conversations.lock() {
            convos.push(msg.clone());
        }
    }
    let _ = crate::event_bus::emit_ui(app, "new-message", msg);
}

/// The whole flow for a pid the drag overlay (or a tool) resolved:
/// identify, ladder, announce. Errors become a failed event, never a
/// silent drop.
pub async fn pair_pid_and_announce(app: tauri::AppHandle, pid: i64, hint: &str) {
    emit_event(&app, "identifying", &format!("Looking at {}…", if hint.is_empty() { "that app" } else { hint }), None);
    let identity = match identify_pid(pid).await {
        Ok(i) => i,
        Err(e) => {
            emit_event(&app, "failed", &format!("Could not tell which app that is: {}", e), None);
            return;
        }
    };
    let app2 = app.clone();
    let progress = move |t: &str| emit_event(&app2, "connecting", t, None);
    let for_learning = identity.clone();
    match pair(identity, Some(&app), PairProgress(&progress)).await {
        Ok(peer) => {
            emit_event(&app, "connected", &format!("{} connected.", peer.app), Some(&peer));
            let learning = learn_in_background(&app, &peer, &for_learning);
            bob_says(
                &app,
                &format!(
                    "**{} paired.** {}{}",
                    peer.app,
                    peer.bob_line,
                    if learning { " It has a scripting dictionary, so I am reading it now to see if I can go deeper." } else { "" }
                ),
            );
        }
        Err(e) => emit_event(&app, "failed", &e, None),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn builtin_recipes_parse() {
        let recipes = builtin_recipes();
        assert_eq!(recipes.len(), BUILTIN.len());
        let claude = recipes.iter().find(|r| r.app == "Claude").unwrap();
        assert!(claude.mcp.as_ref().unwrap().restart_required);
        assert_eq!(claude.transports[0], Transport::Mcp);
        let chatgpt = recipes.iter().find(|r| r.app == "ChatGPT").unwrap();
        assert!(chatgpt.mcp.as_ref().unwrap().config_path.is_none());
        assert!(chatgpt.mcp.as_ref().unwrap().manual.is_some());
    }

    #[test]
    fn recipe_lookup_by_bundle_then_name() {
        let id = AppIdentity {
            pid: 1,
            name: "Something".into(),
            process_name: "x".into(),
            bundle_id: "com.openai.codex".into(),
            bundle_path: String::new(),
            scriptable: false,
            url_schemes: vec![],
            chromium: false,
            exe_path: String::new(),
        };
        assert_eq!(find_recipe(&id).unwrap().app, "ChatGPT");
        let id = AppIdentity {
            pid: 1,
            name: "claude desktop".into(),
            process_name: "x".into(),
            bundle_id: String::new(),
            bundle_path: String::new(),
            scriptable: false,
            url_schemes: vec![],
            chromium: false,
            exe_path: String::new(),
        };
        assert_eq!(find_recipe(&id).unwrap().app, "Claude");
        let id = AppIdentity {
            pid: 1,
            name: "Numbers".into(),
            process_name: "Numbers".into(),
            bundle_id: "com.apple.iWork.Numbers".into(),
            bundle_path: String::new(),
            scriptable: true,
            url_schemes: vec![],
            chromium: false,
            exe_path: String::new(),
        };
        assert!(find_recipe(&id).is_none());
    }

    #[test]
    fn mcp_register_keeps_other_entries_and_backs_up() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("claude_desktop_config.json");
        std::fs::write(
            &path,
            r#"{"mcpServers":{"filesystem":{"command":"npx","args":["-y","fs"]}},"preferences":{"a":1}}"#,
        )
        .unwrap();
        // A fake, big enough "installed" CLI so cli_command_path resolves.
        let home = dir.path().join("home");
        std::fs::create_dir_all(home.join(".local/bin")).unwrap();
        std::fs::write(home.join(".local/bin/orbix"), vec![0u8; 200_000]).unwrap();
        let cli = home.join(".local/bin/orbix");
        // register_mcp resolves the command through cli_command_path, which
        // looks at the real home dir; exercise the JSON logic directly here.
        let mut config = read_config(&path).unwrap();
        config["mcpServers"]["orbix"] = json!({"command": cli.to_string_lossy(), "args": ["mcp", "--client", "Claude"]});
        write_config(&path, &config).unwrap();
        let after: Value = serde_json::from_str(&std::fs::read_to_string(&path).unwrap()).unwrap();
        assert_eq!(after["mcpServers"]["filesystem"]["command"], "npx");
        assert_eq!(after["mcpServers"]["orbix"]["args"][0], "mcp");
        assert_eq!(after["preferences"]["a"], 1);
        assert!(path.with_extension("json.orbix-backup").exists());
        remove_mcp_entry(&path).unwrap();
        let after: Value = serde_json::from_str(&std::fs::read_to_string(&path).unwrap()).unwrap();
        assert!(after["mcpServers"].get("orbix").is_none());
        assert_eq!(after["mcpServers"]["filesystem"]["command"], "npx");
    }

    #[cfg(unix)]
    #[test]
    fn supports_mcp_rejects_binaries_without_the_subcommand() {
        use std::os::unix::fs::PermissionsExt;
        let dir = tempfile::tempdir().unwrap();
        let good = dir.path().join("orbix-new");
        std::fs::write(&good, "#!/bin/sh\n[ \"$1\" = mcp ] && [ \"$2\" = --client ] && exit 0\nexit 2\n").unwrap();
        std::fs::set_permissions(&good, std::fs::Permissions::from_mode(0o755)).unwrap();
        let old = dir.path().join("orbix-old");
        std::fs::write(&old, "#!/bin/sh\necho \"error: unexpected argument\" >&2\nexit 2\n").unwrap();
        std::fs::set_permissions(&old, std::fs::Permissions::from_mode(0o755)).unwrap();
        assert!(supports_mcp(&good));
        assert!(!supports_mcp(&old));
        assert!(!supports_mcp(&dir.path().join("missing")));
    }

    #[test]
    fn missing_config_file_is_an_empty_object() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("nested/mcp.json");
        assert_eq!(read_config(&path).unwrap(), json!({}));
        write_config(&path, &json!({"mcpServers": {}})).unwrap();
        assert!(path.exists());
    }

    #[test]
    fn bob_line_is_honest_per_rung() {
        let full = compose_line(
            "ChatGPT",
            &[Transport::Accessibility, Transport::Clipboard],
            &[Capability::Read, Capability::Send],
            PeerStatus::Connected,
            None,
            &[],
        );
        assert!(full.contains("type messages into ChatGPT"));
        let clip = compose_line("Numbers", &[Transport::Clipboard], &[], PeerStatus::Limited, None, &[]);
        assert!(clip.contains("copy things for you to paste"));
        let tools = compose_line(
            "Claude",
            &[Transport::Mcp, Transport::Clipboard],
            &[Capability::ToolsExposed],
            PeerStatus::NeedsRestart,
            Some("Claude can now call me.".into()),
            &[],
        );
        assert!(tools.starts_with("Claude can now call me."));
        assert!(tools.contains("needs a restart"));
    }

    #[test]
    fn url_templates_encode_and_check_args() {
        let a = UrlAction {
            id: "open_channel".into(),
            description: "x".into(),
            template: "slack://channel?team={{team}}&id={{channel}}".into(),
            args: vec![
                UrlArg { name: "team".into(), description: "".into(), required: true },
                UrlArg { name: "channel".into(), description: "".into(), required: true },
            ],
            verified: true,
        };
        let url = render_url(&a, &json!({"team": "T1", "channel": "C 2/3"})).unwrap();
        assert_eq!(url, "slack://channel?team=T1&id=C%202%2F3");
        assert!(render_url(&a, &json!({"team": "T1"})).unwrap_err().contains("channel"));
        let b = UrlAction {
            id: "search".into(), description: "".into(), template: "spotify:search:{{query}}".into(),
            args: vec![UrlArg { name: "query".into(), description: "".into(), required: false }], verified: true,
        };
        assert_eq!(render_url(&b, &json!({})).unwrap(), "spotify:search:");
        assert_eq!(app_specific_schemes(vec!["HTTPS".into(), "slack".into(), "slack".into(), "file".into()]), vec!["slack".to_string()]);
    }

    #[test]
    fn devtools_ports_are_stable_and_in_range() {
        let a = devtools_port_for("com.openai.codex");
        assert_eq!(a, devtools_port_for("COM.openai.codex"));
        assert!((9400..9800).contains(&a));
        assert_ne!(a, devtools_port_for("com.anthropic.claudefordesktop"));
        assert!(!has_v8_snapshot(Path::new("/nonexistent"), 0));
    }

    #[test]
    fn digest_mentions_peers_or_the_handle() {
        let d = prompt_digest();
        assert!(d.contains("PAIRED APPS"));
    }
}
