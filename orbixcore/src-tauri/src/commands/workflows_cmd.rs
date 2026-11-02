//! Commands split out of main.rs (W0.3). Mechanical move, no behavior change.

#[allow(unused_imports)]
use crate::prelude::*;
#[allow(unused_imports)]
use crate::*;

// ─────────────────────────────────────────────
//  Commands - Workflows / Teach (Epic G)
// ─────────────────────────────────────────────

#[tauri::command]
pub(crate) async fn list_workflows(state: State<'_, AppState>) -> Result<Vec<Workflow>, String> {
    let store = state.automation.workflow_store.lock().unwrap_or_else(|e| e.into_inner());
    Ok(store.list())
}

#[tauri::command]
pub(crate) async fn get_workflow(id: String, state: State<'_, AppState>) -> Result<Workflow, String> {
    let store = state.automation.workflow_store.lock().unwrap_or_else(|e| e.into_inner());
    store.get(&id).ok_or_else(|| format!("Workflow {} not found", id))
}

#[tauri::command]
pub(crate) async fn create_workflow(
    name: String,
    description: String,
    state: State<'_, AppState>,
) -> Result<Workflow, String> {
    let mut store = state.automation.workflow_store.lock().unwrap_or_else(|e| e.into_inner());
    let w = store.create(name, description);
    store.save();
    Ok(w)
}

#[tauri::command]
pub(crate) async fn update_workflow(
    id: String,
    name: Option<String>,
    description: Option<String>,
    state: State<'_, AppState>,
) -> Result<Workflow, String> {
    let mut store = state.automation.workflow_store.lock().unwrap_or_else(|e| e.into_inner());
    let w = store.update(&id, name, description)
        .ok_or_else(|| format!("Workflow {} not found", id))?;
    store.save();
    Ok(w)
}

/// Replace a workflow's steps wholesale. The single write path for canvas
/// editing: add / edit config / rename / delete / reorder all flow through
/// here, so the Build canvas never needs a teaching round-trip and every
/// edit (including labels, which previously only lived in UI state) is
/// persisted.
#[tauri::command]
pub(crate) async fn update_workflow_steps(
    workflow_id: String,
    steps: Vec<workflow::WorkflowStep>,
    state: State<'_, AppState>,
) -> Result<Workflow, String> {
    let mut store = state.automation.workflow_store.lock().unwrap_or_else(|e| e.into_inner());
    let wf = store.workflows.iter_mut().find(|w| w.id == workflow_id)
        .ok_or_else(|| format!("Workflow {} not found", workflow_id))?;
    // Normalise order to array position so callers just send the list.
    let mut steps = steps;
    for (i, s) in steps.iter_mut().enumerate() { s.order = i; }
    wf.steps = steps;
    wf.updated_at = Utc::now().to_rfc3339();
    let result = wf.clone();
    store.save();
    Ok(result)
}

#[tauri::command]
pub(crate) async fn delete_workflow(id: String, state: State<'_, AppState>) -> Result<(), String> {
    {
        let mut store = state.automation.workflow_store.lock().unwrap_or_else(|e| e.into_inner());
        if !store.delete(&id) {
            return Err(format!("Workflow {} not found", id));
        }
        store.save();
    }
    // Nothing may keep firing a workflow that no longer exists: its
    // schedules and triggers go with it (they used to linger as "Unknown").
    {
        let mut sched = state.automation.scheduler_store.lock().unwrap_or_else(|e| e.into_inner());
        let doomed: Vec<String> = sched.list_schedules().into_iter().filter(|s| s.workflow_id == id).map(|s| s.id).collect();
        for sid in doomed {
            sched.delete_schedule(&sid);
        }
        sched.save();
    }
    {
        let mut trig = state.automation.trigger_store.lock().unwrap_or_else(|e| e.into_inner());
        trig.rules.retain(|r| r.workflow_id != id);
        trig.webhooks.retain(|w| w.workflow_id != id);
        trig.file_watches.retain(|f| f.workflow_id != id);
        trig.save();
    }
    Ok(())
}

#[tauri::command]
pub(crate) async fn run_workflow(
    id: String,
    state: State<'_, AppState>,
    app: tauri::AppHandle,
) -> Result<WorkflowRunState, String> {
    run_workflow_core(&id, &state, &app).await
}

/// Core workflow runner, callable from background tasks (scheduler tick,
/// triggers) as well as the Tauri command. Unlike the old command body this
/// FAILS the run when a step fails instead of always reporting Completed,
/// carries Prompt-step text into the next step's context, and honours a
/// mid-run Stop.
pub async fn run_workflow_core(
    id: &str,
    state: &AppState,
    app: &tauri::AppHandle,
) -> Result<WorkflowRunState, String> {
    // W2: every execution is a durable, keyed Run. A unique run id means
    // concurrent workflows never share state, and each transition is
    // persisted so a crash mid-run leaves a record the startup resume pass
    // can continue.
    let run_id = Uuid::new_v4().to_string();
    run_one_workflow(&run_id, id, state, app, 0, None).await
}

/// Run a workflow triggered by an event (webhook, email, ...), handing
/// its payload to the steps as {{trigger}} / {{trigger.<field>}} and as
/// the first context entry so the AI mapping step can process it.
pub async fn run_workflow_with_trigger(
    id: &str,
    payload: serde_json::Value,
    state: &AppState,
    app: &tauri::AppHandle,
) -> Result<WorkflowRunState, String> {
    let run_id = Uuid::new_v4().to_string();
    run_one_workflow(&run_id, id, state, app, 0, Some(payload)).await
}

/// Per-step retry budget before a step (and its Run) is failed. Reuses the
/// backoff shape from job_scheduler: attempt 1 immediate, then growing
/// waits. Kept small so a genuinely broken step dead-letters quickly
/// instead of hanging the Run.
const STEP_MAX_ATTEMPTS: u32 = 3;

fn step_backoff_secs(attempt: u32) -> u64 {
    match attempt {
        0 | 1 => 0,
        2 => 3,
        _ => 10,
    }
}

