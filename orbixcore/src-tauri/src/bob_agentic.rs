// ─────────────────────────────────────────────
//  Bob's native agentic loop - local-first consultant
//
//  The previous bob_send path was single-turn: call LLM → execute
//  any <action> tags → done. That meant the model could never SEE
//  what its actions returned, so a file_read couldn't drive an edit
//  and a shell_exec couldn't drive an error fix. "Build me X"
//  produced confident prose because the model couldn't observe the
//  workspace.
//
//  This module wraps the same call_local_llm_streaming +
//  execute_actions_counted primitives in a real ReAct loop:
//
//      for turn in 1..=MAX_TURNS:
//          text = stream_llm(history)
//          actions, results = execute_actions(text)
//          if no actions executed: return text   (model is done)
//          history.push(assistant: text)
//          history.push(user: "Tool results:\n<formatted>\nWhat next?")
//
//  Tool results go back as native `role: tool` turns after the model's
//  own tool-call turn, so every shipped model (Gemma 4, Qwen 3.x) sees
//  them the way its chat template was trained to. Stops when the model
//  emits a turn with no tool calls, when MAX_TURNS is hit, or when
//  BOB_CANCEL_FLAG fires.
//
//  Streaming UX: chunks flow continuously across turns. The user
//  sees Bob "thinking out loud" between actions; the final
//  message-stream-end carries the consolidated, action-tag-stripped
//  text from the LAST turn. bob-step ribbon items show each tool
//  the model invoked.
// ─────────────────────────────────────────────

use std::sync::atomic::Ordering;

use serde::Serialize;
use tauri::AppHandle;
use uuid::Uuid;

use crate::llm_engine::{call_local_chat, LocalChat, NativeToolCall, CANCELLED, TOOL_RESULT_MAX_CHARS};
use crate::{
    execute_actions_counted, execute_tool_calls_counted,
    strip_post_action_decoration, strip_hallucinated_content, AppState, Message, BOB_CANCEL_FLAG,
};

const MAX_TURNS: u32 = 10;

#[derive(Debug, Clone, Serialize)]
pub struct AgentTurn {
    pub index: u32,
    pub raw_text: String,
    pub actions_attempted: usize,
    pub actions_executed: usize,
    pub actions_failed: usize,
}

#[derive(Debug, Clone, Serialize)]
pub struct AgentResult {
    /// Final reply text the user sees (cleaned, no action tags).
    pub final_text: String,
    /// Last turn's raw text (with action tags), preserved for audit.
    pub raw_final: String,
    pub turns: Vec<AgentTurn>,
    /// Compact digest of real tool output from this run (capped ~6k
    /// chars). Stored in the message's context so follow-up turns can
    /// reuse the evidence instead of inventing detail.
    pub evidence: String,
    /// True if the loop stopped because the model produced a turn
    /// with no action tags (the natural success). False = we hit
    /// MAX_TURNS or BOB_CANCEL_FLAG.
    pub completed_naturally: bool,
    /// This run's tool-call turns and results, for the reply's
    /// Message::tool_turns (see history_tool_turns).
    pub tool_turns: Vec<serde_json::Value>,
}

/// Per stored tool result when a run's turns are kept for later history:
/// enough to show the answer came from a tool, small enough that two
/// replayed runs cost little next to the tool schemas.
const HISTORY_TOOL_RESULT_CHARS: usize = 1200;

/// The run's native turns as they will be replayed in later history:
/// only assistant tool-call turns and their `role: tool` results (steer
/// messages from the monitor are dropped), results capped. Each kept
/// result's call stays directly before it, so the sequence is always one
/// the chat template accepts.
fn history_tool_turns(turns: &[serde_json::Value]) -> Vec<serde_json::Value> {
    let mut out = Vec::new();
    for t in turns {
        match t["role"].as_str() {
            Some("assistant") if t.get("tool_calls").is_some() => out.push(t.clone()),
            Some("tool") => {
                let mut t = t.clone();
                let content = t["content"].as_str().unwrap_or("").to_string();
                t["content"] = serde_json::Value::String(cap_chars(&content, HISTORY_TOOL_RESULT_CHARS));
                out.push(t);
            }
            _ => {}
        }
    }
    out
}

