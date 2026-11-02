//! Commands split out of main.rs (W0.3). Mechanical move, no behavior change.

#[allow(unused_imports)]
use crate::prelude::*;
#[allow(unused_imports)]
use crate::*;

// ─────────────────────────────────────────────
//  Commands - Bob Send (local LLM only - Orbix is local-first)
// ─────────────────────────────────────────────

/// Save the in-memory conversation after a Bob reply. Every reply path
/// calls this: the agentic path (most turns) used to rely on the window's
/// beforeunload, so a headless app, a crash or a menu-bar quit lost it.
fn autosave_conversation(state: &AppState) {
    let convos = state.chat.conversations.lock().unwrap_or_else(|e| e.into_inner());
    let conv_id = state.chat.conversation_id.lock().unwrap_or_else(|e| e.into_inner()).clone();
    let title = convos.first()
        .map(|m| { let t = m.content.chars().take(50).collect::<String>(); if m.content.len() > 50 { format!("{}...", t) } else { t } })
        .unwrap_or_else(|| "Untitled".to_string());
    storage::save_conversation(&conv_id, &title, &convos).ok();
}

#[tauri::command]
pub(crate) async fn bob_send(content: String, images: Option<Vec<String>>, state: State<'_, AppState>, app: tauri::AppHandle) -> Result<Message, String> {
    let result = bob_send_inner(content, images, state, app).await;
    // Run monitor: close the turn's entry whichever way it ended.
    match &result {
        Ok(reply) => {
            let status = if BOB_CANCEL_FLAG.load(std::sync::atomic::Ordering::SeqCst) { "stopped" } else { "done" };
            crate::run_log::run_ended(&reply.id, status, &crate::llm_engine::short(&reply.content, 160));
            crate::run_log::clear_active(&reply.id);
        }
        Err(e) => {
            let id = crate::run_log::current_run_id();
            crate::run_log::run_ended(&id, "failed", e);
            crate::run_log::clear_active(&id);
        }
    }
    result
}