/// Run (or resume) a workflow as Run `run_id`, starting at `start_step`.
/// Resume passes the persisted `current_step`; a fresh run passes 0.
pub async fn run_one_workflow(
    run_id: &str,
    id: &str,
    state: &AppState,
    app: &tauri::AppHandle,
    start_step: usize,
    trigger_input: Option<serde_json::Value>,
) -> Result<WorkflowRunState, String> {
    let steps = {
        let store = state.automation.workflow_store.lock().unwrap_or_else(|e| e.into_inner());
        store.get(id).map(|w| w.steps).unwrap_or_default()
    };
    if steps.is_empty() {
        return Err("Workflow has no steps".into());
    }

    // Create the Run record (or reuse it on resume) and announce it.
    let run = {
        let mut runs = state.automation.run_store.lock().unwrap_or_else(|e| e.into_inner());
        match runs.get(run_id) {
            Some(existing) => existing,
            None => {
                let run = runs.create_workflow_run(
                    run_id.to_string(),
                    id.to_string(),
                    steps.iter().map(|s| s.label.clone()).collect(),
                    Utc::now().to_rfc3339(),
                );
                // W2.8: record the originating instruction. The workflow name
                // is the closest prompt we have at the run boundary.
                let wf_name = {
                    let store = state.automation.workflow_store.lock().unwrap_or_else(|e| e.into_inner());
                    store.get(id).map(|w| w.name).unwrap_or_default()
                };
                runs.set_prompt(
                    run_id,
                    crate::run::Prompt::new(format!("Run workflow: {}", wf_name), "workflow", Utc::now().to_rfc3339()),
                    Utc::now().to_rfc3339(),
                );
                run
            }
        }
    };
    crate::event_bus::emit_ui(app, "workflow-status-change", run.to_wire()).ok();

    let agents_snapshot = { state.chat.agents.lock().unwrap_or_else(|e| e.into_inner()).clone() };
    let connectors_snapshot =
        { state.connect.universal_connectors.lock().unwrap_or_else(|e| e.into_inner()).clone() };
    let memory_context = {
        let mem = state.chat.memory.lock().unwrap_or_else(|e| e.into_inner());
        mem.build_context_prompt()
    };
    let wf_name = {
        let store = state.automation.workflow_store.lock().unwrap_or_else(|e| e.into_inner());
        store.get(id).map(|w| w.name.clone()).unwrap_or_default()
    };

    // Seed forward-context from any already-completed steps (resume case).
    let mut raw_ctx: Vec<serde_json::Value> = run
        .outputs()
        .into_iter()
        .map(|o| serde_json::json!({ "text": o }))
        .collect();
    // An event trigger's payload is the first data the pipeline sees, so a
    // step-1 mapping/prompt can process it as the "previous output".
    if let Some(payload) = &trigger_input {
        if raw_ctx.is_empty() {
            raw_ctx.push(payload.clone());
        }
    }
    // Labeled outputs for {{prev}} / {{step:<label>}} templates. Seeded from
    // the resume outputs positionally so resumed runs resolve too.
    let mut step_outputs: Vec<(String, String)> = run
        .outputs()
        .into_iter()
        .enumerate()
        .map(|(i, o)| (steps.get(i).map(|s| s.label.clone()).unwrap_or_default(), o))
        .collect();

    crate::run_log::set_active(run_id);
    crate::run_log::run_started(run_id, "workflow", &wf_name);
    for (_i, step) in steps.iter().enumerate().skip(start_step) {
        crate::run_log::step(&format!("Step {} of {}: {}", _i + 1, steps.len(), step.label));
        // Honour a Stop issued mid-run.
        {
            let runs = state.automation.run_store.lock().unwrap_or_else(|e| e.into_inner());
            if matches!(runs.get(run_id).map(|r| r.status), Some(workflow::WorkflowRunStatus::Stopped)) {
                crate::run_log::run_ended(run_id, "stopped", "Stopped by user");
                crate::run_log::clear_active(run_id);
                let stopped = runs.get(run_id).map(|r| r.to_wire());
                return Ok(stopped.unwrap_or_else(|| WorkflowRunState {
                    workflow_id: id.to_string(),
                    status: workflow::WorkflowRunStatus::Stopped,
                    current_step: _i,
                    total_steps: steps.len(),
                    output: Vec::new(),
                    error: None,
                }));
            }
        }

        let agent_name = infer_agent_for_step(&step, &agents_snapshot);
        let cmd_detail = step.config.get("command")
            .and_then(|v| v.as_str()).map(|s| s.to_string())
            .or_else(|| step.config.get("prompt").and_then(|v| v.as_str()).map(|s| s.to_string()));
        let stype = match step.step_type {
            workflow::StepType::Action => "shell",
            workflow::StepType::Prompt => "prompt",
            workflow::StepType::Condition => "condition",
            workflow::StepType::Wait => "wait",
            workflow::StepType::Loop => "loop",
            workflow::StepType::Relay => "relay",
        };

        let start_activity = AgentActivity {
            id: Uuid::new_v4().to_string(),
            agent_name: agent_name.clone(),
            action: step.label.clone(),
            status: "start".into(),
            workflow_name: wf_name.clone(),
            timestamp: Utc::now().to_rfc3339(),
            detail: cmd_detail.clone(),
            result: None,
            step_type: Some(stype.into()),
            rationale: None, model_used: None, tokens_in: None, tokens_out: None,
            cost_usd: None, inference_location: None, agent_depth: None,
        };
        crate::event_bus::emit_ui(app, "agent-activity", start_activity.clone()).ok();
        crate::models::agent::push_capped(
            &mut state.chat.activity_log.lock().unwrap_or_else(|e| e.into_inner()),
            start_activity,
        );

        // Rate-limit cooldown ONLY after rate-limited external tools
        // (web search/fetch). Local inference has no rate limit; the old
        // blanket 15s pause made a 3-step local workflow spend 30s asleep.
        if _i > 0 {
            let prev_tool = steps.get(_i - 1).and_then(|s| s.config.get("tool").and_then(|v| v.as_str()));
            if prev_tool == Some("web_search") || prev_tool == Some("web_fetch") {
                let wait_secs: u64 = 65;
                wf_log(&format!("Step {}: waiting {}s for web rate limit...", _i, wait_secs));
                // Visible in the run monitor; a silent minute reads as a hang.
                crate::run_log::step(&format!("Waiting {}s before the next web request (the search service rate-limits)…", wait_secs));
                tokio::time::sleep(tokio::time::Duration::from_secs(wait_secs)).await;
            }
        }
        wf_log(&format!("Step {}: '{}' type={:?}", _i, step.label, step.step_type));

        // Resolve {{prev}} / {{step:<label>}} / {{trigger.<field>}}
        // templates so steps operate on data instead of blind re-prompts.
        let resolved_step = resolve_step_templates(step, &step_outputs, trigger_input.as_ref());

        // W2.5: retry the step with backoff before failing the Run.
        let mut outcome = None;
        for attempt in 1..=STEP_MAX_ATTEMPTS {
            {
                let mut runs = state.automation.run_store.lock().unwrap_or_else(|e| e.into_inner());
                runs.begin_step(run_id, _i, Utc::now().to_rfc3339());
            }
            if attempt > 1 {
                let backoff = step_backoff_secs(attempt);
                wf_log(&format!("Step {}: retry {}/{} after {}s", _i, attempt, STEP_MAX_ATTEMPTS, backoff));
                tokio::time::sleep(tokio::time::Duration::from_secs(backoff)).await;
            }
            let o = run_step_checked(&resolved_step, &raw_ctx, &agents_snapshot, &connectors_snapshot, &memory_context, state).await;
            if o.success {
                outcome = Some(o);
                break;
            }
            outcome = Some(o);
        }
        let outcome = outcome.expect("STEP_MAX_ATTEMPTS >= 1");
        let step_ok = outcome.success;
        let display = outcome.display;
        let raw = outcome.raw;
        wf_log(&format!("Step {} done: ok={} display_len={}", _i, step_ok, display.len()));
        step_outputs.push((step.label.clone(), display.clone()));

        match raw.clone() {
            Some(r) => raw_ctx.push(r),
            None if !display.trim().is_empty() => raw_ctx.push(serde_json::json!({ "text": display.clone() })),
            None => {}
        }

        // Auto-save company context from qualifying Prompt steps.
        if step.step_type == workflow::StepType::Prompt && display.len() > 50 {
            let label_lower = step.label.to_lowercase();
            let prompt_text = step.config.get("prompt").and_then(|v| v.as_str()).unwrap_or("").to_lowercase();
            let is_context_step = label_lower.contains("competenc") || label_lower.contains("analys") || label_lower.contains("context")
                || label_lower.contains("profile") || label_lower.contains("score") || label_lower.contains("qualify")
                || prompt_text.contains("competenc") || prompt_text.contains("ideal customer")
                || prompt_text.contains("target") || prompt_text.contains("product") || prompt_text.contains("company");
            if is_context_step {
                let mem_key = format!("workflow:{}", step.label.to_lowercase().replace(' ', "_").chars().take(60).collect::<String>());
                let truncated = if display.len() > 2000 { &display[..2000] } else { &display };
                let mut mem = state.chat.memory.lock().unwrap_or_else(|e| e.into_inner());
                mem.add(mem_key, truncated.to_string(), memory::MemoryCategory::Context, format!("workflow:{}", wf_name));
                mem.save().ok();
            }
        }

        let complete_activity = AgentActivity {
            id: Uuid::new_v4().to_string(),
            agent_name: agent_name.clone(),
            action: step.label.clone(),
            status: "complete".into(),
            workflow_name: wf_name.clone(),
            timestamp: Utc::now().to_rfc3339(),
            detail: cmd_detail,
            result: Some(display.clone()),
            step_type: Some(stype.into()),
            rationale: None, model_used: None, tokens_in: None, tokens_out: None,
            cost_usd: None, inference_location: None, agent_depth: None,
        };
        crate::event_bus::emit_ui(app, "agent-activity", complete_activity.clone()).ok();
        crate::models::agent::push_capped(
            &mut state.chat.activity_log.lock().unwrap_or_else(|e| e.into_inner()),
            complete_activity,
        );

        crate::run_log::emit(
            run_id,
            "step",
            if step_ok { "ok" } else { "error" },
            format!("{} {}", step.label, if step_ok { "done" } else { "failed" }),
            Some(display.clone()),
            None,
            None,
        );
        if !step_ok {
            let err_msg = format!("Step '{}' failed after {} attempts: {}", step.label, STEP_MAX_ATTEMPTS, display.chars().take(300).collect::<String>());
            crate::run_log::run_ended(run_id, "failed", &err_msg);
            crate::run_log::clear_active(run_id);
            crate::peer::bob_says(
                app,
                &format!(
                    "**{} stopped at step {} of {}** ({}). {}",
                    wf_name,
                    _i + 1,
                    steps.len(),
                    step.label,
                    plain_step_error(&display)
                ),
            );
            let failed = {
                let mut runs = state.automation.run_store.lock().unwrap_or_else(|e| e.into_inner());
                runs.fail_step(run_id, _i, err_msg.clone(), Utc::now().to_rfc3339())
            };
            if let Some(r) = failed.clone() {
                crate::event_bus::emit_ui(app, "workflow-status-change", r.to_wire()).ok();
            }
            return Ok(failed.map(|r| r.to_wire()).unwrap_or_else(|| WorkflowRunState {
                workflow_id: id.to_string(),
                status: workflow::WorkflowRunStatus::Failed,
                current_step: _i,
                total_steps: steps.len(),
                output: vec![display.clone()],
                error: Some(err_msg),
            }));
        }

        let advanced = {
            let mut runs = state.automation.run_store.lock().unwrap_or_else(|e| e.into_inner());
            runs.complete_step(run_id, _i, display, Utc::now().to_rfc3339())
        };
        if let Some(r) = advanced {
            crate::event_bus::emit_ui(app, "workflow-status-change", r.to_wire()).ok();
        }
    }

    crate::run_log::run_ended(run_id, "done", &format!("{} steps completed", steps.len()));
    crate::run_log::clear_active(run_id);
    let final_run = {
        let runs = state.automation.run_store.lock().unwrap_or_else(|e| e.into_inner());
        runs.get(run_id)
    };
    // Where the result goes. The same for every way a run started: Run
    // button, chat, schedule, trigger, CLI.
    {
        let final_output = final_run
            .as_ref()
            .and_then(|r| r.steps.iter().rev().find_map(|s| s.output.clone().filter(|o| !o.trim().is_empty())))
            .unwrap_or_default();
        let wf = {
            let store = state.automation.workflow_store.lock().unwrap_or_else(|e| e.into_inner());
            store.workflows.iter().find(|w| w.id == id).cloned()
        };
        if let Some(wf) = wf {
            deliver_result(app, state, &wf, &final_output).await;
        }
    }
    Ok(final_run.map(|r| r.to_wire()).unwrap_or_else(|| WorkflowRunState {
        workflow_id: id.to_string(),
        status: workflow::WorkflowRunStatus::Completed,
        current_step: steps.len(),
        total_steps: steps.len(),
        output: Vec::new(),
        error: None,
    }))
}

