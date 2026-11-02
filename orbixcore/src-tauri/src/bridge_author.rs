//! Bridge author: Bob teaches himself an app's scripting.
//!
//! Pair mode's answer to "the app is unknown": if the app publishes an
//! AppleScript dictionary, read it, have the local model draft an
//! app_bridge manifest (probe + a few read actions), refuse anything that
//! is not read-only, run the probe and the actions against the live app,
//! and only then save the manifest into <data_dir>/bridges/ where
//! app_bridge picks it up with no rebuild. One retry with the failures
//! fed back. Everything Bob learns is a JSON file the user can inspect.
//!
//! Steps mirror bob_connector_author: discover, draft, scan, smoke test,
//! activate. Nothing here types, clicks, or changes app state; the policy
//! scan makes sure the drafted scripts cannot either.

use serde::Serialize;
use serde_json::Value;
use std::collections::HashSet;
use std::sync::{Mutex, OnceLock};

use crate::app_bridge::AppBridgeManifest;

const SDEF_TIMEOUT_SECS: u64 = 20;
/// Budget for the dictionary summary handed to the model (chars).
const SUMMARY_MAX_CHARS: usize = 5000;
const MAX_ACTIONS: usize = 6;

// ─────────────────────────────────────────────
//  Step 1: discover the dictionary
// ─────────────────────────────────────────────

#[derive(Debug, Clone, Serialize)]
pub struct SdefSummary {
    pub suites: usize,
    pub classes: usize,
    pub commands: usize,
    /// Compact, model-readable outline of the dictionary.
    pub text: String,
}

/// `sdef <bundle>` and summarise. Fails when the app has no dictionary.
pub async fn discover(bundle_path: &str) -> Result<SdefSummary, String> {
    if bundle_path.trim().is_empty() {
        return Err("no app bundle to read a dictionary from".to_string());
    }
    let out = tokio::time::timeout(
        std::time::Duration::from_secs(SDEF_TIMEOUT_SECS),
        crate::process_ext::tokio_command("sdef").arg(bundle_path).output(),
    )
    .await
    .map_err(|_| "reading the scripting dictionary took too long".to_string())?
    .map_err(|e| format!("sdef: {}", e))?;
    let xml = String::from_utf8_lossy(&out.stdout);
    if !out.status.success() || xml.trim().is_empty() {
        return Err("the app publishes no scripting dictionary".to_string());
    }
    let summary = summarise_sdef(&xml);
    if summary.classes == 0 && summary.commands == 0 {
        return Err("the scripting dictionary is empty".to_string());
    }
    Ok(summary)
}

#[derive(Default)]
struct SuiteOutline {
    name: String,
    classes: Vec<String>,
    commands: Vec<String>,
}