async fn bob_send_inner(content: String, images: Option<Vec<String>>, state: State<'_, AppState>, app: tauri::AppHandle) -> Result<Message, String> {
    // Dropped images (absolute paths) for THIS turn. A vision model inlines
    // them; anything else is guided to switch models below.
    let images_vec: Vec<String> = images
        .unwrap_or_default()
        .into_iter()
        .filter(|p| !p.trim().is_empty())
        .collect();
    let has_images = !images_vec.is_empty();

    // Store user message
    let user_msg = Message {
        id: Uuid::new_v4().to_string(),
        role: "user".to_string(),
        content: content.clone(),
        context: None,
        agent_id: None,
        timestamp: Utc::now().to_rfc3339(),
        tool_turns: None,
    };
    {
        let mut convos = state.chat.conversations.lock().map_err(|e| e.to_string())?;
        convos.push(user_msg.clone());
    }
    // A turn from outside the window (terminal, quick bar, a paired app):
    // the window did not draw this bubble itself, so tell it.
    if crate::turn_scope::origin().is_some() {
        crate::event_bus::emit_ui(&app, "new-message", user_msg.clone()).ok();
    }

    // ── COMMAND ROUTER ──────────────────────────────────────────────
    // Try to handle this as a deterministic command first. ~70% of real
    // user messages are simple commands ("list agents", "run workflow X",
    // "what's happening") that don't need an LLM at all. If the router
    // matches, we return instantly with zero LLM / GPU / RAM cost. An image
    // drop is never a deterministic command - it needs the vision model - so
    // skip the router when images are attached.
    let state_ref: &AppState = &state;
    if !has_images {
    // "open the monitor": a window, not a model call.
    let routed = if router::is_monitor_request(&content) {
        crate::shift_monitor::open_window(&app);
        Some(router::RouterResult {
            display: "The run monitor is open beside the window.".to_string(),
            spoken: "The run monitor is open.".to_string(),
            step_label: "Opening the run monitor…".to_string(),
        })
    } else {
        router::try_route(&content, state_ref)
    };
    if let Some(routed) = routed {
        bob_step(&app, &routed.step_label);
        let msg_id = Uuid::new_v4().to_string();
        crate::run_log::run_started(&msg_id, "chat", &content);
        crate::event_bus::emit_ui(&app, "message-stream-start", serde_json::json!({
            "message_id": &msg_id,
            "agent_id": serde_json::Value::Null,
        })).ok();
        crate::event_bus::emit_ui(&app, "message-chunk", serde_json::json!({
            "message_id": &msg_id,
            "content": &routed.display,
        })).ok();
        crate::event_bus::emit_ui(&app, "message-stream-end", serde_json::json!({
            "message_id": &msg_id,
            "content": &routed.display,
        })).ok();
        bob_step_clear(&app);

        let reply = Message {
            id: msg_id,
            role: "assistant".to_string(),
            content: routed.display.clone(),
            context: Some(routed.spoken.clone()),
            agent_id: None,
            timestamp: Utc::now().to_rfc3339(),
            tool_turns: None,
        };
        {
            let mut convos = state.chat.conversations.lock().map_err(|e| e.to_string())?;
            convos.push(reply.clone());
        }
        autosave_conversation(state_ref);
        crate::event_bus::emit_ui(&app, "new-message", reply.clone()).ok();
        crate::event_bus::emit_ui(&app, "data-changed", "bob-router").ok();
        return Ok(reply);
    }
    }

    // Reset cancel flag and set orb state to thinking
    BOB_CANCEL_FLAG.store(false, std::sync::atomic::Ordering::SeqCst);
    {
        let mut orb = state.chat.orb_state.lock().map_err(|e| e.to_string())?;
        *orb = OrbState::Thinking;
    }
    crate::event_bus::emit_ui(&app, "orb-state-change", "thinking").ok();

    // Open the assistant message stream so the frontend can display the
    // typing cursor and receive chunks immediately.
    let msg_id = Uuid::new_v4().to_string();
    crate::run_log::set_active(&msg_id);
    crate::run_log::run_started(&msg_id, "chat", &content);
    crate::event_bus::emit_ui(&app, "message-stream-start", serde_json::json!({
        "message_id": &msg_id,
        "agent_id": serde_json::Value::Null,
    })).ok();

    // ── Model pool: ensure the LLM is loaded before calling it ──
    // Lazily starts the llama-server on first use. Subsequent calls are
    // warm and return instantly. Eviction happens after 3 min idle.
    bob_step(&app, "Loading model…");
    if let Err(e) = ensure_model_loaded_for(&app, has_images).await {
        bob_step_clear(&app);
        let friendly = bob_speak_error(&e);
        crate::event_bus::emit_ui(&app, "message-stream-end", serde_json::json!({
            "message_id": &msg_id,
            "content": friendly.clone(),
        })).ok();
        return Err(friendly);
    }
    // One honest state while the model reads the conversation; the
    // stream loop replaces it with the specific tool the moment the
    // model reaches for one.
    bob_step(&app, "Thinking…");

    // Get conversation history for the LLM
    let history = {
        let convos = state.chat.conversations.lock().map_err(|e| e.to_string())?;
        convos.clone()
    };

    // ── Native agentic loop on the local LLM ──
    // Every non-image turn runs the multi-turn ReAct loop, which
    // advertises Bob's tools natively and feeds results back until the
    // model emits a turn with no tool calls. A pure chat question exits
    // the loop after one model call (the model answers, calls nothing),
    // so latency is identical to the old single-turn path.
    //
    // Why no keyword classifier: routing used to be decided by
    // classify_request, a keyword heuristic that repeatedly misrouted
    // turns that NEED a tool onto the toolless single-turn path - news
    // queries, "build me a panel", app-data asks, attached files, and
    // finally "where do I buy X" (field failure, 2026-07-20). On that
    // path no tools are advertised, so a reasoning model plans a tool
    // call it cannot make, emits no answer, and the blank-bubble
    // fallback surfaces its raw reasoning as the reply. No keyword list
    // can enumerate which questions need a tool; the model itself
    // decides that reliably when the tools are simply advertised.
    //
    // Why a native loop and not the `claude` CLI: Orbix is local-first
    // (Gemma 4 default). Routing technical work through a separately-
    // installed cloud CLI broke the "open the app and it works"
    // promise. The loop uses the same call_local_llm_streaming +
    // execute_actions_counted primitives the single-turn path uses.
    // ── Vision gate ──────────────────────────────────────────────
    // If the user attached images, they must be seen by a vision model.
    // Resolve the loaded model's capability once. When it can't see, answer
    // with a short, actionable pointer instead of silently dropping the
    // image or hallucinating about it. When it can, take the single-turn
    // vision path (below) rather than the agentic loop.
    if has_images {
        let selected_id = {
            let sel = state.engine.selected_local_model.lock().map_err(|e| e.to_string())?;
            sel.clone()
        }.or_else(|| crate::storage::load_json::<String>("selected_model.json").ok());
        let is_vision = selected_id.as_deref().map(local_llm::is_vision_model).unwrap_or(false);
        let can_see = selected_id.as_deref().map(local_llm::mmproj_downloaded).unwrap_or(false);
        if !can_see {
            // Two distinct failures: a text model loaded, or a vision model
            // whose projector never finished downloading. Say which.
            let guide = if is_vision {
                "This model can see images, but its image projector didn't finish downloading, \
so I can't look yet. Re-download the model in Settings › Models and send the image again."
            } else {
                "The model that's loaded right now can't see images. \
Every Gemma 4 and Qwen 3 model in Settings › Models can - pick one there, \
then send the image again and I'll take a look."
            };
            crate::event_bus::emit_ui(&app, "message-chunk", serde_json::json!({
                "message_id": &msg_id, "content": guide,
            })).ok();
            crate::event_bus::emit_ui(&app, "message-stream-end", serde_json::json!({
                "message_id": &msg_id, "content": guide,
            })).ok();
            bob_step_clear(&app);
            { let mut orb = state.chat.orb_state.lock().map_err(|e| e.to_string())?; *orb = OrbState::Idle; }
            crate::event_bus::emit_ui(&app, "orb-state-change", "idle").ok();
            let reply = Message {
                id: msg_id.clone(),
                role: "assistant".to_string(),
                content: guide.to_string(),
                context: None,
                agent_id: None,
                timestamp: Utc::now().to_rfc3339(),
                tool_turns: None,
            };
            {
                let mut convos = state.chat.conversations.lock().map_err(|e| e.to_string())?;
                convos.push(reply.clone());
            }
            crate::event_bus::emit_ui(&app, "new-message", reply.clone()).ok();
            return Ok(reply);
        }
    }

    // Image turns go through the same loop: the run's tool turns are
    // appended AFTER the stored history, so the image stays on the current
    // user message and a vision model sees it on every iteration. The
    // single-turn path below remains the fallback when the loop itself
    // errors (model server hiccup).
    {
        // No extra ribbon step here: the turn's ack phrase is already up
        // and holds until a real tool step replaces it. (The old label
        // leaked architecture-speak: "Agentic mode - multi-turn loop".)
        let client = reqwest::Client::new();
        match bob_agentic::run(&client, history.clone(), &msg_id, &app, state_ref, &images_vec).await {
            Ok(result) => {
                let display = result.final_text.clone();
                crate::event_bus::emit_ui(&app, "message-chunk", serde_json::json!({
                    "message_id": &msg_id,
                    "content": display,
                })).ok();
                crate::event_bus::emit_ui(&app, "message-stream-end", serde_json::json!({
                    "message_id": &msg_id,
                    "content": result.final_text.clone(),
                })).ok();
                bob_step_clear(&app);
                {
                    let mut orb = state.chat.orb_state.lock().map_err(|e| e.to_string())?;
                    *orb = OrbState::Idle;
                }
                crate::event_bus::emit_ui(&app, "orb-state-change", "idle").ok();

                // Context = what the MODEL sees for this message in later
                // turns. Attach the run's tool evidence so "go deeper"
                // follow-ups have real material; the UI shows content only.
                let history_ctx = if result.evidence.trim().is_empty() {
                    result.raw_final
                } else {
                    format!(
                        "{}\n\n[[evidence]]\nTool evidence gathered for this answer (reuse for follow-ups; do not contradict):\n{}",
                        result.raw_final, result.evidence
                    )
                };
                let reply = Message {
                    id: msg_id.clone(),
                    role: "assistant".to_string(),
                    content: result.final_text.clone(),
                    context: Some(history_ctx),
                    agent_id: None,
                    timestamp: Utc::now().to_rfc3339(),
                    tool_turns: (!result.tool_turns.is_empty()).then(|| result.tool_turns.clone()),
                };
                {
                    let mut convos = state.chat.conversations.lock().map_err(|e| e.to_string())?;
                    convos.push(reply.clone());
                }
                autosave_conversation(state_ref);
                crate::event_bus::emit_ui(&app, "new-message", reply.clone()).ok();
                crate::event_bus::emit_ui(&app, "data-changed", "bob-agentic").ok();
                return Ok(reply);
            }
            Err(e) => {
                // Local model unreachable / hard error - fall through
                // to the single-turn chat path so the user gets some
                // reply rather than a hang. Same model is tried again
                // below; the loop's failure was likely the health check.
                eprintln!("[bob] bob_agentic failed, falling through: {}", e);
                bob_step(&app, "Agent loop failed, retrying chat…");
            }
        }
    }

    // Local LLM is Bob's brain - Orbix is local-first. No cloud-CLI
    // fallback: if the local model is unreachable, surface that
    // directly so the user can fix the cause (server not running,
    // model not downloaded, etc.) rather than masking it behind a
    // chatbot-mode shell-out to claude.
    let client = reqwest::Client::new();
    let streamed = if has_images {
        crate::llm_engine::call_local_llm_streaming_vision(&client, &history, &msg_id, &app, Some(state_ref), &images_vec).await
    } else {
        call_local_llm_streaming(&client, &history, &msg_id, &app, Some(state_ref)).await
    };
    let raw_text = match streamed {
        Ok(text) => text,
        Err(local_err) => {
            // W3: graceful cloud fallback. Local-first is the default, but a
            // local outage (server not running, model not loaded) should
            // degrade to a configured cloud agent rather than hard-fail -
            // and that egress goes through the same gateway (redaction +
            // compliance) as any other cloud call. Only when no cloud agent
            // is configured do we surface the local error.
            let cloud_agent = {
                let agents = state.chat.agents.lock().map_err(|e| e.to_string())?;
                agents.iter()
                    .find(|a| a.enabled && a.provider == AgentProvider::Anthropic && !a.api_key.is_empty())
                    .cloned()
            };
            match cloud_agent {
                Some(agent) => {
                    bob_step(&app, "Local model unavailable - falling back to cloud…");
                    match call_anthropic(&client, &agent, &history).await {
                        Ok(text) => {
                            crate::event_bus::emit_ui(&app, "message-chunk", serde_json::json!({
                                "message_id": &msg_id,
                                "content": &text,
                            })).ok();
                            text
                        }
                        Err(cloud_err) => {
                            if let Ok(mut orb) = state.chat.orb_state.lock() { *orb = OrbState::Idle; }
                            crate::event_bus::emit_ui(&app, "orb-state-change", "idle").ok();
                            let msg = format!("{} (cloud fallback also failed: {})", local_err, cloud_err);
                            crate::event_bus::emit_ui(&app, "message-stream-end", serde_json::json!({
                                "message_id": &msg_id,
                                "content": &msg,
                            })).ok();
                            return Err(msg);
                        }
                    }
                }
                None => {
                    if let Ok(mut orb) = state.chat.orb_state.lock() { *orb = OrbState::Idle; }
                    crate::event_bus::emit_ui(&app, "orb-state-change", "idle").ok();
                    crate::event_bus::emit_ui(&app, "message-stream-end", serde_json::json!({
                        "message_id": &msg_id,
                        "content": &local_err,
                    })).ok();
                    return Err(local_err);
                }
            }
        }
    };

    // Process any action tags in the response (Bob's tools).
    // execute_actions also emits bob-step events for each action it runs.
    bob_step(&app, "Running actions…");
    let (processed, _stats) =
        execute_actions_counted(&raw_text, state_ref, &app).await;

    let display_text = enforce_reply_contract(&strip_post_action_decoration(
        &strip_hallucinated_content(&strip_think_blocks(&processed)),
    ));

    // Stream-end carries the final cleaned text so the frontend replaces
    // the in-progress chunks with the post-action result.
    crate::event_bus::emit_ui(&app, "message-chunk", serde_json::json!({
        "message_id": &msg_id,
        "content": display_text,
    })).ok();
    crate::event_bus::emit_ui(&app, "message-stream-end", serde_json::json!({
        "message_id": &msg_id,
        "content": display_text,
    })).ok();

    // Clear progress + reset orb
    bob_step_clear(&app);
    {
        let mut orb = state.chat.orb_state.lock().map_err(|e| e.to_string())?;
        *orb = OrbState::Idle;
    }
    crate::event_bus::emit_ui(&app, "orb-state-change", "idle").ok();

    let reply = Message {
        id: msg_id,
        role: "assistant".to_string(),
        content: display_text,
        context: Some(raw_text),
        agent_id: None,
        timestamp: Utc::now().to_rfc3339(),
        tool_turns: None,
    };

    {
        let mut convos = state.chat.conversations.lock().map_err(|e| e.to_string())?;
        convos.push(reply.clone());
    }

    autosave_conversation(state_ref);

    crate::event_bus::emit_ui(&app, "data-changed", "bob-action").ok();
    crate::event_bus::emit_ui(&app, "new-message", reply.clone()).ok();
    mark_model_used(&app).await;
    Ok(reply)
}


