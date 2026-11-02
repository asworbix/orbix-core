//! Shared runtime helpers split out of main.rs (W0): RBAC scopes, cost
//! accounting, model routing, activity and twin mirroring emit helpers,
//! and text sanitizers for LLM output.

#[allow(unused_imports)]
use crate::prelude::*;
#[allow(unused_imports)]
use crate::*;


// ─────────────────────────────────────────────
//  State
// ─────────────────────────────────────────────

/// Holds the tray "Stop Recording" menu item so we can enable/disable dynamically
// ─────────────────────────────────────────────
//  Agent Activity (Watch tab events)
// ─────────────────────────────────────────────


/// Get the permission scopes for an agent based on its role/capabilities
pub(crate) fn agent_scopes(agent: &Agent) -> Vec<PermissionScope> {
    // Default: all agents can read, notify, and use relay
    let mut scopes = vec![
        PermissionScope::ReadLocal,
        PermissionScope::NotifyHuman,
    ];
    // Local agents get full local access
    if agent.provider == AgentProvider::Local {
        scopes.push(PermissionScope::WriteLocal);
        scopes.push(PermissionScope::ExecShell);
        scopes.push(PermissionScope::CallApiRead);
        scopes.push(PermissionScope::CallApiWrite);
        scopes.push(PermissionScope::SpawnAgent);
    }
    // Agents with API keys can use cloud and read/write APIs
    if !agent.api_key.is_empty() {
        scopes.push(PermissionScope::RelayCloud);
        scopes.push(PermissionScope::CallApiRead);
        scopes.push(PermissionScope::CallApiWrite);
    }
    // Default agents get everything
    if agent.is_default {
        scopes.push(PermissionScope::WriteLocal);
        scopes.push(PermissionScope::ExecShell);
        scopes.push(PermissionScope::CallApiRead);
        scopes.push(PermissionScope::CallApiWrite);
        scopes.push(PermissionScope::SpawnAgent);
        scopes.push(PermissionScope::AdminFull);
    }
    scopes
}

/// Check if a tool action is permitted for an agent
pub(crate) fn check_rbac(agent: Option<&Agent>, tool_name: &str) -> Result<(), String> {
    let agent = match agent {
        Some(a) => a,
        None => return Ok(()), // No agent context = Bob default = allow all
    };
    let scopes = agent_scopes(agent);

    let required = match tool_name {
        "shell_exec" => PermissionScope::ExecShell,
        "file_write" => PermissionScope::WriteLocal,
        "file_read" | "file_list" | "app_bridge" | "read_app_window" => PermissionScope::ReadLocal,
        "spawn_agent" | "create_agent" => PermissionScope::SpawnAgent,
        t if t.contains("api_call") || t.contains("http") => PermissionScope::CallApiWrite,
        _ => return Ok(()), // Unknown tool = allow
    };

    if scopes.contains(&required) || scopes.contains(&PermissionScope::AdminFull) {
        Ok(())
    } else {
        Err(format!("RBAC: Agent '{}' lacks {:?} permission for tool '{}'", agent.name, required, tool_name))
    }
}

/// Model cost rates per million tokens (input/output)
pub(crate) fn model_cost_per_m(model: &str, is_local: bool) -> (f64, f64) {
    if is_local { return (0.0, 0.0); }
    match model {
        m if m.contains("haiku") => (0.80, 4.0),
        m if m.contains("sonnet") => (3.0, 15.0),
        m if m.contains("opus") => (15.0, 75.0),
        _ => (1.0, 5.0), // default estimate
    }
}

/// Calculate cost from token usage
pub(crate) fn calculate_cost(model: &str, is_local: bool, tokens_in: u64, tokens_out: u64) -> f64 {
    let (rate_in, rate_out) = model_cost_per_m(model, is_local);
    (tokens_in as f64 * rate_in / 1_000_000.0) + (tokens_out as f64 * rate_out / 1_000_000.0)
}

