// ─────────────────────────────────────────────
//  Imagine - Bob with free rein
//
//  No request. One button hands Bob's agentic loop the run of the
//  device: look at what is open, read what is worth reading, and make
//  one real thing the user did not ask for - a briefing, a panel, a
//  document, an app page, a workflow, a skill. Everything it makes is an
//  ordinary Orbix object, and the run lands in the chat like any other
//  turn so the user can carry on from it ("make that a workflow").
//
//  Same loop as Bob's chat and agent shifts (bob_agentic), with two
//  differences: the tool catalog drops everything that reaches out or
//  tears down (tool_schemas::is_outward_tool / is_disruptive_tool), and
//  the run carries its own stop flag and inbox, so stopping Bob in chat
//  never kills an Imagine run and a nudge typed into the overlay reaches
//  the model between turns.
//
//  Wire: imagine-started {run} once the scan is in, step labels on the
//  shift-step channel under the run id, the model's turns on the
//  message-chunk stream under the same id, imagine-ended {result} last.
// ─────────────────────────────────────────────

use std::path::{Path, PathBuf};
use std::sync::atomic::Ordering;
use std::sync::Mutex;

use serde::{Deserialize, Serialize};
use tauri::Manager;

use crate::shift_monitor::ShiftControl;
use crate::Message;

const MAX_APPS: usize = 12;
const MAX_WINDOWS_PER_APP: usize = 6;
const MAX_TABS_PER_BROWSER: usize = 15;

// Apps that say nothing about the user's work.
const NOISE_APPS: &[&str] = &[
    "Orbix Core", "orbixcore", "Finder", "Dock", "SystemUIServer",
    "Control Center", "Notification Center", "Spotlight", "loginwindow",
    "Window Server", "Screenshot", "screencaptureui",
];

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct ScannedApp {
    pub app: String,
    pub windows: Vec<String>,
    /// Open tabs ("title - host/path") for browsers. Most business
    /// systems live in a browser; the tab list IS the context.
    #[serde(default)]
    pub tabs: Vec<String>,
}

/// The running (or just started) Imagine run, as the UI sees it.
#[derive(Clone, Debug, Serialize)]
pub struct ImagineRun {
    pub run_id: String,
    pub seed: Option<String>,
    pub scan: Vec<ScannedApp>,
    pub started_at: String,
}

/// One run at a time: the local model is one model, and two Imagines
/// racing each other would build two half-things.
static CURRENT: Mutex<Option<(ImagineRun, ShiftControl)>> = Mutex::new(None);

fn current_slot() -> std::sync::MutexGuard<'static, Option<(ImagineRun, ShiftControl)>> {
    CURRENT.lock().unwrap_or_else(|e| e.into_inner())
}

/// The run in progress, if any. The overlay asks on open so closing and
/// reopening it re-attaches instead of starting a second run.
pub fn current() -> Option<ImagineRun> {
    current_slot().as_ref().map(|(run, _)| run.clone())
}

/// Ask the running Imagine to stop after its current step.
pub fn request_stop() -> Result<(), String> {
    let guard = current_slot();
    let (_, control) = guard.as_ref().ok_or("Nothing is being imagined right now.")?;
    control.cancel.store(true, Ordering::SeqCst);
    Ok(())
}

/// Queue a user hint for the running Imagine; the loop reads it before
/// its next model turn.
pub fn nudge(content: &str) -> Result<(), String> {
    let guard = current_slot();
    let (_, control) = guard.as_ref().ok_or("Nothing is being imagined right now.")?;
    control
        .inbox
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .push(content.to_string());
    Ok(())
}

/// Where loose files an Imagine run writes should land.
pub fn output_dir() -> PathBuf {
    crate::docgen::output_dir().join("Imagine")
}

/// Bob's catalog minus anything that reaches out of the device or tears
/// down what the user built. Reading, researching, and creating stay.
pub(crate) fn tools() -> serde_json::Value {
    let kept: Vec<serde_json::Value> = crate::tool_schemas::bob_tools()
        .as_array()
        .cloned()
        .unwrap_or_default()
        .into_iter()
        .filter(|t| {
            t.pointer("/function/name")
                .and_then(|v| v.as_str())
                .map(|n| {
                    !crate::tool_schemas::is_outward_tool(n)
                        && !crate::tool_schemas::is_disruptive_tool(n)
                })
                .unwrap_or(false)
        })
        .collect();
    serde_json::Value::Array(kept)
}

