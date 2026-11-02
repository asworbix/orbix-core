// ─────────────────────────────────────────────
//  Run log - one timeline of everything a run does
//
//  Every kind of run (a Bob chat turn, an Imagine run, an agent shift, a
//  workflow run) already emits events for its own UI. The run log is the
//  one stream that carries all of them as small, uniform entries: run
//  started and ended, each step label, each tool call with its outcome
//  and a capped excerpt of the result, each model turn, each egress
//  decision, each error, each message the user sent mid-run. The Run
//  monitor window renders it live; a ring buffer keeps the last entries
//  so a window opened mid-run backfills instead of starting blank.
//
//  Entries attach to a run id: the task's step scope when inside a shift
//  or an Imagine run, otherwise the run marked active (a chat turn or a
//  workflow run), otherwise "orbix". Nothing here blocks or fails a run:
//  the log is a side channel.
// ─────────────────────────────────────────────

use std::collections::VecDeque;
use std::sync::{Mutex, MutexGuard, OnceLock};

use serde::Serialize;

const CAPACITY: usize = 2000;
const TEXT_CAP: usize = 400;
const DETAIL_CAP: usize = 1500;

#[derive(Clone, Debug, Serialize)]
pub struct RunLogEntry {
    pub seq: u64,
    pub ts: String,
    /// The run this belongs to: a chat message id, "imagine-...",
    /// "agent-shift-...", or a workflow run id.
    pub run_id: String,
    /// run | step | tool | model | egress | error | user
    pub kind: &'static str,
    /// info | ok | warn | error
    pub level: &'static str,
    pub text: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub detail: Option<String>,
    /// For kind "run": chat | imagine | shift | workflow.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub run_kind: Option<String>,
    /// For kind "run": started | done | failed | stopped.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub status: Option<&'static str>,
}

struct Buffer {
    seq: u64,
    entries: VecDeque<RunLogEntry>,
    active: Option<String>,
}

static BUF: Mutex<Buffer> = Mutex::new(Buffer { seq: 0, entries: VecDeque::new(), active: None });
static APP: OnceLock<tauri::AppHandle> = OnceLock::new();

fn buf() -> MutexGuard<'static, Buffer> {
    BUF.lock().unwrap_or_else(|e| e.into_inner())
}

/// Called once from setup; until then entries are buffered, not emitted.
pub fn init(app: tauri::AppHandle) {
    let _ = APP.set(app);
}

fn cap(s: String, max: usize) -> String {
    if s.chars().count() <= max {
        return s;
    }
    let cut: String = s.chars().take(max).collect();
    format!("{}…", cut.trim_end())
}

/// The run that entries without an explicit id attach to.
pub fn current_run_id() -> String {
    if let Some(id) = crate::shift_monitor::current_scope() {
        return id;
    }
    buf().active.clone().unwrap_or_else(|| "orbix".to_string())
}

/// Mark a run as the one plain steps and tool calls belong to (a chat
/// turn or a workflow run; shifts and Imagine carry a task scope instead).
pub fn set_active(run_id: &str) {
    buf().active = Some(run_id.to_string());
}

pub fn clear_active(run_id: &str) {
    let mut b = buf();
    if b.active.as_deref() == Some(run_id) {
        b.active = None;
    }
}

pub fn emit(
    run_id: &str,
    kind: &'static str,
    level: &'static str,
    text: impl Into<String>,
    detail: Option<String>,
    run_kind: Option<String>,
    status: Option<&'static str>,
) {
    let text = cap(text.into(), TEXT_CAP);
    let entry = {
        let mut b = buf();
        // Consecutive identical step labels (streaming previews repeat
        // while arguments arrive) collapse into one line.
        if kind == "step" {
            if let Some(last) = b.entries.back() {
                if last.kind == "step" && last.run_id == run_id && last.text == text {
                    return;
                }
            }
        }
        b.seq += 1;
        let entry = RunLogEntry {
            seq: b.seq,
            ts: chrono::Utc::now().to_rfc3339(),
            run_id: run_id.to_string(),
            kind,
            level,
            text,
            detail: detail.map(|d| cap(d, DETAIL_CAP)).filter(|d| !d.trim().is_empty()),
            run_kind,
            status,
        };
        if b.entries.len() >= CAPACITY {
            b.entries.pop_front();
        }
        b.entries.push_back(entry.clone());
        entry
    };
    if let Some(app) = APP.get() {
        crate::event_bus::emit_ui(app, "run-log", &entry).ok();
    }
}

