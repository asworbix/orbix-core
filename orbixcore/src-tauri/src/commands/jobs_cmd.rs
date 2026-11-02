//! Commands split out of main.rs (W0.3). Mechanical move, no behavior change.

#[allow(unused_imports)]
use crate::prelude::*;
#[allow(unused_imports)]
use crate::*;

// ─────────────────────────────────────────────
//  Commands - Job profiles (Phase 7)
// ─────────────────────────────────────────────

#[tauri::command]
pub(crate) fn job_profile_create(
    profile: job_profile::JobProfile,
    state: State<'_, AppState>,
) -> Result<job_profile::JobProfile, String> {
    let mut store = state.automation.job_profiles.lock().map_err(|e| e.to_string())?;
    store.create(profile)
}

#[tauri::command]
pub(crate) fn job_profile_list(state: State<'_, AppState>) -> Result<Vec<job_profile::JobProfile>, String> {
    let store = state.automation.job_profiles.lock().map_err(|e| e.to_string())?;
    Ok(store.list().into_iter().cloned().collect())
}

#[tauri::command]
pub(crate) fn job_profile_get(
    id: String,
    state: State<'_, AppState>,
) -> Result<Option<job_profile::JobProfile>, String> {
    let store = state.automation.job_profiles.lock().map_err(|e| e.to_string())?;
    Ok(store.get(&id).cloned())
}

#[tauri::command]
pub(crate) fn job_profile_update(
    id: String,
    goal: Option<String>,
    cadence: Option<job_profile::Cadence>,
    review_gate: Option<job_profile::ReviewGate>,
    engagement_scope: Option<Vec<job_profile::Engagement>>,
    sources: Option<Vec<job_profile::ContentSource>>,
    enabled: Option<bool>,
    state: State<'_, AppState>,
) -> Result<job_profile::JobProfile, String> {
    let mut store = state.automation.job_profiles.lock().map_err(|e| e.to_string())?;
    store.update(&id, |p| {
        if let Some(g) = goal { p.goal = g; }
        if let Some(c) = cadence { p.cadence = c; }
        if let Some(gate) = review_gate {
            // Promotion to AutoPublish goes through the explicit
            // command below; reject here so a malicious frontend cannot
            // skip the approved-count check via update.
            if !matches!(gate, job_profile::ReviewGate::AutoPublish) {
                p.review_gate = gate;
            }
        }
        if let Some(e) = engagement_scope { p.engagement_scope = e; }
        if let Some(s) = sources { p.sources = s; }
        if let Some(b) = enabled { p.enabled = b; }
    })
}

#[tauri::command]
pub(crate) fn job_profile_delete(id: String, state: State<'_, AppState>) -> Result<(), String> {
    let mut store = state.automation.job_profiles.lock().map_err(|e| e.to_string())?;
    store.delete(&id)
}

#[tauri::command]
pub(crate) fn job_profile_record_approval(
    id: String,
    state: State<'_, AppState>,
) -> Result<job_profile::JobProfile, String> {
    let mut store = state.automation.job_profiles.lock().map_err(|e| e.to_string())?;
    store.record_approval(&id)
}

#[tauri::command]
pub(crate) fn job_profile_promote_autopublish(
    id: String,
    state: State<'_, AppState>,
) -> Result<job_profile::JobProfile, String> {
    let mut store = state.automation.job_profiles.lock().map_err(|e| e.to_string())?;
    store.promote_to_autopublish(&id)
}

#[tauri::command]
pub(crate) fn job_profile_due_now(
    state: State<'_, AppState>,
) -> Result<Vec<job_profile::JobProfile>, String> {
    let store = state.automation.job_profiles.lock().map_err(|e| e.to_string())?;
    Ok(store.due(chrono::Utc::now()).into_iter().cloned().collect())
}


// ─────────────────────────────────────────────
//  Commands - Review queue (companion to job profiles)
// ─────────────────────────────────────────────

#[tauri::command]
pub(crate) fn review_queue_list(state: State<'_, AppState>) -> Result<Vec<review_queue::PendingReview>, String> {
    let q = state.automation.review_queue.lock().map_err(|e| e.to_string())?;
    Ok(q.list().into_iter().cloned().collect())
}

#[tauri::command]
pub(crate) fn review_queue_get(
    id: String,
    state: State<'_, AppState>,
) -> Result<Option<review_queue::PendingReview>, String> {
    let q = state.automation.review_queue.lock().map_err(|e| e.to_string())?;
    Ok(q.get(&id).cloned())
}

#[tauri::command]
pub(crate) fn review_queue_reject(id: String, state: State<'_, AppState>) -> Result<(), String> {
    let mut q = state.automation.review_queue.lock().map_err(|e| e.to_string())?;
    q.delete(&id)
}

/// Approve a pending draft: fires the publish action through the bus
/// with the saved payload, records the approval on the profile (which
/// may flip a FirstN gate to AutoPublish), and removes the queue entry.
#[tauri::command]
pub(crate) async fn review_queue_approve(
    id: String,
    state: State<'_, AppState>,
) -> Result<connector_bus::ConnectorResult, String> {
    // Take the pending item under a short critical section so the lock
    // is released before we await the bus.
    let item = {
        let mut q = state.automation.review_queue.lock().map_err(|e| e.to_string())?;
        q.take(&id)?
    };

    let job = connector_bus::ConnectorJob {
        connector_slug: item.connector_slug.clone(),
        action_id: item.publish_action.clone(),
        // Publish action receives the connector's own draft payload
        // verbatim. The connector is responsible for treating its
        // own draft output as valid publish inputs.
        inputs: item.payload.clone(),
        idempotency_key: format!("approve-{}", item.id),
        caller: format!("review_queue/approve/{}", item.id),
    };

    let result = state.connect.connector_bus.dispatch(job).await;

    if result.success {
        // Record the approval; if the gate was FirstN this is what
        // ticks it toward AutoPublish.
        if let Ok(mut profiles) = state.automation.job_profiles.lock() {
            let _ = profiles.record_approval(&item.profile_id);
        }
    } else {
        // Bus refused to publish - put the item back so the user can
        // retry. The bus circuit breaker and idempotency cache make
        // this safe to retry.
        if let Ok(mut q) = state.automation.review_queue.lock() {
            q.items.insert(item.id.clone(), item.clone());
            let _ = q.save();
        }
    }
    Ok(result)
}