/// Start a run. Returns at once with the run's id; the scan, the model,
/// and the loop happen on a background task and report through events.
pub fn start(app: tauri::AppHandle, seed: Option<String>) -> Result<ImagineRun, String> {
    let seed = seed.map(|s| s.trim().to_string()).filter(|s| !s.is_empty());
    let run = ImagineRun {
        run_id: format!("imagine-{}", uuid::Uuid::new_v4()),
        seed,
        scan: Vec::new(),
        started_at: chrono::Utc::now().to_rfc3339(),
    };
    let control = ShiftControl::default();
    {
        let mut slot = current_slot();
        if slot.is_some() {
            return Err("Bob is already imagining something. Stop that run first, or wait for it.".into());
        }
        *slot = Some((run.clone(), control.clone()));
    }
    let run_id = run.run_id.clone();
    let seed = run.seed.clone();
    tauri::async_runtime::spawn(async move {
        // Every step emitted inside (including the ones deep in the tool
        // executors) routes to the overlay under the run id, never to
        // Bob's chat ribbon.
        crate::shift_monitor::STEP_SCOPE
            .scope(run_id.clone(), run_to_end(app, run_id.clone(), seed, control))
            .await;
        *current_slot() = None;
    });
    Ok(run)
}

async fn run_to_end(app: tauri::AppHandle, run_id: String, seed: Option<String>, control: ShiftControl) {
    crate::run_log::run_started(&run_id, "imagine", seed.as_deref().unwrap_or("Free rein"));
    let outcome = run(&app, &run_id, seed.as_deref(), &control).await;
    crate::bob_core::bob_step_clear(&app);
    let stopped = control.cancel.load(Ordering::SeqCst);
    let payload = match &outcome {
        Ok(result) => serde_json::json!({
            "run_id": run_id,
            "ok": true,
            "stopped": stopped,
            "result": result,
            "error": serde_json::Value::Null,
        }),
        Err(e) => serde_json::json!({
            "run_id": run_id,
            "ok": false,
            "stopped": stopped,
            "result": "",
            "error": e,
        }),
    };
    crate::event_bus::emit_ui(&app, "imagine-ended", payload).ok();
    match &outcome {
        Ok(result) if stopped => crate::run_log::run_ended(&run_id, "stopped", result),
        Ok(result) => crate::run_log::run_ended(&run_id, "done", result),
        Err(e) => crate::run_log::run_ended(&run_id, "failed", e),
    }
}

