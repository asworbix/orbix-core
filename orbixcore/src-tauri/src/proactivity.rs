//! Bob's proactivity engine (W3).
//!
//! The suggestion generators in `suggestions.rs` were fully built but never
//! called: `generate_pipeline_suggestions` and `generate_workflow_suggestions`
//! had zero call sites, so Bob was purely reactive. This module runs them on
//! an interval, sourcing their inputs from live state, and surfaces any new
//! suggestion through Bob (a toast + a data-changed refresh for the
//! SuggestionsPanel). Accepting a suggestion is handled by the existing
//! accept_suggestion command.

use std::time::Duration;
use tauri::Manager;

use crate::AppState;

/// Spawn the proactivity loop. It self-paces to the user's configured
/// `check_interval_mins` (default 5), checking on a 60s granularity.
pub fn spawn(app: tauri::AppHandle) {
    tauri::async_runtime::spawn(async move {
        // Offset so the first pass does not race boot/hydrate.
        tokio::time::sleep(Duration::from_secs(20)).await;
        let mut elapsed_mins: u32 = u32::MAX; // fire on first eligible tick
        loop {
            let interval_mins = {
                let state = app.state::<AppState>();
                let store = state.automation.suggestion_store.lock().unwrap_or_else(|e| e.into_inner());
                store.preferences.check_interval_mins.max(1)
            };
            if elapsed_mins >= interval_mins {
                tick(&app);
                elapsed_mins = 0;
            }
            tokio::time::sleep(Duration::from_secs(60)).await;
            elapsed_mins = elapsed_mins.saturating_add(1);
        }
    });
}

/// One proactivity pass: gather signals, run the generators, surface new
/// suggestions. Pure-ish (only touches state), so it is exercised via the
/// generator unit tests plus this thin assembly.
fn tick(app: &tauri::AppHandle) {
    let state = app.state::<AppState>();

    // Signal 1: stale board items -> "review these" suggestions.
    let stale: Vec<(String, String, String, u64)> = {
        let board = state.automation.pipeline_store.lock().unwrap_or_else(|e| e.into_inner());
        board
            .stale_items()
            .into_iter()
            .map(|item| {
                let hours = item
                    .entered_stage_at
                    .as_ref()
                    .and_then(|e| e.parse::<chrono::DateTime<chrono::Utc>>().ok())
                    .map(|dt| (chrono::Utc::now() - dt).num_hours().max(0) as u64)
                    .unwrap_or(0);
                let stage = item.stage_id.clone().unwrap_or_default();
                (item.id.clone(), item.title.clone(), stage, hours)
            })
            .collect()
    };

    // Signal 2: manual workflows run repeatedly -> "schedule this" suggestions.
    // Sourced from the Run registry (W2): count completed workflow runs in the
    // last 7 days per workflow whose trigger is still Manual.
    let manual_runs: Vec<(String, String, u32)> = collect_manual_workflow_runs(&state);

    let added = {
        let mut store = state.automation.suggestion_store.lock().unwrap_or_else(|e| e.into_inner());
        let mut added = store.generate_pipeline_suggestions(stale);
        added.extend(store.generate_workflow_suggestions(manual_runs));
        added
    };

    if !added.is_empty() {
        crate::event_bus::emit_ui(app, "suggestions-updated", added.len()).ok();
        // Surface the freshest one through Bob so proactivity is visible
        // without opening a panel.
        if let Some(first) = added.first() {
            crate::event_bus::emit_ui(
                app,
                "bob-toast",
                serde_json::json!({
                    "kind": "suggestion",
                    "title": first.title,
                    "message": first.description,
                }),
            )
            .ok();
        }
    }
}

/// Count Manual-triggered workflows by how many completed Runs they had in
/// the last 7 days. Only workflows still on a Manual trigger are candidates
/// for a "schedule this" suggestion.
fn collect_manual_workflow_runs(state: &AppState) -> Vec<(String, String, u32)> {
    use std::collections::HashMap;
    let cutoff = chrono::Utc::now() - chrono::Duration::days(7);

    let mut counts: HashMap<String, u32> = HashMap::new();
    {
        let runs = state.automation.run_store.lock().unwrap_or_else(|e| e.into_inner());
        for run in runs.runs.values() {
            if run.status != crate::run::RunStatus::Completed {
                continue;
            }
            let crate::run::RunKind::Workflow { workflow_id } = &run.kind else { continue };
            let recent = run
                .created_at
                .parse::<chrono::DateTime<chrono::Utc>>()
                .map(|dt| dt >= cutoff)
                .unwrap_or(false);
            if recent {
                *counts.entry(workflow_id.clone()).or_insert(0) += 1;
            }
        }
    }
    if counts.is_empty() {
        return Vec::new();
    }

    let store = state.automation.workflow_store.lock().unwrap_or_else(|e| e.into_inner());
    counts
        .into_iter()
        .filter_map(|(wf_id, count)| {
            store.get(&wf_id).and_then(|w| {
                if matches!(w.trigger, crate::workflow::WorkflowTrigger::Manual) {
                    Some((wf_id, w.name, count))
                } else {
                    None
                }
            })
        })
        .collect()
}