// ─────────────────────────────────────────────
//  Bob progress + cancel infrastructure
//  - BOB_CANCEL_FLAG: set true to abort the current Bob run
//  - bob_step(): tells the frontend what Bob is currently doing
//  - bob_step_clear(): hides the progress indicator
// ─────────────────────────────────────────────

pub static BOB_CANCEL_FLAG: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);

// ─────────────────────────────────────────────
//  Model pool - lazy load, idle unload
//
//  Models are loaded on first use and unloaded after 3 minutes idle.
//  Max one model loaded at a time on port 8847. This keeps Orbix's
//  idle RAM footprint tiny while still giving instant responses on
//  subsequent prompts after warm-up.
// ─────────────────────────────────────────────

pub(crate) const MODEL_IDLE_UNLOAD_SECS: u64 = 180; // 3 minutes (idle)

/// Number of model requests or agentic runs in flight. The idle-unload
/// loop never evicts while this is non-zero. Before this guard, the idle
/// clock was refreshed only when a turn STARTED, so a single run longer
/// than the idle timeout (a slide deck on a 12B model: field failure
/// 2026-09-06) had its server killed between two of its own tool turns
/// and the next call reported "server is not running".
pub static MODEL_BUSY: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);

/// RAII marker: the model is in use for as long as this lives.
pub struct ModelBusyGuard;

impl ModelBusyGuard {
    pub fn new() -> Self {
        MODEL_BUSY.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
        ModelBusyGuard
    }
}

impl Drop for ModelBusyGuard {
    fn drop(&mut self) {
        MODEL_BUSY.fetch_sub(1, std::sync::atomic::Ordering::SeqCst);
    }
}

pub fn model_busy() -> bool {
    MODEL_BUSY.load(std::sync::atomic::Ordering::SeqCst) > 0
}
pub(crate) const MODEL_IDLE_UNLOAD_SECS_VOICE: u64 = 600; // 10 minutes (active voice conversation)

/// Set to true while the user is in active voice conversation. The eviction
/// loop respects this and uses a longer idle timeout so the model doesn't get
/// unloaded mid-conversation when the user pauses to think.
pub static VOICE_CONVERSATION_ACTIVE: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);

#[tauri::command]
pub(crate) fn voice_conversation_set(active: bool) -> Result<(), String> {
    VOICE_CONVERSATION_ACTIVE.store(active, std::sync::atomic::Ordering::SeqCst);
    Ok(())
}

/// Pre-warm the local model. Called from the frontend the moment whisper
/// starts transcribing, so by the time the user's text reaches Bob, the
/// model is loaded and the HTTP keep-alive is open. Saves ~300-800ms
/// perceived latency per turn.
#[tauri::command]
pub(crate) async fn prewarm_llm(app: tauri::AppHandle) -> Result<(), String> {
    // Best-effort: if it fails, no-op. Don't block the user's turn.
    let _ = ensure_model_loaded(&app).await;
    // Touch the keep-alive by hitting /health
    let endpoint = local_llm::server_endpoint();
    let client = reqwest::Client::new();
    let _ = client.get(format!("{}/health", endpoint)).send().await;
    Ok(())
}

/// Battery state: (on_ac_power, percent_remaining)
/// On macOS we shell out to `pmset -g batt` since IOKit FFI is overkill.
pub(crate) fn battery_state() -> Option<(bool, u8)> {
    let out = std::process::Command::new("pmset").args(["-g", "batt"]).output().ok()?;
    let txt = String::from_utf8_lossy(&out.stdout);
    // Look for "AC Power" / "Battery Power"
    let on_ac = txt.contains("AC Power") || txt.contains("'AC Power'");
    // Look for "NN%;" pattern
    let percent = txt.split_whitespace()
        .find_map(|w| w.trim_end_matches(';').trim_end_matches('%').parse::<u8>().ok())
        .unwrap_or(100);
    Some((on_ac, percent))
}

/// Returns true if Orbix should switch to the smaller (E4B) model to save battery.
/// Conditions: on battery AND below 30% remaining.
pub(crate) fn should_use_battery_saver() -> bool {
    matches!(battery_state(), Some((false, p)) if p < 30)
}

/// Drop a starter set of agents and workflows into the empty stores on first
/// launch. Without these, a new user opens the app to a blank canvas and
/// doesn't know what to say. With these, they immediately see things they can
/// click, run, or remix into something of their own.
pub(crate) fn seed_starter_pack(agents: &mut Vec<Agent>, workflow_store: &mut WorkflowStore) {
    let now = Utc::now().to_rfc3339();
    let local_endpoint = local_llm::llama_base_url();

    // ── Starter agents ──
    let starter_agents = [
        ("Daily Briefing",
         "Reads your inbox, calendar, and recent activity. Summarizes what matters this morning."),
        ("Email Drafter",
         "Writes replies to emails in your tone. Learns from your past sent messages."),
        ("Research",
         "Fetches a URL, extracts key points, summarizes in your style."),
        ("Meeting Notes",
         "Listens to a meeting (or reads a transcript), pulls out decisions, action items, and owners."),
    ];

    for (name, _desc) in starter_agents {
        agents.push(Agent { charter: String::new(), granted_tools: Vec::new(), schedule_minutes: None, last_run_at: None,
            id: Uuid::new_v4().to_string(),
            name: name.to_string(),
            provider: AgentProvider::Local,
            model: "gemma-4-e4b".to_string(),
            endpoint: local_endpoint.clone(),
            api_key: String::new(),
            enabled: true,
            is_default: false,
            created_at: now.clone(),
            connector_ids: Vec::new(),
            workflow_ids: Vec::new(),
            capabilities: agent_router::AgentCapabilities::default(),
        });
    }

    // ── Starter workflows ──
    let mk_step = |label: &str, prompt: &str| -> workflow::WorkflowStep {
        let mut config = HashMap::new();
        config.insert("prompt".to_string(), serde_json::Value::String(prompt.to_string()));
        workflow::WorkflowStep {
            id: Uuid::new_v4().to_string(),
            order: 0,
            step_type: workflow::StepType::Prompt,
            label: label.to_string(),
            config,
        }
    };

    let starter_workflows = [
        (
            "Morning Briefing",
            "Quick summary of inbox, calendar, and overnight activity",
            vec![mk_step(
                "Generate briefing",
                "Give me a 5-bullet summary of what I should know this morning. Use my recent activity and any memory the user has shared. Be concrete, no fluff.",
            )],
        ),
        (
            "Draft Email Reply",
            "Write a polite reply to a given email",
            vec![mk_step(
                "Draft reply",
                "Draft a professional but warm reply to this email. Keep it under 6 sentences. End with a clear next step.",
            )],
        ),
        (
            "Quick Research",
            "Summarize a URL or topic in 3 bullet points",
            vec![mk_step(
                "Research and summarize",
                "Research the given topic. Return exactly 3 bullets: what it is, why it matters, and one surprising fact.",
            )],
        ),
        (
            "Meeting Action Items",
            "Pull decisions and action items out of meeting notes",
            vec![mk_step(
                "Extract action items",
                "Read these meeting notes. Return: (1) decisions made, (2) action items with owners, (3) open questions. Markdown lists, no commentary.",
            )],
        ),
    ];

    for (name, desc, steps) in starter_workflows {
        // Set proper order on each step
        let mut numbered_steps = steps;
        for (i, step) in numbered_steps.iter_mut().enumerate() {
            step.order = i;
        }
        workflow_store.workflows.push(workflow::Workflow {
            id: Uuid::new_v4().to_string(),
            name: name.to_string(),
            description: desc.to_string(),
            steps: numbered_steps,
            trigger: workflow::WorkflowTrigger::Manual,
            deliver: Default::default(),
            created_at: now.clone(),
            updated_at: now.clone(),
        });
    }
}