/// Run the loop with Bob's full tool catalog. `images` are absolute paths
/// attached to the current turn; a vision model sees them on every
/// iteration because they stay on the last stored user message while
/// the run's tool turns are appended after it.
pub async fn run(
    client: &reqwest::Client,
    history: Vec<Message>,
    msg_id: &str,
    app: &AppHandle,
    state: &AppState,
    images: &[String],
) -> Result<AgentResult, String> {
    run_loop(client, history, msg_id, app, state, crate::tool_schemas::bob_tools(), None, images).await
}

/// Same loop, but the model is advertised only `tools` - the mechanism
/// behind scoped agent shifts (P2): an employee agent gets exactly the
/// tools it was granted, nothing more.
pub async fn run_with_tools(
    client: &reqwest::Client,
    history: Vec<Message>,
    msg_id: &str,
    app: &AppHandle,
    state: &AppState,
    tools: serde_json::Value,
) -> Result<AgentResult, String> {
    run_loop(client, history, msg_id, app, state, tools, None, &[]).await
}

/// The loop with an optional per-run control channel (shift monitor).
/// With a control: the run stops on ITS cancel flag (never Bob's - the
/// user stopping Bob must not kill a background shift and vice versa),
/// user messages queued in the control's inbox are drained into the
/// history between turns, and step events go to the shift-step channel
/// so Bob's chat ribbon never shows another agent's work.
pub async fn run_with_tools_controlled(
    client: &reqwest::Client,
    history: Vec<Message>,
    msg_id: &str,
    app: &AppHandle,
    state: &AppState,
    tools: serde_json::Value,
    control: Option<crate::shift_monitor::ShiftControl>,
) -> Result<AgentResult, String> {
    run_loop(client, history, msg_id, app, state, tools, control, &[]).await
}

/// A user message arriving mid-run (shift steering). Before any tool
/// turn exists it folds into the trailing stored user turn (some chat
/// templates reject non-alternating roles); afterwards it is its own
/// user turn after the tool results.
fn push_user_turn(working: &mut Vec<Message>, turns: &mut Vec<serde_json::Value>, text: String) {
    if !turns.is_empty() {
        turns.push(serde_json::json!({ "role": "user", "content": text }));
        return;
    }
    match working.last_mut().filter(|m| m.role == "user") {
        Some(last) => last.content.push_str(&format!("\n\n{}", text)),
        None => working.push(Message {
            id: Uuid::new_v4().to_string(),
            role: "user".to_string(),
            content: text,
            context: None,
            agent_id: None,
            timestamp: chrono::Utc::now().to_rfc3339(),
            tool_turns: None,
        }),
    }
}

/// Run one native tool call through the executor. Unknown names and
/// unparseable arguments come back as [ERR] results the model reads on
/// its next turn and corrects - never dropped silently (the old bridge
/// discarded them, so a typo'd tool name ended the run with a blank
/// reply).
async fn execute_native_call(
    call: &NativeToolCall,
    state: &AppState,
    app: &AppHandle,
) -> (String, crate::ExecStats) {
    let mut failed = crate::ExecStats::default();
    failed.attempted = 1;
    failed.failed = 1;
    if !crate::tool_schemas::is_known_tool(&call.name) {
        return (
            format!(
                "**[ERR] Unknown tool `{}`.** Use one of the tools provided in this conversation.",
                call.name
            ),
            failed,
        );
    }
    match crate::llm_engine::reconstruct_tool_call(&call.name, &call.arguments) {
        Some(action) => execute_tool_calls_counted(&[action], state, app).await,
        None => (
            format!(
                "**[ERR] The arguments for `{}` were not valid JSON.** Call it again with a JSON object that matches its schema.",
                call.name
            ),
            failed,
        ),
    }
}

fn cap_chars(s: &str, max: usize) -> String {
    if s.len() <= max {
        return s.to_string();
    }
    format!("{}\n[result truncated]", crate::llm_engine::char_prefix(s, max))
}

