//! The one scheduler and one queue (W2.3).
//!
//! Orbix used to run three independent scheduler loops: a cron tick that ran
//! workflows directly, a 5s job worker that drained a queue nothing fed, and
//! a job-profile cadence tick that fired connectors. They shared no queue and
//! could each drive work concurrently.
//!
//! This module replaces all three with a single dispatcher loop over one
//! queue (`core_job_scheduler`). Each tick it:
//!   1. Detects everything DUE - cron schedules and job-profile cadences -
//!      and enqueues a Job for each into the one queue.
//!   2. Drains the queue, dispatching each Job by type through the Run engine
//!      (workflows) or the connector bus (job profiles), with the queue's
//!      retry / dead-letter policy.
//!
//! Cron and cadence remain the persisted *definitions* of when to run;
//! `JobScheduler` is the single transient queue of what to run next. Because
//! due-detection is idempotent (get_due_schedules advances next_run; a
//! job-profile records its run on success), a crash between enqueue and drain
//! just re-detects the due item next tick - at-least-once, never lost.

use std::time::Duration;
use tauri::Manager;

use crate::job_scheduler::JobInput;
use crate::AppState;

const TICK: Duration = Duration::from_secs(15);

/// Job types the dispatcher understands.
const JOB_SCHEDULED_WORKFLOW: &str = "scheduled_workflow";
const JOB_JOB_PROFILE: &str = "job_profile";

/// Spawn the single dispatcher loop. Replaces the three prior spawn loops.
pub fn spawn(app: tauri::AppHandle) {
    tauri::async_runtime::spawn(async move {
        // Small offset so the first tick does not race the rest of boot.
        tokio::time::sleep(Duration::from_secs(6)).await;
        loop {
            tick(&app).await;
            tokio::time::sleep(TICK).await;
        }
    });
}

async fn tick(app: &tauri::AppHandle) {
    let state = app.state::<AppState>();

    enqueue_due(&state);
    drain(&state, app).await;

    // Autonomous email trigger: poll each connected account on its own
    // poll_interval_secs and run any matched rule's workflow with the email
    // as its {{trigger.*}} payload. This is what makes "when an email
    // arrives, do X" fire without a manual poke.
    crate::commands::triggers_cmd::poll_email_accounts(&state, app, false).await;
}

/// Detect due cron schedules and job-profile cadences, enqueueing one Job
/// per due item into the single queue.
fn enqueue_due(state: &AppState) {
    let now = chrono::Utc::now();

    // Cron schedules → scheduled_workflow jobs.
    let due_schedules = {
        let mut sched = state.automation.scheduler_store.lock().unwrap_or_else(|e| e.into_inner());
        let d = sched.get_due_schedules();
        if !d.is_empty() {
            sched.save();
        }
        d
    };
    if !due_schedules.is_empty() {
        let mut queue = state.core.core_job_scheduler.lock().unwrap_or_else(|e| e.into_inner());
        for entry in due_schedules {
            let _ = queue.submit(JobInput {
                job_type: JOB_SCHEDULED_WORKFLOW.to_string(),
                payload: serde_json::json!({
                    "schedule_id": entry.id,
                    "workflow_id": entry.workflow_id,
                }),
                priority: Some(5),
                // Each step already retries up to STEP_MAX_ATTEMPTS inside
                // the run; retrying the whole job on top ran a failing
                // side-effect step up to nine times.
                max_retries: Some(0),
                timeout_ms: None,
                schedule_at: None,
            });
        }
    }

    // Job-profile cadences → job_profile jobs.
    let due_profiles: Vec<String> = {
        let profiles = state.automation.job_profiles.lock().unwrap_or_else(|e| e.into_inner());
        profiles.due(now).into_iter().map(|p| p.id.clone()).collect()
    };
    if !due_profiles.is_empty() {
        let mut queue = state.core.core_job_scheduler.lock().unwrap_or_else(|e| e.into_inner());
        for profile_id in due_profiles {
            let _ = queue.submit(JobInput {
                job_type: JOB_JOB_PROFILE.to_string(),
                payload: serde_json::json!({ "profile_id": profile_id }),
                priority: Some(5),
                max_retries: Some(1),
                timeout_ms: None,
                schedule_at: None,
            });
        }
    }
}