/// Translate technical error strings into friendly Bob-voice messages.
/// Used wherever errors bubble up to the user. If no pattern matches,
/// returns the original (so we never swallow useful info).
pub fn bob_speak_error(err: &str) -> String {
    let lower = err.to_lowercase();
    if lower.contains("connection refused") || lower.contains("os error 61") {
        "I can't reach the model server right now. Want me to restart it from Settings → Models?".to_string()
    } else if lower.contains("connection reset") || lower.contains("broken pipe") {
        "The connection dropped mid-request. Try that again in a moment.".to_string()
    } else if lower.contains("no space left") || lower.contains("disk full") || lower.contains("enospc") {
        "Your device is out of disk space. Free some up and try again.".to_string()
    } else if lower.contains("out of memory") || lower.contains("oom") || lower.contains("cannot allocate") {
        "Your device is running low on memory. I'll switch to a smaller model - try again.".to_string()
    } else if lower.contains("model file not found") || lower.contains("no such file") {
        "I can't find that model file. Open Settings → Models to download it.".to_string()
    } else if lower.contains("no downloaded local model") {
        "No local model is installed yet. Open Settings → Models to download Gemma 4 - it's the brain Bob uses.".to_string()
    } else if lower.contains("local model server is not running") {
        "The local model server is down. I'll start it on your next message.".to_string()
    } else if lower.contains("unauthorized") || lower.contains("401") || lower.contains("403") {
        "That request was rejected. Check your API key or sign in again.".to_string()
    } else if lower.contains("not found") || lower.contains("404") {
        "I couldn't find that. Want me to list what's available?".to_string()
    } else if lower.contains("timeout") || lower.contains("timed out") {
        "That took too long. Bob may be wedged - try a simpler request or restart from the Stop button.".to_string()
    } else if lower.contains("dns") || lower.contains("resolve") {
        "Can't reach the internet right now. Check your wifi and try again.".to_string()
    } else if lower.contains("rate limit") || lower.contains("429") {
        "We're hitting an API rate limit. Wait a minute and try again.".to_string()
    } else {
        err.to_string()
    }
}

/// Ensure a local model is loaded and return its endpoint URL.
/// - If the model is already loaded, updates last_used and returns.
/// - If a different model is loaded, unloads it first.
/// - If nothing is loaded, picks the user's selected model (or suite reasoning).
/// Emits `local-llm-status` events so the UI can show loading spinners.
pub async fn ensure_model_loaded(app: &tauri::AppHandle) -> Result<String, String> {
    ensure_model_loaded_for(app, false).await
}

/// Same, for a turn that may carry images: `vision` needs the server
/// launched with the projector. A text-only server is reloaded once with
/// it, and stays image-capable until the next idle unload; everything
/// else runs text-only so the prompt cache works (see local_llm).
pub async fn ensure_model_loaded_for(app: &tauri::AppHandle, vision: bool) -> Result<String, String> {
    let holder = app.try_state::<LlamaServerHolder>()
        .ok_or("LlamaServerHolder not registered")?;

    // Fast path: model is already loaded - just touch last_used and return.
    // "Loaded" means the process is alive, not merely spawned: a server
    // that died (Metal OOM, template error) used to sit here as a corpse
    // and every request failed its health check until the next restart.
    {
        let mut proc = holder.process.lock().await;
        let dead = proc
            .child
            .as_mut()
            .map(|c| matches!(c.try_wait(), Ok(Some(_))))
            .unwrap_or(false);
        if dead {
            eprintln!("[pool] {} exited on its own; reloading", proc.model_id);
            proc.child = None;
            proc.model_id.clear();
            proc.loaded_at = None;
            proc.last_used = None;
        } else if proc.child.is_some() && vision && !proc.vision {
            eprintln!("[pool] {} reloading with its image projector", proc.model_id);
            local_llm::stop_server(&mut proc.child).await;
            proc.model_id.clear();
            proc.loaded_at = None;
            proc.last_used = None;
        } else if proc.child.is_some() {
            proc.last_used = Some(std::time::Instant::now());
            return Ok(local_llm::server_endpoint());
        }
    }
    local_llm::LAUNCH_VISION.store(vision, std::sync::atomic::Ordering::SeqCst);

    // Slow path: need to load. The 26B-A4B is the recommended Bob brain
    // (IQ3_XS quant + KV cache quant + flash-attn → ~9GB resident on 16GB Mac).
    //
    // BATTERY SAVER: if the user is on battery and below 30%, prefer the
    // smaller E4B model regardless of selection. The 26B drains battery fast.
    // We notify the user via the bob-step pill so they know what happened.
    let saver = should_use_battery_saver();
    let model_to_load = {
        let state = app.state::<AppState>();
        let selected = state.engine.selected_local_model.lock().unwrap_or_else(|e| e.into_inner()).clone();
        let models = local_llm::list_models_with_status();

        if saver {
            // Battery saver active - try E4B first regardless of preference
            crate::event_bus::emit_ui(app, "battery-saver", serde_json::json!({ "active": true })).ok();
            models.iter()
                .find(|m| m.id == local_llm::SUITE_AGENT_ID && m.downloaded).cloned()
                .or_else(|| selected.as_ref()
                    .and_then(|id| models.iter().find(|m| &m.id == id && m.downloaded).cloned()))
                .or_else(|| models.iter().find(|m| m.downloaded).cloned())
        } else {
            let chosen = if let Some(id) = selected {
                models.iter().find(|m| m.id == id && m.downloaded).cloned()
            } else {
                None
            };
            chosen
                .or_else(|| models.iter()
                    .find(|m| m.id == local_llm::SUITE_REASONING_ID && m.downloaded).cloned())
                .or_else(|| models.iter()
                    .find(|m| m.id == local_llm::SUITE_AGENT_ID && m.downloaded).cloned())
                .or_else(|| models.iter().find(|m| m.downloaded).cloned())
        }
    };

    let model = model_to_load.ok_or("No downloaded local model. Open Settings → Models to download one.")?;

    crate::event_bus::emit_ui(app, "local-llm-status", "starting").ok();
    // 120s timeout - if the model fails to load (OOM, corrupt file), don't hang forever
    let child = tokio::time::timeout(
        std::time::Duration::from_secs(120),
        local_llm::start_server(&model.filename, &model.id),
    ).await
        .map_err(|_| "Model took too long to load (>2 min). Your device may not have enough free RAM. Try closing other apps.".to_string())?
        ?;
    {
        let mut proc = holder.process.lock().await;
        proc.child = Some(child);
        proc.model_id = model.id.clone();
        proc.vision = vision;
        proc.port = local_llm::llama_port();
        proc.loaded_at = Some(std::time::Instant::now());
        proc.last_used = Some(std::time::Instant::now());
    }
    // Bob's prompt as it was before the last idle unload: ~40ms here
    // instead of ~18s of reading it again on the first turn.
    local_llm::restore_prompt_cache(&local_llm::server_endpoint(), &model.id).await;
    crate::event_bus::emit_ui(app, "local-llm-status", "running").ok();
    crate::event_bus::emit_ui(app, "model-pool-changed", serde_json::json!({ "loaded": [model.id] })).ok();
    Ok(local_llm::server_endpoint())
}