/// Model routing: pick the cheapest capable model for a task
pub(crate) fn route_model_for_task(task: &str, agents: &[Agent]) -> (String, String, bool) {
    let task_lower = task.to_lowercase();

    // Complexity signals
    let is_complex = task_lower.contains("research") || task_lower.contains("analyz")
        || task_lower.contains("strategy") || task_lower.contains("compare")
        || task_lower.contains("synthesize") || task_lower.contains("architect")
        || task.len() > 500;
    let is_high_stakes = task_lower.contains("financial") || task_lower.contains("legal")
        || task_lower.contains("contract") || task_lower.contains("compliance");

    // Tier 1: Local model (free) - handles 70-80% of tasks
    if !is_complex && !is_high_stakes {
        if let Some(local) = agents.iter().find(|a| a.enabled && a.provider == AgentProvider::Local) {
            return (local.model.clone(), "local".to_string(), true);
        }
    }

    // Tier 2: Haiku (cheap cloud) - moderate tasks
    if !is_high_stakes {
        if let Some(a) = agents.iter().find(|a| a.enabled && !a.api_key.is_empty() && a.model.contains("haiku")) {
            return (a.model.clone(), "anthropic-us".to_string(), false);
        }
    }

    // Tier 3: Sonnet (mid cloud) - complex tasks
    if let Some(a) = agents.iter().find(|a| a.enabled && !a.api_key.is_empty() && a.model.contains("sonnet")) {
        return (a.model.clone(), "anthropic-us".to_string(), false);
    }

    // Tier 4: Opus (expensive) - only when explicitly needed
    if is_high_stakes {
        if let Some(a) = agents.iter().find(|a| a.enabled && !a.api_key.is_empty() && a.model.contains("opus")) {
            return (a.model.clone(), "anthropic-us".to_string(), false);
        }
    }

    // Fallback: any local model
    if let Some(local) = agents.iter().find(|a| a.enabled && a.provider == AgentProvider::Local) {
        return (local.model.clone(), "local".to_string(), true);
    }

    ("unknown".to_string(), "none".to_string(), false)
}

/// Maximum sub-agent spawn depth
pub(crate) const MAX_AGENT_DEPTH: u32 = 2;
/// Maximum iterations per agent task
pub(crate) const MAX_AGENT_ITERATIONS: u32 = 10;



/// Make a real HTTP call to OrbixSystems and return (status_code, response_body).
/// Automatically attaches the stored auth token as a Bearer header.

/// Broadcast a JSON event to all connected WebSocket (iOS) clients.
pub(crate) fn broadcast_ws(app: &tauri::AppHandle, event_type: &str, data: serde_json::Value) {
    if let Some(state) = app.try_state::<AppState>() {
        let msg = serde_json::json!({ "type": event_type, "data": data });
        let _ = state.connect.ws_broadcast.send(msg.to_string());
    }
}

/// Emit a Tauri event AND broadcast to WebSocket clients.
pub(crate) fn emit_and_broadcast<S: Serialize + Clone>(app: &tauri::AppHandle, event: &str, payload: S) {
    crate::event_bus::emit_ui(app, event, payload.clone()).ok();
    let ws_type = match event {
        "orb-state-change" => "orb-state",
        "new-message" => "new-message",
        "data-changed" => "data-changed",
        "agent-activity" => "agent-activity",
        other => other,
    };
    if let Ok(data) = serde_json::to_value(&payload) {
        broadcast_ws(app, ws_type, data);
    }
}

/// Emit an agent-activity event with real API call details and store it in the log.
pub(crate) fn emit_activity(
    app: &tauri::AppHandle,
    agent_name: &str,
    action: &str,
    status: &str,
    workflow_name: &str,
    step_type: &str,
    detail: Option<String>,
    result: Option<String>,
) {
    let activity = AgentActivity {
        id: Uuid::new_v4().to_string(),
        agent_name: agent_name.to_string(),
        action: action.to_string(),
        status: status.to_string(),
        workflow_name: workflow_name.to_string(),
        timestamp: Utc::now().to_rfc3339(),
        detail,
        result,
        step_type: Some(step_type.to_string()),
        rationale: None,
        model_used: None,
        tokens_in: None,
        tokens_out: None,
        cost_usd: None,
        inference_location: None,
        agent_depth: None,
    };
    emit_and_broadcast(app, "agent-activity", activity.clone());
    if let Some(state) = app.try_state::<AppState>() {
        if let Ok(mut log) = state.chat.activity_log.lock() {
            crate::models::agent::push_capped(&mut log, activity);
        }
    }
}