/// Turn sdef XML into a short outline: one line per class (with its
/// readable properties and elements) and per command (with parameters).
/// App-specific suites come first; the Standard and Text suites are
/// trimmed hardest since every app has them.
pub fn summarise_sdef(xml: &str) -> SdefSummary {
    let tag_re = regex::Regex::new(
        r#"<(/?)(suite|class|class-extension|property|element|command|parameter|direct-parameter|result)\b([^>]*)>"#,
    )
    .unwrap();
    let attr = |attrs: &str, name: &str| -> Option<String> {
        let re = regex::Regex::new(&format!(r#"\b{}="([^"]*)""#, regex::escape(name))).unwrap();
        re.captures(attrs).map(|c| c[1].to_string())
    };

    let mut suites: Vec<SuiteOutline> = Vec::new();
    let mut cur_class: Option<(String, Vec<String>, Vec<String>)> = None; // name, props, elements
    let mut cur_cmd: Option<(String, Vec<String>, Option<String>)> = None; // name, params, result
    let mut classes = 0;
    let mut commands = 0;

    for cap in tag_re.captures_iter(xml) {
        let closing = &cap[1] == "/";
        let tag = &cap[2];
        let attrs = cap.get(3).map(|m| m.as_str()).unwrap_or("");
        match (tag, closing) {
            ("suite", false) => {
                suites.push(SuiteOutline { name: attr(attrs, "name").unwrap_or_default(), ..Default::default() });
            }
            ("class" | "class-extension", false) => {
                let name = attr(attrs, "name").or_else(|| attr(attrs, "extends")).unwrap_or_default();
                cur_class = Some((name, Vec::new(), Vec::new()));
            }
            ("class" | "class-extension", true) => {
                if let Some((name, props, elements)) = cur_class.take() {
                    classes += 1;
                    let mut line = format!("  class {}", name);
                    if !elements.is_empty() {
                        line.push_str(&format!(" [contains: {}]", elements.join(", ")));
                    }
                    if !props.is_empty() {
                        line.push_str(&format!(": {}", props.iter().take(14).cloned().collect::<Vec<_>>().join(", ")));
                    }
                    if let Some(s) = suites.last_mut() {
                        s.classes.push(line);
                    }
                }
            }
            ("property", false) => {
                if let Some((_, props, _)) = cur_class.as_mut() {
                    let name = attr(attrs, "name").unwrap_or_default();
                    let ty = attr(attrs, "type").unwrap_or_default();
                    let access = attr(attrs, "access").unwrap_or_else(|| "rw".to_string());
                    props.push(if ty.is_empty() { name } else { format!("{} ({}{})", name, ty, if access == "r" { ", r" } else { "" }) });
                }
            }
            ("element", false) => {
                if let Some((_, _, elements)) = cur_class.as_mut() {
                    if let Some(t) = attr(attrs, "type") {
                        elements.push(t);
                    }
                }
            }
            ("command", false) => {
                cur_cmd = Some((attr(attrs, "name").unwrap_or_default(), Vec::new(), None));
            }
            ("command", true) => {
                if let Some((name, params, result)) = cur_cmd.take() {
                    commands += 1;
                    let mut line = format!("  command {}", name);
                    if !params.is_empty() {
                        line.push_str(&format!(" ({})", params.join(", ")));
                    }
                    if let Some(r) = result {
                        line.push_str(&format!(" -> {}", r));
                    }
                    if let Some(s) = suites.last_mut() {
                        s.commands.push(line);
                    }
                }
            }
            ("direct-parameter", false) => {
                if let Some((_, params, _)) = cur_cmd.as_mut() {
                    params.push(format!("direct: {}", attr(attrs, "type").unwrap_or_else(|| "any".into())));
                }
            }
            ("parameter", false) => {
                if let Some((_, params, _)) = cur_cmd.as_mut() {
                    let name = attr(attrs, "name").unwrap_or_default();
                    let ty = attr(attrs, "type").unwrap_or_default();
                    let opt = attr(attrs, "optional").map(|o| o == "yes").unwrap_or(false);
                    params.push(format!("{}{}{}", name, if ty.is_empty() { String::new() } else { format!(": {}", ty) }, if opt { "?" } else { "" }));
                }
            }
            ("result", false) => {
                if let Some((_, _, result)) = cur_cmd.as_mut() {
                    *result = attr(attrs, "type");
                }
            }
            _ => {}
        }
    }

    // App-specific suites first, generic ones last and trimmed.
    let generic = |n: &str| {
        let n = n.to_lowercase();
        n.contains("standard") || n.contains("text suite") || n.contains("type definitions") || n.contains("internet")
    };
    suites.sort_by_key(|s| generic(&s.name));
    let mut text = String::new();
    for s in &suites {
        let (class_cap, cmd_cap) = if generic(&s.name) { (4, 3) } else { (40, 30) };
        let mut block = format!("Suite \"{}\":\n", s.name);
        for c in s.classes.iter().take(class_cap) {
            block.push_str(c);
            block.push('\n');
        }
        for c in s.commands.iter().take(cmd_cap) {
            block.push_str(c);
            block.push('\n');
        }
        if text.len() + block.len() > SUMMARY_MAX_CHARS {
            let room = SUMMARY_MAX_CHARS.saturating_sub(text.len());
            if room > 200 {
                let cut: String = block.chars().take(room).collect();
                text.push_str(&cut);
                text.push_str("\n…\n");
            }
            break;
        }
        text.push_str(&block);
    }
    SdefSummary { suites: suites.len(), classes, commands, text }
}

// ─────────────────────────────────────────────
//  Step 2: draft with the local model
// ─────────────────────────────────────────────

/// A compact example: the shape, not a full shipped manifest, so the
/// prompt stays short and the model copies structure rather than content.
const EXAMPLE_MANIFEST: &str = r#"{
  "app": "Ledger", "display_name": "Ledger", "aliases": [], "process_name": "Ledger",
  "bundle_paths": ["/Applications/Ledger.app"],
  "probe": ["tell application \"Ledger\"", "  return \"ok|\" & (count of accounts) & \" account(s)\"", "end tell"],
  "actions": [
    {"id": "accounts", "description": "Every account with its balance.",
     "args": [{"name": "limit", "kind": "int", "default": 20, "min": 1, "max": 100}],
     "returns": {"kind": "records", "columns": ["name", "balance"], "limit_arg": "limit"}, "timeout_secs": 30,
     "script": ["tell application \"Ledger\"", "  set out to \"\"", "  repeat with a in accounts",
                "    set out to out & (name of a) & fsep & ((balance of a) as string) & rsep", "  end repeat", "  return out", "end tell"]},
    {"id": "find_account", "description": "Accounts whose name contains the query.",
     "args": [{"name": "query", "kind": "string"}],
     "returns": {"kind": "lines"}, "timeout_secs": 30,
     "script": ["tell application \"Ledger\"", "  set out to \"\"", "  repeat with a in (every account whose name contains \"{{query}}\")",
                "    set out to out & (name of a) & rsep", "  end repeat", "  return out", "end tell"]}
  ]
}"#;

fn draft_prompt(display_name: &str, process_name: &str, bundle_path: &str, summary: &SdefSummary, previous_failure: Option<&str>) -> String {
    let mut p = format!(
        "Write an Orbix app-bridge manifest for the Mac app \"{name}\" so Bob can READ its data through AppleScript.\n\n\
FORMAT: one JSON object exactly like this example (same field names), nothing else:\n{example}\n\n\
RULES:\n\
- \"app\" and \"display_name\" are \"{name}\"; \"process_name\" is \"{process}\"; \"bundle_paths\" is [\"{bundle}\"].\n\
- The probe must return \"ok|<short detail>\" only when real data is reachable (count something the app actually holds), or \"limited|<reason>\".\n\
- Give 3 or 4 READ actions (never more than {max}) that a person would actually ask for (recent items, current selection, the open document's name and text, counts). Read only: never delete, make, save, open, close, move, quit, set properties, run shell commands, or send keystrokes.\n\
- Scripts are arrays of AppleScript lines wrapped in tell application \"{name}\" ... end tell. Records are joined with fsep between fields and rsep between records (both are predefined). Coerce values with `as string`; dates as ((d) as «class isot» as string).\n\
- Args: only {{\"name\":..,\"kind\":\"int\"|\"string\",\"default\":..,\"min\":..,\"max\":..}}. Every string arg is inserted as {{{{name}}}} inside quotes. Int args get \"default\", \"min\", \"max\".\n\
- \"returns\": {{\"kind\":\"records\",\"columns\":[..]}} for lists (columns match the fsep order), {{\"kind\":\"lines\"}} for one item per line, {{\"kind\":\"text\"}} for one blob.\n\
- Prefer whole-collection reads (`name of every item of ...`) over loops where the dictionary allows; keep each script short and robust.\n\n\
SCRIPTING DICTIONARY OF \"{name}\" ({suites} suites, {classes} classes, {commands} commands):\n{summary}\n",
        name = display_name,
        process = process_name,
        bundle = bundle_path,
        example = EXAMPLE_MANIFEST,
        max = MAX_ACTIONS,
        suites = summary.suites,
        classes = summary.classes,
        commands = summary.commands,
        summary = summary.text,
    );
    if let Some(f) = previous_failure {
        p.push_str(&format!(
            "\nYOUR PREVIOUS DRAFT FAILED WHEN RUN AGAINST THE LIVE APP. Fix these and answer with the corrected JSON only:\n{}\n",
            f
        ));
    }
    p.push_str("\nAnswer with the JSON object only.");
    p
}

async fn draft(display_name: &str, process_name: &str, bundle_path: &str, summary: &SdefSummary, previous_failure: Option<&str>) -> Result<AppBridgeManifest, String> {
    let raw = crate::llm_engine::call_model_plain(
        "You write Orbix app-bridge manifests: JSON that lets a local agent read a Mac app through AppleScript. Answer with one JSON object and nothing else. No prose, no fences, no thinking aloud.",
        &draft_prompt(display_name, process_name, bundle_path, summary, previous_failure),
    )
    .await?;
    parse_draft(&raw, display_name, process_name, bundle_path)
}

/// Parse and normalise the model's manifest. Tolerates prose and fences
/// around the JSON and fills the identity fields the model may get wrong.
pub fn parse_draft(raw: &str, display_name: &str, process_name: &str, bundle_path: &str) -> Result<AppBridgeManifest, String> {
    let json_text = crate::llm_engine::extract_json_object(raw).ok_or("the model did not return a JSON object")?;
    let mut v: Value = serde_json::from_str(&json_text)
        .or_else(|_| crate::repair_json(&json_text).and_then(|f| serde_json::from_str(&f).ok()).ok_or(()))
        .map_err(|_| "the model's manifest was not valid JSON".to_string())?;
    let obj = v.as_object_mut().ok_or("the manifest is not a JSON object")?;
    obj.insert("app".into(), Value::String(display_name.to_string()));
    obj.insert("display_name".into(), Value::String(display_name.to_string()));
    obj.insert("process_name".into(), Value::String(process_name.to_string()));
    if !bundle_path.is_empty() {
        obj.insert("bundle_paths".into(), Value::Array(vec![Value::String(bundle_path.to_string())]));
    }
    // Windows fields are never right from a Mac dictionary; drop them.
    for k in ["windows_script", "windows_probe", "windows_prelude", "windows_prog_id", "windows_process_name", "windows_limited_paths", "windows_limited_hint"] {
        obj.remove(k);
    }
    if let Some(actions) = obj.get_mut("actions").and_then(|a| a.as_array_mut()) {
        for a in actions.iter_mut() {
            if let Some(ao) = a.as_object_mut() {
                ao.remove("windows_script");
                if !ao.contains_key("returns") {
                    ao.insert("returns".into(), serde_json::json!({ "kind": "text" }));
                }
                if !ao.contains_key("timeout_secs") {
                    ao.insert("timeout_secs".into(), serde_json::json!(30));
                }
            }
        }
        actions.truncate(MAX_ACTIONS);
    }
    let manifest: AppBridgeManifest = serde_json::from_value(v).map_err(|e| format!("manifest shape: {}", e))?;
    validate(&manifest)?;
    Ok(manifest)
}

// ─────────────────────────────────────────────
//  Step 3: scan (read-only policy)
// ─────────────────────────────────────────────

/// Words that change state or reach outside the app. A drafted script
/// containing any of them is rejected, never run.
const FORBIDDEN: &[&str] = &[
    "do shell script", "keystroke", "key code", "system events", "delete", "make new", "make ", "save", "quit",
    "close", "move", "duplicate", "activate", "open", "reveal", "empty", "set the clipboard", "restart", "shut down",
    "sleep", "log out", "send", "post", "run script", "load script", "store script", "mount", "eject", "launch",
];

pub fn scan_script(script: &str) -> Result<(), String> {
    let lower = script.to_lowercase();
    for word in FORBIDDEN {
        let re = regex::Regex::new(&format!(r"(^|[^a-z_]){}([^a-z_]|$)", regex::escape(word.trim_end()))).unwrap();
        if re.is_match(&lower) {
            return Err(format!("script uses \"{}\", which is not read-only", word.trim()));
        }
    }
    // `set <thing> of <object> to ...` assigns an app property.
    let assign = regex::Regex::new(r"(?m)^\s*set\s+(?:the\s+)?[\w ]+?\s+(of|'s)\b[^\n]*\bto\b").unwrap();
    if assign.is_match(&lower) {
        return Err("script assigns an app property (set … of … to)".to_string());
    }
    Ok(())
}

pub fn validate(m: &AppBridgeManifest) -> Result<(), String> {
    if m.app.trim().is_empty() {
        return Err("manifest has no app name".to_string());
    }
    let probe = m.probe.as_ref().ok_or("manifest has no probe")?;
    scan_script(&probe.text()).map_err(|e| format!("probe: {}", e))?;
    if m.actions.is_empty() {
        return Err("manifest has no actions".to_string());
    }
    let mut ids = HashSet::new();
    for a in &m.actions {
        if a.id.trim().is_empty() || !a.id.chars().all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '_') {
            return Err(format!("action id \"{}\" must be snake_case", a.id));
        }
        if !ids.insert(a.id.clone()) {
            return Err(format!("duplicate action id \"{}\"", a.id));
        }
        let script = a.script.as_ref().ok_or_else(|| format!("action {} has no script", a.id))?.text();
        if !script.to_lowercase().contains("tell application") {
            return Err(format!("action {} does not tell the application", a.id));
        }
        scan_script(&script).map_err(|e| format!("action {}: {}", a.id, e))?;
        for arg in &a.args {
            let placeholder = format!("{{{{{}}}}}", arg.name);
            if !script.contains(&placeholder) {
                return Err(format!("action {} declares arg {} but never uses {}", a.id, arg.name, placeholder));
            }
        }
    }
    Ok(())
}