#[allow(clippy::too_many_arguments)]
async fn run_loop(
    client: &reqwest::Client,
    history: Vec<Message>,
    msg_id: &str,
    app: &AppHandle,
    state: &AppState,
    tools: serde_json::Value,
    control: Option<crate::shift_monitor::ShiftControl>,
    images: &[String],
) -> Result<AgentResult, String> {
    let mut working = history;
    // The whole run counts as model use, tool execution included: a slide
    // deck or a research dossier between two model turns must not let
    // the idle timer unload the server the next turn needs.
    let _busy = crate::bob_core::ModelBusyGuard::new();
    // This run's native turns, appended after the stored history: the
    // model's own tool-call turns and their `role: tool` results, in the
    // exact shape its chat template renders (verified on Gemma 4 and
    // Qwen 3.5 via /apply-template). The old loop fed results back as a
    // synthetic user message and pushed the assistant turn WITHOUT its
    // calls, so on the next iteration the model saw results for calls it
    // had no record of making - on both families.
    let mut turns: Vec<serde_json::Value> = Vec::new();
    let mut turn_log: Vec<AgentTurn> = Vec::new();
    let mut last_text = String::new();
    let mut completed_naturally = false;
    // Chip markers ([[doc:...]] / [[panel:...]]) are produced inside tool
    // confirmations, which are fed back to the model - but the model's
    // final reply rarely echoes them, so the card never rendered. Collect
    // them here and append any the final reply is missing.
    let mut artifact_markers: Vec<String> = Vec::new();
    // Step labels across all turns: rendered as a visible trail in the
    // reply ("what Bob did"), so tool use is transparent without ever
    // showing raw machinery.
    let mut step_trail: Vec<String> = Vec::new();
    // Evidence digest: real tool output accumulated across the run, kept
    // so follow-up turns can build on it instead of guessing (field
    // failure 2026-08-01: "go more in depth" answered from thin air).
    let mut evidence = String::new();
    // What the last turn's tools did, past tense, for the ribbon while
    // the model reads their results ("Read Calendar. Thinking…").
    let mut last_done: Vec<String> = Vec::new();

    let record_evidence = |evidence: &mut String, text: &str| {
        if evidence.len() < 6000 {
            let room = 6000 - evidence.len();
            let mut chunk: String = text.chars().take(room).collect();
            if !chunk.trim().is_empty() {
                chunk.push('\n');
                evidence.push_str(&chunk);
            }
        }
    };

    for turn in 1..=MAX_TURNS {
        // A controlled run answers to ITS stop flag only; Bob's global
        // flag only governs Bob's own chat runs. This is what keeps the
        // main-window Stop button and the monitor's Stop button from
        // killing each other's work.
        let cancelled = match &control {
            Some(c) => c.cancel.load(Ordering::SeqCst),
            None => BOB_CANCEL_FLAG.load(Ordering::SeqCst),
        };
        if cancelled {
            bob_step(app, "Stopped by user");
            break;
        }

        // Steering: user messages typed into the monitor window since the
        // last turn join the conversation before the model speaks again.
        let steer: Vec<String> = control
            .as_ref()
            .map(|c| std::mem::take(&mut *c.inbox.lock().unwrap_or_else(|e| e.into_inner())))
            .unwrap_or_default();
        if !steer.is_empty() {
            for m in &steer {
                crate::run_log::user(m);
            }
            push_user_turn(
                &mut working,
                &mut turns,
                format!("MESSAGE FROM THE USER (steer your work accordingly): {}", steer.join("\n")),
            );
        }

        // Per-turn ribbon hint so the user sees "Turn 2…" rather than a
        // silent block of text. The frontend already renders bob-step
        // as the typing-progress indicator.
        if turn > 1 {
            let label = if last_done.is_empty() {
                "Thinking…".to_string()
            } else {
                format!("{}. Thinking…", last_done.join(", "))
            };
            bob_step(app, &label);
        }

        let cancel_flag: &std::sync::atomic::AtomicBool = match &control {
            Some(c) => &c.cancel,
            None => &BOB_CANCEL_FLAG,
        };
        let reply = match call_local_chat(
            client,
            LocalChat { history: &working, turns: &turns, tools: Some(tools.clone()), images, cancel: Some(cancel_flag) },
            msg_id,
            app,
            Some(state),
        )
        .await
        {
            Ok(r) => r,
            Err(e) if e == CANCELLED => {
                bob_step(app, "Stopped by user");
                break;
            }
            Err(e) => return Err(e),
        };
        let mut raw_text = reply.text.clone();

        // A reply that is nothing but calls written as text is a native
        // turn the server failed to parse (see text_calls.rs). Run it as
        // one; the pushed-back turn carries the calls, not the broken text.
        let mut reply = reply;
        if reply.calls.is_empty() {
            let recovered = crate::text_calls::recover(&raw_text);
            if !recovered.is_empty() {
                eprintln!("[bob] recovered {} tool call(s) written as text", recovered.len());
                reply.calls = recovered;
                raw_text.clear();
            }
        }

        if !reply.calls.is_empty() {
            // ── Native path ──
            let mut stats = crate::ExecStats::default();
            let mut confirmations = String::new();
            let mut results: Vec<serde_json::Value> = Vec::new();
            last_done.clear();
            for call in &reply.calls {
                if let Some(action) = crate::llm_engine::reconstruct_tool_call(&call.name, &call.arguments) {
                    let t = action["type"].as_str().unwrap_or("").to_string();
                    if last_done.len() < 2 {
                        last_done.push(crate::bob_core::action_done_label(&t, &action));
                    }
                }
                let (conf, st) = execute_native_call(call, state, app).await;
                stats.attempted += st.attempted;
                stats.executed += st.executed;
                stats.failed += st.failed;
                stats.repaired += st.repaired;
                stats.labels.extend(st.labels);
                confirmations.push_str(&conf);
                confirmations.push('\n');
                // What the MODEL reads back: plain results, no executor
                // markup (models copy the formats they are fed), capped
                // so one huge page can never crowd out the conversation.
                let shown = cap_chars(&sanitize_for_model(&conf), TOOL_RESULT_MAX_CHARS);
                results.push(serde_json::json!({
                    "role": "tool",
                    "tool_call_id": call.id,
                    "name": call.name,
                    "content": shown,
                }));
            }
            collect_artifact_markers(&confirmations, &mut artifact_markers);
            for label in &stats.labels {
                if step_trail.last() != Some(label) {
                    step_trail.push(label.clone());
                }
            }
            turn_log.push(AgentTurn {
                index: turn,
                raw_text: raw_text.clone(),
                actions_attempted: stats.attempted,
                actions_executed: stats.executed,
                actions_failed: stats.failed,
            });
            last_text = raw_text.clone();
            record_evidence(&mut evidence, &sanitize_for_model(&confirmations));

            let tool_calls: Vec<serde_json::Value> = reply
                .calls
                .iter()
                .map(|c| serde_json::json!({
                    "id": c.id,
                    "type": "function",
                    "function": { "name": c.name, "arguments": c.arguments },
                }))
                .collect();
            turns.push(serde_json::json!({
                "role": "assistant",
                "content": crate::strip_think_blocks(&raw_text),
                "tool_calls": tool_calls,
            }));
            turns.extend(results);
            continue;
        }

        // ── Tag fallback: the model wrote <action> tags in its text ──
        let (confirmations, stats) = execute_actions_counted(&raw_text, state, app).await;
        collect_artifact_markers(&confirmations, &mut artifact_markers);
        for label in &stats.labels {
            if step_trail.last() != Some(label) {
                step_trail.push(label.clone());
            }
        }
        turn_log.push(AgentTurn {
            index: turn,
            raw_text: raw_text.clone(),
            actions_attempted: stats.attempted,
            actions_executed: stats.executed,
            actions_failed: stats.failed,
        });
        // Display text: the executor returns the content with every
        // <action> block replaced by its confirmation or [ERR] note -
        // never show the raw tags (field failure 2026-07-31).
        last_text = confirmations.clone();

        if stats.executed == 0 {
            // No tools ran this turn - the model considers itself done.
            // But a monitor message that arrived while it was finishing
            // must not fall on the floor: hand it over and keep going.
            let late_steer: Vec<String> = control
                .as_ref()
                .map(|c| std::mem::take(&mut *c.inbox.lock().unwrap_or_else(|e| e.into_inner())))
                .unwrap_or_default();
            if late_steer.is_empty() {
                completed_naturally = true;
                break;
            }
            for m in &late_steer {
                crate::run_log::user(m);
            }
            turns.push(serde_json::json!({ "role": "assistant", "content": raw_text }));
            turns.push(serde_json::json!({
                "role": "user",
                "content": format!(
                    "MESSAGE FROM THE USER (steer your work accordingly): {}",
                    late_steer.join("\n")
                ),
            }));
            continue;
        }

        last_done = vec![format!("Ran {} action{}", stats.executed, if stats.executed == 1 { "" } else { "s" })];
        let mut results_summary = cap_chars(&sanitize_for_model(&confirmations), TOOL_RESULT_MAX_CHARS);
        if stats.failed > 0 {
            results_summary.push_str(&format!("\n({} of {} actions failed - see above)", stats.failed, stats.attempted));
        }
        record_evidence(&mut evidence, &results_summary);
        turns.push(serde_json::json!({ "role": "assistant", "content": raw_text }));
        turns.push(serde_json::json!({
            "role": "user",
            "content": format!(
                "Tool results from your last turn:\n\n{}\n\n\
                 If the task is complete, reply with the final \
                 user-facing answer and call no more tools. \
                 Otherwise take the next action now.",
                results_summary,
            ),
        }));
    }

    // The display text is the last turn with action decoration stripped.
    let clean = |s: &str| {
        crate::enforce_reply_contract(&strip_post_action_decoration(
            &crate::support::strip_action_tags(&strip_hallucinated_content(
                &crate::strip_think_blocks(s),
            )),
        ))
    };
    let mut final_text = clean(&last_text);

    // Synthesis retry: a turn that ended on tool calls (or on hidden
    // reasoning only) cleans down to nothing. One extra model call asking
    // for the answer beats showing private deliberation or an empty
    // bubble; local models land it on the second ask almost every time.
    let was_cancelled = match &control {
        Some(c) => c.cancel.load(Ordering::SeqCst),
        None => BOB_CANCEL_FLAG.load(Ordering::SeqCst),
    };
    if was_cancelled && stub_reply(&final_text) {
        final_text = if evidence.trim().is_empty() {
            "Stopped.".to_string()
        } else {
            format!("Stopped. What I had gathered so far:\n\n```\n{}\n```", present_evidence(&evidence))
        };
    }
    if !was_cancelled && stub_reply(&final_text) && !turn_log.is_empty() {
        bob_step(app, "Writing the final answer…");
        turns.push(serde_json::json!({
            "role": "user",
            "content": "Write your final user-facing answer now, based on the work above. \
                        If there are tool results above, answer directly from them. \
                        Plain text only - no tool calls, no reasoning, no metadata.",
        }));
        let cancel_flag: &std::sync::atomic::AtomicBool = match &control {
            Some(c) => &c.cancel,
            None => &BOB_CANCEL_FLAG,
        };
        if let Ok(retry) = call_local_chat(
            client,
            LocalChat { history: &working, turns: &turns, tools: None, images, cancel: Some(cancel_flag) },
            msg_id,
            app,
            Some(state),
        )
        .await
        {
            final_text = clean(&retry.text);
        }
        if stub_reply(&final_text) {
            // Never dead-end the user (field failure 2026-08-03: the old
            // "ask me to continue" line looped - continue ran the tools
            // again and hit the same snag). If the run gathered evidence,
            // SHOW it: raw results beat a shrug. Only a run with nothing
            // gathered gets an apology, and one that owns the failure.
            final_text = if evidence.trim().is_empty() {
                "I couldn't put an answer together for that one - my fault, not yours. \
                 Try asking again, ideally a bit more specifically."
                    .to_string()
            } else {
                format!(
                    "I had trouble writing this up cleanly, so here is what the tools actually returned:\n\n```\n{}\n```",
                    present_evidence(&evidence)
                )
            };
        }
    }

    // Append any artifact chips the tools produced that the model's final
    // reply left out, so a generated document/panel always shows in chat.
    for marker in &artifact_markers {
        if !final_text.contains(marker.as_str()) {
            if !final_text.trim().is_empty() {
                final_text.push_str("\n\n");
            }
            final_text.push_str(marker);
        }
    }

    // Step trail: prepend "[[steps:a|b|c]]" so the frontend renders a
    // quiet what-Bob-did strip above the reply. Labels are sanitised of
    // marker syntax and capped - transparency, not a log dump.
    if !step_trail.is_empty() {
        let shown: Vec<String> = step_trail
            .iter()
            .take(6)
            .map(|l| l.replace(['[', ']', '|'], " ").trim().to_string())
            .filter(|l| !l.is_empty())
            .collect();
        if !shown.is_empty() {
            let extra = step_trail.len().saturating_sub(6);
            let mut marker = format!("[[steps:{}]]", shown.join("|"));
            if extra > 0 {
                marker = format!("[[steps:{}|+{} more]]", shown.join("|"), extra);
            }
            final_text = format!("{}\n\n{}", marker, final_text);
        }
    }

    Ok(AgentResult {
        final_text,
        raw_final: last_text,
        turns: turn_log,
        completed_naturally,
        evidence,
        tool_turns: history_tool_turns(&turns),
    })
}