/// Emit an LLM-specific activity with full rationale log metadata
pub(crate) fn emit_llm_activity(
    app: &tauri::AppHandle,
    agent_name: &str,
    action: &str,
    model: &str,
    is_local: bool,
    tokens_in: u64,
    tokens_out: u64,
    rationale: Option<String>,
    depth: u32,
) {
    let cost = calculate_cost(model, is_local, tokens_in, tokens_out);
    let location = if is_local { "local" } else { "anthropic-us" };

    let activity = AgentActivity {
        id: Uuid::new_v4().to_string(),
        agent_name: agent_name.to_string(),
        action: action.to_string(),
        status: "complete".to_string(),
        workflow_name: String::new(),
        timestamp: Utc::now().to_rfc3339(),
        detail: None,
        result: None,
        step_type: Some("llm".to_string()),
        rationale,
        model_used: Some(format!("{}/{}", location, model)),
        tokens_in: Some(tokens_in),
        tokens_out: Some(tokens_out),
        cost_usd: Some(cost),
        inference_location: Some(location.to_string()),
        agent_depth: Some(depth),
    };

    emit_and_broadcast(app, "agent-activity", activity.clone());

    if let Some(state) = app.try_state::<AppState>() {
        if let Ok(mut log) = state.chat.activity_log.lock() {
            crate::models::agent::push_capped(&mut log, activity);
        }
        // Update session cost
        if let Ok(mut sc) = state.chat.session_cost.lock() {
            sc.total_tokens_in += tokens_in;
            sc.total_tokens_out += tokens_out;
            sc.total_cost_usd += cost;
            sc.call_count += 1;
            if is_local { sc.local_calls += 1; } else { sc.cloud_calls += 1; }
        }
        // Emit cost update for the ticker
        if let Ok(sc) = state.chat.session_cost.lock() {
            crate::event_bus::emit_ui(app, "cost-update", sc.clone()).ok();
        }
    }
}

/// Mirror a business record into the semantic twin and append a ledger event.
/// Called by Bob's action executor and CRM/ERP mutation commands so the Twin tab
/// always reflects the latest state of every record touched by the system.
pub(crate) fn mirror_to_twin(
    state: &AppState,
    app: &tauri::AppHandle,
    entity_type: semantic_twin::TwinEntityType,
    external_id: &str,
    name: &str,
    state_str: &str,
    properties: std::collections::HashMap<String, serde_json::Value>,
    source: &str,
) {
    

    // Parse state string → EntityState
    let twin_state: semantic_twin::EntityState =
        serde_json::from_value(serde_json::json!(state_str))
            .unwrap_or(semantic_twin::EntityState::Active);

    // Upsert into the twin store
    let (entity, was_created) = {
        let mut store = state.core.twin_store.lock().unwrap_or_else(|e| e.into_inner());
        store.upsert_record(entity_type.clone(), external_id, name, twin_state, properties.clone())
    };

    // Append a ledger event
    {
        let mut ledger = state.core.event_ledger.lock().unwrap_or_else(|e| e.into_inner());
        let event_type = if was_created {
            event_ledger::EventType::EntityCreated
        } else {
            event_ledger::EventType::EntityUpdated
        };
        let type_str = format!("{:?}", entity_type).to_lowercase();
        ledger.append_simple(
            &entity.id,
            name,
            &type_str,
            event_type,
            None,
            Some(serde_json::json!(properties)),
            source,
        );
    }

    // Notify frontend
    crate::event_bus::emit_ui(app, "data-changed", "twin-update").ok();
}



/// Truncate a JSON response for display (keep it readable).
pub(crate) fn truncate_json(s: &str, max: usize) -> String {
    if s.len() <= max { return s.to_string(); }
    format!("{}... [truncated]", &s[..max])
}