// ─────────────────────────────────────────────
//  Steps 4 + 5: smoke test against the live app, then activate
// ─────────────────────────────────────────────

#[derive(Debug, Clone, Serialize)]
pub struct AuthorReport {
    pub app: String,
    pub path: String,
    pub actions: Vec<String>,
    pub attempts: u32,
    /// Actions that ran clean in the smoke test.
    pub verified: Vec<String>,
}

static IN_PROGRESS: OnceLock<Mutex<HashSet<String>>> = OnceLock::new();

fn in_progress() -> &'static Mutex<HashSet<String>> {
    IN_PROGRESS.get_or_init(|| Mutex::new(HashSet::new()))
}

/// The whole loop for one app. `progress` gets one line per step.
pub async fn author(display_name: &str, process_name: &str, bundle_path: &str, progress: &(dyn Fn(&str) + Send + Sync)) -> Result<AuthorReport, String> {
    let key = display_name.to_lowercase();
    {
        let mut set = in_progress().lock().unwrap_or_else(|e| e.into_inner());
        if !set.insert(key.clone()) {
            return Err(format!("already learning {}", display_name));
        }
    }
    let result = author_inner(display_name, process_name, bundle_path, progress).await;
    in_progress().lock().unwrap_or_else(|e| e.into_inner()).remove(&key);
    result
}