/// Scan, load the model, run the loop, land the result in the chat.
/// Returns the text the user sees.
async fn run(
    app: &tauri::AppHandle,
    run_id: &str,
    seed: Option<&str>,
    control: &ShiftControl,
) -> Result<String, String> {
    crate::bob_core::bob_step(app, "Looking at what is open…");
    let scan = scan().await.unwrap_or_default();
    let started_at = {
        let mut slot = current_slot();
        match slot.as_mut() {
            Some((run, _)) => {
                run.scan = scan.clone();
                run.started_at.clone()
            }
            None => chrono::Utc::now().to_rfc3339(),
        }
    };
    crate::event_bus::emit_ui(app, "imagine-started", ImagineRun {
        run_id: run_id.to_string(),
        seed: seed.map(str::to_string),
        scan: scan.clone(),
        started_at,
    }).ok();

    crate::bob_core::bob_step(app, "Loading model…");
    crate::bob_core::ensure_model_loaded(app).await?;
    crate::bob_core::bob_step(app, "Thinking…");

    let dir = output_dir();
    std::fs::create_dir_all(&dir).ok();
    let history = vec![Message {
        id: uuid::Uuid::new_v4().to_string(),
        role: "user".to_string(),
        content: brief(&scan, seed, &dir),
        context: None,
        agent_id: None,
        timestamp: chrono::Utc::now().to_rfc3339(),
        tool_turns: None,
    }];

    let state = app.state::<crate::AppState>();
    let client = reqwest::Client::new();
    let result = crate::bob_agentic::run_with_tools_controlled(
        &client, history, run_id, app, &state, tools(), Some(control.clone()),
    )
    .await?;

    // Land the run in the conversation: a short user turn so the thread
    // reads honestly, then Bob's reply with its evidence attached, the
    // same shape a chat turn stores, so "make that a workflow" works.
    let now = chrono::Utc::now().to_rfc3339();
    let user_msg = Message {
        id: uuid::Uuid::new_v4().to_string(),
        role: "user".to_string(),
        content: match seed {
            Some(s) => format!("Imagine: {}", s),
            None => "Imagine.".to_string(),
        },
        context: None,
        agent_id: None,
        timestamp: now.clone(),
        tool_turns: None,
    };
    let history_ctx = if result.evidence.trim().is_empty() {
        result.raw_final.clone()
    } else {
        format!(
            "{}\n\n[[evidence]]\nTool evidence gathered for this answer (reuse for follow-ups; do not contradict):\n{}",
            result.raw_final, result.evidence
        )
    };
    let reply = Message {
        id: run_id.to_string(),
        role: "assistant".to_string(),
        content: result.final_text.clone(),
        context: Some(history_ctx),
        agent_id: None,
        timestamp: now,
        tool_turns: None,
    };
    {
        let mut convos = state.chat.conversations.lock().unwrap_or_else(|e| e.into_inner());
        convos.push(user_msg.clone());
        convos.push(reply.clone());
        let conv_id = state.chat.conversation_id.lock().unwrap_or_else(|e| e.into_inner()).clone();
        let title = convos
            .first()
            .map(|m| {
                let t = m.content.chars().take(50).collect::<String>();
                if m.content.len() > 50 { format!("{}...", t) } else { t }
            })
            .unwrap_or_else(|| "Untitled".to_string());
        crate::storage::save_conversation(&conv_id, &title, &convos).ok();
    }
    crate::event_bus::emit_ui(app, "new-message", user_msg).ok();
    crate::event_bus::emit_ui(app, "new-message", reply).ok();
    crate::event_bus::emit_ui(app, "data-changed", "imagine").ok();
    crate::bob_core::mark_model_used(app).await;
    Ok(result.final_text)
}

/// The opening message: what is on screen, the freedom, and the lines
/// not to cross. Bob's normal system prompt sits underneath it.
pub(crate) fn brief(scan: &[ScannedApp], seed: Option<&str>, out_dir: &Path) -> String {
    let mut obs = String::new();
    for a in scan {
        obs.push_str(&format!("- {}", a.app));
        if !a.windows.is_empty() {
            obs.push_str(&format!(": {}", a.windows.join(" | ")));
        }
        if !a.tabs.is_empty() {
            obs.push_str(&format!("\n  tabs: {}", a.tabs.join(" | ")));
        }
        obs.push('\n');
    }
    if obs.is_empty() {
        obs.push_str("- (nothing notable is open; go by memory, the calendar, the folders, and the web)\n");
    }
    let hint = match seed.map(str::trim).filter(|s| !s.is_empty()) {
        Some(s) => format!("\nTHE USER'S ONE HINT: {}\nLet it steer what you make; everything else is still your call.\n", s),
        None => String::new(),
    };
    format!(
        "IMAGINE. No request this time: you have free rein. Look at what is going on for this user, \
pick ONE thing that would genuinely help or delight them, and make it now, in this run, with your tools.\n\n\
OPEN ON THIS DEVICE RIGHT NOW (apps, window titles, browser tabs):\n{obs}{hint}\n\
IDEAS, cheapest to boldest: a briefing from today's calendar, mail, and reminders; a panel tracking \
something they clearly care about; a document, deck, or interactive app about the work on screen; a \
workflow that automates the repetitive thing they are visibly doing; a skill that captures how they \
work; research on the site or topic they have open. Or something none of these. Surprise them, as \
long as it is real and grounded in what you actually find.\n\n\
RULES FOR THIS RUN\n\
- Explore first, then build. Read the apps, the folders, memory, the web. Then make one thing well \
rather than three things thin.\n\
- There is nobody to ask. Never pose a question and never emit an <inputs> block. Decide and act.\n\
- You may read anything. You may create documents, panels, workflows, agents, memories, and files. \
Loose files go under {dir}.\n\
- Never send, post, submit, or connect anything. Never delete, stop, or switch off what the user \
built. Never run their existing workflows or agents: creating one is fine, running it is their \
decision, so create agents and workflows without run:true.\n\
- Finish with a short note to the user: what you noticed, what you made and where it is, and one \
thing you could make next.",
        obs = obs,
        hint = hint,
        dir = out_dir.display(),
    )
}