/// Drain the queue, dispatching each Job by type. Runs until the queue is
/// empty this tick; retries/dead-letters are handled by the queue itself.
async fn drain(state: &AppState, app: &tauri::AppHandle) {
    loop {
        let job = {
            let mut queue = state.core.core_job_scheduler.lock().unwrap_or_else(|e| e.into_inner());
            queue.dequeue()
        };
        let Some(job) = job else { break };
        let started = chrono::Utc::now();

        let outcome: Result<serde_json::Value, String> = match job.job_type.as_str() {
            JOB_SCHEDULED_WORKFLOW | "workflow" => dispatch_workflow(&job, state, app).await,
            JOB_JOB_PROFILE => dispatch_job_profile(&job, state).await,
            other => Err(format!("Unknown job type: {}", other)),
        };

        let dur = (chrono::Utc::now() - started).num_milliseconds().max(0) as u64;
        let mut queue = state.core.core_job_scheduler.lock().unwrap_or_else(|e| e.into_inner());
        match outcome {
            Ok(output) => {
                queue.complete(&job.id, output, dur);
            }
            Err(e) => {
                queue.fail(&job.id, e);
            }
        }
    }
}

async fn dispatch_workflow(
    job: &crate::job_scheduler::Job,
    state: &AppState,
    app: &tauri::AppHandle,
) -> Result<serde_json::Value, String> {
    let workflow_id = job
        .payload
        .get("workflow_id")
        .and_then(|v| v.as_str())
        .map(|s| s.to_string())
        .or_else(|| job.payload.as_str().map(|s| s.to_string()))
        .ok_or("workflow job missing workflow_id")?;
    let schedule_id = job.payload.get("schedule_id").and_then(|v| v.as_str()).map(|s| s.to_string());

    let started = chrono::Utc::now();
    let wf_name = {
        let wfs = state.automation.workflow_store.lock().unwrap_or_else(|e| e.into_inner());
        wfs.get(&workflow_id).map(|w| w.name).unwrap_or_default()
    };
    let run = crate::commands::workflows_cmd::run_workflow_core(&workflow_id, state, app).await;

    // For scheduled workflows, record the run against the schedule entry so
    // last_run / run_count / fail_count stay accurate (bookkeeping the old
    // cron tick did inline).
    let (result_kind, output, error) = match &run {
        Ok(rs) if rs.status == crate::workflow::WorkflowRunStatus::Failed => {
            (crate::scheduler::ScheduleRunResult::Failed, rs.output.clone(), rs.error.clone())
        }
        Ok(rs) => (crate::scheduler::ScheduleRunResult::Success, rs.output.clone(), None),
        Err(e) => (crate::scheduler::ScheduleRunResult::Failed, Vec::new(), Some(e.clone())),
    };
    if let Some(sched_id) = schedule_id {
        let dur = (chrono::Utc::now() - started).num_milliseconds().max(0) as u64;
        let mut sched = state.automation.scheduler_store.lock().unwrap_or_else(|e| e.into_inner());
        sched.record_run(&sched_id, &workflow_id, &wf_name, result_kind, output, error, started.to_rfc3339(), dur);
        sched.save();
    }

    match run {
        Ok(rs) if rs.status == crate::workflow::WorkflowRunStatus::Failed => {
            Err(rs.error.unwrap_or_else(|| "workflow failed".into()))
        }
        Ok(rs) => Ok(serde_json::json!({ "status": format!("{:?}", rs.status) })),
        Err(e) => Err(e),
    }
}

