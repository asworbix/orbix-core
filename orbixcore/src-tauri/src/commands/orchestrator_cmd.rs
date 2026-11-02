//! Commands split out of main.rs (W0.3). Mechanical move, no behavior change.

#[allow(unused_imports)]
use crate::prelude::*;
#[allow(unused_imports)]
use crate::*;

// ─────────────────────────────────────────────
//  Orchestrator Commands (Epic Y)
// ─────────────────────────────────────────────

#[tauri::command]
pub(crate) async fn list_chains(state: State<'_, AppState>) -> Result<Vec<orchestrator::AgentChain>, String> {
    let store = state.automation.orchestrator_store.lock().unwrap_or_else(|e| e.into_inner());
    Ok(store.list_chains())
}

#[tauri::command]
pub(crate) async fn get_chain(state: State<'_, AppState>, id: String) -> Result<orchestrator::AgentChain, String> {
    let store = state.automation.orchestrator_store.lock().unwrap_or_else(|e| e.into_inner());
    store.get_chain(&id).ok_or("Chain not found".into())
}

#[tauri::command]
pub(crate) async fn create_chain(
    state: State<'_, AppState>,
    name: String,
    description: String,
    mode: orchestrator::ChainMode,
    nodes: Vec<orchestrator::AgentChainNode>,
) -> Result<orchestrator::AgentChain, String> {
    let mut store = state.automation.orchestrator_store.lock().unwrap_or_else(|e| e.into_inner());
    Ok(store.create_chain(name, description, mode, nodes))
}

#[tauri::command]
pub(crate) async fn update_chain(
    state: State<'_, AppState>,
    id: String,
    name: Option<String>,
    description: Option<String>,
    mode: Option<orchestrator::ChainMode>,
    nodes: Option<Vec<orchestrator::AgentChainNode>>,
) -> Result<orchestrator::AgentChain, String> {
    let mut store = state.automation.orchestrator_store.lock().unwrap_or_else(|e| e.into_inner());
    store.update_chain(&id, name, description, mode, nodes).ok_or("Chain not found".into())
}

#[tauri::command]
pub(crate) async fn delete_chain(state: State<'_, AppState>, id: String) -> Result<bool, String> {
    let mut store = state.automation.orchestrator_store.lock().unwrap_or_else(|e| e.into_inner());
    Ok(store.delete_chain(&id))
}

#[tauri::command]
pub(crate) async fn list_chain_templates() -> Result<Vec<orchestrator::ChainTemplate>, String> {
    Ok(orchestrator::builtin_templates())
}

#[tauri::command]
pub(crate) async fn run_chain(
    state: State<'_, AppState>,
    chain_id: String,
    input: String,
    app: tauri::AppHandle,
) -> Result<orchestrator::ChainRunState, String> {
    run_chain_internal(&state, &chain_id, &input, &app).await
}