#[tauri::command]
pub(crate) async fn stop_workflow(
    workflow_id: Option<String>,
    state: State<'_, AppState>,
    app: tauri::AppHandle,
) -> Result<(), String> {
    // Stop THIS workflow's running Run when the caller names one; else the
    // most recent running Run (the old behaviour, which could hit the
    // wrong workflow when two ran at once). The run loop reads the Run's
    // status each step and returns when it sees Stopped.
    let mut runs = state.automation.run_store.lock().unwrap_or_else(|e| e.into_inner());
    let target = runs
        .list(Some(workflow::WorkflowRunStatus::Running), 50)
        .into_iter()
        .find(|r| match &workflow_id {
            Some(id) if !id.is_empty() => r.to_wire().workflow_id == *id,
            _ => true,
        });
    if let Some(running) = target {
        if let Some(r) = runs.stop(&running.id, Utc::now().to_rfc3339()) {
            crate::event_bus::emit_ui(&app, "workflow-status-change", r.to_wire()).ok();
        }
    }
    Ok(())
}

#[tauri::command]
pub(crate) async fn get_workflow_status(state: State<'_, AppState>) -> Result<Option<WorkflowRunState>, String> {
    let runs = state.automation.run_store.lock().unwrap_or_else(|e| e.into_inner());
    Ok(runs.list(None, 1).into_iter().next().map(|r| r.to_wire()))
}

/// Resume Runs that were mid-flight when the process last stopped (W2).
/// Called once from setup on a background thread. Each resumable Run
/// continues from its persisted `current_step`; a workflow that no longer
/// exists is marked stopped.
pub async fn resume_interrupted_runs(state: &AppState, app: &tauri::AppHandle) {
    let pending = {
        let runs = state.automation.run_store.lock().unwrap_or_else(|e| e.into_inner());
        runs.resumable()
    };
    if pending.is_empty() {
        return;
    }
    wf_log(&format!("Resuming {} interrupted run(s)", pending.len()));
    for run in pending {
        let workflow_id = match &run.kind {
            crate::run::RunKind::Workflow { workflow_id } => workflow_id.clone(),
            // Only workflow runs are resumable today; mark others stopped so
            // they do not linger as forever-Running.
            _ => {
                let mut runs = state.automation.run_store.lock().unwrap_or_else(|e| e.into_inner());
                runs.stop(&run.id, Utc::now().to_rfc3339());
                continue;
            }
        };
        let exists = {
            let store = state.automation.workflow_store.lock().unwrap_or_else(|e| e.into_inner());
            store.get(&workflow_id).is_some()
        };
        if !exists {
            let mut runs = state.automation.run_store.lock().unwrap_or_else(|e| e.into_inner());
            runs.stop(&run.id, Utc::now().to_rfc3339());
            continue;
        }
        let _ = run_one_workflow(&run.id, &workflow_id, state, app, run.current_step, None).await;
    }
}