/// Pull `[[doc:...]]` / `[[panel:...]]` chip markers out of a tool
/// confirmation string so they can be re-attached to the final reply.
fn collect_artifact_markers(confirmations: &str, out: &mut Vec<String>) {
    let mut rest = confirmations;
    while let Some(open) = rest.find("[[") {
        let after = &rest[open..];
        let Some(close) = after.find("]]") else { break };
        let marker = &after[..close + 2];
        if (marker.starts_with("[[doc:") || marker.starts_with("[[panel:"))
            && !out.iter().any(|m| m == marker)
        {
            out.push(marker.to_string());
        }
        rest = &after[close + 2..];
    }
}

/// Strip executor presentation from text destined for MODEL context:
/// bold/backtick markup and OK-confirmation headers become plain
/// "tool: result" lines. Models copy the formats they are fed; feed
/// them data, not decoration.
pub(crate) fn sanitize_for_model(confirmations: &str) -> String {
    let mut out = String::with_capacity(confirmations.len());
    for line in confirmations.lines() {
        let t = line.trim_start();
        if t.contains("action(s) attempted") && t.contains("executed") {
            continue;
        }
        let cleaned = t.replace("**", "").replace('`', "");
        // "OK: web_search" header lines become "web_search results:"
        let cleaned = if let Some(rest) = cleaned.strip_prefix("OK: ") {
            format!("{} result:", rest.trim())
        } else {
            cleaned
        };
        out.push_str(&cleaned);
        out.push('\n');
    }
    out.trim().to_string()
}

