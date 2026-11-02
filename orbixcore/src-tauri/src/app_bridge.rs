//! App-Bridge (FABLE Part A, Tier 1): read desktop apps the user is already
//! logged into through their scripting interface - AppleScript dictionaries
//! on macOS, COM object models driven from PowerShell on Windows. No OAuth,
//! no app registration. Apps are data: each bridge is a JSON manifest
//! declaring a probe plus read actions, with one script per platform
//! (`script` / `windows_script`). Rust stays generic - it validates args,
//! interpolates them safely, runs the script, and parses delimited output
//! into JSON. Built-in manifests are embedded from resources/bridges/;
//! users can add or override bridges by dropping a manifest JSON into
//! <data_dir>/bridges/ - zero Rust changes.

use serde::{Deserialize, Serialize};
use serde_json::{json, Value};

/// Field separator emitted by bridge scripts (AppleScript `character id 31`,
/// PowerShell `$fsep`).
const FIELD_SEP: char = '\u{1f}';
/// Record separator emitted by bridge scripts (AppleScript `character id 30`,
/// PowerShell `$rsep`).
const RECORD_SEP: char = '\u{1e}';

/// The scripting language a bridge script is written in. Each platform
/// runs exactly one; rendering and wrapping take the flavor as a value so
/// both are unit-tested on every host. (Outside tests a host only ever
/// constructs its own variant, hence the allow.)
#[allow(dead_code)]
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Flavor {
    AppleScript,
    PowerShell,
}

#[cfg(target_os = "macos")]
const HOST_FLAVOR: Option<Flavor> = Some(Flavor::AppleScript);
#[cfg(target_os = "windows")]
const HOST_FLAVOR: Option<Flavor> = Some(Flavor::PowerShell);
#[cfg(not(any(target_os = "macos", target_os = "windows")))]
const HOST_FLAVOR: Option<Flavor> = None;

/// Hard cap on raw script output before parsing (chars).
const MAX_RAW_OUTPUT: usize = 200_000;
/// Hard cap on parsed records per action.
const MAX_RECORDS: usize = 500;
/// Probe scripts get this long before we give up (seconds).
const PROBE_TIMEOUT_SECS: u64 = 15;

fn default_timeout() -> u64 {
    30
}

// ─────────────────────────────────────────────
//  Manifest model
// ─────────────────────────────────────────────

/// A script in a manifest: either one string or an array of lines
/// (arrays keep the JSON readable).
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(untagged)]
pub enum Script {
    Lines(Vec<String>),
    One(String),
}