// ─────────────────────────────────────────────
//  Commands - Workflow Templates (Epic 3)
// ─────────────────────────────────────────────

#[tauri::command]
pub(crate) async fn list_workflow_templates(state: State<'_, AppState>) -> Result<Vec<workflow_templates::WorkflowTemplate>, String> {
    let store = state.automation.template_store.lock().unwrap_or_else(|e| e.into_inner());
    Ok(store.list())
}

#[tauri::command]
pub(crate) async fn get_workflow_template(id: String, state: State<'_, AppState>) -> Result<workflow_templates::WorkflowTemplate, String> {
    let store = state.automation.template_store.lock().unwrap_or_else(|e| e.into_inner());
    store.get(&id).ok_or_else(|| format!("Template {} not found", id))
}

#[tauri::command]
pub(crate) async fn list_template_categories(state: State<'_, AppState>) -> Result<Vec<workflow_templates::TemplateCategory>, String> {
    let store = state.automation.template_store.lock().unwrap_or_else(|e| e.into_inner());
    Ok(store.categories())
}

#[tauri::command]
pub(crate) async fn instantiate_template(
    template_id: String,
    state: State<'_, AppState>,
) -> Result<Workflow, String> {
    let (name, desc, steps, suggested) = {
        let store = state.automation.template_store.lock().unwrap_or_else(|e| e.into_inner());
        let (n, d, st) = store.instantiate(&template_id)?;
        let sched = store.get(&template_id).and_then(|t| t.schedule.clone());
        (n, d, st, sched)
    };
    let now = Utc::now().to_rfc3339();
    let workflow = Workflow {
        id: Uuid::new_v4().to_string(),
        name,
        description: desc,
        steps,
        trigger: workflow::WorkflowTrigger::Manual,
        deliver: Default::default(),
        created_at: now.clone(),
        updated_at: now,
    };
    {
        let mut wf_store = state.automation.workflow_store.lock().unwrap_or_else(|e| e.into_inner());
        wf_store.workflows.push(workflow.clone());
        wf_store.save();
    }
    // A template can arrive already scheduled ("every weekday at 07:30").
    if let Some(spec) = suggested.as_deref().and_then(ScheduleSpec::from_cron) {
        let _ = apply_schedule(&state, &workflow.id, Some(spec));
    }
    let wf_store = state.automation.workflow_store.lock().unwrap_or_else(|e| e.into_inner());
    Ok(wf_store.workflows.iter().find(|w| w.id == workflow.id).cloned().unwrap_or(workflow))
}


#[tauri::command]
pub(crate) async fn save_workflow_as_template(
    workflow_id: String,
    input: SaveAsTemplateInput,
    state: State<'_, AppState>,
) -> Result<workflow_templates::WorkflowTemplate, String> {
    let steps = {
        let wf_store = state.automation.workflow_store.lock().unwrap_or_else(|e| e.into_inner());
        let wf = wf_store.get(&workflow_id).ok_or("Workflow not found")?;
        wf.steps.iter().map(|s| {
            let step_type = format!("{:?}", s.step_type).to_lowercase();
            workflow_templates::TemplateStep {
                step_type,
                label: s.label.clone(),
                config: s.config.clone(),
                connector_id: s.config.get("connector_id").and_then(|v| v.as_str().map(String::from)),
                endpoint_key: s.config.get("endpoint_key").and_then(|v| v.as_str().map(String::from)),
            }
        }).collect::<Vec<_>>()
    };
    let mut tpl_store = state.automation.template_store.lock().unwrap_or_else(|e| e.into_inner());
    let template = tpl_store.save_as_template(input.name, input.description, input.category, steps);
    Ok(template)
}

#[tauri::command]
pub(crate) async fn delete_workflow_template(id: String, state: State<'_, AppState>) -> Result<(), String> {
    let mut store = state.automation.template_store.lock().unwrap_or_else(|e| e.into_inner());
    if store.delete(&id) { Ok(()) } else { Err("Template not found or is built-in".into()) }
}

#[tauri::command]
pub(crate) async fn update_workflow_step_connector(
    workflow_id: String,
    step_id: String,
    connector_id: Option<String>,
    endpoint_key: Option<String>,
    state: State<'_, AppState>,
) -> Result<Workflow, String> {
    let mut store = state.automation.workflow_store.lock().unwrap_or_else(|e| e.into_inner());
    let wf = store.workflows.iter_mut().find(|w| w.id == workflow_id)
        .ok_or("Workflow not found")?;
    let step = wf.steps.iter_mut().find(|s| s.id == step_id)
        .ok_or("Step not found")?;

    // Update connector binding in step config
    if let Some(cid) = connector_id {
        step.config.insert("connector_id".into(), serde_json::Value::String(cid));
    } else {
        step.config.remove("connector_id");
    }
    if let Some(ek) = endpoint_key {
        step.config.insert("endpoint_key".into(), serde_json::Value::String(ek));
    } else {
        step.config.remove("endpoint_key");
    }

    wf.updated_at = Utc::now().to_rfc3339();
    let result = wf.clone();
    store.save();
    Ok(result)
}


/// Compact run history for one workflow, newest first. Reuses the durable
/// Run registry; returns summaries only (status, timing, error), never step
/// payloads, so the sidebar stays lean.
#[tauri::command]
pub(crate) async fn list_workflow_runs(
    workflow_id: String,
    limit: Option<usize>,
    state: State<'_, AppState>,
) -> Result<Vec<serde_json::Value>, String> {
    let limit = limit.unwrap_or(10).min(50);
    let runs = state.automation.run_store.lock().unwrap_or_else(|e| e.into_inner());
    let mut out = Vec::new();
    for r in runs.list(None, 500) {
        if let crate::run::RunKind::Workflow { workflow_id: wid } = &r.kind {
            if *wid == workflow_id {
                out.push(serde_json::json!({
                    "id": r.id,
                    "status": r.status,
                    "current_step": r.current_step,
                    "total_steps": r.steps.len(),
                    "error": r.error,
                    "created_at": r.created_at,
                    "updated_at": r.updated_at,
                }));
                if out.len() >= limit { break; }
            }
        }
    }
    Ok(out)
}


/// Resolve {{prev}} and {{step:<label>}} templates in a step's string
/// config values (including nested tool_args) against earlier step
/// outputs. Labels match case-insensitively; unknown templates are left
/// as-is so nothing silently disappears.
/// Look up a dotted path in the trigger payload, e.g. "sender.address".
/// Returns the value as a plain string (scalars as-is, objects/arrays as
/// compact JSON) so it can be substituted into a template.
fn trigger_field(trigger: &serde_json::Value, path: &str) -> Option<String> {
    let mut cur = trigger;
    for key in path.split('.') {
        cur = cur.get(key)?;
    }
    Some(match cur {
        serde_json::Value::String(s) => s.clone(),
        serde_json::Value::Null => String::new(),
        other => other.to_string(),
    })
}

