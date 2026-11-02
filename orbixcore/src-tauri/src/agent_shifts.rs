// ─────────────────────────────────────────────
//  Agent shifts (P2) - agents as employees
//
//  An agent with a charter is an employee, not a model connection.
//  A SHIFT runs Bob's agentic loop scoped to exactly the tools the
//  agent was granted: the charter is its standing instruction, the
//  outcome is filed as an Inbox notification and remembered in a
//  per-agent memory namespace, and last_run_at is stamped so the
//  scheduler tick knows when the next shift is due.
//
//  Lean by design: shifts reuse bob_agentic::run_with_tools (no second
//  loop implementation), reports are short summaries (never payload
//  dumps), and scheduled shifts run sequentially so the local model is
//  never contended by a fleet waking at once.
// ─────────────────────────────────────────────

use tauri::Manager;

use crate::{AppState, Message};

/// Safe default grants for employees with no explicit tool list:
/// read-only awareness, nothing that mutates or sends.
const DEFAULT_GRANTS: &[&str] = &[
    "search_memory",
    "recent_activity",
    "list_calendar_events",
    "list_notifications",
    "list_connectors",
];

/// Filter the full bob tool universe down to this agent's grants.
pub fn scoped_tools(granted: &[String]) -> serde_json::Value {
    let all = crate::tool_schemas::bob_tools();
    let allow: Vec<&str> = if granted.is_empty() {
        DEFAULT_GRANTS.to_vec()
    } else {
        granted.iter().map(|s| s.as_str()).collect()
    };
    let filtered: Vec<serde_json::Value> = all
        .as_array()
        .cloned()
        .unwrap_or_default()
        .into_iter()
        .filter(|t| {
            t.pointer("/function/name")
                .and_then(|v| v.as_str())
                .map(|n| allow.contains(&n))
                .unwrap_or(false)
        })
        .collect();
    serde_json::Value::Array(filtered)
}

/// The grantable tool catalog for the UI: (name, description) straight
/// from the bob tool schemas so the checkbox list can never drift from
/// what shifts actually advertise.
pub fn grantable_tools() -> Vec<serde_json::Value> {
    crate::tool_schemas::bob_tools()
        .as_array()
        .cloned()
        .unwrap_or_default()
        .into_iter()
        .filter_map(|t| {
            let name = t.pointer("/function/name")?.as_str()?.to_string();
            let description = t
                .pointer("/function/description")
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string();
            Some(serde_json::json!({ "name": name, "description": description }))
        })
        .collect()
}