/// Evidence digest -> something a human can read in a chat bubble:
/// drop the executor's bookkeeping headers and markup, cap the length.
pub(crate) fn present_evidence(evidence: &str) -> String {
    let mut out = String::new();
    for line in evidence.lines() {
        let t = line.trim();
        // Skip the accounting header ("2 action(s) attempted · ...") and
        // fence markers from the confirmations; keep real content.
        if t.is_empty()
            || t.starts_with("```")
            || (t.contains("action(s) attempted") && t.contains("executed"))
        {
            continue;
        }
        let cleaned = t.replace("**", "").replace('`', "");
        out.push_str(&cleaned);
        out.push('\n');
        if out.len() > 1500 {
            out.push_str("[truncated]");
            break;
        }
    }
    out.trim().to_string()
}

/// A stub is as useless as an empty reply: small models sometimes end
/// on a fake confirmation ("**OK: `web_search`**") parroting the
/// executor's format instead of answering (field failures 2026-08-01
/// and 2026-08-02 - the first gate missed the leading ** and didn't
/// know executor tool names). Stubs trigger the synthesis retry.
pub(crate) fn stub_reply(t: &str) -> bool {
    let t = t.trim();
    if t.is_empty() {
        return true;
    }
    if t.len() >= 80 {
        return false;
    }
    // Markdown decoration must not hide an ack: strip surrounding symbols.
    let lower = t.to_lowercase();
    let bare = lower.trim_matches(|c: char| !c.is_alphanumeric());
    let acky = ["ok", "okay", "done", "finished", "completed"]
        .iter()
        .any(|a| bare == *a || bare.starts_with(&format!("{}:", a)) || bare.starts_with(&format!("{}.", a)) || bare.starts_with(&format!("{} ", a)));
    // Both action types (create_workflow...) and executor tool names
    // (web_search...) count as "just names a tool".
    const EXEC_TOOLS: &[&str] = &[
        "web_search", "web_fetch", "file_read", "file_write", "file_list",
        "shell_exec", "screen_capture", "app_bridge", "spreadsheet_read",
    ];
    let names_tool = t
        .split(|c: char| !c.is_alphanumeric() && c != '_')
        .any(|w| !w.is_empty() && (crate::tool_schemas::is_known_tool(w) || EXEC_TOOLS.contains(&w)));
    acky || names_tool
}