async fn author_inner(display_name: &str, process_name: &str, bundle_path: &str, progress: &(dyn Fn(&str) + Send + Sync)) -> Result<AuthorReport, String> {
    progress(&format!("Reading {}'s scripting dictionary…", display_name));
    let summary = discover(bundle_path).await?;
    let mut failure: Option<String> = None;
    let mut last_err = String::new();
    for attempt in 1..=2u32 {
        progress(&format!(
            "{} a bridge for {}…",
            if attempt == 1 { "Drafting" } else { "Redrafting" },
            display_name
        ));
        let manifest = match draft(display_name, process_name, bundle_path, &summary, failure.as_deref()).await {
            Ok(m) => m,
            Err(e) => {
                last_err = e.clone();
                failure = Some(e);
                continue;
            }
        };
        progress(&format!("Trying the bridge against {}…", display_name));
        match crate::app_bridge::smoke_test(&manifest).await {
            Ok(smoke) => {
                let verified: Vec<String> = smoke.iter().filter(|s| s.ok).map(|s| s.id.clone()).collect();
                if verified.is_empty() {
                    let detail = smoke.iter().map(|s| format!("{}: {}", s.id, s.detail)).collect::<Vec<_>>().join("\n");
                    last_err = format!("no action returned data:\n{}", detail);
                    failure = Some(last_err.clone());
                    continue;
                }
                // Keep only the actions that proved themselves.
                let mut m = manifest.clone();
                m.actions.retain(|a| verified.contains(&a.id));
                m.learned = true;
                let path = crate::app_bridge::save_user_manifest(&m)?;
                return Ok(AuthorReport {
                    app: m.app.clone(),
                    path: path.to_string_lossy().into_owned(),
                    actions: m.actions.iter().map(|a| format!("{} ({})", a.id, a.description)).collect(),
                    attempts: attempt,
                    verified,
                });
            }
            Err(e) => {
                last_err = e.clone();
                failure = Some(e);
            }
        }
    }
    Err(format!("{} did not answer the bridge Bob drafted: {}", display_name, crate::truncate_str(&last_err, 400)))
}