fn resolve_step_templates(
    step: &workflow::WorkflowStep,
    outputs: &[(String, String)],
    trigger: Option<&serde_json::Value>,
) -> workflow::WorkflowStep {
    if outputs.is_empty() && trigger.is_none() {
        return step.clone();
    }
    fn subst(s: &str, outputs: &[(String, String)], trigger: Option<&serde_json::Value>) -> String {
        let mut out = s.to_string();
        // Trigger payload: {{trigger}} = whole payload as JSON;
        // {{trigger.<dotted.path>}} = one field. Done first so a field can
        // then feed {{prev}}-style logic if ever chained.
        if let Some(t) = trigger {
            if out.contains("{{trigger}}") {
                out = out.replace("{{trigger}}", &t.to_string());
            }
            let mut cursor = 0;
            while let Some(rel) = out[cursor..].find("{{trigger.") {
                let start = cursor + rel;
                let Some(end_rel) = out[start..].find("}}") else { break };
                let end = start + end_rel;
                let path = out[start + 10..end].trim().to_string();
                let val = trigger_field(t, &path).unwrap_or_default();
                out = format!("{}{}{}", &out[..start], val, &out[end + 2..]);
                cursor = start + val.len();
            }
        }
        if out.contains("{{prev}}") {
            let prev = outputs.last().map(|(_, o)| o.as_str()).unwrap_or("");
            out = out.replace("{{prev}}", prev);
        }
        let mut cursor = 0;
        while let Some(rel) = out[cursor..].find("{{step:") {
            let start = cursor + rel;
            let Some(end_rel) = out[start..].find("}}") else { break };
            let end = start + end_rel;
            let label = out[start + 7..end].trim().to_string();
            match outputs.iter().rev().find(|(l, _)| l.eq_ignore_ascii_case(&label)) {
                Some((_, v)) => {
                    out = format!("{}{}{}", &out[..start], v, &out[end + 2..]);
                    cursor = start + v.len();
                }
                None => {
                    cursor = end + 2; // unknown label: skip past it
                }
            }
        }
        out
    }
    fn walk(v: &mut serde_json::Value, outputs: &[(String, String)], trigger: Option<&serde_json::Value>) {
        match v {
            serde_json::Value::String(s) => *s = subst(s, outputs, trigger),
            serde_json::Value::Object(m) => m.values_mut().for_each(|vv| walk(vv, outputs, trigger)),
            serde_json::Value::Array(a) => a.iter_mut().for_each(|vv| walk(vv, outputs, trigger)),
            _ => {}
        }
    }
    let mut resolved = step.clone();
    for v in resolved.config.values_mut() {
        walk(v, outputs, trigger);
    }
    resolved
}

/// Run a single step in isolation ("Test this step"). Same executor as a
/// real run but no Run record, no retries, empty context; {{templates}}
/// are left literal since there are no prior outputs to resolve from.
#[tauri::command]
pub(crate) async fn test_workflow_step(
    workflow_id: String,
    step_id: String,
    state: State<'_, AppState>,
) -> Result<serde_json::Value, String> {
    let step = {
        let store = state.automation.workflow_store.lock().unwrap_or_else(|e| e.into_inner());
        let wf = store.get(&workflow_id).ok_or_else(|| format!("Workflow {} not found", workflow_id))?;
        wf.steps.iter().find(|s| s.id == step_id).cloned()
            .ok_or_else(|| format!("Step {} not found", step_id))?
    };
    let agents_snapshot = { state.chat.agents.lock().unwrap_or_else(|e| e.into_inner()).clone() };
    let connectors_snapshot =
        { state.connect.universal_connectors.lock().unwrap_or_else(|e| e.into_inner()).clone() };
    let o = run_step_checked(&step, &[], &agents_snapshot, &connectors_snapshot, "", &state).await;
    Ok(serde_json::json!({ "success": o.success, "output": o.display }))
}

#[cfg(test)]
mod template_tests {
    use super::*;

    fn step_with(config: std::collections::HashMap<String, serde_json::Value>) -> workflow::WorkflowStep {
        workflow::WorkflowStep {
            id: "s".into(),
            order: 0,
            step_type: workflow::StepType::Prompt,
            label: "X".into(),
            config,
        }
    }

    #[test]
    fn resolves_prev_and_labels_case_insensitive() {
        let mut config = std::collections::HashMap::new();
        config.insert("prompt".into(), serde_json::json!("Summarize: {{prev}} and {{step:Fetch data}}"));
        config.insert("tool_args".into(), serde_json::json!({ "query": "{{step:fetch DATA}}" }));
        let outputs = vec![
            ("Fetch data".to_string(), "42 rows".to_string()),
            ("Analyse".to_string(), "looks good".to_string()),
        ];
        let r = resolve_step_templates(&step_with(config), &outputs, None);
        assert_eq!(r.config["prompt"].as_str().unwrap(), "Summarize: looks good and 42 rows");
        assert_eq!(r.config["tool_args"]["query"].as_str().unwrap(), "42 rows");
    }

    #[test]
    fn unknown_label_left_intact() {
        let mut config = std::collections::HashMap::new();
        config.insert("command".into(), serde_json::json!("echo {{step:Nope}} then {{step:A}}"));
        let outputs = vec![("A".to_string(), "out".to_string())];
        let r = resolve_step_templates(&step_with(config), &outputs, None);
        assert_eq!(r.config["command"].as_str().unwrap(), "echo {{step:Nope}} then out");
    }

    #[test]
    fn no_outputs_is_identity() {
        let mut config = std::collections::HashMap::new();
        config.insert("prompt".into(), serde_json::json!("{{prev}} stays literal in isolation"));
        let r = resolve_step_templates(&step_with(config), &[], None);
        assert_eq!(r.config["prompt"].as_str().unwrap(), "{{prev}} stays literal in isolation");
    }

    #[test]
    fn trigger_fields_resolve_including_dotted_paths() {
        let mut config = std::collections::HashMap::new();
        config.insert("prompt".into(), serde_json::json!("From {{trigger.sender.address}} re: {{trigger.subject}}"));
        config.insert("tool_args".into(), serde_json::json!({ "whole": "{{trigger}}" }));
        let trigger = serde_json::json!({
            "subject": "Invoice #42",
            "sender": { "address": "billing@acme.com", "name": "Acme" }
        });
        let r = resolve_step_templates(&step_with(config), &[], Some(&trigger));
        assert_eq!(r.config["prompt"].as_str().unwrap(), "From billing@acme.com re: Invoice #42");
        // {{trigger}} expands to the whole payload as JSON.
        assert!(r.config["tool_args"]["whole"].as_str().unwrap().contains("billing@acme.com"));
    }

    #[test]
    fn unknown_trigger_field_becomes_empty() {
        let mut config = std::collections::HashMap::new();
        config.insert("prompt".into(), serde_json::json!("x={{trigger.missing}}y"));
        let trigger = serde_json::json!({ "subject": "hi" });
        let r = resolve_step_templates(&step_with(config), &[], Some(&trigger));
        assert_eq!(r.config["prompt"].as_str().unwrap(), "x=y");
    }
}

// ─────────────────────────────────────────────
//  Authoring from plain language (the Build tab's "What should happen?")
//
//  Bob drafts the steps from one sentence, or revises an existing
//  workflow from an instruction, through a plain model call: no chat
//  turn, no persona, JSON only, the same runnability check
//  create_workflow applies. The vocabulary (paired apps, bridges,
//  connected services, MCP servers) comes from the same digests Bob's
//  prompt carries, so the draft can only use what this computer has.
// ─────────────────────────────────────────────