/// Resolve a chain by name OR id, then execute it. Used by both the Tauri
/// command and Bob's <action>{"type":"run_chain"}</action> dispatcher.
pub async fn run_chain_internal(
    state: &AppState,
    chain_id_or_name: &str,
    input: &str,
    app: &tauri::AppHandle,
) -> Result<orchestrator::ChainRunState, String> {
    let chain = {
        let store = state.automation.orchestrator_store.lock().unwrap_or_else(|e| e.into_inner());
        // Try id first, then case-insensitive name match
        store.get_chain(chain_id_or_name)
            .or_else(|| {
                store.list_chains()
                    .into_iter()
                    .find(|c| c.name.to_lowercase() == chain_id_or_name.to_lowercase())
            })
            .ok_or("Chain not found")?
    };

    if chain.nodes.is_empty() {
        return Err("Chain has no agent nodes".into());
    }

    {
        let mut store = state.automation.orchestrator_store.lock().unwrap_or_else(|e| e.into_inner());
        let run = store.start_run(&chain, input.to_string());
        crate::event_bus::emit_ui(app, "chain-run-update", run).ok();
    }

    // W2.8: also track the chain execution as a unified Run so it lands in
    // the Run registry (durable, keyed) like a workflow run. The per-node
    // ChainRunState above still drives the detailed frontend view.
    let chain_run_id = Uuid::new_v4().to_string();
    {
        let mut runs = state.automation.run_store.lock().unwrap_or_else(|e| e.into_inner());
        runs.create_chain_run(
            chain_run_id.clone(),
            chain.id.clone(),
            chain.nodes.iter().map(|n| n.agent_name.clone()).collect(),
            Utc::now().to_rfc3339(),
        );
        runs.set_prompt(
            &chain_run_id,
            crate::run::Prompt::new(input.to_string(), "chain", Utc::now().to_rfc3339()),
            Utc::now().to_rfc3339(),
        );
    }

    let final_output = match chain.mode {
        orchestrator::ChainMode::Sequential => execute_sequential(state, &chain, input, app).await,
        orchestrator::ChainMode::FanOut    => execute_fan_out(state, &chain, input, app).await,
        orchestrator::ChainMode::Fallback  => execute_fallback(state, &chain, input, app).await,
        orchestrator::ChainMode::Specialist => execute_specialist(state, &chain, input, app).await,
    };
    // Reflect chain outcome onto the unified Run.
    let final_output = match final_output {
        Ok(out) => {
            let mut runs = state.automation.run_store.lock().unwrap_or_else(|e| e.into_inner());
            // Mark every node task complete and finish the Run.
            for i in 0..chain.nodes.len() {
                runs.begin_step(&chain_run_id, i, Utc::now().to_rfc3339());
                runs.complete_step(&chain_run_id, i, String::new(), Utc::now().to_rfc3339());
            }
            out
        }
        Err(e) => {
            let mut runs = state.automation.run_store.lock().unwrap_or_else(|e| e.into_inner());
            runs.fail_step(&chain_run_id, 0, e.clone(), Utc::now().to_rfc3339());
            return Err(e);
        }
    };

    // ── Agent Council validation on final output ──
    let council_decision = {
        let council = state.engine.council_store.lock().unwrap_or_else(|e| e.into_inner());
        let config = council.config.clone();
        let policies = council.policies.clone();
        drop(council);

        if agent_council::should_review(&config, true) {
            let tokens_used = agent_council::estimate_tokens(input)
                + agent_council::estimate_tokens(&final_output);
            let decision = agent_council::run_council(
                &final_output, &chain.name, input, &[],
                tokens_used, config.token_budget, None, &config, &policies,
            );
            crate::event_bus::emit_ui(app, "council-decision", &decision).ok();
            let mut council = state.engine.council_store.lock().unwrap_or_else(|e| e.into_inner());
            council.decisions.push(decision.clone());
            Some(decision)
        } else {
            None
        }
    };

    let validated_output = if let Some(ref d) = council_decision {
        if d.final_verdict == agent_council::Verdict::Reject {
            let reasons: Vec<&str> = d.validator_results.iter()
                .filter(|r| r.verdict == agent_council::Verdict::Reject)
                .map(|r| r.reason.as_str())
                .collect();
            format!("{}\n\n---\n**Council Review: REJECTED**\n{}", final_output, reasons.join("\n"))
        } else if d.final_verdict == agent_council::Verdict::Flag {
            let flags: Vec<&str> = d.validator_results.iter()
                .filter(|r| r.verdict == agent_council::Verdict::Flag)
                .map(|r| r.reason.as_str())
                .collect();
            format!("{}\n\n---\n**Council Review: FLAGGED**\n{}", final_output, flags.join("\n"))
        } else {
            final_output
        }
    } else {
        final_output
    };

    let result = {
        let mut store = state.automation.orchestrator_store.lock().unwrap_or_else(|e| e.into_inner());
        store.complete_run(validated_output).ok_or("No active run")?
    };
    crate::event_bus::emit_ui(app, "chain-run-update", result.clone()).ok();
    Ok(result)
}