static LAST_RELEARN: OnceLock<Mutex<std::collections::HashMap<String, std::time::Instant>>> = OnceLock::new();
const RELEARN_COOLDOWN: std::time::Duration = std::time::Duration::from_secs(30 * 60);

/// A learned bridge stopped answering (an app update moved things, or
/// the draft was fragile): learn it again in the background, at most once
/// per half hour per app. Returns whether a re-learn was scheduled. Only
/// bridges Bob authored are touched; shipped manifests are never
/// overwritten. Nothing changes on disk unless the new draft passes the
/// same smoke test the first one did.
pub fn relearn_in_background(app: &tauri::AppHandle, app_name: &str) -> bool {
    let Some(m) = crate::app_bridge::find_manifest(app_name) else { return false };
    if !m.learned {
        return false;
    }
    let Some(bundle) = m.bundle_paths.first().cloned().filter(|b| !b.is_empty()) else { return false };
    {
        let mut last = LAST_RELEARN.get_or_init(|| Mutex::new(Default::default())).lock().unwrap_or_else(|e| e.into_inner());
        let key = m.app.to_lowercase();
        if last.get(&key).map(|t| t.elapsed() < RELEARN_COOLDOWN).unwrap_or(false) {
            return false;
        }
        last.insert(key, std::time::Instant::now());
    }
    let app = app.clone();
    let (display, process) = (m.app.clone(), m.process_name.clone());
    tauri::async_runtime::spawn(async move {
        let app2 = app.clone();
        let progress = move |t: &str| crate::peer::emit_event(&app2, "connecting", t, None);
        match author(&display, &process, &bundle, &progress).await {
            Ok(report) => {
                if let Ok(peer) = crate::peer::refresh(&display, Some(&app)).await {
                    crate::peer::emit_event(&app, "updated", &format!("{} re-learned.", peer.app), Some(&peer));
                }
                let names: Vec<String> = report.verified.iter().map(|s| s.replace('_', " ")).collect();
                crate::peer::bob_says(&app, &format!("**{} works again.** I re-learned its scripting: {}.", report.app, names.join(", ")));
            }
            Err(e) => {
                log::info!("[bridge_author] relearn {}: {}", display, e);
                crate::peer::bob_says(&app, &format!("I tried to re-learn {}'s scripting after it stopped answering, and it still would not. I will read its window instead.", display));
            }
        }
    });
    true
}