pub(crate) fn truncate_str(s: &str, max: usize) -> String {
    if s.len() <= max { return s.to_string(); }
    // Byte-slicing at `max` can land mid-codepoint and panic. Back up to
    // the nearest char boundary at or below max.
    let mut end = max;
    while end > 0 && !s.is_char_boundary(end) {
        end -= 1;
    }
    format!("{}...", &s[..end])
}

/// Generic anti-hallucination: after actions execute, keep ONLY the real action results.
/// The format_api_result function produces trusted output - the LLM's surrounding text is unreliable.
/// Trim Bob's redundant prose decoration that follows a successful action.
///
/// After an action like `create_workflow` runs, the action handler injects a
/// trusted confirmation line (`**Workflow "X" created** with N steps. Run it
/// from the Workflows panel.`). Small models (Gemma 4 E4B) keep rambling
/// after that, restating the workflow definition as markdown headings
/// (`## Workflow: X`, `### Step 1 - ...`, plain `Step 1 -`). That prose
/// duplicates data that's already saved in the Workflows panel and
/// frequently truncates mid-stream when Gemma burns its remaining tokens.
///
/// Walks the message line by line. Once it has seen a confirmation line, any
/// subsequent line that opens a decoration block triggers a cut.
/// Last-resort display guard: remove any <action>...</action> block (or a
/// dangling unclosed <action> tail) that survived to a user-facing reply.
/// Executed actions are substituted with confirmations upstream; anything
/// still tagged here is machinery, never content.
pub fn strip_action_tags(content: &str) -> String {
    let mut out = String::with_capacity(content.len());
    let mut rest = content;
    while let Some(open) = rest.find("<action>") {
        out.push_str(&rest[..open]);
        match rest[open + 8..].find("</action>") {
            Some(close) => rest = &rest[open + 8 + close + 9..],
            None => {
                rest = "";
            }
        }
    }
    out.push_str(rest);
    out.trim().to_string()
}

pub fn strip_post_action_decoration(content: &str) -> String {
    fn norm(s: &str) -> String {
        // Normalise: lowercase, drop bold/italic markers, collapse spaces.
        s.to_lowercase().replace("**", "").replace('*', "").trim().to_string()
    }

    fn is_confirmation(line: &str) -> bool {
        let n = norm(line);
        // Action handlers emit lines like:
        //   workflow "x" created with 1 steps. run it from the workflows panel.
        //   agent "x" created.
        //   workflow saved as ...
        //   workflow live
        n.contains(" created") && (n.contains(" steps") || n.contains(" with") || n.contains(" run "))
            || n.contains("workflow saved")
            || n.contains("workflow live")
            || n.starts_with("agent created")
            || n.starts_with("agent updated")
            || n.starts_with("agent deleted")
    }

    fn is_decoration_start(line: &str) -> bool {
        let n = norm(line);
        if n.is_empty() { return false; }
        // Markdown headers that restate the workflow or its steps.
        if n.starts_with("## workflow") || n.starts_with("### workflow") { return true; }
        if n.starts_with("## step ") || n.starts_with("### step ") { return true; }
        // Plain headings or labels the model leaves in the body.
        if n.starts_with("workflow:") || n.starts_with("workflow ") && n.contains(":") { return true; }
        // "Step 1 - ...", "Step 1 - ...", "Step 1: ..."
        if let Some(rest) = n.strip_prefix("step ") {
            // look for digit then a separator
            let bytes = rest.as_bytes();
            if bytes.first().map_or(false, |c| c.is_ascii_digit()) {
                if rest.contains(" - ") || rest.contains(" - ") || rest.contains(": ") || rest.contains(' ') {
                    return true;
                }
            }
        }
        false
    }

    let mut saw_confirmation = false;
    let mut cut_byte: Option<usize> = None;
    let mut byte_cursor: usize = 0;
    for line in content.split_inclusive('\n') {
        if !saw_confirmation {
            if is_confirmation(line) { saw_confirmation = true; }
        } else if is_decoration_start(line) {
            cut_byte = Some(byte_cursor);
            break;
        }
        byte_cursor += line.len();
    }

    match cut_byte {
        Some(idx) => content[..idx].trim_end().to_string(),
        None => content.to_string(),
    }
}