async fn dispatch_job_profile(
    job: &crate::job_scheduler::Job,
    state: &AppState,
) -> Result<serde_json::Value, String> {
    let profile_id = job
        .payload
        .get("profile_id")
        .and_then(|v| v.as_str())
        .ok_or("job_profile job missing profile_id")?
        .to_string();
    let profile = {
        let profiles = state.automation.job_profiles.lock().unwrap_or_else(|e| e.into_inner());
        profiles.get(&profile_id).cloned()
    };
    let Some(profile) = profile else {
        return Err(format!("job profile {} not found", profile_id));
    };

    // A profile whose connector has no registered backend can never
    // succeed - pause it immediately instead of letting it churn. This
    // also neutralises profiles that appear from stale data or external
    // callers (field failure 2026-08-01: 'post daily' zombies kept
    // respawning under fresh ids and flooding the audit log).
    if !state.connect.connector_bus.has_backend(&profile.connector_slug) {
        let mut profiles = state.automation.job_profiles.lock().unwrap_or_else(|e| e.into_inner());
        let _ = profiles.update(&profile_id, |p| p.enabled = false);
        if let Ok(mut log) = state.core.audit_log.lock() {
            log.append(
                &format!("scheduler/{}", profile_id),
                crate::audit_log::AuditKind::JobProfileFired,
                serde_json::json!({
                    "profile_id": profile_id,
                    "connector_slug": profile.connector_slug,
                    "phase": "paused",
                    "error_code": "no_backend",
                    "reason": "no backend registered for this connector - profile disabled; re-enable after installing the connector",
                }),
                None,
            );
        }
        return Ok(serde_json::json!({ "skipped": "no backend - profile paused" }));
    }

    // Failure backoff. A failing profile used to refire on every dispatcher
    // tick (~15s) because a failed run leaves last_run_at unset and is_due
    // stays true - one broken LinkedIn profile wrote 10k audit rows in a
    // day. Exponential backoff (1min doubling to a 1h cap) throttles the
    // retries, and 20 consecutive failures pauses the profile with an
    // audit entry so the user can fix the cause and re-enable it.
    {
        let backoff = JOB_PROFILE_BACKOFF.lock().unwrap_or_else(|e| e.into_inner());
        if let Some((fails, last_attempt)) = backoff.get(&profile_id) {
            let wait = 60i64.saturating_mul(1 << (*fails).min(6) as i64).min(3600);
            if chrono::Utc::now().timestamp() - last_attempt < wait {
                return Ok(serde_json::json!({ "skipped": "failure backoff", "consecutive_failures": fails }));
            }
        }
    }

    let shipped = crate::job_profile_scheduler::fire_profile(
        &profile,
        &state.connect.connector_bus,
        &state.core.audit_log,
        &state.automation.review_queue,
    )
    .await;
    if shipped {
        JOB_PROFILE_BACKOFF.lock().unwrap_or_else(|e| e.into_inner()).remove(&profile_id);
        let mut profiles = state.automation.job_profiles.lock().unwrap_or_else(|e| e.into_inner());
        let _ = profiles.record_run(&profile_id, chrono::Utc::now());
        Ok(serde_json::json!({ "shipped": true }))
    } else {
        let fails = {
            let mut backoff = JOB_PROFILE_BACKOFF.lock().unwrap_or_else(|e| e.into_inner());
            let entry = backoff.entry(profile_id.clone()).or_insert((0, 0));
            entry.0 += 1;
            entry.1 = chrono::Utc::now().timestamp();
            entry.0
        };
        if fails >= 20 {
            let mut profiles = state.automation.job_profiles.lock().unwrap_or_else(|e| e.into_inner());
            let _ = profiles.update(&profile_id, |p| p.enabled = false);
            if let Ok(mut log) = state.core.audit_log.lock() {
                log.append(
                    &format!("scheduler/{}", profile_id),
                    crate::audit_log::AuditKind::JobProfileFired,
                    serde_json::json!({
                        "profile_id": profile_id,
                        "phase": "paused",
                        "error_code": "too_many_failures",
                        "reason": "paused after 20 consecutive failures - fix the connector and re-enable the profile",
                    }),
                    None,
                );
            }
        }
        Err("job profile produced no output".to_string())
    }
}

/// (consecutive failures, unix seconds of last attempt) per profile id.
static JOB_PROFILE_BACKOFF: std::sync::LazyLock<
    std::sync::Mutex<std::collections::HashMap<String, (u32, i64)>>,
> = std::sync::LazyLock::new(|| std::sync::Mutex::new(std::collections::HashMap::new()));