/// Group the open windows by application and drop system noise. Cheap,
/// so it runs before the model loads and the overlay can show what Bob
/// is looking at while the model spins up.
pub async fn scan() -> Result<Vec<ScannedApp>, String> {
    // The app list comes from the PROCESS list, not window enumeration:
    // System Events only sees windows on the current Space, so a
    // fullscreen browser on its own Space would otherwise vanish from
    // the scan entirely (field failure 2026-08-02: 'nothing meaningful
    // is open' with Copilot Studio fullscreen in the browser).
    let apps = crate::os_auto::list_running_apps().await?;
    let mut by_app: Vec<(String, Vec<String>)> = apps
        .into_iter()
        .map(|a| a.name.trim().to_string())
        .filter(|a| !a.is_empty() && !NOISE_APPS.iter().any(|n| a.eq_ignore_ascii_case(n)))
        .map(|a| (a, Vec::new()))
        .collect();

    // Window titles are best-effort enrichment (current Space only).
    if let Ok(windows) = crate::os_auto::list_windows().await {
        for w in windows {
            let app = w.app_name.trim().to_string();
            let title = w.title.trim().to_string();
            if title.is_empty() {
                continue;
            }
            if let Some((_, titles)) = by_app.iter_mut().find(|(a, _)| *a == app) {
                if titles.len() < MAX_WINDOWS_PER_APP && !titles.contains(&title) {
                    titles.push(title);
                }
            }
        }
    }
    by_app.truncate(MAX_APPS);

    let mut out = Vec::new();
    for (app, windows) in by_app {
        let tabs = browser_tabs(&app).await;
        out.push(ScannedApp { app, windows, tabs });
    }
    Ok(out)
}

/// Windows: enumerate a browser's open tabs through UI Automation.
/// Chromium-family browsers (Chrome, Edge, Brave, Vivaldi, Opera, Arc)
/// and modern Firefox builds (including Zen) expose the tab strip as
/// TabItem elements. Titles only - the strip carries no URLs. Failures
/// return empty; the scan still carries the window titles.
#[cfg(target_os = "windows")]
async fn browser_tabs(app: &str) -> Vec<String> {
    const BROWSERS: [&str; 8] = [
        "chrome", "msedge", "brave", "vivaldi", "opera", "arc", "firefox", "zen",
    ];
    let a = app.to_ascii_lowercase();
    if !BROWSERS.iter().any(|b| a.contains(b)) {
        return Vec::new();
    }
    let Some(proc) = crate::window_intelligence::find_running_app(app) else {
        return Vec::new();
    };
    let pid = proc.pid as u32;
    // UIA is synchronous COM; keep the walk off the async runtime.
    tokio::task::spawn_blocking(move || {
        crate::window_intelligence::uia::tab_titles(pid, MAX_TABS_PER_BROWSER)
    })
    .await
    .unwrap_or_default()
    .into_iter()
    .filter(|t| !t.eq_ignore_ascii_case("new tab"))
    .collect()
}

#[cfg(not(any(target_os = "macos", target_os = "windows")))]
async fn browser_tabs(_app: &str) -> Vec<String> {
    Vec::new()
}