/// Remove <think>...</think> deliberation blocks (and anything after an
/// unclosed <think>) from user-facing text. The chat frontend also strips
/// these for display, but voice and every other consumer of final_text
/// must never see them either.
/// The reply contract when machinery follows the prose: an <inputs>
/// block (or a stray <action> tag) is preceded by the user-facing text
/// only. Lines that narrate deliberation ("The user wants...", "I need
/// to...", "Plan:") are dropped; everything else is kept in full - an
/// answer is never cut to two sentences. Replies without machinery are
/// left alone.
pub fn enforce_reply_contract(content: &str) -> String {
    let block_start = ["<inputs", "<action"]
        .iter()
        .filter_map(|t| content.find(t))
        .min();
    let Some(start) = block_start else { return content.to_string() };
    let (prose, rest) = content.split_at(start);
    let mut kept_lines: Vec<String> = Vec::new();
    for line in prose.lines() {
        // Sentence by sentence: deliberation and the real lead-in often
        // share one line ("The user wants X. Two quick questions.").
        let mut sentences: Vec<&str> = Vec::new();
        let mut cur = 0;
        for (i, c) in line.char_indices() {
            if matches!(c, '.' | '!' | '?') {
                let end = i + c.len_utf8();
                if !line[cur..end].trim().is_empty() {
                    sentences.push(line[cur..end].trim());
                }
                cur = end;
            }
        }
        if !line[cur..].trim().is_empty() {
            sentences.push(line[cur..].trim());
        }
        let kept: Vec<&str> = sentences.into_iter().filter(|s| !is_planning_line(s)).collect();
        if !kept.is_empty() {
            kept_lines.push(kept.join(" "));
        }
    }
    let kept = kept_lines.join("\n").trim().to_string();
    if kept.is_empty() {
        rest.to_string()
    } else {
        format!("{}\n{}", kept, rest)
    }
}

/// A line that narrates the model's own deliberation rather than
/// addressing the user. Mirrors the chat frontend's planning-line
/// detector so voice and every other consumer see the same reply.
pub fn is_planning_line(line: &str) -> bool {
    let l = line
        .trim()
        .trim_start_matches(|c: char| c == '*' || c == '-' || c == '#' || c == ' ' || c == '>')
        .to_lowercase();
    if l.is_empty() {
        return false;
    }
    const STARTS: &[&str] = &[
        "the user ", "the user's", "i need to", "i should", "i must", "i will ", "i'll ", "i'm going to",
        "let me ", "plan:", "goal:", "approach:", "strategy:", "reasoning:", "here's the plan",
        "here is the plan", "this falls under", "first, i ", "next, i ", "then, i ",
    ];
    STARTS.iter().any(|p| l.starts_with(p))
}

pub fn strip_think_blocks(content: &str) -> String {
    let mut out = String::with_capacity(content.len());
    let mut rest = content;
    loop {
        match rest.find("<think>") {
            None => {
                out.push_str(rest);
                break;
            }
            Some(start) => {
                out.push_str(&rest[..start]);
                match rest[start..].find("</think>") {
                    Some(end_rel) => rest = &rest[start + end_rel + "</think>".len()..],
                    None => break, // unclosed: drop the tail
                }
            }
        }
    }
    out.trim().to_string()
}