/// Sequential: pass output of each agent to the next
pub(crate) async fn execute_sequential(
    state: &AppState,
    chain: &orchestrator::AgentChain,
    input: &str,
    app: &tauri::AppHandle,
) -> Result<String, String> {
    let mut current_input = input.to_string();

    for node in &chain.nodes {
        let start = std::time::Instant::now();
        let output = { let agents = state.chat.agents.lock().unwrap_or_else(|e| e.into_inner()).clone(); real_agent_call(&node.agent_name, &node.role_hint, &current_input, &agents).await };
        let duration = start.elapsed().as_millis() as u64;

        let result = orchestrator::NodeRunResult {
            node_id: node.id.clone(),
            agent_name: node.agent_name.clone(),
            status: orchestrator::NodeRunStatus::Completed,
            output: output.clone(),
            error: None,
            duration_ms: duration,
            tokens_used: estimate_tokens(&output),
        };

        {
            let mut store = state.automation.orchestrator_store.lock().unwrap_or_else(|e| e.into_inner());
            store.record_node_result(result);
            if let Some(run) = store.get_active_run() {
                crate::event_bus::emit_ui(app, "chain-run-update", run).ok();
            }
        }

        current_input = output;
    }

    Ok(current_input)
}

/// Fan-out: all nodes process in parallel, last node synthesizes
pub(crate) async fn execute_fan_out(
    state: &AppState,
    chain: &orchestrator::AgentChain,
    input: &str,
    app: &tauri::AppHandle,
) -> Result<String, String> {
    if chain.nodes.len() < 2 {
        return Err("Fan-out needs at least 2 nodes".into());
    }

    let parallel_nodes = &chain.nodes[..chain.nodes.len() - 1];
    let synthesizer = chain.nodes.last().unwrap();
    let mut outputs = Vec::new();

    // Run parallel nodes
    for node in parallel_nodes {
        let start = std::time::Instant::now();
        let output = { let agents = state.chat.agents.lock().unwrap_or_else(|e| e.into_inner()).clone(); real_agent_call(&node.agent_name, &node.role_hint, input, &agents).await };
        let duration = start.elapsed().as_millis() as u64;

        let result = orchestrator::NodeRunResult {
            node_id: node.id.clone(),
            agent_name: node.agent_name.clone(),
            status: orchestrator::NodeRunStatus::Completed,
            output: output.clone(),
            error: None,
            duration_ms: duration,
            tokens_used: estimate_tokens(&output),
        };

        {
            let mut store = state.automation.orchestrator_store.lock().unwrap_or_else(|e| e.into_inner());
            store.record_node_result(result);
            if let Some(run) = store.get_active_run() {
                crate::event_bus::emit_ui(app, "chain-run-update", run).ok();
            }
        }

        outputs.push(format!("[{}]: {}", node.role_hint, output));
    }

    // Synthesize
    let combined = outputs.join("\n\n");
    let synth_input = format!("Original task: {}\n\nAgent outputs:\n{}", input, combined);
    let start = std::time::Instant::now();
    let final_output = { let agents = state.chat.agents.lock().unwrap_or_else(|e| e.into_inner()).clone(); real_agent_call(&synthesizer.agent_name, &synthesizer.role_hint, &synth_input, &agents).await };
    let duration = start.elapsed().as_millis() as u64;

    let result = orchestrator::NodeRunResult {
        node_id: synthesizer.id.clone(),
        agent_name: synthesizer.agent_name.clone(),
        status: orchestrator::NodeRunStatus::Completed,
        output: final_output.clone(),
        error: None,
        duration_ms: duration,
        tokens_used: estimate_tokens(&final_output),
    };

    {
        let mut store = state.automation.orchestrator_store.lock().unwrap_or_else(|e| e.into_inner());
        store.record_node_result(result);
    }

    Ok(final_output)
}

/// Fallback: try each node until one succeeds
pub(crate) async fn execute_fallback(
    state: &AppState,
    chain: &orchestrator::AgentChain,
    input: &str,
    app: &tauri::AppHandle,
) -> Result<String, String> {
    for node in &chain.nodes {
        let start = std::time::Instant::now();
        let output = { let agents = state.chat.agents.lock().unwrap_or_else(|e| e.into_inner()).clone(); real_agent_call(&node.agent_name, &node.role_hint, input, &agents).await };
        let duration = start.elapsed().as_millis() as u64;

        // In a real system, check for errors/empty responses
        let succeeded = !output.is_empty();

        let result = orchestrator::NodeRunResult {
            node_id: node.id.clone(),
            agent_name: node.agent_name.clone(),
            status: if succeeded { orchestrator::NodeRunStatus::Completed } else { orchestrator::NodeRunStatus::Failed },
            output: output.clone(),
            error: if succeeded { None } else { Some("Empty response".into()) },
            duration_ms: duration,
            tokens_used: estimate_tokens(&output),
        };

        {
            let mut store = state.automation.orchestrator_store.lock().unwrap_or_else(|e| e.into_inner());
            store.record_node_result(result);
            if let Some(run) = store.get_active_run() {
                crate::event_bus::emit_ui(app, "chain-run-update", run).ok();
            }
        }

        if succeeded {
            return Ok(output);
        }
    }

    Err("All agents in fallback chain failed".into())
}