/// Run one shift for `agent_id`. Returns the report text.
pub async fn run_shift(
    agent_id: &str,
    state: &AppState,
    app: &tauri::AppHandle,
) -> Result<String, String> {
    let agent = {
        let agents = state.chat.agents.lock().unwrap_or_else(|e| e.into_inner());
        agents
            .iter()
            .find(|a| a.id == agent_id)
            .cloned()
            .ok_or_else(|| format!("Agent {} not found", agent_id))?
    };
    if !agent.enabled {
        return Err(format!("Agent '{}' is disabled", agent.name));
    }
    if agent.charter.trim().is_empty() {
        return Err(format!(
            "Agent '{}' has no charter. Give it a job description first.",
            agent.name
        ));
    }

    // The shift conversation: the charter as the standing instruction,
    // one user turn that opens the shift. A distinct message id keeps
    // the streamed chunks out of the chat UI (which only updates
    // messages it created).
    let msg_id = format!("agent-shift-{}", uuid::Uuid::new_v4());
    let now = chrono::Utc::now().to_rfc3339();
    let system = format!(
        "You are {name}, an autonomous employee agent inside Orbix Core.\n\
         YOUR CHARTER (standing job description):\n{charter}\n\n\
         You are on a scheduled shift with no human watching. Work the charter \
         with the tools you have been granted. Do not ask questions - use \
         sensible defaults. When done, write a SHIFT REPORT: 2-6 short lines of \
         what you checked, what you did, and anything that needs the user's \
         attention. If nothing needed doing, one line saying so is a good report.",
        name = agent.name,
        charter = agent.charter.trim(),
    );
    let history = vec![
        Message {
            id: uuid::Uuid::new_v4().to_string(),
            role: "system".into(),
            content: system,
            context: None,
            agent_id: Some(agent.id.clone()),
            timestamp: now.clone(),
            tool_turns: None,
        },
        Message {
            id: uuid::Uuid::new_v4().to_string(),
            role: "user".into(),
            content: format!("Begin your shift now. Current time: {}.", now),
            context: None,
            agent_id: Some(agent.id.clone()),
            timestamp: now.clone(),
            tool_turns: None,
        },
    ];

    // Register the live session (monitor window control channel), open
    // the monitor beside the main window, and run the loop inside the
    // shift's STEP_SCOPE so every step event routes to the monitor
    // instead of Bob's chat ribbon.
    let control = crate::shift_monitor::register(&msg_id, &agent.id, &agent.name);
    crate::event_bus::emit_ui(app, "shift-started", serde_json::json!({
        "shift_id": msg_id,
        "agent_id": agent.id,
        "agent_name": agent.name,
        "started_at": now,
    })).ok();
    crate::shift_monitor::open_window_for_run(app);

    crate::run_log::run_started(&msg_id, "shift", &agent.name);
    let client = reqwest::Client::new();
    let tools = scoped_tools(&agent.granted_tools);
    let result = crate::shift_monitor::STEP_SCOPE
        .scope(
            msg_id.clone(),
            crate::bob_agentic::run_with_tools_controlled(
                &client, history, &msg_id, app, state, tools, Some(control.clone()),
            ),
        )
        .await;
    crate::shift_monitor::unregister(&msg_id);
    let result = match result {
        Ok(r) => r,
        Err(e) => {
            crate::event_bus::emit_ui(app, "shift-ended", serde_json::json!({
                "shift_id": msg_id,
                "agent_name": agent.name,
                "ok": false,
                "stopped": false,
                "report": e,
            })).ok();
            crate::run_log::run_ended(&msg_id, "failed", &e);
            return Err(e);
        }
    };
    let report = result.final_text.trim().to_string();
    crate::run_log::run_ended(
        &msg_id,
        if control.cancel.load(std::sync::atomic::Ordering::SeqCst) { "stopped" } else { "done" },
        &report,
    );
    crate::event_bus::emit_ui(app, "shift-ended", serde_json::json!({
        "shift_id": msg_id,
        "agent_name": agent.name,
        "ok": true,
        "stopped": control.cancel.load(std::sync::atomic::Ordering::SeqCst),
        "report": report,
    })).ok();

    // Stamp the clock first so a failing report path can't cause a
    // scheduled agent to run hot in a loop.
    {
        let mut agents = state.chat.agents.lock().unwrap_or_else(|e| e.into_inner());
        if let Some(a) = agents.iter_mut().find(|a| a.id == agent_id) {
            a.last_run_at = Some(chrono::Utc::now().to_rfc3339());
        }
        crate::storage::save_agents(&agents).ok();
    }

    // File the report: Inbox notification + per-agent memory namespace.
    {
        let mut store = state
            .connect
            .notification_store
            .lock()
            .unwrap_or_else(|e| e.into_inner());
        let mut notif = crate::notifications::Notification::new(
            crate::notifications::NotificationSource::Schedule,
            crate::notifications::NotificationLevel::Info,
            format!("Shift report: {}", agent.name),
            if report.is_empty() {
                "(the agent produced no report)".to_string()
            } else {
                report.clone()
            },
        );
        notif.action_panel = Some("agents".into());
        notif.action_id = Some(agent.id.clone());
        store.add(notif);
    }
    {
        let mut mem = state.chat.memory.lock().unwrap_or_else(|e| e.into_inner());
        let truncated: String = report.chars().take(2000).collect();
        mem.add(
            format!("shift:{}:{}", agent.name.to_lowercase().replace(' ', "_"), now),
            truncated,
            crate::memory::MemoryCategory::Context,
            format!("agent:{}", agent.id),
        );
        mem.save().ok();
    }

    crate::event_bus::emit_ui(app, "data-changed", "agent-shift-complete").ok();
    Ok(report)
}

/// Background ticker: wake agents whose interval has elapsed. Runs due
/// shifts sequentially - a fleet waking at once must not contend for
/// the local model.
pub fn spawn(app: tauri::AppHandle) {
    tauri::async_runtime::spawn(async move {
        loop {
            tokio::time::sleep(std::time::Duration::from_secs(60)).await;
            let due: Vec<String> = {
                let state = app.state::<AppState>();
                let agents = state.chat.agents.lock().unwrap_or_else(|e| e.into_inner());
                agents
                    .iter()
                    .filter(|a| a.enabled && !a.charter.trim().is_empty())
                    .filter(|a| match a.schedule_minutes {
                        Some(m) if m > 0 => match &a.last_run_at {
                            None => true,
                            Some(ts) => chrono::DateTime::parse_from_rfc3339(ts)
                                .map(|t| {
                                    let elapsed = chrono::Utc::now().signed_duration_since(t);
                                    elapsed.num_minutes() >= m as i64
                                })
                                .unwrap_or(true),
                        },
                        _ => false,
                    })
                    .map(|a| a.id.clone())
                    .collect()
            };
            for id in due {
                let state = app.state::<AppState>();
                if let Err(e) = run_shift(&id, &state, &app).await {
                    eprintln!("[agent_shifts] shift for {} failed: {}", id, e);
                }
            }
        }
    });
}