/// Enumerate a browser's open tabs (every window) via AppleScript.
/// Safari calls a tab's title "name"; the Chromium family calls it
/// "title". Non-browsers and script failures return empty - the scan
/// still carries the window titles.
#[cfg(target_os = "macos")]
async fn browser_tabs(app: &str) -> Vec<String> {
    let title_prop = match app {
        "Safari" => "name",
        "Google Chrome" | "Microsoft Edge" | "Brave Browser" | "Vivaldi" | "Opera" | "Arc" => "title",
        _ => return Vec::new(),
    };
    let script = format!(
        r#"set out to ""
tell application "{app}"
    repeat with w in windows
        repeat with t in tabs of w
            try
                set out to out & ({title_prop} of t) & "|" & (URL of t) & linefeed
            end try
        end repeat
    end repeat
end tell
return out"#
    );
    let fut = crate::os_auto::run_applescript_raw(&script);
    let raw = match tokio::time::timeout(std::time::Duration::from_secs(8), fut).await {
        Ok(Ok(o)) => o,
        _ => return Vec::new(),
    };
    raw.lines()
        .filter_map(|l| {
            let (title, url) = l.split_once('|')?;
            let title = title.trim();
            if title.is_empty() {
                return None;
            }
            Some(match compact_url(url.trim()) {
                Some(loc) => format!("{} - {}", title, loc),
                None => title.to_string(),
            })
        })
        .take(MAX_TABS_PER_BROWSER)
        .collect()
}

/// "https://app.example.com/dashboard/x?y=1" -> "app.example.com/dashboard".
/// Host plus first path segment names the system without dragging query
/// strings (or their tokens) into the prompt.
pub(crate) fn compact_url(url: &str) -> Option<String> {
    let rest = url.strip_prefix("https://").or_else(|| url.strip_prefix("http://"))?;
    let no_query = rest.split(['?', '#']).next().unwrap_or(rest);
    let mut parts = no_query.splitn(3, '/');
    let host = parts.next()?.to_string();
    if host.is_empty() {
        return None;
    }
    match parts.next().filter(|s| !s.is_empty()) {
        Some(seg) => Some(format!("{}/{}", host, seg)),
        None => Some(host),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn names(tools: &serde_json::Value) -> Vec<String> {
        tools
            .as_array()
            .unwrap()
            .iter()
            .map(|t| t["function"]["name"].as_str().unwrap().to_string())
            .collect()
    }

    #[test]
    fn catalog_keeps_creating_and_drops_sending() {
        let n = names(&tools());
        // app_bridge is the macOS AppleScript tier; bob_tools() strips it
        // on other platforms so the model never reaches for a dead path.
        #[cfg(target_os = "macos")]
        let keeps: &[&str] = &["tool", "app_bridge", "read_app_window", "research", "create_document",
                     "create_panel", "create_workflow", "create_agent", "add_memory", "use_skill"];
        #[cfg(not(target_os = "macos"))]
        let keeps: &[&str] = &["tool", "read_app_window", "research", "create_document",
                     "create_panel", "create_workflow", "create_agent", "add_memory", "use_skill"];
        for keep in keeps {
            assert!(n.iter().any(|x| x == keep), "Imagine lost {}", keep);
        }
        for drop in ["send_email", "create_calendar_event", "api_call", "connect_oauth",
                     "browse_type", "delete_workflow", "delete_agent", "run_workflow"] {
            assert!(!n.iter().any(|x| x == drop), "Imagine must not advertise {}", drop);
        }
    }

    #[test]
    fn brief_carries_observation_hint_and_lines() {
        let scan = vec![ScannedApp {
            app: "Google Chrome".into(),
            windows: vec!["HubSpot".into()],
            tabs: vec!["Deals - app.hubspot.com/contacts".into()],
        }];
        let b = brief(&scan, Some("something for sales"), Path::new("/tmp/Imagine"));
        assert!(b.contains("Google Chrome: HubSpot"));
        assert!(b.contains("app.hubspot.com/contacts"));
        assert!(b.contains("THE USER'S ONE HINT: something for sales"));
        assert!(b.contains("Never send, post, submit, or connect"));
        assert!(b.contains("/tmp/Imagine"));
        assert!(!b.contains('\u{2014}'), "no em dashes in Orbix copy");

        let empty = brief(&[], None, Path::new("/tmp/Imagine"));
        assert!(empty.contains("nothing notable is open"));
        assert!(!empty.contains("ONE HINT"));
    }

    #[test]
    fn compact_url_names_the_system_without_secrets() {
        assert_eq!(compact_url("https://app.hubspot.com/contacts/123?token=abc").as_deref(), Some("app.hubspot.com/contacts"));
        assert_eq!(compact_url("https://notion.so").as_deref(), Some("notion.so"));
        assert_eq!(compact_url("chrome://newtab"), None);
    }
}