/// Mark the currently-loaded model as just-used. Called at the end of
/// every LLM request so idle eviction uses accurate timestamps.
pub async fn mark_model_used(app: &tauri::AppHandle) {
    if let Some(holder) = app.try_state::<LlamaServerHolder>() {
        let mut proc = holder.process.lock().await;
        proc.last_used = Some(std::time::Instant::now());
    }
}

/// Background task: every 30 seconds, unload the model if it's been idle
/// longer than MODEL_IDLE_UNLOAD_SECS (or the longer voice timeout if the
/// user is in active voice conversation).
pub(crate) async fn model_pool_eviction_loop(app: tauri::AppHandle) {
    loop {
        tokio::time::sleep(tokio::time::Duration::from_secs(30)).await;
        let holder = match app.try_state::<LlamaServerHolder>() {
            Some(h) => h,
            None => continue,
        };
        let mut proc = holder.process.lock().await;
        let timeout_secs = if VOICE_CONVERSATION_ACTIVE.load(std::sync::atomic::Ordering::SeqCst) {
            MODEL_IDLE_UNLOAD_SECS_VOICE
        } else {
            MODEL_IDLE_UNLOAD_SECS
        };
        let should_evict = proc.child.is_some()
            && !model_busy()
            && proc.last_used
                .map(|t| t.elapsed().as_secs() > timeout_secs)
                .unwrap_or(false);
        if should_evict {
            let model_id = proc.model_id.clone();
            eprintln!("[pool] Unloading {} (idle > {}s)", model_id, MODEL_IDLE_UNLOAD_SECS);
            // Keep Bob's prompt for the reload (local_llm prompt cache).
            local_llm::save_prompt_cache(&local_llm::server_endpoint(), &model_id).await;
            proc.vision = false;
            if let Some(mut child) = proc.child.take() {
                let _ = child.start_kill();
            }
            proc.model_id.clear();
            proc.loaded_at = None;
            proc.last_used = None;
            drop(proc);
            crate::event_bus::emit_ui(&app, "local-llm-status", "idle-unloaded").ok();
            crate::event_bus::emit_ui(&app, "model-pool-changed", serde_json::json!({ "loaded": serde_json::Value::Array(vec![]) })).ok();
        }
    }
}

#[tauri::command]
pub(crate) async fn list_loaded_models(app: tauri::AppHandle) -> Result<Vec<local_llm::LoadedModelInfo>, String> {
    let holder = app.try_state::<LlamaServerHolder>().ok_or("No pool")?;
    let proc = holder.process.lock().await;
    if proc.child.is_none() {
        return Ok(vec![]);
    }
    let loaded_secs = proc.loaded_at.map(|t| t.elapsed().as_secs()).unwrap_or(0);
    let idle_secs = proc.last_used.map(|t| t.elapsed().as_secs()).unwrap_or(0);
    Ok(vec![local_llm::LoadedModelInfo {
        model_id: proc.model_id.clone(),
        port: proc.port,
        loaded_secs,
        idle_secs,
    }])
}

#[tauri::command]
pub(crate) async fn unload_all_models(app: tauri::AppHandle) -> Result<(), String> {
    let holder = app.try_state::<LlamaServerHolder>().ok_or("No pool")?;
    let mut proc = holder.process.lock().await;
    if let Some(mut child) = proc.child.take() {
        let _ = child.start_kill();
    }
    proc.model_id.clear();
    proc.loaded_at = None;
    proc.last_used = None;
    drop(proc);
    crate::event_bus::emit_ui(&app, "local-llm-status", "unloaded").ok();
    crate::event_bus::emit_ui(&app, "model-pool-changed", serde_json::json!({ "loaded": serde_json::Value::Array(vec![]) })).ok();
    Ok(())
}

/// A varied "on it" acknowledgment for the start of a turn - the ribbon
/// (and voice mode) should sound like Bob, not like a spinner. The pool
/// is walked with a stride coprime to its length, so every phrase gets
/// used and two consecutive turns never say the same thing.
pub fn ack_phrase() -> &'static str {
    const POOL: [&str; 16] = [
        "Let me check.",
        "Let me think about that.",
        "Sure, looking into it.",
        "On it.",
        "Give me a second.",
        "Checking now.",
        "Let me have a look.",
        "One moment.",
        "Digging in.",
        "Let me see.",
        "Taking a look now.",
        "Right, let me look.",
        "Working on it.",
        "Let me pull that up.",
        "Hmm, let me think.",
        "Good question, checking.",
    ];
    static NEXT: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);
    let n = NEXT.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    POOL[(n * 7) % POOL.len()]
}

#[cfg(test)]
mod ack_phrase_tests {
    use super::ack_phrase;

    #[test]
    fn never_the_same_twice_in_a_row_and_full_variety() {
        let mut seen = std::collections::HashSet::new();
        let mut prev = "";
        for _ in 0..32 {
            let p = ack_phrase();
            assert!(!p.is_empty());
            assert_ne!(p, prev, "consecutive acks must differ");
            prev = p;
            seen.insert(p);
        }
        // The stride is coprime to the pool size: every phrase gets a turn.
        assert_eq!(seen.len(), 16, "the whole pool should be used");
    }
}

pub fn bob_step(app: &tauri::AppHandle, label: &str) {
    crate::run_log::step(label);
    crate::turn_scope::step(label);
    // Inside a shift's STEP_SCOPE every step (including the ones the tool
    // executors emit) belongs to the monitor window, not Bob's chat ribbon.
    if let Some(shift_id) = crate::shift_monitor::current_scope() {
        crate::event_bus::emit_ui(app, "shift-step", serde_json::json!({ "shift_id": shift_id, "label": label })).ok();
        return;
    }
    crate::event_bus::emit_ui(app, "bob-step", serde_json::json!({ "label": label })).ok();
}

pub fn bob_step_clear(app: &tauri::AppHandle) {
    if let Some(shift_id) = crate::shift_monitor::current_scope() {
        crate::event_bus::emit_ui(app, "shift-step", serde_json::json!({ "shift_id": shift_id, "label": serde_json::Value::Null })).ok();
        return;
    }
    crate::event_bus::emit_ui(app, "bob-step", serde_json::json!({ "label": serde_json::Value::Null })).ok();
}