/// Specialist: route to best-matching agent by keywords
pub(crate) async fn execute_specialist(
    state: &AppState,
    chain: &orchestrator::AgentChain,
    input: &str,
    app: &tauri::AppHandle,
) -> Result<String, String> {
    let node = orchestrator::OrchestratorStore::route_specialist(chain, input)
        .ok_or("No specialist matched")?;

    let start = std::time::Instant::now();
    let output = { let agents = state.chat.agents.lock().unwrap_or_else(|e| e.into_inner()).clone(); real_agent_call(&node.agent_name, &node.role_hint, input, &agents).await };
    let duration = start.elapsed().as_millis() as u64;

    let result = orchestrator::NodeRunResult {
        node_id: node.id.clone(),
        agent_name: node.agent_name.clone(),
        status: orchestrator::NodeRunStatus::Completed,
        output: output.clone(),
        error: None,
        duration_ms: duration,
        tokens_used: estimate_tokens(&output),
    };

    {
        let mut store = state.automation.orchestrator_store.lock().unwrap_or_else(|e| e.into_inner());
        store.record_node_result(result);
        if let Some(run) = store.get_active_run() {
            crate::event_bus::emit_ui(app, "chain-run-update", run).ok();
        }
    }

    Ok(output)
}

/// Placeholder for actual agent API calls - returns simulated output
pub(crate) async fn real_agent_call(agent_name: &str, role_hint: &str, input: &str, agents: &[Agent]) -> String {
    let client = reqwest::Client::new();

    // Find the named agent, or fall back to any enabled local agent
    let agent = agents.iter()
        .find(|a| a.name.to_lowercase() == agent_name.to_lowercase() && a.enabled)
        .or_else(|| agents.iter().find(|a| a.enabled && a.provider == AgentProvider::Local))
        .or_else(|| agents.iter().find(|a| a.enabled));

    let truncated_input = if input.len() > 1500 { &input[..1500] } else { input };

    let prompt = format!(
        "You are {} (role: {}). Process this input concisely:\n\n{}",
        agent_name, role_hint, truncated_input
    );

    let history = vec![Message {
        id: String::new(),
        role: "user".to_string(),
        content: prompt,
        context: None,
        agent_id: None,
        timestamp: String::new(),
        tool_turns: None,
    }];

    // Try the agent's own endpoint first, then fall back to local LLM
    if let Some(a) = agent {
        if let Ok(result) = call_agent_api(a, &history).await {
            return result;
        }
    }

    // Fallback: call local LLM directly
    match call_local_llm(&client, &history).await {
        Ok(result) => result,
        Err(e) => format!("[{} ({})] Error: {}", agent_name, role_hint, e),
    }
}

pub(crate) fn estimate_tokens(text: &str) -> u64 {
    (text.len() as u64) / 4 // rough estimate
}

#[tauri::command]
pub(crate) async fn stop_chain(state: State<'_, AppState>, app: tauri::AppHandle) -> Result<(), String> {
    let mut store = state.automation.orchestrator_store.lock().unwrap_or_else(|e| e.into_inner());
    if let Some(stopped) = store.stop_run() {
        crate::event_bus::emit_ui(&app, "chain-run-update", stopped).ok();
    }
    Ok(())
}

#[tauri::command]
pub(crate) async fn get_chain_run(state: State<'_, AppState>) -> Result<Option<orchestrator::ChainRunState>, String> {
    let store = state.automation.orchestrator_store.lock().unwrap_or_else(|e| e.into_inner());
    Ok(store.get_active_run())
}

#[tauri::command]
pub(crate) async fn get_chain_history(state: State<'_, AppState>, limit: Option<usize>) -> Result<Vec<orchestrator::ChainRunState>, String> {
    let store = state.automation.orchestrator_store.lock().unwrap_or_else(|e| e.into_inner());
    Ok(store.get_run_history(limit.unwrap_or(20)))
}