/// Very compact summary of what just ran. Kept for the audit view; the
/// loop now feeds the model the real (capped) results instead.
#[allow(dead_code)]
fn summarise_results(raw_text: &str, stats: &crate::ExecStats) -> String {
    let mut out = String::new();
    out.push_str(&format!(
        "{} action(s) attempted · {} executed · {} failed{}\n",
        stats.attempted,
        stats.executed,
        stats.failed,
        if stats.repaired > 0 {
            format!(" · {} JSON-repaired", stats.repaired)
        } else { String::new() },
    ));
    // Echo the action types the model emitted so it can correlate.
    // Cap the list to keep this digestible.
    let mut types: Vec<String> = Vec::new();
    let mut cur = 0;
    while let Some(open) = raw_text[cur..].find("<action>") {
        let abs = cur + open + 8;
        let close = raw_text[abs..].find("</action>").map(|i| abs + i).unwrap_or(raw_text.len());
        if let Ok(v) = serde_json::from_str::<serde_json::Value>(raw_text[abs..close].trim()) {
            if let Some(t) = v.get("type").and_then(|s| s.as_str()) {
                types.push(t.to_string());
            }
        }
        cur = close + 9;
        if types.len() >= 12 { break; }
    }
    if !types.is_empty() {
        out.push_str("Actions in this turn: ");
        out.push_str(&types.join(", "));
    }
    out
}