/// Map an action type + payload to a human-friendly progress label.
/// Used by the action dispatcher so the user sees what Bob is doing
/// in plain English ("Creating agent 'Mailman'…") instead of nothing.
pub(crate) fn action_label_for(action_type: &str, action: &serde_json::Value) -> String {
    let name = || action["name"].as_str().or(action["query"].as_str()).unwrap_or("");
    match action_type {
        "create_agent" => format!("Creating agent '{}'…", name()),
        "update_agent" => format!("Updating agent '{}'…", name()),
        "delete_agent" => format!("Deleting agent '{}'…", name()),
        "toggle_agent" => format!("Toggling agent '{}'…", name()),
        "create_workflow" => format!("Building workflow '{}'…", name()),
        "use_skill" => format!("Loading skill '{}'…", name()),
        "research" => format!(
            "Researching '{}'…",
            action["topic"].as_str().or(action["query"].as_str()).unwrap_or("")
        ),
        "create_document" => format!(
            "Creating {} '{}'…",
            match action["kind"].as_str().unwrap_or("document") {
                "website" => "website",
                "pptx" => "slides",
                "docx" => "document",
                _ => "page",
            },
            action["title"].as_str().unwrap_or("untitled")
        ),
        "extract_brand_kit" => "Reading the brand guideline…".to_string(),
        "create_panel" => format!("Building panel '{}'…", action["title"].as_str().unwrap_or("")),
        "update_panel" => format!("Updating panel '{}'…", action["title"].as_str().unwrap_or(action["id"].as_str().unwrap_or(""))),
        "remove_panel" => "Removing panel…".to_string(),
        "run_workflow" => format!("Running workflow '{}'…", name()),
        "update_workflow" => format!("Updating workflow '{}'…", name()),
        "delete_workflow" => format!("Deleting workflow '{}'…", name()),
        "stop_workflow" => "Stopping workflow…".to_string(),
        "run_chain" => format!("Running chain '{}'…", name()),
        "list_chains" => "Listing chains…".to_string(),
        "add_memory" => format!("Remembering '{}'…", action["key"].as_str().unwrap_or("")),
        "search_memory" => format!("Searching memory for '{}'…", name()),
        "delete_memory" => "Forgetting…".to_string(),
        "api_call" => format!("Calling {}…", action["url"].as_str().unwrap_or("API")),
        "tool" => {
            let args = &action["args"];
            let arg = |k: &str| args[k].as_str().or(action[k].as_str()).unwrap_or("");
            match action["tool"].as_str().unwrap_or("") {
                "web_search" => format!("Searching the web for '{}'…", crate::llm_engine::short(arg("query"), 60)),
                "web_fetch" => format!("Reading {}…", host_label(arg("url"))),
                "file_read" => format!("Reading {}…", base_label(arg("path"))),
                "file_list" => format!("Listing {}…", crate::llm_engine::short(arg("path"), 50)),
                "file_write" => format!("Writing {}…", base_label(arg("path"))),
                "file_edit" => format!("Editing {}…", base_label(arg("path"))),
                "shell_exec" => format!("Running `{}`…", crate::llm_engine::short(arg("command").lines().next().unwrap_or(""), 60)),
                other => format!("Running {}…", other.replace('_', " ")),
            }
        }
        "app_bridge" => format!("Reading {}…", action["app"].as_str().unwrap_or("your apps")),
        "read_app_window" => format!("Reading the {} window…", action["app"].as_str().unwrap_or("app")),
        "list_app_bridges" => "Checking which apps I can read…".to_string(),
        "list_calendar_events" => "Reading your calendar…".to_string(),
        "create_calendar_event" => format!("Adding '{}' to your calendar…", action["title"].as_str().unwrap_or("event")),
        "send_email" => match action["to"].as_str() {
            Some(to) => format!("Sending an email to {}…", to),
            None => "Sending an email…".to_string(),
        },
        "browse_navigate" => format!("Opening {}…", host_label(action["url"].as_str().unwrap_or("a page"))),
        "browse_click" => "Clicking on the page…".to_string(),
        "browse_type" => "Typing on the page…".to_string(),
        "browse_read" => "Reading the page…".to_string(),
        "create_connector" => format!("Building connector '{}'…", name()),
        "connect_oauth" => format!("Connecting {}…", action["service"].as_str().unwrap_or("service")),
        "check_connectors" => "Checking connected services…".to_string(),
        "list_connectors" => "Listing connected services…".to_string(),
        "disconnect_connector" => format!("Disconnecting '{}'…", name()),
        "list_mcp" | "list_mcp_servers" => "Listing MCP servers…".to_string(),
        "remove_mcp" | "delete_mcp" => format!("Removing MCP '{}'…", name()),
        "toggle_mcp" => format!("Toggling MCP '{}'…", name()),
        "list_triggers" => "Listing triggers…".to_string(),
        "delete_trigger" => format!("Deleting trigger '{}'…", name()),
        "toggle_trigger" => format!("Toggling trigger '{}'…", name()),
        "recent_activity" => "Reading activity…".to_string(),
        "audit_search" => "Searching audit log…".to_string(),
        "twin_list" => "Listing twin entities…".to_string(),
        "twin_search" => format!("Searching twin for '{}'…", name()),
        "twin_get" => format!("Reading entity '{}'…", name()),
        "list_notifications" => "Reading notifications…".to_string(),
        "dismiss_notifications" => "Clearing notifications…".to_string(),
        "set_voice" => "Updating voice settings…".to_string(),
        other => format!("{}…", other.replace('_', " ")),
    }
}

fn host_label(url: &str) -> String {
    let h = url.split("://").nth(1).unwrap_or(url).split('/').next().unwrap_or(url).trim_start_matches("www.");
    if h.is_empty() { "a page".to_string() } else { h.to_string() }
}

fn base_label(path: &str) -> String {
    let b = path.trim_end_matches('/').rsplit('/').next().unwrap_or(path);
    if b.is_empty() { "a file".to_string() } else { b.to_string() }
}

/// What just happened, in the past tense, for the ribbon between two
/// model turns ("Read Calendar. Thinking…" instead of "Thinking… (turn
/// 2)"). Short on purpose: it sits next to a spinner.
pub(crate) fn action_done_label(action_type: &str, action: &serde_json::Value) -> String {
    let name = || action["name"].as_str().unwrap_or("");
    match action_type {
        "tool" => match action["tool"].as_str().unwrap_or("") {
            "web_search" => "Searched the web".to_string(),
            "web_fetch" => format!("Read {}", host_label(action["args"]["url"].as_str().or(action["url"].as_str()).unwrap_or(""))),
            "file_read" => format!("Read {}", base_label(action["args"]["path"].as_str().or(action["path"].as_str()).unwrap_or(""))),
            "file_list" => "Listed the folder".to_string(),
            "file_write" => format!("Wrote {}", base_label(action["args"]["path"].as_str().or(action["path"].as_str()).unwrap_or(""))),
            "file_edit" => format!("Edited {}", base_label(action["args"]["path"].as_str().or(action["path"].as_str()).unwrap_or(""))),
            "shell_exec" => "Ran the command".to_string(),
            other => format!("Ran {}", other.replace('_', " ")),
        },
        "app_bridge" => format!("Read {}", action["app"].as_str().unwrap_or("your apps")),
        "read_app_window" => format!("Read the {} window", action["app"].as_str().unwrap_or("app")),
        "list_calendar_events" => "Read your calendar".to_string(),
        "research" => "Finished the research".to_string(),
        "create_document" => match action["kind"].as_str().unwrap_or("") {
            "pptx" => "Created the slides".to_string(),
            "website" => "Built the website".to_string(),
            _ => "Created the document".to_string(),
        },
        "search_memory" => "Searched memory".to_string(),
        "add_memory" => "Saved that to memory".to_string(),
        "create_workflow" => format!("Built workflow '{}'", name()),
        "create_agent" => format!("Created agent '{}'", name()),
        "run_workflow" => format!("Ran workflow '{}'", name()),
        "send_email" => "Sent the email".to_string(),
        "create_calendar_event" => "Added the event".to_string(),
        "browse_navigate" | "browse_read" => "Read the page".to_string(),
        "check_connectors" | "list_connectors" => "Checked connected services".to_string(),
        other => {
            // Fall back to the active label with its ellipsis dropped.
            let l = action_label_for(other, action);
            l.trim_end_matches('…').to_string()
        }
    }
}

#[cfg(test)]
mod label_tests {
    use super::{action_done_label, action_label_for};
    use serde_json::json;