/// Pair mode's background path: learn the app after the quick pairing
/// returned, then re-run the ladder so the peer gains the bridge, and say
/// so in chat. Failures stay quiet apart from one honest line.
pub async fn author_for_peer(app: tauri::AppHandle, peer_name: String, display_name: String, process_name: String, bundle_path: String) {
    if crate::app_bridge::find_manifest(&display_name).is_some() {
        return;
    }
    let app2 = app.clone();
    let progress = move |t: &str| crate::peer::emit_event(&app2, "connecting", t, None);
    match author(&display_name, &process_name, &bundle_path, &progress).await {
        Ok(report) => {
            let names: Vec<String> = report.verified.iter().map(|s| s.replace('_', " ")).collect();
            match crate::peer::refresh(&peer_name, Some(&app)).await {
                Ok(peer) => {
                    crate::peer::emit_event(&app, "updated", &format!("{} learned.", peer.app), Some(&peer));
                    crate::peer::bob_says(
                        &app,
                        &format!(
                            "**I taught myself {}'s scripting.** I can now read it directly: {}. Ask for any of those, or ask me to learn more.",
                            report.app,
                            names.join(", ")
                        ),
                    );
                }
                Err(e) => log::warn!("[bridge_author] learned {} but could not refresh the peer: {}", report.app, e),
            }
        }
        Err(e) => {
            log::info!("[bridge_author] {}: {}", display_name, e);
            crate::peer::bob_says(
                &app,
                &format!("I tried to learn {}'s scripting so I could read it directly. It did not work out, so I will use its window instead.", display_name),
            );
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const SDEF: &str = r#"<?xml version="1.0" encoding="UTF-8"?>
<dictionary title="Dictionary">
  <suite name="Standard Suite" code="core" description="Common classes.">
    <class name="window" code="cwin"><property name="name" code="pnam" type="text" access="r"/></class>
    <command name="close" code="coreclos"><direct-parameter type="specifier"/></command>
  </suite>
  <suite name="Ledger Suite" code="LdgS" description="Ledger specifics.">
    <class name="account" code="Acct" description="A ledger account.">
      <element type="entry"/>
      <property name="name" code="pnam" type="text" access="r"/>
      <property name="balance" code="Blnc" type="number" access="r"/>
    </class>
    <class name="entry" code="Entr">
      <property name="amount" code="Amnt" type="number"/>
      <property name="posted" code="Pstd" type="date" access="r"/>
    </class>
    <command name="reconcile" code="LdgSRcnl" description="Reconcile an account.">
      <direct-parameter type="account"/>
      <parameter name="through" code="Thru" type="date" optional="yes"/>
      <result type="boolean"/>
    </command>
  </suite>
</dictionary>"#;

    #[test]
    fn sdef_summary_is_compact_and_app_suite_first() {
        let s = summarise_sdef(SDEF);
        assert_eq!(s.suites, 2);
        assert_eq!(s.classes, 3);
        assert_eq!(s.commands, 2);
        assert!(s.text.starts_with("Suite \"Ledger Suite\""), "{}", s.text);
        assert!(s.text.contains("class account [contains: entry]: name (text, r), balance (number, r)"));
        assert!(s.text.contains("command reconcile (direct: account, through: date?) -> boolean"));
        assert!(s.text.len() < SUMMARY_MAX_CHARS);
    }

    #[test]
    fn summary_respects_the_budget() {
        let mut big = String::from("<dictionary>");
        for i in 0..400 {
            big.push_str(&format!("<suite name=\"S{i}\"><class name=\"c{i}\"><property name=\"p\" type=\"text\"/></class></suite>"));
        }
        big.push_str("</dictionary>");
        let s = summarise_sdef(&big);
        assert!(s.text.len() <= SUMMARY_MAX_CHARS + 8, "{}", s.text.len());
        assert_eq!(s.classes, 400);
    }

    #[test]
    fn policy_rejects_state_changes_and_allows_reads() {
        assert!(scan_script("tell application \"Ledger\"\n  return name of every account\nend tell").is_ok());
        assert!(scan_script("tell application \"Ledger\"\n  set out to out & (name of a) & fsep\nend tell").is_ok());
        assert!(scan_script("tell application \"Ledger\"\n  delete account 1\nend tell").is_err());
        assert!(scan_script("do shell script \"rm -rf /\"").is_err());
        assert!(scan_script("tell application \"System Events\" to keystroke \"a\"").is_err());
        assert!(scan_script("tell application \"Ledger\"\n  set name of account 1 to \"x\"\nend tell").is_err());
        assert!(scan_script("tell application \"Ledger\"\n  make new entry\nend tell").is_err());
        assert!(scan_script("tell application \"Ledger\"\n  open location \"http://x\"\nend tell").is_err());
        // "closeable" is a property read, not the close command.
        assert!(scan_script("tell application \"Ledger\"\n  return closeable of window 1\nend tell").is_ok());
    }

    #[test]
    fn draft_parse_normalises_identity_and_validates() {
        let raw = r#"Here is the manifest:
```json
{"app":"wrong","display_name":"wrong","process_name":"x","bundle_paths":[],
 "probe":["tell application \"Ledger\"","  return \"ok|\" & (count of accounts)","end tell"],
 "actions":[{"id":"accounts","description":"All accounts with balances.",
   "args":[],"returns":{"kind":"records","columns":["name","balance"]},
   "script":["tell application \"Ledger\"","  set out to \"\"","  repeat with a in accounts","    set out to out & (name of a) & fsep & (balance of a as string) & rsep","  end repeat","  return out","end tell"],
   "windows_script":"should be dropped"}]}
```"#;
        let m = parse_draft(raw, "Ledger", "Ledger", "/Applications/Ledger.app").unwrap();
        assert_eq!(m.app, "Ledger");
        assert_eq!(m.process_name, "Ledger");
        assert_eq!(m.bundle_paths, vec!["/Applications/Ledger.app".to_string()]);
        assert_eq!(m.actions.len(), 1);
        assert!(m.actions[0].windows_script.is_none());
        assert!(validate(&m).is_ok());
    }

    #[test]
    fn draft_with_forbidden_script_is_refused() {
        let raw = r#"{"app":"L","display_name":"L","process_name":"L",
 "probe":["tell application \"L\"","  return \"ok\"","end tell"],
 "actions":[{"id":"nuke","description":"bad","returns":{"kind":"text"},
   "script":["tell application \"L\"","  delete every account","end tell"]}]}"#;
        let err = parse_draft(raw, "L", "L", "").unwrap_err();
        assert!(err.contains("not read-only"), "{}", err);
        let raw = r#"{"app":"L","display_name":"L","process_name":"L",
 "probe":["tell application \"L\"","  return \"ok\"","end tell"],
 "actions":[{"id":"find","description":"x","args":[{"name":"query","kind":"string"}],"returns":{"kind":"text"},
   "script":["tell application \"L\"","  return name of account 1","end tell"]}]}"#;
        let err = parse_draft(raw, "L", "L", "").unwrap_err();
        assert!(err.contains("never uses {{query}}"), "{}", err);
    }
}

#[cfg(all(test, target_os = "macos"))]
mod live {
    //! Live authoring against a running scriptable app and the local model.
    //! Ignored by default:
    //!   cargo test --bin orbixcore live_author_finder -- --ignored --nocapture
    use super::*;

    #[tokio::test]
    #[ignore]
    async fn live_author_finder() {
        let report = author("Finder", "Finder", "/System/Library/CoreServices/Finder.app", &|t| eprintln!("[progress] {}", t))
            .await
            .expect("author Finder");
        eprintln!("{:#?}", report);
        assert!(!report.verified.is_empty());
    }
}