async fn author_vocabulary(state: &AppState) -> String {
    let mut v = String::new();
    v.push_str("STEP TYPES: \"prompt\" (Bob writes/thinks; needs `prompt`) and \"action\" (needs `tool` + `tool_args`, or `command` for a shell line). Also \"wait\" (`seconds`).\n");
    v.push_str(&format!("ACTION TOOLS: {}.\n", crate::tool_schemas::step_tool_names().join(", ")));
    v.push_str("TOOL ARGS: web_search {query}; web_fetch {url}; file_read/file_write/file_list {path[, content]}; shell_exec {command}; app_bridge {app, action, args}; peer_send {app, text}; peer_read {app}; peer_open {app, action, args}; peer_press {app, control}; peer_fill {app, field, value}; mcp_call {server, tool, args}; api_call {connector, method, url, body}.\n");
    v.push_str("DATA FLOW: {{prev}} inserts the previous step's result; {{step:Label}} an earlier step's. Prompt steps automatically see recent results, so a prompt like \"Write a short brief\" works without templates.\n");
    v.push_str(&crate::app_bridge::capability_digest().await);
    v.push_str(&crate::app_bridge::action_vocabulary());
    v.push_str(&crate::peer::prompt_digest());
    let services = crate::services::prompt_lines(state);
    if !services.is_empty() {
        v.push_str(&format!("\nCONNECTED SERVICES:\n{}\n", services.join("\n")));
    }
    v.push_str(&crate::mcp_client::prompt_digest(state).await);
    v
}

const AUTHOR_SYSTEM: &str = "You turn a plain-language request into an Orbix workflow: a short ordered list of runnable steps. Answer with one JSON object only: {\"name\": \"...\", \"description\": \"one plain sentence\", \"steps\": [ {\"type\": \"prompt\"|\"action\"|\"wait\", \"label\": \"plain sentence naming the app, e.g. Read unread email in Mail\", ...} ]}. Rules: every step must be runnable with the tools listed; never invent tools, apps, servers or connectors that are not listed; never add a step that asks the user anything; prefer app_bridge for Mail/Calendar/Notes/Reminders and peer_send/peer_read for paired apps; 2 to 7 steps; labels are what a person would say, not tool names; the last step should produce the thing the user asked for (a prompt step that writes it is fine: the result shows in chat).";

fn parse_authored(raw: &str) -> Result<(String, String, Vec<workflow::WorkflowStep>), String> {
    let json_text = crate::llm_engine::extract_json_object(raw).ok_or("Bob did not return a workflow")?;
    let v: serde_json::Value = serde_json::from_str(&json_text)
        .or_else(|_| crate::llm_engine::repair_json(&json_text).and_then(|f| serde_json::from_str(&f).ok()).ok_or(()))
        .map_err(|_| "Bob's draft was not readable".to_string())?;
    let name = v["name"].as_str().unwrap_or("").trim().to_string();
    let description = v["description"].as_str().unwrap_or("").trim().to_string();
    let steps = crate::llm_engine::workflow_steps_from_json(&v["steps"]);
    if steps.is_empty() {
        return Err("Bob's draft had no steps".to_string());
    }
    let gaps: Vec<String> = steps
        .iter()
        .filter_map(|st| crate::llm_engine::workflow_step_gap(&st.step_type, &st.config).map(|g| format!("\"{}\" {}", st.label, g)))
        .collect();
    if !gaps.is_empty() {
        return Err(format!("some steps could not run yet: {}", gaps.join("; ")));
    }
    Ok((name, description, steps))
}

async fn author_with_retry(system: &str, user: &str) -> Result<(String, String, Vec<workflow::WorkflowStep>), String> {
    let mut last = String::new();
    for attempt in 0..2 {
        let prompt = if attempt == 0 {
            user.to_string()
        } else {
            format!("{}\n\nYOUR PREVIOUS ANSWER WAS REJECTED: {}. Fix it and answer with the JSON only.", user, last)
        };
        let raw = crate::llm_engine::call_model_plain(system, &prompt).await?;
        match parse_authored(&raw) {
            Ok(ok) => return Ok(ok),
            Err(e) => last = e,
        }
    }
    Err(last)
}

/// Draft, validate and save a workflow from one sentence.
#[tauri::command]
pub(crate) async fn draft_workflow(description: String, state: State<'_, AppState>) -> Result<Workflow, String> {
    let description = description.trim().to_string();
    if description.is_empty() {
        return Err("say what should happen".to_string());
    }
    let vocab = author_vocabulary(&state).await;
    let user = format!("REQUEST: {}\n\nWHAT THIS COMPUTER CAN DO:\n{}\n\nAnswer with the JSON object only.", description, vocab);
    let (name, model_desc, steps) = author_with_retry(AUTHOR_SYSTEM, &user).await?;
    let name = if name.is_empty() { crate::truncate_str(&description, 48) } else { name };
    let desc = if model_desc.is_empty() { description.clone() } else { model_desc };
    let wf = {
        let mut store = state.automation.workflow_store.lock().unwrap_or_else(|e| e.into_inner());
        let mut wf = store.create(name, desc);
        wf.steps = steps;
        wf.updated_at = Utc::now().to_rfc3339();
        if let Some(slot) = store.workflows.iter_mut().find(|w| w.id == wf.id) {
            *slot = wf.clone();
        }
        store.save();
        wf
    };
    Ok(wf)
}

/// Revise an existing workflow from an instruction ("make step 2 post to
/// #sales instead", "add a step that saves it to Notes"). The whole step
/// list is redrafted and validated; nothing changes unless it passes.
#[tauri::command]
pub(crate) async fn revise_workflow(id: String, instruction: String, state: State<'_, AppState>) -> Result<Workflow, String> {
    let instruction = instruction.trim().to_string();
    if instruction.is_empty() {
        return Err("say what to change".to_string());
    }
    let current = {
        let store = state.automation.workflow_store.lock().unwrap_or_else(|e| e.into_inner());
        store.workflows.iter().find(|w| w.id == id).cloned().ok_or_else(|| format!("Workflow {} not found", id))?
    };
    let current_json = serde_json::json!({
        "name": current.name,
        "description": current.description,
        "steps": current.steps.iter().map(|st| {
            let mut o = serde_json::Map::new();
            o.insert("type".into(), serde_json::json!(match st.step_type { workflow::StepType::Prompt => "prompt", workflow::StepType::Wait => "wait", _ => "action" }));
            o.insert("label".into(), serde_json::json!(st.label));
            for (k, v) in &st.config { o.insert(k.clone(), v.clone()); }
            serde_json::Value::Object(o)
        }).collect::<Vec<_>>(),
    });
    let vocab = author_vocabulary(&state).await;
    let user = format!(
        "CURRENT WORKFLOW:\n{}\n\nCHANGE REQUESTED: {}\n\nWHAT THIS COMPUTER CAN DO:\n{}\n\nReturn the COMPLETE updated workflow as the JSON object only (keep steps that were not mentioned exactly as they are).",
        serde_json::to_string_pretty(&current_json).unwrap_or_default(),
        instruction,
        vocab
    );
    let (name, desc, steps) = author_with_retry(AUTHOR_SYSTEM, &user).await?;
    let wf = {
        let mut store = state.automation.workflow_store.lock().unwrap_or_else(|e| e.into_inner());
        let wf = store.workflows.iter_mut().find(|w| w.id == id).ok_or_else(|| format!("Workflow {} not found", id))?;
        if !name.is_empty() { wf.name = name; }
        if !desc.is_empty() { wf.description = desc; }
        wf.steps = steps;
        wf.updated_at = Utc::now().to_rfc3339();
        let out = wf.clone();
        store.save();
        out
    };
    Ok(wf)
}

