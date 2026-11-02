//! Commands split out of main.rs (W0.3). Mechanical move, no behavior change.

#[allow(unused_imports)]
use crate::prelude::*;
#[allow(unused_imports)]
use crate::*;

// ─────────────────────────────────────────────
//  Agent Council commands
// ─────────────────────────────────────────────

#[tauri::command]
pub(crate) fn council_review(
    output: String,
    agent_name: String,
    input_context: String,
    prior_outputs: Vec<String>,
    tokens_used: u64,
    is_high_stakes: bool,
    state: State<'_, AppState>,
) -> Result<agent_council::CouncilDecision, String> {
    let mut store = state.engine.council_store.lock().unwrap_or_else(|e| e.into_inner());
    if !agent_council::should_review(&store.config, is_high_stakes) {
        return Ok(agent_council::CouncilDecision {
            id: Uuid::new_v4().to_string(),
            output_id: Uuid::new_v4().to_string(),
            agent_name,
            final_verdict: agent_council::Verdict::Approve,
            validator_results: vec![],
            original_output: output,
            revised_output: None,
            total_duration_ms: 0,
            timestamp: Utc::now().to_rfc3339(),
        });
    }
    let policies = store.policies.clone();
    let config = store.config.clone();
    let decision = agent_council::run_council(
        &output, &agent_name, &input_context, &prior_outputs,
        tokens_used, config.token_budget, None, &config, &policies,
    );
    store.decisions.push(decision.clone());
    Ok(decision)
}

#[tauri::command]
pub(crate) fn council_get_decisions(state: State<'_, AppState>, limit: Option<usize>) -> Result<Vec<agent_council::CouncilDecision>, String> {
    let store = state.engine.council_store.lock().unwrap_or_else(|e| e.into_inner());
    let n = limit.unwrap_or(50);
    Ok(store.decisions.iter().rev().take(n).cloned().collect())
}

#[tauri::command]
pub(crate) fn council_get_config(state: State<'_, AppState>) -> Result<agent_council::CouncilConfig, String> {
    let store = state.engine.council_store.lock().unwrap_or_else(|e| e.into_inner());
    Ok(store.config.clone())
}

#[tauri::command]
pub(crate) fn council_set_scope(scope: String, state: State<'_, AppState>) -> Result<(), String> {
    let mut store = state.engine.council_store.lock().unwrap_or_else(|e| e.into_inner());
    store.config.scope = match scope.as_str() {
        "always" => agent_council::CouncilScope::Always,
        "high_stakes" => agent_council::CouncilScope::HighStakes,
        "sampling" => agent_council::CouncilScope::Sampling,
        "never" => agent_council::CouncilScope::Never,
        _ => return Err("Invalid scope".into()),
    };
    Ok(())
}

#[tauri::command]
pub(crate) fn council_get_policies(state: State<'_, AppState>) -> Result<Vec<agent_council::PolicyRule>, String> {
    let store = state.engine.council_store.lock().unwrap_or_else(|e| e.into_inner());
    Ok(store.policies.clone())
}

#[tauri::command]
pub(crate) fn council_add_policy(name: String, pattern: String, action: String, state: State<'_, AppState>) -> Result<agent_council::PolicyRule, String> {
    // Validate regex
    regex::Regex::new(&pattern).map_err(|e| format!("Invalid regex: {}", e))?;
    let rule = agent_council::PolicyRule {
        id: Uuid::new_v4().to_string(),
        name,
        pattern,
        action,
    };
    let mut store = state.engine.council_store.lock().unwrap_or_else(|e| e.into_inner());
    store.policies.push(rule.clone());
    Ok(rule)
}

#[tauri::command]
pub(crate) fn council_remove_policy(id: String, state: State<'_, AppState>) -> Result<(), String> {
    let mut store = state.engine.council_store.lock().unwrap_or_else(|e| e.into_inner());
    store.policies.retain(|p| p.id != id);
    Ok(())
}