    #[test]
    fn tool_labels_say_what_and_where() {
        assert_eq!(action_label_for("tool", &json!({"tool":"web_search","args":{"query":"apple news"}})), "Searching the web for 'apple news'…");
        assert_eq!(action_label_for("tool", &json!({"tool":"web_fetch","args":{"url":"https://www.orbixcore.ai/releases/1.4"}})), "Reading orbixcore.ai…");
        assert_eq!(action_label_for("tool", &json!({"tool":"file_list","args":{"path":"/Users/x/Documents"}})), "Listing /Users/x/Documents…");
        assert_eq!(action_label_for("tool", &json!({"tool":"shell_exec","args":{"command":"ls -la ~/Desktop\necho done"}})), "Running `ls -la ~/Desktop`…");
        assert_eq!(action_label_for("app_bridge", &json!({"app":"Calendar","action":"upcoming_events"})), "Reading Calendar…");
        assert_eq!(action_label_for("list_calendar_events", &json!({})), "Reading your calendar…");
    }

    #[test]
    fn done_labels_are_past_tense_and_short() {
        assert_eq!(action_done_label("app_bridge", &json!({"app":"Outlook"})), "Read Outlook");
        assert_eq!(action_done_label("tool", &json!({"tool":"web_search","args":{"query":"x"}})), "Searched the web");
        assert_eq!(action_done_label("create_document", &json!({"kind":"pptx","title":"Solar"})), "Created the slides");
        assert_eq!(action_done_label("toggle_agent", &json!({"name":"Scout"})), "Toggling agent 'Scout'");
    }
}

#[tauri::command]
pub(crate) async fn bob_cancel() -> Result<(), String> {
    BOB_CANCEL_FLAG.store(true, std::sync::atomic::Ordering::SeqCst);
    Ok(())
}


// ─────────────────────────────────────────────
//  Bob Lite - fast conversational shell.
//
//  Voice-mode router: a tiny local model (Gemma E4B on port 8848)
//  handles every conversational turn with sub-400ms first-token
//  latency. For chitchat, simple lookups, and acknowledgments it
//  responds directly. For complex tasks it speaks a brief filler
//  AND emits <think>{"task":"..."}</think>, which we extract and
//  forward to the existing Bob "Brain" path (call_local_llm /
//  cloud agents / full action dispatcher).
//
//  Result: user always hears a fast natural voice. Brain runs in
//  the background. The user perceives one continuous Bob.
// ─────────────────────────────────────────────

// Slim ~250-token voice prompt. Optimized for fast first-token: minimal
// context for the model to chew through on every turn.
pub(crate) const BOB_LITE_SYSTEM_PROMPT: &str = "You are Bob - Orbix Core's voice. Talking on a call. Be terse, warm, conversational. Like a sharp friend.\n\n\
ANSWER DIRECTLY for greetings, chitchat, jokes, simple recall.\n\
ESCALATE for complex tasks: say a brief filler (<8 words), then ONE <think>{\"task\":\"...\"}</think> tag describing what Brain should do.\n\n\
EX 1 - User: \"hey\" → You: \"Hey! Ready when you are.\"\n\
EX 2 - User: \"what chains do I have?\" → You: \"Let me check.<think>{\"task\":\"list chains\"}</think>\"\n\
EX 3 - User: \"build an agent that posts to LinkedIn daily\" → You: \"Setting it up.<think>{\"task\":\"create agent + workflow + linkedin connector + daily cron\"}</think>\"\n\n\
RULES: speak first, tag second. Direct answers under 2 sentences. Never mention models or architecture - you ARE Bob. No <action> tags, only <think>.";

/// Strip <think>…</think> tags from a string, returning (clean_text, extracted_task).
pub(crate) fn split_think_tag(text: &str) -> (String, Option<String>) {
    let open = match text.find("<think>") {
        Some(i) => i,
        None => return (text.to_string(), None),
    };
    let after = open + 7;
    let close = match text[after..].find("</think>") {
        Some(j) => after + j,
        None => return (text[..open].trim().to_string(), Some(text[after..].trim().to_string())),
    };
    let raw_task = text[after..close].trim();
    // Try parse JSON; if it has {"task": "..."}, use that. Otherwise the raw string.
    let task = if let Ok(v) = serde_json::from_str::<serde_json::Value>(raw_task) {
        v.get("task").and_then(|s| s.as_str()).map(|s| s.to_string()).unwrap_or_else(|| raw_task.to_string())
    } else {
        raw_task.to_string()
    };
    let mut clean = String::new();
    clean.push_str(text[..open].trim_end());
    if close + 8 < text.len() {
        clean.push(' ');
        clean.push_str(text[close + 8..].trim_start());
    }
    (clean.trim().to_string(), Some(task))
}

/// Call the local model server (port 8847) with the Bob Lite system prompt.
/// We share the main model server instead of running a separate 8848 instance,
/// because running two llama-server processes loads the model into RAM twice
/// and thrashes the user's Mac. Lite vs Brain is a prompt difference, not a model split.
pub(crate) async fn call_bob_lite(client: &reqwest::Client, history: &[Message]) -> Result<String, String> {
    let endpoint = local_llm::server_endpoint();
    let health_url = format!("{}/health", endpoint);

    let health = client.get(&health_url).send().await;
    if health.is_err() || !health.unwrap().status().is_success() {
        return Err(format!("Local model server not running on port {}.", local_llm::llama_port()));
    }

    // Voice mode: only the last 6 messages. Anything older is dead weight
    // for prompt processing - every extra message means more tokens to
    // prefill on the GPU before the first new token can stream.
    let messages: Vec<serde_json::Value> = std::iter::once(serde_json::json!({
        "role": "system",
        "content": BOB_LITE_SYSTEM_PROMPT
    }))
    .chain(history.iter().rev().take(6).rev().map(|m| {
        serde_json::json!({
            "role": m.role,
            "content": if m.role == "assistant" { m.context.as_deref().unwrap_or(&m.content) } else { &m.content }
        })
    }))
    .collect();

    let body = serde_json::json!({
        "messages": messages,
        "temperature": 0.6,
        "max_tokens": 512,
        "stream": false,
    });

    let url = format!("{}/v1/chat/completions", endpoint);
    let resp = client.post(&url)
        .header("Content-Type", "application/json")
        .json(&body)
        .send()
        .await
        .map_err(|e| format!("Bob Lite request failed: {}", e))?;

    if !resp.status().is_success() {
        return Err(format!("Bob Lite error: {}", resp.status()));
    }

    let json: serde_json::Value = resp.json().await
        .map_err(|e| format!("Failed to parse Bob Lite response: {}", e))?;

    json.get("choices")
        .and_then(|c| c.get(0))
        .and_then(|c| c.get("message"))
        .and_then(|m| m.get("content"))
        .and_then(|c| c.as_str())
        .map(|s| s.to_string())
        .ok_or_else(|| "No content in Bob Lite response".to_string())
}