pub fn strip_hallucinated_content(content: &str) -> String {
    // Check if content contains any action result markers from our formatters
    let has_action_results = content.contains("**OK:") || content.contains("**[ERR]") || content.contains("**Found ");
    if !has_action_results {
        return content.to_string();
    }

    // Extract only the trusted action result blocks, discard all LLM-generated text
    let mut result_lines: Vec<String> = Vec::new();
    let mut in_result_block = false;

    for line in content.lines() {
        let t = line.trim();

        // Start of a trusted result block (produced by format_api_result or action handlers)
        if t.starts_with("**OK:") || t.starts_with("**[ERR]") || t.starts_with("**Found ") {
            in_result_block = true;
            result_lines.push(line.to_string());
            continue;
        }

        // Lines within a trusted result block (formatted bullets, status indicators, continuation)
        if in_result_block {
            if t.starts_with("•") || t.starts_with("[open]") || t.starts_with("[done]") ||
               t.starts_with("[wip]") || t.starts_with("[-]") || t.starts_with("... and") ||
               t.starts_with("**CRM**") || t.starts_with("**Finance**") || t.starts_with("**ERP**") ||
               t.starts_with("**PPM**") || t.starts_with("**Support**") ||
               t.starts_with("No ") || t.is_empty() {
                result_lines.push(line.to_string());
                continue;
            }
            // Non-matching line - end of trusted block
            in_result_block = false;
        }
        // Everything else is LLM commentary - discard it
    }

    // If we extracted results, return just those
    if !result_lines.is_empty() {
        // Clean up trailing empty lines
        while result_lines.last().map_or(false, |l| l.trim().is_empty()) {
            result_lines.pop();
        }
        return result_lines.join("\n");
    }

    // Fallback: no trusted blocks found, return as-is
    content.to_string()
}