pub fn run_started(run_id: &str, run_kind: &str, title: &str) {
    emit(run_id, "run", "info", title, None, Some(run_kind.to_string()), Some("started"));
}

/// `status`: done | failed | stopped.
pub fn run_ended(run_id: &str, status: &'static str, summary: &str) {
    let level = match status {
        "done" => "ok",
        "stopped" => "warn",
        _ => "error",
    };
    emit(run_id, "run", level, summary, None, None, Some(status));
}

pub fn step(label: &str) {
    if label.trim().is_empty() {
        return;
    }
    emit(&current_run_id(), "step", "info", label, None, None, None);
}

/// One tool call: its label, whether it worked, and what came back.
pub fn tool(label: &str, ok: bool, result: &str) {
    emit(
        &current_run_id(),
        "tool",
        if ok { "ok" } else { "error" },
        label,
        Some(strip_markup(result)),
        None,
        None,
    );
}

pub fn model(run_id: &str, text: impl Into<String>) {
    emit(run_id, "model", "info", text, None, None, None);
}

pub fn egress(allowed: bool, text: impl Into<String>) {
    emit(&current_run_id(), "egress", if allowed { "ok" } else { "warn" }, text, None, None, None);
}

pub fn error(text: impl Into<String>) {
    emit(&current_run_id(), "error", "error", text, None, None, None);
}

/// A message the user sent into a running shift or Imagine.
pub fn user(text: &str) {
    emit(&current_run_id(), "user", "info", text, None, None, None);
}

pub fn recent(limit: usize) -> Vec<RunLogEntry> {
    let b = buf();
    let skip = b.entries.len().saturating_sub(limit);
    b.entries.iter().skip(skip).cloned().collect()
}

/// Executor confirmations carry markdown for the chat bubble; the log
/// wants the plain text.
fn strip_markup(s: &str) -> String {
    s.replace("**", "").replace("```", "").trim().to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn entries_attach_to_the_active_run_and_backfill_in_order() {
        set_active("run-a");
        run_started("run-a", "chat", "what is on my calendar");
        step("Reading Calendar…");
        step("Reading Calendar…"); // duplicate collapses
        tool("Read Calendar", true, "**OK: `app_bridge`**\n3 events");
        run_ended("run-a", "done", "3 events today");
        clear_active("run-a");
        let recent = recent(10);
        let mine: Vec<&RunLogEntry> = recent.iter().filter(|e| e.run_id == "run-a").collect();
        assert_eq!(mine.iter().filter(|e| e.kind == "step").count(), 1);
        let tool = mine.iter().find(|e| e.kind == "tool").unwrap();
        assert_eq!(tool.level, "ok");
        assert!(tool.detail.as_deref().unwrap().starts_with("OK: `app_bridge`"));
        let ended = mine.last().unwrap();
        assert_eq!(ended.status, Some("done"));
        assert!(mine.windows(2).all(|w| w[0].seq < w[1].seq));
        assert_eq!(current_run_id(), "orbix");
    }

    #[test]
    fn long_text_is_capped_not_dropped() {
        let long = "x".repeat(5000);
        emit("run-b", "tool", "ok", long.clone(), Some(long.clone()), None, None);
        let e = recent(1).pop().unwrap();
        assert!(e.text.chars().count() <= TEXT_CAP + 1);
        assert!(e.detail.unwrap().chars().count() <= DETAIL_CAP + 1);
    }
}