#[tauri::command]
pub(crate) async fn bob_lite_send(
    content: String,
    state: State<'_, AppState>,
    app: tauri::AppHandle,
) -> Result<Message, String> {
    // Persist user message to conversation history
    let user_msg = Message {
        id: Uuid::new_v4().to_string(),
        role: "user".to_string(),
        content: content.clone(),
        context: None,
        agent_id: None,
        timestamp: Utc::now().to_rfc3339(),
        tool_turns: None,
    };
    {
        let mut convos = state.chat.conversations.lock().map_err(|e| e.to_string())?;
        convos.push(user_msg.clone());
    }
    crate::event_bus::emit_ui(&app, "orb-state-change", "thinking").ok();

    // ── COMMAND ROUTER (voice path) ─────────────────────────────────
    // Try deterministic routing first. If the user's command matches a
    // known pattern, we skip the LLM entirely and return the spoken result.
    let state_ref: &AppState = &state;
    if let Some(routed) = router::try_route(&content, state_ref) {
        bob_step(&app, &routed.step_label);
        let msg_id = Uuid::new_v4().to_string();
        crate::event_bus::emit_ui(&app, "message-stream-start", serde_json::json!({
            "message_id": &msg_id,
            "agent_id": serde_json::Value::Null,
        })).ok();
        // Use the spoken version for TTS (strips markdown, truncates).
        crate::event_bus::emit_ui(&app, "message-chunk", serde_json::json!({
            "message_id": &msg_id,
            "content": &routed.spoken,
        })).ok();
        crate::event_bus::emit_ui(&app, "message-stream-end", serde_json::json!({
            "message_id": &msg_id,
            "content": &routed.display,
        })).ok();
        bob_step_clear(&app);
        {
            let mut orb = state.chat.orb_state.lock().map_err(|e| e.to_string())?;
            *orb = OrbState::Idle;
        }
        crate::event_bus::emit_ui(&app, "orb-state-change", "idle").ok();

        let reply = Message {
            id: msg_id,
            role: "assistant".to_string(),
            content: routed.display.clone(),
            context: Some(routed.spoken.clone()),
            agent_id: None,
            timestamp: Utc::now().to_rfc3339(),
            tool_turns: None,
        };
        {
            let mut convos = state.chat.conversations.lock().map_err(|e| e.to_string())?;
            convos.push(reply.clone());
        }
        crate::event_bus::emit_ui(&app, "new-message", reply.clone()).ok();
        return Ok(reply);
    }

    let history = {
        let convos = state.chat.conversations.lock().map_err(|e| e.to_string())?;
        convos.clone()
    };

    // Ensure a model is loaded - lazy start on first voice interaction.
    if let Err(e) = ensure_model_loaded(&app).await {
        return Err(e);
    }

    let client = reqwest::Client::new();

    // Always emit message-stream-start FIRST so the frontend sentence
    // streaming pipeline is ready to consume chunks regardless of which
    // path runs (Lite-only / Lite+Brain / Brain-only fallback).
    let msg_id = Uuid::new_v4().to_string();
    crate::event_bus::emit_ui(&app, "message-stream-start", serde_json::json!({
        "message_id": msg_id,
        "agent_id": serde_json::Value::Null,
    })).ok();

    // ── Phase 1: Bob Lite responds (fast, conversational) ──
    let lite_raw = call_bob_lite(&client, &history).await
        .unwrap_or_else(|e| {
            eprintln!("[bob_lite] Lite unreachable: {} - running Brain directly", e);
            String::new()
        });

    let (lite_clean, brain_task) = split_think_tag(&lite_raw);
    let lite_failed = lite_clean.is_empty() && brain_task.is_none();

    // Ensure the filler ends with a period so TTS pauses cleanly between
    // the filler and Brain's answer instead of running them together.
    let filler = if !lite_clean.is_empty() {
        let trimmed = lite_clean.trim();
        if trimmed.ends_with('.') || trimmed.ends_with('!') || trimmed.ends_with('?') {
            trimmed.to_string()
        } else {
            format!("{}.", trimmed)
        }
    } else {
        String::new()
    };

    // Emit the filler immediately as the first chunk so TTS starts speaking
    // it within ~50ms while Brain (if needed) does its work.
    if !filler.is_empty() {
        crate::event_bus::emit_ui(&app, "message-chunk", serde_json::json!({
            "message_id": msg_id,
            "content": filler,
        })).ok();
    }

    // ── Phase 2: run Brain if Lite escalated OR Lite was unreachable ──
    let needs_brain = brain_task.is_some() || lite_failed;
    let final_text = if needs_brain {
        // Use the original user content as the task when Lite failed,
        // otherwise use Lite's extracted task description.
        let task = brain_task.unwrap_or_else(|| content.clone());

        let brain_history = if lite_failed {
            // Pass the full conversation history so Brain has context.
            history.clone()
        } else {
            // Lite already framed the task - give Brain a clean single-message context.
            vec![Message {
                id: Uuid::new_v4().to_string(),
                role: "user".to_string(),
                content: task.clone(),
                context: None,
                agent_id: None,
                timestamp: Utc::now().to_rfc3339(),
                tool_turns: None,
            }]
        };

        let brain_raw = match call_local_llm(&client, &brain_history).await {
            Ok(text) => text,
            Err(e) => format!("Sorry, the reasoning model is offline. {}", e),
        };

        let state_ref: &AppState = &state;
        let processed = execute_actions(&brain_raw, state_ref, &app).await;
        let display = strip_hallucinated_content(&processed);
        let brain_voice = trim_for_voice(&display);

        // Compose: filler (with period) + space + brain answer.
        // Sentence-streaming TTS will see the new sentences and queue them
        // separately, giving Kokoro consistent short chunks instead of one
        // big run-on.
        let composed = if filler.is_empty() {
            brain_voice.clone()
        } else {
            format!("{} {}", filler, brain_voice)
        };

        crate::event_bus::emit_ui(&app, "message-chunk", serde_json::json!({
            "message_id": msg_id,
            "content": composed,
        })).ok();

        composed
    } else {
        filler.clone()
    };

    crate::event_bus::emit_ui(&app, "message-stream-end", serde_json::json!({
        "message_id": msg_id,
        "content": final_text,
    })).ok();

    // Reset orb
    {
        let mut orb = state.chat.orb_state.lock().map_err(|e| e.to_string())?;
        *orb = OrbState::Idle;
    }
    crate::event_bus::emit_ui(&app, "orb-state-change", "idle").ok();

    let reply = Message {
        id: msg_id,
        role: "assistant".to_string(),
        content: final_text.clone(),
        context: Some(lite_raw),
        agent_id: None,
        timestamp: Utc::now().to_rfc3339(),
        tool_turns: None,
    };

    {
        let mut convos = state.chat.conversations.lock().map_err(|e| e.to_string())?;
        convos.push(reply.clone());
    }

    {
        let convos = state.chat.conversations.lock().unwrap_or_else(|e| e.into_inner());
        let conv_id = state.chat.conversation_id.lock().unwrap_or_else(|e| e.into_inner()).clone();
        let title = convos.first()
            .map(|m| { let t = m.content.chars().take(50).collect::<String>(); if m.content.len() > 50 { format!("{}...", t) } else { t } })
            .unwrap_or_else(|| "Untitled".to_string());
        storage::save_conversation(&conv_id, &title, &convos).ok();
    }

    crate::event_bus::emit_ui(&app, "new-message", reply.clone()).ok();
    mark_model_used(&app).await;
    Ok(reply)
}

/// Trim a Brain response down to something natural for voice playback -
/// strip markdown, action tags, code fences, then take the first 2-3 sentences
/// or 280 chars, whichever is shorter.
pub(crate) fn trim_for_voice(text: &str) -> String {
    let cleaned = text
        .lines()
        .filter(|l| !l.trim().starts_with("```"))
        .collect::<Vec<_>>()
        .join(" ")
        .replace("**", "")
        .replace("*", "")
        .replace("##", "")
        .replace("#", "");
    let cleaned = cleaned.split_whitespace().collect::<Vec<_>>().join(" ");

    // First 2-3 sentences
    let sentences: Vec<&str> = cleaned
        .split(|c: char| c == '.' || c == '!' || c == '?')
        .filter(|s| !s.trim().is_empty())
        .collect();
    let take = sentences.iter().take(3).cloned().collect::<Vec<_>>().join(". ");
    let take = format!("{}.", take.trim());
    if take.len() > 320 {
        format!("{}…", &take[..320])
    } else {
        take
    }
}


#[cfg(test)]
mod model_busy_tests {
    use super::{model_busy, ModelBusyGuard, MODEL_BUSY};
    use std::sync::atomic::Ordering;

    #[test]
    fn guard_counts_nested_use_and_releases_on_drop() {
        let before = MODEL_BUSY.load(Ordering::SeqCst);
        {
            let _run = ModelBusyGuard::new();
            let _call = ModelBusyGuard::new();
            assert!(model_busy());
            assert_eq!(MODEL_BUSY.load(Ordering::SeqCst), before + 2);
        }
        assert_eq!(MODEL_BUSY.load(Ordering::SeqCst), before);
    }
}