/// Smart formatting for OrbixSystems API responses - summarize instead of dumping raw JSON.
pub(crate) fn format_api_result(method: &str, url: &str, status: u16, body: &str) -> String {
    // Detect HTML responses (API route not found - server returned SPA fallback)
    let trimmed = body.trim_start();
    if trimmed.starts_with("<!DOCTYPE") || trimmed.starts_with("<html") || trimmed.starts_with("<HTML") {
        return format!("**[ERR] {} {} - API route not found.** The server returned HTML instead of JSON. Check the endpoint path.", method, url);
    }

    let parsed: Result<serde_json::Value, _> = serde_json::from_str(body);
    let json = match parsed {
        Ok(v) => v,
        Err(_) => return format!("**OK: {} {} ({})**\n{}", method, url, status,
            if body.len() > 500 { format!("{}...", &body[..500]) } else { body.to_string() }),
    };

    // Detect response shape and format accordingly
    // Array of items (tickets, companies, etc.)
    if let Some(arr) = json.get("data").and_then(|d| d.as_array()).or_else(|| json.as_array()) {
        let path_lower = url.to_lowercase();
        let entity = if path_lower.contains("/tickets") { "ticket" }
            else if path_lower.contains("/companies") { "company" }
            else if path_lower.contains("/contacts") { "contact" }
            else if path_lower.contains("/deals") { "deal" }
            else if path_lower.contains("/invoices") { "invoice" }
            else if path_lower.contains("/products") { "product" }
            else if path_lower.contains("/projects") { "project" }
            else if path_lower.contains("/tasks") { "task" }
            else if path_lower.contains("/expenses") { "expense" }
            else { "item" };

        let total = arr.len();
        if total == 0 {
            return format!("**Found 0 {}s.**\n\nNo {} data exists.", entity, entity);
        }
        let mut lines = Vec::new();
        lines.push(format!("**OK: Found {} {}s**", total, entity));
        lines.push(String::new());

        // Generic formatter: auto-detect the best fields from each JSON object
        for item in arr.iter().take(15) {
            let obj = match item.as_object() {
                Some(o) => o,
                None => continue,
            };

            // Find the best "label" field (name/title/subject/description/number)
            let label_keys = ["name", "title", "subject", "ticket_number", "invoice_number",
                              "order_number", "description", "label", "key"];
            let label = label_keys.iter()
                .find_map(|k| obj.get(*k).and_then(|v| v.as_str()).filter(|s| !s.is_empty()))
                .unwrap_or("-");
            let label_short = if label.len() > 60 { format!("{}...", &label[..60]) } else { label.to_string() };

            // Find monetary value (amount/value/total/price/cost)
            let money_keys = ["amount", "value", "total", "price", "cost", "revenue"];
            let money = money_keys.iter()
                .find_map(|k| obj.get(*k).and_then(|v| v.as_f64()));

            // Find status
            let status = obj.get("status").and_then(|v| v.as_str()).or_else(||
                obj.get("stage").and_then(|v| v.as_str()));

            // Find category/type
            let cat_keys = ["category", "type", "industry", "priority", "department"];
            let category = cat_keys.iter()
                .find_map(|k| obj.get(*k).and_then(|v| v.as_str()));

            // Find date
            let date_keys = ["date", "created_at", "expense_date", "due_date", "start_date", "updated_at"];
            let date = date_keys.iter()
                .find_map(|k| obj.get(*k).and_then(|v| v.as_str()));

            // Find secondary info (vendor/company/assignee/owner)
            let sec_keys = ["vendor", "company_name", "company", "assignee", "owner", "client", "contact_name"];
            let secondary = sec_keys.iter()
                .find_map(|k| obj.get(*k).and_then(|v| v.as_str()).filter(|s| !s.is_empty()));

            // Build the line: • **Label** - $amount `status` `category` (secondary) date
            let mut line = format!("• **{}**", label_short);
            if let Some(m) = money { line.push_str(&format!(" - ${:.2}", m)); }
            if let Some(s) = status { line.push_str(&format!(" `{}`", s)); }
            if let Some(c) = category { line.push_str(&format!(" `{}`", c)); }
            if let Some(s) = secondary { line.push_str(&format!(" ({})", s)); }
            if let Some(d) = date {
                let d_short = d.split('T').next().unwrap_or(d).split(' ').next().unwrap_or(d);
                line.push_str(&format!(" {}", d_short));
            }

            lines.push(line);
        }

        if total > 15 {
            lines.push(format!("\n... and {} more", total - 15));
        }

        return lines.join("\n");
    }

    // Dashboard overview - format KPIs
    if url.contains("/dashboard/overview") {
        let mut lines = vec!["**OK: Dashboard Overview**".to_string(), String::new()];
        if let Some(crm) = json.get("crm") {
            let companies = crm["totalCompanies"].as_i64().unwrap_or(0);
            let deals = crm["totalDeals"].as_i64().unwrap_or(0);
            let pipeline = crm["pipelineValue"].as_f64().unwrap_or(0.0);
            lines.push(format!("**CRM**: {} companies, {} deals, ${:.0} pipeline", companies, deals, pipeline));
        }
        if let Some(fin) = json.get("finance") {
            let revenue = fin["totalRevenue"].as_f64().unwrap_or(0.0);
            let expenses = fin["totalExpenses"].as_f64().unwrap_or(0.0);
            lines.push(format!("**Finance**: ${:.0} revenue, ${:.0} expenses", revenue, expenses));
        }
        if let Some(erp) = json.get("erp") {
            let products = erp["totalProducts"].as_i64().unwrap_or(0);
            let low = erp["lowStockItems"].as_i64().unwrap_or(0);
            lines.push(format!("**ERP**: {} products, {} low stock", products, low));
        }
        if let Some(ppm) = json.get("ppm") {
            let projects = ppm["activeProjects"].as_i64().unwrap_or(0);
            lines.push(format!("**PPM**: {} active projects", projects));
        }
        if let Some(sup) = json.get("support") {
            let open = sup["openTickets"].as_i64().unwrap_or(0);
            let critical = sup["criticalTickets"].as_i64().unwrap_or(0);
            lines.push(format!("**Support**: {} open tickets ({} critical)", open, critical));
        }
        return lines.join("\n");
    }

    // Single object (e.g., one ticket, one company)
    if json.is_object() && !json.as_object().unwrap().is_empty() {
        let pretty = serde_json::to_string_pretty(&json).unwrap_or_default();
        if pretty.len() > 3000 {
            return format!("**OK: {} {} ({})**\n```json\n{}...\n```", method, url, status, &pretty[..3000]);
        }
        return format!("**OK: {} {} ({})**\n```json\n{}\n```", method, url, status, pretty);
    }

    // Fallback
    format!("**OK: {} {} ({})**\n```json\n{}\n```", method, url, status,
        if body.len() > 3000 { format!("{}...", &body[..3000]) } else { body.to_string() })
}