#[cfg(test)]
mod authoring_tests {
    use super::*;

    #[test]
    fn authored_json_parses_and_is_checked() {
        let raw = r#"Sure! ```json
{"name":"Morning brief","description":"Reads calendar and mail, writes a brief.","steps":[
 {"type":"action","label":"Read today's events in Calendar","tool":"app_bridge","tool_args":{"app":"Calendar","action":"upcoming_events","args":{"days":1}}},
 {"type":"action","label":"Read unread email in Mail","tool":"app_bridge","tool_args":{"app":"Mail","action":"recent_messages","args":{"limit":10}}},
 {"type":"prompt","label":"Write a short brief","prompt":"Write a five-line brief of the day from the events and email."}
]}
```"#;
        let (name, desc, steps) = parse_authored(raw).unwrap();
        assert_eq!(name, "Morning brief");
        assert!(desc.starts_with("Reads"));
        assert_eq!(steps.len(), 3);
        assert_eq!(steps[2].step_type, workflow::StepType::Prompt);
        let bad = r#"{"name":"x","steps":[{"type":"action","label":"Teleport","tool":"teleport"}]}"#;
        assert!(parse_authored(bad).unwrap_err().contains("could not run"));
        let asks = r#"{"name":"x","steps":[{"type":"prompt","label":"Ask","prompt":"Ask the user for their goals"}]}"#;
        assert!(parse_authored(asks).unwrap_err().contains("asks the user"));
    }
}

#[cfg(test)]
mod authoring_live {
    //! cargo test --bin orbixcore live_draft_workflow -- --ignored --nocapture
    use super::*;

    #[tokio::test]
    #[ignore]
    async fn live_draft_workflow() {
        let vocab = format!(
            "STEP TYPES: prompt/action/wait.\nACTION TOOLS: {}.\nTOOL ARGS: app_bridge {{app, action, args}}; peer_send {{app, text}}; web_search {{query}}.\n{}{}",
            crate::tool_schemas::step_tool_names().join(", "),
            crate::app_bridge::capability_digest().await,
            crate::app_bridge::action_vocabulary()
        );
        let user = format!("REQUEST: Every weekday morning, read my calendar and unread mail and write me a short brief.\n\nWHAT THIS COMPUTER CAN DO:\n{}\n\nAnswer with the JSON object only.", vocab);
        let (name, desc, steps) = author_with_retry(AUTHOR_SYSTEM, &user).await.expect("draft");
        eprintln!("name={} desc={}", name, desc);
        for st in &steps { eprintln!("- [{:?}] {} :: {}", st.step_type, st.label, serde_json::to_string(&st.config).unwrap()); }
        assert!(steps.len() >= 2);
    }
}

// ─────────────────────────────────────────────
//  Where the result goes
// ─────────────────────────────────────────────

/// The engine's error markers, in words a person can act on.
fn plain_step_error(display: &str) -> String {
    let d = display
        .replace("*Connector error:*", "")
        .replace("*Agent error:*", "")
        .replace("*Failed:*", "")
        .replace("*Failed:", "")
        .replace("[ERR]", "");
    crate::truncate_str(d.trim(), 300)
}

async fn deliver_result(app: &tauri::AppHandle, state: &AppState, wf: &Workflow, output: &str) {
    let name = wf.name.clone();
    let body = if output.trim().is_empty() { "It finished, but the last step produced nothing to show.".to_string() } else { crate::truncate_str(output, 6000) };
    match &wf.deliver {
        workflow::Deliver::Chat => {
            crate::peer::bob_says(app, &format!("**{}**\n\n{}", name, body));
        }
        workflow::Deliver::Document => {
            let spec = crate::handoff::doc_spec_from_markdown(&name, output);
            let kit = crate::brand::active_kit(state);
            match crate::docgen::generate("docx", &spec, &kit) {
                Ok(path) => {
                    crate::event_bus::emit_ui(app, "data-changed", "documents").ok();
                    crate::peer::bob_says(app, &format!("**{}** ran. Saved as a document: {}", name, path.to_string_lossy()));
                }
                Err(e) => crate::peer::bob_says(app, &format!("**{}** ran, but saving the document failed ({}). Here it is:\n\n{}", name, e, body)),
            }
        }
        workflow::Deliver::Panel => {
            let block = serde_json::json!({ "kind": "text", "content": output });
            let panel: Result<crate::dynamic_panels::DynamicPanel, _> = serde_json::from_value(serde_json::json!({
                "id": "", "title": name, "description": format!("Result of {}", name), "blocks": [block], "created_at": "", "updated_at": "",
            }));
            match panel {
                Ok(p) => {
                    let saved = {
                        let mut panels = state.automation.dynamic_panels_store.lock().unwrap_or_else(|e| e.into_inner());
                        let saved = panels.upsert(p, "workflow");
                        let _ = panels.save();
                        saved
                    };
                    crate::event_bus::emit_ui(app, "data-changed", "dynamic-panel-saved").ok();
                    crate::peer::bob_says(app, &format!("**{}** ran. The result is in My UI as \"{}\".", name, saved.title));
                }
                Err(e) => crate::peer::bob_says(app, &format!("**{}** ran, but the panel could not be made ({}). Here it is:\n\n{}", name, e, body)),
            }
        }
        workflow::Deliver::Notify => {
            let first = output.lines().find(|l| !l.trim().is_empty()).unwrap_or("Done.");
            let _ = crate::os_auto::send_notification(app, &name, &crate::truncate_str(first, 160)).await;
            crate::peer::bob_says(app, &format!("**{}**\n\n{}", name, body));
        }
        workflow::Deliver::App(target) => {
            match crate::peer::send(target, output).await {
                Ok(r) => crate::peer::bob_says(app, &format!("**{}** ran and I handed the result to {}. {}", name, r.app, r.detail)),
                Err(e) => crate::peer::bob_says(app, &format!("**{}** ran, but I could not hand it to {} ({}). Here it is:\n\n{}", name, target, e, body)),
            }
        }
    }
}

#[tauri::command]
pub(crate) async fn set_workflow_deliver(id: String, deliver: workflow::Deliver, state: State<'_, AppState>) -> Result<Workflow, String> {
    let mut store = state.automation.workflow_store.lock().unwrap_or_else(|e| e.into_inner());
    let wf = store.workflows.iter_mut().find(|w| w.id == id).ok_or_else(|| format!("Workflow {} not found", id))?;
    wf.deliver = deliver;
    wf.updated_at = Utc::now().to_rfc3339();
    let out = wf.clone();
    store.save();
    Ok(out)
}

// ─────────────────────────────────────────────
//  When it runs: one schedule per workflow, in plain terms
// ─────────────────────────────────────────────

/// A schedule as a person states it. Times are local.
#[derive(Clone, Debug, serde::Serialize, serde::Deserialize)]
pub struct ScheduleSpec {
    /// daily | weekdays | weekly | monthly | hourly | minutes
    pub kind: String,
    #[serde(default)]
    pub hour: u32,
    #[serde(default)]
    pub minute: u32,
    /// weekly: 0 = Sunday … 6 = Saturday
    #[serde(default)]
    pub weekday: u32,
    /// monthly: day of month 1-28
    #[serde(default)]
    pub day: u32,
    /// minutes: every N minutes
    #[serde(default)]
    pub every: u32,
}

impl ScheduleSpec {
    pub fn to_cron(&self) -> Result<String, String> {
        let h = self.hour.min(23);
        let m = self.minute.min(59);
        Ok(match self.kind.as_str() {
            "daily" => format!("{} {} * * *", m, h),
            "weekdays" => format!("{} {} * * 1-5", m, h),
            "weekly" => format!("{} {} * * {}", m, h, self.weekday.min(6)),
            "monthly" => format!("{} {} {} * *", m, h, self.day.clamp(1, 28)),
            "hourly" => "0 * * * *".to_string(),
            "minutes" => format!("*/{} * * * *", self.every.clamp(1, 59)),
            other => return Err(format!("unknown schedule kind \"{}\"", other)),
        })
    }

    /// Recover a spec from a cron the app wrote (best effort).
    pub fn from_cron(cron: &str) -> Option<ScheduleSpec> {
        let p: Vec<&str> = cron.split_whitespace().collect();
        if p.len() != 5 {
            return None;
        }
        if p[0].starts_with("*/") && p[1] == "*" {
            return Some(ScheduleSpec { kind: "minutes".into(), every: p[0][2..].parse().ok()?, ..Default::default() });
        }
        if p[0] == "0" && p[1] == "*" {
            return Some(ScheduleSpec { kind: "hourly".into(), ..Default::default() });
        }
        let minute: u32 = p[0].parse().ok()?;
        let hour: u32 = p[1].parse().ok()?;
        if p[2] != "*" {
            return Some(ScheduleSpec { kind: "monthly".into(), hour, minute, day: p[2].parse().ok()?, ..Default::default() });
        }
        match p[4] {
            "*" => Some(ScheduleSpec { kind: "daily".into(), hour, minute, ..Default::default() }),
            "1-5" => Some(ScheduleSpec { kind: "weekdays".into(), hour, minute, ..Default::default() }),
            d => Some(ScheduleSpec { kind: "weekly".into(), hour, minute, weekday: d.parse().ok()?, ..Default::default() }),
        }
    }
}

impl Default for ScheduleSpec {
    fn default() -> Self {
        ScheduleSpec { kind: "daily".into(), hour: 9, minute: 0, weekday: 1, day: 1, every: 15 }
    }
}

/// Everything about when a workflow runs, for the header line.
#[derive(Clone, Debug, serde::Serialize)]
pub struct WhenInfo {
    pub schedule: Option<ScheduleSpec>,
    pub cron: Option<String>,
    /// Next run in the user's local time, e.g. "Tue 07:30".
    pub next_run: Option<String>,
    /// Triggers bound to the workflow, in plain words.
    pub triggers: Vec<String>,
}

fn when_info(state: &AppState, id: &str) -> WhenInfo {
    let entry = {
        let sched = state.automation.scheduler_store.lock().unwrap_or_else(|e| e.into_inner());
        sched.list_schedules().into_iter().find(|s| s.workflow_id == id && s.enabled)
    };
    let (schedule, cron, next_run) = match entry {
        Some(e) => {
            let next = e
                .next_run
                .as_deref()
                .and_then(|t| chrono::DateTime::parse_from_rfc3339(t).ok())
                .map(|t| t.with_timezone(&chrono::Local).format("%a %H:%M").to_string());
            (ScheduleSpec::from_cron(&e.cron_expr), Some(e.cron_expr.clone()), next)
        }
        None => (None, None, None),
    };
    let triggers = {
        let trig = state.automation.trigger_store.lock().unwrap_or_else(|e| e.into_inner());
        let mut out = Vec::new();
        for r in trig.rules.iter().filter(|r| r.workflow_id == id && r.enabled) {
            out.push(format!("when an email matches \"{}\"", r.name));
        }
        for w in trig.webhooks.iter().filter(|w| w.workflow_id == id && w.enabled) {
            out.push(format!("when something posts to {}", w.path));
        }
        for f in trig.file_watches.iter().filter(|f| f.workflow_id == id) {
            out.push(format!("when a file appears in {}", f.path));
        }
        out
    };
    WhenInfo { schedule, cron, next_run, triggers }
}

#[tauri::command]
pub(crate) async fn get_workflow_when(id: String, state: State<'_, AppState>) -> Result<WhenInfo, String> {
    Ok(when_info(&state, &id))
}

/// Set (Some) or clear (None) the workflow's schedule. One schedule per
/// workflow; the workflow's own trigger field follows it.
#[tauri::command]
pub(crate) async fn set_workflow_schedule(id: String, spec: Option<ScheduleSpec>, state: State<'_, AppState>) -> Result<WhenInfo, String> {
    apply_schedule(&state, &id, spec)?;
    Ok(when_info(&state, &id))
}

fn apply_schedule(state: &AppState, id: &str, spec: Option<ScheduleSpec>) -> Result<(), String> {
    {
        let wf_store = state.automation.workflow_store.lock().unwrap_or_else(|e| e.into_inner());
        if !wf_store.workflows.iter().any(|w| w.id == id) {
            return Err(format!("Workflow {} not found", id));
        }
    }
    let cron = match &spec {
        Some(s) => Some(s.to_cron()?),
        None => None,
    };
    {
        let mut sched = state.automation.scheduler_store.lock().unwrap_or_else(|e| e.into_inner());
        let old: Vec<String> = sched.list_schedules().into_iter().filter(|s| s.workflow_id == id).map(|s| s.id).collect();
        for sid in old {
            sched.delete_schedule(&sid);
        }
        if let Some(c) = &cron {
            sched.create_schedule(id.to_string(), scheduler::SchedulePreset::Custom, Some(c.clone()), Some("local".into()))?;
        }
        sched.save();
    }
    {
        let mut wf_store = state.automation.workflow_store.lock().unwrap_or_else(|e| e.into_inner());
        if let Some(wf) = wf_store.workflows.iter_mut().find(|w| w.id == id) {
            wf.trigger = match &cron {
                Some(c) => workflow::WorkflowTrigger::Schedule(c.clone()),
                None => workflow::WorkflowTrigger::Manual,
            };
            wf.updated_at = Utc::now().to_rfc3339();
        }
        wf_store.save();
    }
    Ok(())
}

#[cfg(test)]
mod when_tests {
    use super::*;

    #[test]
    fn schedule_specs_round_trip_through_cron() {
        let s = ScheduleSpec { kind: "weekdays".into(), hour: 7, minute: 30, ..Default::default() };
        assert_eq!(s.to_cron().unwrap(), "30 7 * * 1-5");
        let back = ScheduleSpec::from_cron("30 7 * * 1-5").unwrap();
        assert_eq!((back.kind.as_str(), back.hour, back.minute), ("weekdays", 7, 30));
        assert_eq!(ScheduleSpec { kind: "weekly".into(), hour: 16, minute: 0, weekday: 5, ..Default::default() }.to_cron().unwrap(), "0 16 * * 5");
        assert_eq!(ScheduleSpec::from_cron("0 16 * * 5").unwrap().weekday, 5);
        assert_eq!(ScheduleSpec { kind: "monthly".into(), hour: 9, minute: 0, day: 1, ..Default::default() }.to_cron().unwrap(), "0 9 1 * *");
        assert_eq!(ScheduleSpec::from_cron("*/15 * * * *").unwrap().every, 15);
        assert_eq!(ScheduleSpec::from_cron("0 * * * *").unwrap().kind, "hourly");
        assert!(ScheduleSpec { kind: "never".into(), ..Default::default() }.to_cron().is_err());
    }
}