impl Script {
    pub fn text(&self) -> String {
        match self {
            Script::One(s) => s.clone(),
            Script::Lines(v) => v.join("\n"),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum ArgKind {
    Int,
    String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BridgeArg {
    pub name: String,
    pub kind: ArgKind,
    #[serde(default)]
    pub default: Option<Value>,
    #[serde(default)]
    pub min: Option<i64>,
    #[serde(default)]
    pub max: Option<i64>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum ReturnKind {
    Records,
    Lines,
    Text,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BridgeReturns {
    pub kind: ReturnKind,
    #[serde(default)]
    pub columns: Vec<String>,
    /// Sort parsed records by this column before returning.
    #[serde(default)]
    pub sort_column: Option<String>,
    #[serde(default)]
    pub sort_desc: bool,
    /// After sorting, keep only as many records as this int arg resolves
    /// to (for scripts that bulk-fetch and let Rust do the top-N).
    #[serde(default)]
    pub limit_arg: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BridgeAction {
    pub id: String,
    pub description: String,
    /// AppleScript (macOS). An action may exist on one platform only.
    #[serde(default)]
    pub script: Option<Script>,
    /// PowerShell (Windows). Int args substitute as literals; string args
    /// substitute as an expression, so write `$q = {{query}}` unquoted.
    #[serde(default)]
    pub windows_script: Option<Script>,
    #[serde(default)]
    pub args: Vec<BridgeArg>,
    pub returns: BridgeReturns,
    #[serde(default = "default_timeout")]
    pub timeout_secs: u64,
}

impl BridgeAction {
    pub fn script_for(&self, flavor: Flavor) -> Option<&Script> {
        match flavor {
            Flavor::AppleScript => self.script.as_ref(),
            Flavor::PowerShell => self.windows_script.as_ref(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AppBridgeManifest {
    /// Short name Bob uses: "Outlook", "Mail", "Notes".
    pub app: String,
    /// Full app name for install lookup: "Microsoft Outlook".
    pub display_name: String,
    #[serde(default)]
    pub aliases: Vec<String>,
    /// Exact process name as pgrep sees it.
    pub process_name: String,
    #[serde(default)]
    pub bundle_paths: Vec<String>,
    /// Must return "ok" / "ok|detail" when the app's data is reachable,
    /// or "limited|reason" when the app responds but hides its data.
    #[serde(default)]
    pub probe: Option<Script>,
    /// Actionable next step shown when the probe reports limited.
    #[serde(default)]
    pub limited_hint: Option<String>,
    /// Windows: the COM ProgID whose registration means the scriptable
    /// app is installed, e.g. "Outlook.Application".
    #[serde(default)]
    pub windows_prog_id: Option<String>,
    /// Windows: image name as tasklist sees it, e.g. "OUTLOOK.EXE".
    #[serde(default)]
    pub windows_process_name: Option<String>,
    /// Windows: paths (%VAR% expanded) whose presence means a variant of
    /// the app WITHOUT a scripting interface is installed (new Outlook).
    /// Found without the ProgID, the bridge reports limited, not missing.
    #[serde(default)]
    pub windows_limited_paths: Vec<String>,
    /// Windows: PowerShell run before the probe and every action (shared
    /// COM setup and helper functions).
    #[serde(default)]
    pub windows_prelude: Option<Script>,
    /// Windows: same contract as `probe`.
    #[serde(default)]
    pub windows_probe: Option<Script>,
    #[serde(default)]
    pub windows_limited_hint: Option<String>,
    /// Web-app URL Bob can offer via browse_navigate as a fallback.
    #[serde(default)]
    pub web_url: Option<String>,
    /// Written by bridge_author (Bob learned it) rather than shipped or
    /// hand-made. Learned bridges get re-learned when they stop working.
    #[serde(default)]
    pub learned: bool,
    pub actions: Vec<BridgeAction>,
}

impl AppBridgeManifest {
    pub fn probe_for(&self, flavor: Flavor) -> Option<&Script> {
        match flavor {
            Flavor::AppleScript => self.probe.as_ref(),
            Flavor::PowerShell => self.windows_probe.as_ref(),
        }
    }

    /// A bridge exists on a platform when it has a probe and at least one
    /// action written in that platform's flavor.
    pub fn supports(&self, flavor: Flavor) -> bool {
        self.probe_for(flavor).is_some()
            && self.actions.iter().any(|a| a.script_for(flavor).is_some())
    }

    fn prelude_for(&self, flavor: Flavor) -> String {
        match flavor {
            Flavor::AppleScript => String::new(),
            Flavor::PowerShell => self.windows_prelude.as_ref().map(|s| s.text()).unwrap_or_default(),
        }
    }

    fn limited_hint_for(&self, flavor: Flavor) -> Option<String> {
        match flavor {
            Flavor::AppleScript => self.limited_hint.clone(),
            Flavor::PowerShell => self
                .windows_limited_hint
                .clone()
                .or_else(|| self.limited_hint.clone()),
        }
    }

    /// (id, description) of the actions this platform can run.
    fn host_actions(&self) -> Vec<(String, String)> {
        self.actions
            .iter()
            .filter(|a| HOST_FLAVOR.map(|f| a.script_for(f).is_some()).unwrap_or(false))
            .map(|a| (a.id.clone(), a.description.clone()))
            .collect()
    }
}

// ─────────────────────────────────────────────
//  Manifest loading (embedded + user dir)
// ─────────────────────────────────────────────

const BUILTIN: &[&str] = &[
    include_str!("../resources/bridges/outlook.json"),
    include_str!("../resources/bridges/mail.json"),
    include_str!("../resources/bridges/calendar.json"),
    include_str!("../resources/bridges/notes.json"),
    include_str!("../resources/bridges/reminders.json"),
    include_str!("../resources/bridges/messages.json"),
];

pub fn builtin_manifests() -> Vec<AppBridgeManifest> {
    BUILTIN
        .iter()
        .filter_map(|raw| match serde_json::from_str(raw) {
            Ok(m) => Some(m),
            Err(e) => {
                log::error!("[app_bridge] embedded manifest failed to parse: {}", e);
                None
            }
        })
        .collect()
}

/// User bridges live in <data_dir>/bridges/*.json and override embedded
/// ones with the same app name, so a manifest fix never needs a rebuild.
fn user_manifests() -> Vec<AppBridgeManifest> {
    let dir = crate::storage::data_dir().join("bridges");
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
            .and_then(|raw| serde_json::from_str::<AppBridgeManifest>(&raw).map_err(|e| e.to_string()))
        {
            Ok(m) => out.push(m),
            Err(e) => log::warn!("[app_bridge] skipping {}: {}", path.display(), e),
        }
    }
    out
}

/// Every bridge this platform can run: built-ins plus user overrides,
/// minus manifests with no script in the host's flavor.
pub fn all_manifests() -> Vec<AppBridgeManifest> {
    let Some(flavor) = HOST_FLAVOR else {
        return Vec::new();
    };
    let mut manifests = builtin_manifests();
    for user in user_manifests() {
        if let Some(existing) = manifests
            .iter_mut()
            .find(|m| m.app.eq_ignore_ascii_case(&user.app))
        {
            *existing = user;
        } else {
            manifests.push(user);
        }
    }
    manifests.retain(|m| m.supports(flavor));
    manifests
}

pub fn find_manifest(name: &str) -> Option<AppBridgeManifest> {
    let needle = name.trim().to_lowercase();
    all_manifests().into_iter().find(|m| {
        m.app.to_lowercase() == needle
            || m.display_name.to_lowercase() == needle
            || m.aliases.iter().any(|a| a.to_lowercase() == needle)
    })
}

// ─────────────────────────────────────────────
//  App state checks (no TCC prompts, no app launches)
// ─────────────────────────────────────────────

#[derive(Debug, Clone, Copy, PartialEq)]
enum InstallState {
    /// The scriptable app is present.
    Scriptable,
    /// Only a variant without a scripting interface is present (Windows:
    /// the new Outlook without classic Outlook).
    #[cfg_attr(not(target_os = "windows"), allow(dead_code))]
    LimitedOnly,
    Missing,
}

#[cfg(target_os = "macos")]
fn install_state(m: &AppBridgeManifest) -> InstallState {
    if installed_path(m).is_some() {
        InstallState::Scriptable
    } else {
        InstallState::Missing
    }
}

#[cfg(target_os = "windows")]
fn install_state(m: &AppBridgeManifest) -> InstallState {
    if m.windows_prog_id.as_deref().map(prog_id_registered).unwrap_or(false) {
        return InstallState::Scriptable;
    }
    // symlink_metadata, not exists(): the WindowsApps entries are app
    // execution aliases, reparse points that exists() cannot follow.
    let limited = m
        .windows_limited_paths
        .iter()
        .any(|p| std::fs::symlink_metadata(expand_env_vars(p)).is_ok());
    if limited {
        InstallState::LimitedOnly
    } else {
        InstallState::Missing
    }
}

#[cfg(not(any(target_os = "macos", target_os = "windows")))]
fn install_state(_m: &AppBridgeManifest) -> InstallState {
    InstallState::Missing
}

/// Whether the app_bridge tools are worth advertising to the model. macOS
/// always has the tier (its bridged apps ship with the OS). Elsewhere it
/// exists only while an app with a scripting interface is installed: a
/// Windows PC with nothing but the new Outlook has no bridge to offer,
/// and offering one anyway is a dead path for the model.
pub fn tier_available() -> bool {
    if cfg!(target_os = "macos") {
        return true;
    }
    all_manifests()
        .iter()
        .any(|m| install_state(m) == InstallState::Scriptable)
}

/// True when a COM ProgID resolves to a class. A registry lookup only:
/// nothing is instantiated and no app starts.
#[cfg(target_os = "windows")]
fn prog_id_registered(prog_id: &str) -> bool {
    let wide = windows::core::HSTRING::from(prog_id);
    unsafe { windows::Win32::System::Com::CLSIDFromProgID(&wide).is_ok() }
}

/// Expand %VAR% references; unknown variables are left as written.
#[cfg_attr(not(target_os = "windows"), allow(dead_code))]
fn expand_env_vars(input: &str) -> String {
    let mut out = String::new();
    let mut rest = input;
    while let Some(start) = rest.find('%') {
        let Some(len) = rest[start + 1..].find('%') else { break };
        let name = &rest[start + 1..start + 1 + len];
        out.push_str(&rest[..start]);
        match std::env::var(name) {
            Ok(v) if !name.is_empty() => out.push_str(&v),
            _ => out.push_str(&rest[start..start + len + 2]),
        }
        rest = &rest[start + len + 2..];
    }
    out.push_str(rest);
    out
}

const NO_SCRIPTING_DETAIL: &str = "only a version without a scripting interface is installed";

#[cfg_attr(not(target_os = "macos"), allow(dead_code))]
fn installed_path(m: &AppBridgeManifest) -> Option<String> {
    for p in &m.bundle_paths {
        if std::path::Path::new(p).exists() {
            return Some(p.clone());
        }
    }
    // Fall back to the standard app dirs by display name.
    for dir in ["/Applications", "/System/Applications"] {
        let candidate = format!("{}/{}.app", dir, m.display_name);
        if std::path::Path::new(&candidate).exists() {
            return Some(candidate);
        }
    }
    if let Some(home) = dirs::home_dir() {
        let candidate = home.join(format!("Applications/{}.app", m.display_name));
        if candidate.exists() {
            return Some(candidate.to_string_lossy().to_string());
        }
    }
    None
}

#[cfg(not(target_os = "windows"))]
async fn is_running(m: &AppBridgeManifest) -> bool {
    tokio::process::Command::new("pgrep")
        .args(["-x", &m.process_name])
        .output()
        .await
        .map(|o| o.status.success())
        .unwrap_or(false)
}

#[cfg(target_os = "windows")]
async fn is_running(m: &AppBridgeManifest) -> bool {
    let Some(image) = m.windows_process_name.as_deref() else {
        return false;
    };
    let filter = format!("IMAGENAME eq {}", image);
    crate::process_ext::tokio_command("tasklist")
        .args(["/FI", &filter, "/NH", "/FO", "CSV"])
        .output()
        .await
        .map(|o| {
            String::from_utf8_lossy(&o.stdout)
                .to_lowercase()
                .contains(&image.to_lowercase())
        })
        .unwrap_or(false)
}

// ─────────────────────────────────────────────
//  Probe
// ─────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum ProbeStatus {
    /// Installed, running, and the probe returned ok.
    Ready,
    /// The app responds to scripting but hides its data (e.g. new Outlook).
    Limited,
    /// Installed but not running; using it will open the app.
    NotRunning,
    NotInstalled,
    /// The probe script itself failed.
    Error,
}

#[derive(Debug, Clone, Serialize)]
pub struct BridgeStatus {
    pub app: String,
    pub display_name: String,
    pub status: ProbeStatus,
    pub detail: String,
    pub actions: Vec<(String, String)>,
}

/// Wrap a manifest script with the separator variables every bridge
/// script can use (`fsep`, `rsep`) and the platform's guard rails.
/// AppleScript gets its own timeout block. PowerShell gets UTF-8 stdout
/// (the default is the OEM codepage, which mangles non-ASCII subjects), a
/// `Clean` helper that keeps separators and newlines out of field values,
/// the manifest's prelude, and a catch that turns any failure into one
/// plain stderr line instead of a CLIXML blob.
fn wrap_script(flavor: Flavor, prelude: &str, script: &str, timeout_secs: u64) -> String {
    match flavor {
        Flavor::AppleScript => format!(
            "with timeout of {} seconds\n\
             set fsep to character id 31\n\
             set rsep to character id 30\n\
             {}\n\
             end timeout",
            timeout_secs, script
        ),
        Flavor::PowerShell => format!(
            "$ErrorActionPreference = 'Stop'\n\
             $ProgressPreference = 'SilentlyContinue'\n\
             [Console]::OutputEncoding = New-Object System.Text.UTF8Encoding $false\n\
             $fsep = [string][char]31\n\
             $rsep = [string][char]30\n\
             function Clean($v) {{ if ($null -eq $v) {{ return '' }}; return ([string]$v).Replace($fsep, ' ').Replace($rsep, ' ').Replace(\"`r\", ' ').Replace(\"`n\", ' ') }}\n\
             try {{\n\
             {}\n\
             {}\n\
             }} catch {{\n\
             [Console]::Error.Write($_.Exception.Message)\n\
             exit 1\n\
             }}",
            prelude, script
        ),
    }
}

/// Run a wrapped script with the host's interpreter.
async fn run_host_script(wrapped: &str) -> Result<String, String> {
    #[cfg(target_os = "macos")]
    {
        crate::os_auto::run_applescript_raw(wrapped).await
    }
    #[cfg(target_os = "windows")]
    {
        crate::os_auto::run_powershell_raw(wrapped)
            .await
            .map(|out| out.trim_start_matches('\u{feff}').to_string())
    }
    #[cfg(not(any(target_os = "macos", target_os = "windows")))]
    {
        let _ = wrapped;
        Err("App bridges are not available on this platform yet.".to_string())
    }
}

async fn run_wrapped(m: &AppBridgeManifest, script: &str, timeout_secs: u64) -> Result<String, String> {
    let flavor = HOST_FLAVOR.ok_or("App bridges are not available on this platform yet.")?;
    let wrapped = wrap_script(flavor, &m.prelude_for(flavor), script, timeout_secs);
    let fut = run_host_script(&wrapped);
    match tokio::time::timeout(std::time::Duration::from_secs(timeout_secs + 5), fut).await {
        Ok(result) => result,
        Err(_) => Err(format!(
            "script did not finish within {} seconds (the app may be showing a dialog)",
            timeout_secs
        )),
    }
}

/// Run a manifest's probe against a running app. Returns (status, detail).
async fn run_probe(m: &AppBridgeManifest) -> (ProbeStatus, String) {
    let Some(probe) = HOST_FLAVOR.and_then(|f| m.probe_for(f)) else {
        return (ProbeStatus::Error, "no probe for this platform".to_string());
    };
    match run_wrapped(m, &probe.text(), PROBE_TIMEOUT_SECS).await {
        Ok(out) => {
            let out = out.trim();
            if let Some(reason) = out.strip_prefix("limited") {
                let reason = reason.trim_start_matches('|').trim();
                (ProbeStatus::Limited, reason.to_string())
            } else if out.starts_with("ok") {
                let detail = out.strip_prefix("ok").unwrap_or("").trim_start_matches('|').trim();
                (ProbeStatus::Ready, detail.to_string())
            } else {
                (ProbeStatus::Ready, out.to_string())
            }
        }
        Err(e) => (ProbeStatus::Error, e),
    }
}

/// Status of every known bridge. Only probes apps that are already
/// running - listing bridges must never launch anything.
pub async fn list_bridges() -> Vec<BridgeStatus> {
    let mut out = Vec::new();
    for m in all_manifests() {
        let actions = m.host_actions();
        let (status, detail) = match install_state(&m) {
            InstallState::Missing => (ProbeStatus::NotInstalled, String::new()),
            InstallState::LimitedOnly => (ProbeStatus::Limited, NO_SCRIPTING_DETAIL.to_string()),
            InstallState::Scriptable => {
                if !is_running(&m).await {
                    (ProbeStatus::NotRunning, "will open on use".to_string())
                } else {
                    run_probe(&m).await
                }
            }
        };
        out.push(BridgeStatus {
            app: m.app,
            display_name: m.display_name,
            status,
            detail,
            actions,
        });
    }
    out
}

/// One line per bridge, model- and human-readable.
pub fn format_bridge_list(statuses: &[BridgeStatus]) -> String {
    let mut lines = vec!["**App bridges (read apps directly, no login setup):**".to_string()];
    for s in statuses {
        let state = match s.status {
            ProbeStatus::Ready => format!("ready ({})", s.detail),
            ProbeStatus::Limited => format!("limited: {}", s.detail),
            ProbeStatus::NotRunning => "not running (will open on use)".to_string(),
            ProbeStatus::NotInstalled => "not installed".to_string(),
            ProbeStatus::Error => format!("error: {}", s.detail),
        };
        let actions = s
            .actions
            .iter()
            .map(|(id, _)| id.as_str())
            .collect::<Vec<_>>()
            .join(", ");
        lines.push(format!("- {}: {} - actions: {}", s.app, state, actions));
    }
    lines.join("\n")
}

// ─────────────────────────────────────────────
//  Arg resolution + interpolation
// ─────────────────────────────────────────────

/// A caller string as a PowerShell expression. The value travels as
/// base64 and is decoded at run time, so no quoting rule (PowerShell also
/// treats the typographic quotes as string delimiters) lets it end the
/// literal. Manifests use it unquoted: `$q = {{query}}`.
fn powershell_string_expr(value: &str) -> String {
    use base64::Engine;
    format!(
        "([Text.Encoding]::UTF8.GetString([Convert]::FromBase64String('{}')))",
        base64::engine::general_purpose::STANDARD.encode(value.as_bytes())
    )
}

/// Resolve declared args from the caller's JSON, apply defaults and
/// bounds, and substitute {{name}} placeholders. Every substituted value
/// is either a validated integer or a string encoded for the flavor
/// (AppleScript-escaped, or a base64 expression for PowerShell), so
/// caller input can never break out of the script.
fn render_script(
    flavor: Flavor,
    action: &BridgeAction,
    args: &Value,
) -> Result<(String, Vec<(String, Value)>), String> {
    let mut script = action
        .script_for(flavor)
        .ok_or_else(|| format!("action \"{}\" is not available on this platform", action.id))?
        .text();
    let mut resolved = Vec::new();
    for spec in &action.args {
        let raw = args.get(&spec.name).cloned().or_else(|| spec.default.clone());
        let placeholder = format!("{{{{{}}}}}", spec.name);
        match spec.kind {
            ArgKind::Int => {
                let v = match &raw {
                    Some(Value::Number(n)) => n.as_i64(),
                    Some(Value::String(s)) => s.trim().parse::<i64>().ok(),
                    _ => None,
                }
                .ok_or_else(|| format!("arg \"{}\" must be an integer", spec.name))?;
                let v = v
                    .max(spec.min.unwrap_or(i64::MIN))
                    .min(spec.max.unwrap_or(i64::MAX));
                script = script.replace(&placeholder, &v.to_string());
                resolved.push((spec.name.clone(), json!(v)));
            }
            ArgKind::String => {
                let v = match &raw {
                    Some(Value::String(s)) => s.clone(),
                    Some(other) => other.to_string(),
                    None => return Err(format!("arg \"{}\" is required", spec.name)),
                };
                let encoded = match flavor {
                    Flavor::AppleScript => crate::os_auto::escape_applescript_string(&v),
                    Flavor::PowerShell => powershell_string_expr(&v),
                };
                script = script.replace(&placeholder, &encoded);
                resolved.push((spec.name.clone(), json!(v)));
            }
        }
    }
    if script.contains("{{") {
        return Err("script has unresolved placeholders; check the manifest's args list".to_string());
    }
    Ok((script, resolved))
}

// ─────────────────────────────────────────────
//  Output parsing
// ─────────────────────────────────────────────

fn parse_output(raw: &str, returns: &BridgeReturns, resolved: &[(String, Value)]) -> Value {
    let mut raw = raw;
    let truncated = raw.chars().count() > MAX_RAW_OUTPUT;
    let owned;
    if truncated {
        owned = raw.chars().take(MAX_RAW_OUTPUT).collect::<String>();
        raw = &owned;
    }
    match returns.kind {
        ReturnKind::Text => json!({ "text": raw.trim() }),
        ReturnKind::Lines => {
            // Scripts emit one item per line, or per record separator: a
            // drafted script often joins with rsep, and both read the same.
            let lines: Vec<&str> = raw
                .split(|c| c == '\n' || c == RECORD_SEP)
                .map(|l| l.trim_matches(['\r', ' ', '\t']))
                .filter(|l| !l.is_empty())
                .take(MAX_RECORDS)
                .collect();
            json!({ "count": lines.len(), "lines": lines })
        }
        ReturnKind::Records => {
            let mut records: Vec<Value> = raw
                .split(RECORD_SEP)
                .map(|r| r.trim_matches(['\n', '\r']))
                .filter(|r| !r.is_empty())
                .take(MAX_RECORDS)
                .map(|rec| {
                    let mut obj = serde_json::Map::new();
                    let mut fields = rec.split(FIELD_SEP);
                    for col in &returns.columns {
                        let v = fields.next().unwrap_or("").trim();
                        obj.insert(col.clone(), json!(v));
                    }
                    Value::Object(obj)
                })
                .collect();
            if let Some(col) = &returns.sort_column {
                records.sort_by(|a, b| {
                    let av = a.get(col).and_then(|v| v.as_str()).unwrap_or("");
                    let bv = b.get(col).and_then(|v| v.as_str()).unwrap_or("");
                    if returns.sort_desc { bv.cmp(av) } else { av.cmp(bv) }
                });
            }
            if let Some(arg_name) = &returns.limit_arg {
                if let Some((_, v)) = resolved.iter().find(|(n, _)| n == arg_name) {
                    if let Some(n) = v.as_i64() {
                        records.truncate(n.max(0) as usize);
                    }
                }
            }
            let mut envelope = serde_json::Map::new();
            envelope.insert("count".to_string(), json!(records.len()));
            envelope.insert("items".to_string(), Value::Array(records));
            if truncated {
                envelope.insert("truncated".to_string(), json!(true));
            }
            Value::Object(envelope)
        }
    }
}

// ─────────────────────────────────────────────
//  Automation (Apple Events) permission (FABLE A4)
// ─────────────────────────────────────────────

/// FFI into the AppleEvents permission API - the only way to read the
/// Automation TCC state without firing the consent prompt.
#[cfg(target_os = "macos")]
mod ae_permission {
    use std::os::raw::c_void;

    #[repr(C)]
    struct AEDesc {
        descriptor_type: u32,
        data_handle: *mut c_void,
    }

    const TYPE_APPLICATION_BUNDLE_ID: u32 = u32::from_be_bytes(*b"bund");
    const TYPE_WILD_CARD: u32 = u32::from_be_bytes(*b"****");

    #[link(name = "CoreServices", kind = "framework")]
    extern "C" {
        fn AECreateDesc(
            type_code: u32,
            data_ptr: *const c_void,
            data_size: isize,
            result: *mut AEDesc,
        ) -> i16;
        fn AEDisposeDesc(desc: *mut AEDesc) -> i16;
        fn AEDeterminePermissionToAutomateTarget(
            target: *const AEDesc,
            event_class: u32,
            event_id: u32,
            ask_user_if_needed: bool,
        ) -> i32;
    }

    /// 0 = granted; -1743 denied; -1744 undetermined; -600 target not
    /// running. With ask=true this blocks on the OS consent prompt.
    pub fn determine(bundle_id: &str, ask: bool) -> i32 {
        unsafe {
            let mut desc = AEDesc {
                descriptor_type: 0,
                data_handle: std::ptr::null_mut(),
            };
            let bytes = bundle_id.as_bytes();
            let err = AECreateDesc(
                TYPE_APPLICATION_BUNDLE_ID,
                bytes.as_ptr() as *const c_void,
                bytes.len() as isize,
                &mut desc,
            );
            if err != 0 {
                return err as i32;
            }
            let status =
                AEDeterminePermissionToAutomateTarget(&desc, TYPE_WILD_CARD, TYPE_WILD_CARD, ask);
            AEDisposeDesc(&mut desc);
            status
        }
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct AutomationStatus {
    pub granted: bool,
    /// granted | denied | undetermined | unknown
    pub state: String,
    pub message: String,
}

/// Check (ask=false) or request (ask=true) the macOS Automation
/// permission against System Events, the scripting broker every bridge
/// relies on. Individual apps still get their own one-time OS prompt on
/// first read - that is the whole setup cost of the Tier-1 path.
pub async fn automation_status(ask: bool) -> AutomationStatus {
    #[cfg(target_os = "macos")]
    {
        // Give the permission check a live target; -g -j keeps System
        // Events in the background without stealing focus.
        let _ = tokio::process::Command::new("open")
            .args(["-g", "-j", "-a", "System Events"])
            .output()
            .await;
        let code = tokio::task::spawn_blocking(move || {
            ae_permission::determine("com.apple.systemevents", ask)
        })
        .await
        .unwrap_or(i32::MIN);
        let (granted, state, message) = match code {
            0 => (
                true,
                "granted",
                "Automation is on. macOS asks once per app the first time Bob reads it.",
            ),
            -1743 => (
                false,
                "denied",
                "Automation is off. Enable Orbix Core under System Settings > Privacy & Security > Automation.",
            ),
            -1744 => (
                false,
                "undetermined",
                "Not asked yet. macOS will show one prompt the first time Bob reads an app.",
            ),
            _ => (
                false,
                "unknown",
                "Could not check right now. Bob will ask the system when he first reads an app.",
            ),
        };
        AutomationStatus {
            granted,
            state: state.to_string(),
            message: message.to_string(),
        }
    }
    // Windows has no per-app automation consent: COM scripting works for
    // any process running as the same user at the same elevation.
    #[cfg(target_os = "windows")]
    {
        let _ = ask;
        AutomationStatus {
            granted: true,
            state: "granted".to_string(),
            message: "Windows needs no automation grant. Bob reads classic Outlook through its scripting interface.".to_string(),
        }
    }
    #[cfg(not(any(target_os = "macos", target_os = "windows")))]
    {
        let _ = ask;
        AutomationStatus {
            granted: false,
            state: "unknown".to_string(),
            message: "App bridges are not available on this platform yet.".to_string(),
        }
    }
}

// ─────────────────────────────────────────────
//  Capability digest (FABLE A3 - tier routing without a round-trip)
// ─────────────────────────────────────────────

static DIGEST_CACHE: std::sync::Mutex<Option<(std::time::Instant, String)>> =
    std::sync::Mutex::new(None);
const DIGEST_TTL: std::time::Duration = std::time::Duration::from_secs(30);

/// Short digest of the app-access surface, injected into the automation
/// system prompt so the model picks the right tier (app_bridge >
/// read_app_window > browse_read > OAuth) without a tool round-trip.
/// Cheap checks only (fs + pgrep + one System Events call), cached for
/// 30 seconds; never launches or probes an app.
pub async fn capability_digest() -> String {
    if let Some((at, cached)) = DIGEST_CACHE
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .clone()
    {
        if at.elapsed() < DIGEST_TTL {
            return cached;
        }
    }
    let digest = build_digest().await;
    *DIGEST_CACHE.lock().unwrap_or_else(|e| e.into_inner()) = Some((std::time::Instant::now(), digest.clone()));
    digest
}

async fn build_digest() -> String {
    let mut bridged = Vec::new();
    let mut unscriptable = Vec::new();
    for m in all_manifests() {
        match install_state(&m) {
            InstallState::Missing => continue,
            InstallState::LimitedOnly => {
                unscriptable.push(m.app.clone());
                continue;
            }
            InstallState::Scriptable => {}
        }
        if is_running(&m).await {
            bridged.push(format!("{} (running)", m.app));
        } else {
            bridged.push(format!("{} (closed, opens on use)", m.app));
        }
    }
    let bridged_names: Vec<String> = all_manifests()
        .iter()
        .flat_map(|m| {
            [
                m.app.to_lowercase(),
                m.display_name.to_lowercase(),
                m.process_name.to_lowercase(),
            ]
        })
        .collect();
    let running_other = match crate::os_auto::list_running_apps().await {
        Ok(apps) => apps
            .into_iter()
            .map(|a| a.name)
            .filter(|n| !n.is_empty() && !bridged_names.contains(&n.to_lowercase()))
            .take(20)
            .collect::<Vec<_>>()
            .join(", "),
        Err(_) => String::new(),
    };
    let mut out = format!(
        "\nAPP ACCESS (pick the cheapest tier that answers):\n\
         Bridged apps (app_bridge, reads the logged-in app, no setup): {}\n",
        if bridged.is_empty() { "(none installed)".to_string() } else { bridged.join("; ") }
    );
    if !unscriptable.is_empty() {
        out.push_str(&format!(
            "Installed without a scripting interface (app_bridge cannot read them; use the connected-account tools or read_app_window): {}\n",
            unscriptable.join(", ")
        ));
    }
    if !running_other.is_empty() {
        out.push_str(&format!(
            "Other running apps (read_app_window reads their window): {}\n",
            running_other
        ));
    }
    out.push_str("Web pages: browse_read. OAuth connectors: last resort, for sending and background jobs.\n");
    out
}

/// One line per installed bridge with its exact action ids and args, for
/// prompts that must not guess: "Calendar: upcoming_events(days), ...".
/// Installed check only; never probes or launches an app.
pub fn action_vocabulary() -> String {
    let mut lines = Vec::new();
    for m in all_manifests() {
        if install_state(&m) == InstallState::Missing {
            continue;
        }
        let actions: Vec<String> = m
            .actions
            .iter()
            .filter(|a| HOST_FLAVOR.map(|f| a.script_for(f).is_some()).unwrap_or(false))
            .map(|a| {
                let args: Vec<String> = a.args.iter().map(|x| x.name.clone()).collect();
                if args.is_empty() { a.id.clone() } else { format!("{}({})", a.id, args.join(", ")) }
            })
            .collect();
        if !actions.is_empty() {
            lines.push(format!("- {}: {}", m.app, actions.join(", ")));
        }
    }
    if lines.is_empty() {
        String::new()
    } else {
        format!("APP BRIDGE ACTIONS (app_bridge tool_args {{app, action, args}}; use these ids exactly):\n{}\n", lines.join("\n"))
    }
}

/// Does this bridge action exist? Some(valid ids) when the app has a
/// bridge but not that action; None when either is fine or unknown app.
pub fn unknown_action(app: &str, action: &str) -> Option<Vec<String>> {
    let m = find_manifest(app)?;
    if m.actions.iter().any(|a| a.id.eq_ignore_ascii_case(action.trim())) {
        return None;
    }
    Some(m.actions.iter().map(|a| a.id.clone()).collect())
}

// ─────────────────────────────────────────────
//  Authoring support (bridge_author.rs): smoke test + user manifests
// ─────────────────────────────────────────────

#[derive(Debug, Clone, Serialize)]
pub struct ActionSmoke {
    pub id: String,
    pub ok: bool,
    /// The parsed result's shape when ok, else the error.
    pub detail: String,
}

/// Run a manifest that is not saved yet against the live app: the probe
/// must come back ready, then every action runs with its default args.
/// Actions whose string args have no default are skipped, not failed.
pub async fn smoke_test(m: &AppBridgeManifest) -> Result<Vec<ActionSmoke>, String> {
    if HOST_FLAVOR.is_none() {
        return Err("App bridges are not available on this platform yet.".to_string());
    }
    let (status, detail) = run_probe(m).await;
    match status {
        ProbeStatus::Ready => {}
        ProbeStatus::Limited => return Err(format!("probe reported limited: {}", detail)),
        _ => return Err(format!("probe failed: {}", detail)),
    }
    let flavor = HOST_FLAVOR.unwrap();
    let mut out = Vec::new();
    for action in m.actions.iter().take(8) {
        if action.args.iter().any(|a| a.kind == ArgKind::String && a.default.is_none()) {
            out.push(ActionSmoke { id: action.id.clone(), ok: true, detail: "skipped: needs a string argument".to_string() });
            continue;
        }
        let smoke = match render_script(flavor, action, &json!({})) {
            Err(e) => ActionSmoke { id: action.id.clone(), ok: false, detail: e },
            Ok((script, resolved)) => match run_wrapped(m, &script, action.timeout_secs.min(30)).await {
                Ok(raw) => {
                    let parsed = parse_output(&raw, &action.returns, &resolved);
                    let shape = match action.returns.kind {
                        ReturnKind::Records => format!("{} records", parsed["count"].as_u64().unwrap_or(0)),
                        ReturnKind::Lines => format!("{} lines", parsed["count"].as_u64().unwrap_or(0)),
                        ReturnKind::Text => format!("{} chars", parsed["text"].as_str().map(|t| t.len()).unwrap_or(0)),
                    };
                    ActionSmoke { id: action.id.clone(), ok: true, detail: shape }
                }
                Err(e) => ActionSmoke { id: action.id.clone(), ok: false, detail: e },
            },
        };
        out.push(smoke);
    }
    Ok(out)
}

/// Is the bridge for this app one Bob authored himself?
pub fn is_learned(app: &str) -> bool {
    find_manifest(app).map(|m| m.learned).unwrap_or(false)
}

/// Run a manifest's probe against the running app: Ok(detail) when the
/// app answers with real data, Err(reason) otherwise.
pub async fn probe_manifest(m: &AppBridgeManifest) -> Result<String, String> {
    match run_probe(m).await {
        (ProbeStatus::Ready, detail) => Ok(detail),
        (ProbeStatus::Limited, detail) => Err(format!("limited: {}", detail)),
        (_, detail) => Err(detail),
    }
}

/// Save a manifest into the user bridges dir, where all_manifests() finds
/// it on the next call. Returns the path.
pub fn save_user_manifest(m: &AppBridgeManifest) -> Result<std::path::PathBuf, String> {
    let dir = crate::storage::data_dir().join("bridges");
    std::fs::create_dir_all(&dir).map_err(|e| format!("create {}: {}", dir.display(), e))?;
    let slug: String = m
        .app
        .to_lowercase()
        .chars()
        .map(|c| if c.is_ascii_alphanumeric() { c } else { '-' })
        .collect::<String>()
        .trim_matches('-')
        .to_string();
    let path = dir.join(format!("{}.json", if slug.is_empty() { "app" } else { &slug }));
    let pretty = serde_json::to_string_pretty(m).map_err(|e| e.to_string())?;
    crate::storage::atomic_write(&path, pretty).map_err(|e| format!("write {}: {}", path.display(), e))?;
    Ok(path)
}

// ─────────────────────────────────────────────
//  The bridge entry point
// ─────────────────────────────────────────────

/// Run one manifest action against its app. Probes first (installed +
/// scriptable + data reachable), then executes and parses. Every failure
/// mode returns an actionable message, never a crash.
pub async fn run_bridge_action(app: &str, action_id: &str, args: &Value) -> Result<Value, String> {
    let manifest = find_manifest(app).ok_or_else(|| {
        let known = all_manifests()
            .iter()
            .map(|m| m.app.clone())
            .collect::<Vec<_>>()
            .join(", ");
        format!(
            "no app bridge for \"{}\". Bridges exist for: {}. For other apps, read the window with read_app_window.",
            app, known
        )
    })?;

    let flavor = HOST_FLAVOR.ok_or("App bridges are not available on this platform yet.")?;
    let installed = install_state(&manifest);
    if installed == InstallState::Missing {
        let mut msg = format!("{} is not installed on this device.", manifest.display_name);
        if let Some(url) = &manifest.web_url {
            msg.push_str(&format!(
                " The web version at {} works instead - offer to open it with browse_navigate.",
                url
            ));
        }
        return Err(msg);
    }

    // Graph first for Outlook: a connected Microsoft account reads the
    // REAL mailbox and work calendar (new Outlook hides both from
    // scripting, on macOS and on Windows) and needs no Automation grant.
    // The local script remains the no-account fallback.
    if manifest.app == "Outlook" {
        if let Ok(token) = crate::graph_outlook::access_token().await {
            match crate::graph_outlook::bridge_action(&token, action_id, args).await {
                Ok(Some(v)) => return Ok(v),
                Ok(None) => {}
                Err(e) => {
                    eprintln!("[app_bridge] graph outlook failed, falling back to the local script: {}", e);
                }
            }
        }
    }

    let action = manifest
        .actions
        .iter()
        .find(|a| a.id.eq_ignore_ascii_case(action_id) && a.script_for(flavor).is_some())
        .ok_or_else(|| {
            let available = manifest
                .host_actions()
                .iter()
                .map(|(id, description)| format!("{} ({})", id, description))
                .collect::<Vec<_>>()
                .join("; ");
            format!(
                "\"{}\" is not an action of the {} bridge. Available: {}",
                action_id, manifest.app, available
            )
        })?;

    let (status, detail) = if installed == InstallState::LimitedOnly {
        (ProbeStatus::Limited, NO_SCRIPTING_DETAIL.to_string())
    } else {
        run_probe(&manifest).await
    };
    match status {
        ProbeStatus::Ready => {}
        ProbeStatus::Limited => {
            let hint = manifest
                .limited_hint_for(flavor)
                .unwrap_or_else(|| "Try read_app_window on its open window instead.".to_string());
            let mut msg = if installed == InstallState::LimitedOnly {
                format!(
                    "{} cannot be read through scripting on this device ({}). {}",
                    manifest.display_name, detail, hint
                )
            } else {
                format!(
                    "{} responds to scripting but its data is out of reach ({}). {}",
                    manifest.display_name, detail, hint
                )
            };
            if let Some(url) = &manifest.web_url {
                msg.push_str(&format!(" Web version: {}", url));
            }
            return Err(msg);
        }
        _ => {
            return Err(format!(
                "{} is not scriptable right now: {}",
                manifest.display_name, detail
            ));
        }
    }

    let (script, resolved) = render_script(flavor, action, args)?;
    let raw = run_wrapped(&manifest, &script, action.timeout_secs).await.map_err(|e| {
        // A -1743 here is a macOS PERMISSION problem, never a credentials
        // problem - say so in words a model will relay instead of
        // hallucinating an API-key request (field failure 2026-08-03).
        if e.contains("-1743") || e.contains("Not authorized") || e.contains("not allowed") {
            format!(
                "macOS is blocking Orbix from reading {}. This needs no passwords or API keys - the user just has to enable Orbix Core for {} under System Settings > Privacy & Security > Automation, then ask again.",
                manifest.display_name, manifest.display_name
            )
        } else if e.contains("80080005") {
            // CO_E_SERVER_EXEC_FAILURE: COM cannot cross elevation levels.
            format!(
                "Windows would not let Orbix talk to {}. This needs no passwords or API keys - it happens when one of the two runs as administrator and the other does not. Close {}, start it normally (not as administrator), then ask again.",
                manifest.display_name, manifest.display_name
            )
        } else {
            format!("{} bridge action {} failed: {}", manifest.app, action.id, e)
        }
    })?;
    Ok(parse_output(&raw, &action.returns, &resolved))
}

// ─────────────────────────────────────────────
//  Tests (pure; no osascript, CI-safe)
// ─────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn lines_split_on_newlines_and_record_separators() {
        let returns = BridgeReturns { kind: ReturnKind::Lines, columns: vec![], sort_column: None, sort_desc: false, limit_arg: None };
        let v = parse_output("a\nb\n", &returns, &[]);
        assert_eq!(v["count"], 2);
        let v = parse_output(&format!("a{}b{}", RECORD_SEP, RECORD_SEP), &returns, &[]);
        assert_eq!(v["count"], 2);
        assert_eq!(v["lines"][1], "b");
    }

    #[test]
    fn builtin_manifests_parse_and_are_wellformed() {
        let manifests = builtin_manifests();
        assert_eq!(manifests.len(), 6, "all six embedded manifests must parse");
        let mut names = std::collections::HashSet::new();
        const FLAVORS: [Flavor; 2] = [Flavor::AppleScript, Flavor::PowerShell];
        for m in &manifests {
            assert!(names.insert(m.app.to_lowercase()), "duplicate app {}", m.app);
            assert!(
                FLAVORS.iter().any(|f| m.supports(*f)),
                "{} runs on no platform",
                m.app
            );
            assert!(!m.actions.is_empty(), "{} has no actions", m.app);
            for f in FLAVORS {
                if let Some(probe) = m.probe_for(f) {
                    assert!(!probe.text().is_empty(), "{} has an empty probe", m.app);
                }
            }
            for a in &m.actions {
                assert!(
                    FLAVORS.iter().any(|f| a.script_for(*f).is_some()),
                    "{}.{} has no script",
                    m.app,
                    a.id
                );
                if a.returns.kind == ReturnKind::Records {
                    assert!(!a.returns.columns.is_empty(), "{}.{} records need columns", m.app, a.id);
                }
                // in every flavor the action is written for, every declared
                // arg must be used - as a script placeholder or as the
                // Rust-side limit_arg - and no undeclared placeholders may
                // remain
                for f in FLAVORS {
                    let Some(script) = a.script_for(f) else { continue };
                    let mut script = script.text();
                    assert!(!script.is_empty(), "{}.{} has an empty script", m.app, a.id);
                    for arg in &a.args {
                        let ph = format!("{{{{{}}}}}", arg.name);
                        let used_in_script = script.contains(&ph);
                        let used_as_limit = a.returns.limit_arg.as_deref() == Some(arg.name.as_str());
                        assert!(used_in_script || used_as_limit, "{}.{} arg {} unused", m.app, a.id, arg.name);
                        script = script.replace(&ph, "1");
                    }
                    assert!(!script.contains("{{"), "{}.{} has undeclared placeholder", m.app, a.id);
                }
            }
        }
    }

    #[test]
    fn outlook_is_bridged_on_both_desktops() {
        let outlook = builtin_manifests()
            .into_iter()
            .find(|m| m.app == "Outlook")
            .expect("outlook manifest");
        assert!(outlook.supports(Flavor::AppleScript));
        assert!(outlook.supports(Flavor::PowerShell));
        assert_eq!(outlook.windows_prog_id.as_deref(), Some("Outlook.Application"));
        assert!(outlook.windows_limited_hint.is_some(), "new Outlook needs its own hint");
        // the new Outlook's execution alias, as a real path under a variable
        assert_eq!(
            outlook.windows_limited_paths,
            ["%LOCALAPPDATA%/Microsoft/WindowsApps/olk.exe"]
        );
        let windows_actions: Vec<&str> = outlook
            .actions
            .iter()
            .filter(|a| a.script_for(Flavor::PowerShell).is_some())
            .map(|a| a.id.as_str())
            .collect();
        assert_eq!(
            windows_actions,
            ["recent_messages", "unread_count", "upcoming_events", "search_messages", "today_events"]
        );
        // The AppleScript-only bridges must not claim Windows support.
        let mail = builtin_manifests().into_iter().find(|m| m.app == "Mail").unwrap();
        assert!(!mail.supports(Flavor::PowerShell));
    }

    #[test]
    fn find_manifest_matches_aliases_case_insensitive() {
        // Outlook is bridged on macOS and Windows; nothing is elsewhere.
        if HOST_FLAVOR.is_some() {
            assert!(find_manifest("outlook").is_some());
            assert!(find_manifest("Microsoft Outlook").is_some());
            assert!(find_manifest("MS OUTLOOK").is_some());
        }
        assert_eq!(find_manifest("iMessage").is_some(), cfg!(target_os = "macos"));
        assert!(find_manifest("slackk").is_none());
    }

    /// One action carrying the same script text in both flavors.
    fn test_action(script: &str, args: Vec<BridgeArg>) -> BridgeAction {
        BridgeAction {
            id: "t".into(),
            description: String::new(),
            script: Some(Script::One(script.into())),
            windows_script: Some(Script::One(script.into())),
            args,
            returns: BridgeReturns {
                kind: ReturnKind::Text,
                columns: vec![],
                sort_column: None,
                sort_desc: false,
                limit_arg: None,
            },
            timeout_secs: 5,
        }
    }

    #[test]
    fn powershell_strings_travel_as_base64() {
        let action = test_action(
            "$q = {{query}}",
            vec![BridgeArg {
                name: "query".into(),
                kind: ArgKind::String,
                default: None,
                min: None,
                max: None,
            }],
        );
        // straight, typographic and backtick quoting all stay inert
        let inject = "x'; Remove-Item C:/ -Recurse; '\u{2019}\"`$(calc)";
        let (s, resolved) = render_script(Flavor::PowerShell, &action, &json!({ "query": inject })).unwrap();
        assert!(!s.contains("Remove-Item"));
        assert!(!s.contains("calc"));
        assert!(s.starts_with("$q = ([Text.Encoding]::UTF8.GetString([Convert]::FromBase64String('"));
        let b64 = s.split('\'').nth(1).unwrap();
        assert!(b64.chars().all(|c| c.is_ascii_alphanumeric() || "+/=".contains(c)));
        // the caller's value is reported back verbatim
        assert_eq!(resolved[0].1, json!(inject));
    }

    #[test]
    fn actions_missing_a_flavor_refuse_to_render() {
        let mut action = test_action("x", vec![]);
        action.windows_script = None;
        assert!(render_script(Flavor::AppleScript, &action, &json!({})).is_ok());
        assert!(render_script(Flavor::PowerShell, &action, &json!({})).is_err());
    }

    #[test]
    fn env_vars_expand_and_unknown_ones_stay() {
        std::env::set_var("ORBIX_BRIDGE_TEST_DIR", "C:/x");
        assert_eq!(expand_env_vars("%ORBIX_BRIDGE_TEST_DIR%/olk.exe"), "C:/x/olk.exe");
        assert_eq!(expand_env_vars("%ORBIX_NOPE_NOT_SET%/a"), "%ORBIX_NOPE_NOT_SET%/a");
        assert_eq!(expand_env_vars("50% done"), "50% done");
        assert_eq!(expand_env_vars("plain"), "plain");
    }

    #[test]
    fn int_args_are_clamped_and_defaulted() {
        let action = test_action(
            "get {{limit}}",
            vec![BridgeArg {
                name: "limit".into(),
                kind: ArgKind::Int,
                default: Some(json!(10)),
                min: Some(1),
                max: Some(50),
            }],
        );
        // integers render the same way in both flavors
        for f in [Flavor::AppleScript, Flavor::PowerShell] {
            let (s, _) = render_script(f, &action, &json!({})).unwrap();
            assert_eq!(s, "get 10");
            let (s, _) = render_script(f, &action, &json!({"limit": 9999})).unwrap();
            assert_eq!(s, "get 50");
            let (s, _) = render_script(f, &action, &json!({"limit": "3"})).unwrap();
            assert_eq!(s, "get 3");
            assert!(render_script(f, &action, &json!({"limit": "abc"})).is_err());
        }
    }

    #[test]
    fn string_args_cannot_escape_the_quotes() {
        let action = test_action(
            "whose name contains \"{{query}}\"",
            vec![BridgeArg {
                name: "query".into(),
                kind: ArgKind::String,
                default: None,
                min: None,
                max: None,
            }],
        );
        let inject = "x\" & (do shell script \"id\") & \"";
        let (s, _) = render_script(Flavor::AppleScript, &action, &json!({ "query": inject })).unwrap();
        // the quotes arrive escaped, so the payload stays inside the literal
        assert!(s.contains("\\\""));
        assert!(!s.contains("contains \"x\" &"));
        // a required string arg with no value is an error
        assert!(render_script(Flavor::AppleScript, &action, &json!({})).is_err());
        assert!(render_script(Flavor::PowerShell, &action, &json!({})).is_err());
    }

    #[test]
    fn records_parse_sort_and_limit() {
        let returns = BridgeReturns {
            kind: ReturnKind::Records,
            columns: vec!["title".into(), "modified".into()],
            sort_column: Some("modified".into()),
            sort_desc: true,
            limit_arg: Some("limit".into()),
        };
        let raw = format!(
            "a{f}2026-01-01T10:00:00{r}b{f}2026-03-01T10:00:00{r}c{f}2026-02-01T10:00:00{r}",
            f = FIELD_SEP,
            r = RECORD_SEP
        );
        let v = parse_output(&raw, &returns, &[("limit".to_string(), json!(2))]);
        assert_eq!(v["count"], 2);
        assert_eq!(v["items"][0]["title"], "b");
        assert_eq!(v["items"][1]["title"], "c");
    }

    #[test]
    fn short_records_fill_missing_columns_empty() {
        let returns = BridgeReturns {
            kind: ReturnKind::Records,
            columns: vec!["a".into(), "b".into(), "c".into()],
            sort_column: None,
            sort_desc: false,
            limit_arg: None,
        };
        let raw = format!("only{r}", r = RECORD_SEP);
        let v = parse_output(&raw, &returns, &[]);
        assert_eq!(v["items"][0]["a"], "only");
        assert_eq!(v["items"][0]["b"], "");
        assert_eq!(v["items"][0]["c"], "");
    }

    #[test]
    fn empty_output_is_zero_records_not_an_error() {
        let returns = BridgeReturns {
            kind: ReturnKind::Records,
            columns: vec!["x".into()],
            sort_column: None,
            sort_desc: false,
            limit_arg: None,
        };
        let v = parse_output("", &returns, &[]);
        assert_eq!(v["count"], 0);
    }

    #[test]
    fn wrapped_scripts_carry_timeout_and_separators() {
        let s = wrap_script(Flavor::AppleScript, "", "return 1", 30);
        assert!(s.starts_with("with timeout of 30 seconds"));
        assert!(s.contains("set fsep to character id 31"));
        assert!(s.contains("set rsep to character id 30"));
        assert!(s.trim_end().ends_with("end timeout"));
    }

    /// The whole Windows path short of an app: wrap, run through the real
    /// PowerShell, parse. Non-ASCII text must survive (stdout is forced to
    /// UTF-8), Clean must keep separators out of values, and a thrown
    /// error must come back as its plain message.
    #[cfg(target_os = "windows")]
    #[tokio::test]
    async fn powershell_roundtrip_parses_records_and_reports_errors() {
        let mut m = builtin_manifests().into_iter().find(|m| m.app == "Outlook").unwrap();
        m.windows_prelude = Some(Script::One("$who = 'Zo\u{eb} \u{2603}'".into()));
        let script = "(Clean \"a`n$($fsep)b\") + $fsep + $who + $rsep + 'second' + $fsep + $rsep";
        let raw = run_wrapped(&m, script, 30).await.expect("powershell runs");
        let returns = BridgeReturns {
            kind: ReturnKind::Records,
            columns: vec!["title".into(), "who".into()],
            sort_column: None,
            sort_desc: false,
            limit_arg: None,
        };
        let v = parse_output(&raw, &returns, &[]);
        assert_eq!(v["count"], 2);
        assert_eq!(v["items"][0]["title"], "a  b");
        assert_eq!(v["items"][0]["who"], "Zo\u{eb} \u{2603}");
        assert_eq!(v["items"][1]["title"], "second");

        let err = run_wrapped(&m, "throw 'outlook said no'", 30).await.unwrap_err();
        assert_eq!(err, "outlook said no");
    }

    #[test]
    fn wrapped_powershell_carries_separators_prelude_and_error_trap() {
        let s = wrap_script(Flavor::PowerShell, "$ns = 1", "\"ok|$ns\"", 30);
        assert!(s.contains("$fsep = [string][char]31"));
        assert!(s.contains("$rsep = [string][char]30"));
        assert!(s.contains("UTF8Encoding $false"), "stdout must be UTF-8 without a BOM");
        // prelude runs before the script, both inside the trap
        let (try_at, prelude_at, script_at, catch_at) = (
            s.find("try {").unwrap(),
            s.find("$ns = 1").unwrap(),
            s.find("\"ok|$ns\"").unwrap(),
            s.find("} catch {").unwrap(),
        );
        assert!(try_at < prelude_at && prelude_at < script_at && script_at < catch_at);
        assert!(s.contains("[Console]::Error.Write($_.Exception.Message)"));
    }
}

// Live acceptance harness (FABLE A1). Ignored by default - runs real
// AppleScript against whatever apps are installed, may launch them, and
// may trigger the one-time macOS Automation prompt:
//   cargo test --bin orbixcore -- --ignored app_bridge_live --nocapture
#[cfg(test)]
mod live {
    use super::*;

    #[tokio::test]
    #[ignore]
    async fn capability_digest_live() {
        let d = capability_digest().await;
        eprintln!("{}", d);
        assert!(d.contains("APP ACCESS"));
        // second call must come from the cache (same content)
        assert_eq!(capability_digest().await, d);
    }

    #[tokio::test]
    #[ignore]
    async fn app_bridge_live() {
        let statuses = list_bridges().await;
        eprintln!("{}", format_bridge_list(&statuses));
        assert_eq!(statuses.len(), all_manifests().len());
        assert!(statuses.iter().any(|s| s.app == "Outlook"));

        // Exercise one read per app that is installed and running; a
        // Limited probe must produce an actionable message, not a crash.
        for (app, action, args) in [
            ("Outlook", "recent_messages", json!({"limit": 5})),
            ("Notes", "recent_notes", json!({"limit": 5})),
            ("Reminders", "open_reminders", json!({"limit": 3})),
            ("Calendar", "upcoming_events", json!({"days": 7})),
        ] {
            let running = statuses
                .iter()
                .find(|s| s.app == app)
                .map(|s| s.status == ProbeStatus::Ready || s.status == ProbeStatus::Limited)
                .unwrap_or(false);
            if !running {
                eprintln!("[skip] {} not running", app);
                continue;
            }
            match run_bridge_action(app, action, &args).await {
                Ok(v) => eprintln!("[ok] {} {}: {}", app, action, v),
                Err(e) => {
                    eprintln!("[actionable] {} {}: {}", app, action, e);
                    assert!(!e.is_empty());
                }
            }
        }
    }
}