fn bob_step(app: &AppHandle, label: &str) {
    // Shared emitter: redirects to the shift monitor when this task is
    // inside a shift's STEP_SCOPE.
    crate::bob_core::bob_step(app, label);
}

#[cfg(test)]
mod stub_reply_tests {
    use super::{present_evidence, stub_reply};

    #[test]
    fn evidence_presents_clean_without_machinery() {
        let ev = "2 action(s) attempted · 2 executed · 0 failed\n\n**OK: `app_bridge`**\n```\n{\"events\": [\"Standup 09:00\", \"Board call 14:00\"]}\n```\n";
        let shown = present_evidence(ev);
        assert!(shown.contains("Standup 09:00"));
        assert!(!shown.contains("action(s) attempted"));
        assert!(!shown.contains("**"));
        assert!(!shown.contains("```"));
    }

    #[test]
    fn confirmation_parrots_are_stubs() {
        assert!(stub_reply("**OK: `web_search`**"));
        assert!(stub_reply("OK: web_search"));
        assert!(stub_reply("Done."));
        assert!(stub_reply(""));
        assert!(stub_reply("  **Finished** "));
    }

    #[test]
    fn real_answers_are_not_stubs() {
        assert!(!stub_reply("The latest patch is 26.15, focused on support items."));
        assert!(!stub_reply("Patch 26.15 shipped on 28 July with jungle changes, item reworks, and a new skin line - details below."));
        assert!(!stub_reply("Patch 26.15 is the newest."));
    }
}