/// Map a workflow step to the best agent using capability-based routing.
pub(crate) fn infer_agent_for_step(step: &WorkflowStep, agents: &[Agent]) -> String {
    let routable: Vec<agent_router::RoutableAgent> = agents.iter().map(|a| {
        agent_router::RoutableAgent {
            id: &a.id,
            name: &a.name,
            enabled: a.enabled,
            is_default: a.is_default,
            capabilities: &a.capabilities,
            connector_ids: &a.connector_ids,
        }
    }).collect();

    let connector_id = step.config.get("connector_id").and_then(|v| v.as_str());
    let command_url = step.config.get("command").and_then(|v| v.as_str());
    let step_prompt = step.config.get("prompt").and_then(|v| v.as_str());

    agent_router::route_step(&routable, connector_id, command_url, &step.label, step_prompt)
        .map(|m| m.agent_name)
        .unwrap_or_else(|| "Orbix Orchestrator".to_string())
}

// ─────────────────────────────────────────────
//  Agent Model
// ─────────────────────────────────────────────


// ─────────────────────────────────────────────
//  MCP Server Model
// ─────────────────────────────────────────────


// ─────────────────────────────────────────────
//  Message Model
// ─────────────────────────────────────────────



#[cfg(test)]
mod truncate_tests {
    #[test]
    fn truncate_str_never_splits_a_codepoint() {
        // A multi-byte char straddling the cut point must not panic and the
        // result must be valid UTF-8 (guaranteed by returning a String).
        let s = "aaaa\u{1F600}bbbb"; // emoji is 4 bytes at index 4..8
        for max in 0..s.len() {
            let out = super::truncate_str(s, max); // must not panic
            assert!(out.len() >= 3); // at least the "..."
        }
        assert_eq!(super::truncate_str("short", 100), "short");
    }
}

#[cfg(test)]
mod think_strip_tests {
    #[test]
    fn strips_complete_blocks() {
        let s = "Before. <think>secret reasoning</think>After.";
        assert_eq!(super::strip_think_blocks(s), "Before. After.");
    }
    #[test]
    fn drops_unclosed_tail() {
        let s = "Answer here. <think>still going";
        assert_eq!(super::strip_think_blocks(s), "Answer here.");
    }
    #[test]
    fn untouched_without_tags() {
        assert_eq!(super::strip_think_blocks("plain reply"), "plain reply");
    }
}

#[cfg(test)]
mod reply_contract_tests {
    use super::enforce_reply_contract;

    #[test]
    fn cuts_deliberation_before_inputs() {
        let s = "The user wants a panel. This requires data. I need to determine the source. \
I do not have a connector. The most robust approach is to ask. I will ask now.\n\
<inputs>[{\"id\":\"src\",\"label\":\"Which site?\"}]</inputs>";
        let out = enforce_reply_contract(s);
        assert!(out.contains("<inputs>"));
        assert!(!out.contains("The user wants"));
        assert!(!out.contains("I need to determine"));
        assert!(!out.contains("I will ask now."));
        // Sentences that address the user survive, in order.
        assert!(out.starts_with("This requires data. I do not have a connector. The most robust approach is to ask."), "{}", out);
    }

    #[test]
    fn full_answer_before_inputs_is_kept() {
        // The old contract kept only the last two sentences - a real
        // explanation followed by a question lost its body.
        let s = "Three options fit. Option A is cheapest but slow. Option B is fast and costs more. Option C needs a connector you have not set up.
Which matters more to you?
<inputs>[{\"id\":\"pri\",\"label\":\"Speed or cost?\"}]</inputs>";
        let out = enforce_reply_contract(s);
        assert!(out.contains("Option A is cheapest"));
        assert!(out.contains("Option C needs a connector"));
        assert!(out.contains("Which matters more to you?"));
        assert!(out.contains("<inputs>"));
    }

    #[test]
    fn short_leadin_untouched() {
        let s = "Two quick questions. Then I build.\n<inputs>[]</inputs>";
        assert_eq!(enforce_reply_contract(s), s);
    }

    #[test]
    fn no_machinery_untouched() {
        let s = "Here is a long conversational answer. It has many sentences. All of them matter. None should be cut.";
        assert_eq!(enforce_reply_contract(s), s);
    }
}