#[cfg(test)]
mod display_guard_tests {
    #[test]
    fn bare_tool_json_is_promoted_to_action() {
        let out = crate::llm_engine::promote_bare_tool_json(
            "I will try searching again.\n{\"tool\":\"web_search\",\"args\":{\"query\":\"latest on local LLMs\"}}",
        );
        assert!(out.contains("<action>{\"tool\":\"web_search\""), "{}", out);
        assert!(out.contains("I will try searching again."));
    }

    #[test]
    fn ordinary_json_is_left_alone() {
        let text = "Here is the config:\n{\"port\": 8847, \"host\": \"localhost\"}";
        let out = crate::llm_engine::promote_bare_tool_json(text);
        assert!(!out.contains("<action>"), "{}", out);
    }

    #[test]
    fn residual_action_tags_never_reach_display() {
        let cleaned = crate::support::strip_action_tags(
            "Trying again.\n\n<action>{\"tool\":\"web_search\",\"args\":{}}</action>",
        );
        assert_eq!(cleaned, "Trying again.");
        // Unclosed tag: everything from the tag on is machinery.
        let cleaned = crate::support::strip_action_tags("Answer.\n<action>{\"tool\":\"x\"");
        assert_eq!(cleaned, "Answer.");
    }
}

#[cfg(test)]
mod history_tool_turn_tests {
    use super::{history_tool_turns, HISTORY_TOOL_RESULT_CHARS};
    use serde_json::json;

    #[test]
    fn keeps_calls_and_capped_results_drops_steer_turns() {
        let turns = vec![
            json!({"role": "assistant", "content": "", "tool_calls": [{"id": "c1"}]}),
            json!({"role": "tool", "tool_call_id": "c1", "content": "r".repeat(5000)}),
            json!({"role": "user", "content": "monitor: also check the tests"}),
            json!({"role": "assistant", "content": "", "tool_calls": [{"id": "c2"}]}),
            json!({"role": "tool", "tool_call_id": "c2", "content": "ok"}),
        ];
        let out = history_tool_turns(&turns);
        let roles: Vec<&str> = out.iter().map(|t| t["role"].as_str().unwrap()).collect();
        assert_eq!(roles, ["assistant", "tool", "assistant", "tool"]);
        assert!(out[1]["content"].as_str().unwrap().len() < HISTORY_TOOL_RESULT_CHARS + 40);
        assert_eq!(out[3]["content"], "ok");
    }
}

#[cfg(test)]
mod artifact_marker_tests {
    use super::collect_artifact_markers;

    #[test]
    fn collects_doc_and_panel_markers_deduped() {
        let mut out = Vec::new();
        collect_artifact_markers(
            "**Created a branded docx document** ...\n\n[[doc:abc-1|docx|Q3 Report]]",
            &mut out,
        );
        // A later turn re-echoes the same marker plus a panel one.
        collect_artifact_markers(
            "note [[doc:abc-1|docx|Q3 Report]] and [[panel:p-9|Metrics]]",
            &mut out,
        );
        assert_eq!(out.len(), 2);
        assert_eq!(out[0], "[[doc:abc-1|docx|Q3 Report]]");
        assert_eq!(out[1], "[[panel:p-9|Metrics]]");
    }

    #[test]
    fn ignores_non_artifact_brackets() {
        let mut out = Vec::new();
        collect_artifact_markers("array [[1, 2]] and [[link:x]]", &mut out);
        assert!(out.is_empty());
    }

    #[test]
    fn tolerates_unclosed_marker() {
        let mut out = Vec::new();
        collect_artifact_markers("dangling [[doc:oops", &mut out);
        assert!(out.is_empty());
    }
}
