// ─────────────────────────────────────────────
//  JobProfile Scheduler - ticks every minute, fires due profiles
//
//  Wired in `tauri::Builder::setup` once the tokio runtime is up.
//  Reads the JobProfileStore on each tick (so a profile edit takes
//  effect on the very next tick per the §10 Phase 7 acceptance
//  criterion) and:
//
//    1. Anchors an audit row JobProfileFired for traceability.
//    2. For each Engagement in engagement_scope, decides whether to
//       call the draft or the publish action based on review_gate:
//         AutoPublish → "<engagement>"          (real side effect)
//         Always | FirstN → "draft_<engagement>" (no side effect)
//    3. Dispatches each action through the connector bus.
//    4. If the action was a draft and the bus returned success, the
//       output lands in the ReviewQueue for user approval. The
//       publish action will fire later via the approve flow.
//    5. If the action was a publish and the bus returned success, the
//       run is recorded so the profile won't refire same-cadence.
//    6. Failures leave last_run_at unchanged so retry happens on next
//       tick (cadence-bounded - see job_profile.rs is_due).
//
//  Cadence-aware backoff is intentional: a Daily profile that fails
//  at 14:00 doesn't try again until tomorrow's 14:00, because
//  last_run_at is still empty so `is_due` continues to return true
//  for the rest of the day's window. Hourly profiles retry every
//  hour. This matches user intent - "post daily" failing twice today
//  shouldn't suddenly post twice tomorrow.
// ─────────────────────────────────────────────

use std::sync::Arc;

use chrono::Utc;
use uuid::Uuid;

use crate::audit_log::{AuditKind, AuditLogState};
use crate::connector_bus::{ConnectorBus, ConnectorJob};
use crate::job_profile::{Engagement, JobProfile, ReviewGate};
use crate::review_queue::ReviewQueue;

/// Fire one due job profile: for each engagement, dispatch the draft or
/// publish action through the connector bus, queue drafts for review, and
/// audit each phase. Returns true if anything shipped (a publish succeeded
/// or a draft was queued), so the caller records the run; a pure failure
/// returns false and the profile retries on its next cadence.
///
/// This is the per-profile body of the old cadence tick. The single
/// dispatcher (W2.3) now owns the loop and calls this when it drains a
/// job_profile job from the one queue.
pub async fn fire_profile(
    profile: &JobProfile,
    bus: &Arc<ConnectorBus>,
    audit: &Arc<AuditLogState>,
    review_queue: &Arc<std::sync::Mutex<ReviewQueue>>,
) -> bool {
    let now = Utc::now();
    anchor_audit(audit, profile, "fired", None);
    let mut any_publish_succeeded = false;
    let mut any_draft_queued = false;

    for eng in &profile.engagement_scope {
        let suffix = engagement_suffix(eng);
        let publish_action = suffix.to_string();
        let action_id = match profile.review_gate {
            ReviewGate::AutoPublish => publish_action.clone(),
            _ => format!("draft_{}", suffix),
        };

        let inputs = serde_json::json!({
            "goal": profile.goal,
            "engagement": suffix,
            "voice_profile_id": profile.voice_profile_id,
            "sources": profile.sources,
        });

        let job = ConnectorJob {
            connector_slug: profile.connector_slug.clone(),
            action_id: action_id.clone(),
            inputs: inputs.clone(),
            idempotency_key: idempotency_key_for(profile, &action_id, now),
            caller: format!("scheduler/job_profile/{}", profile.id),
        };

        let result = bus.dispatch(job).await;
        if !result.success {
            anchor_audit(audit, profile, "failed",
                result.error_code.as_deref().or(Some("unknown_error")));
            continue;
        }

        if action_id.starts_with("draft_") {
            if let Ok(mut q) = review_queue.lock() {
                let _ = q.enqueue(
                    &profile.id,
                    &profile.connector_slug,
                    &action_id,
                    &publish_action,
                    inputs,
                    result.output.clone(),
                    &profile.goal,
                );
                any_draft_queued = true;
            }
            anchor_audit(audit, profile, "drafted", None);
        } else {
            any_publish_succeeded = true;
            anchor_audit(audit, profile, "published", None);
        }
    }

    any_publish_succeeded || any_draft_queued
}

fn engagement_suffix(e: &Engagement) -> &'static str {
    match e {
        Engagement::Post => "post",
        Engagement::Reply => "reply",
        Engagement::Comment => "comment",
        Engagement::Like => "like",
        Engagement::AcceptRequest => "accept_request",
        Engagement::SendMessage => "send_message",
    }
}

fn idempotency_key_for(
    profile: &JobProfile,
    action_id: &str,
    now: chrono::DateTime<Utc>,
) -> String {
    use crate::job_profile::Cadence;
    let bucket = match &profile.cadence {
        Cadence::Hourly { .. } => now.format("%Y-%m-%dT%H").to_string(),
        Cadence::Daily { .. } => now.format("%Y-%m-%d").to_string(),
        Cadence::Weekly { .. } => now.format("%G-W%V").to_string(),
        Cadence::Custom { .. } => Uuid::new_v4().to_string(),
    };
    format!("profile-{}-{}-{}", profile.id, action_id, bucket)
}

fn anchor_audit(
    audit: &Arc<AuditLogState>,
    profile: &JobProfile,
    phase: &str,
    error_code: Option<&str>,
) {
    let Ok(mut log) = audit.lock() else { return };
    let payload = serde_json::json!({
        "profile_id": profile.id,
        "connector_slug": profile.connector_slug,
        "goal": profile.goal,
        "phase": phase,
        "error_code": error_code,
    });
    log.append(
        &format!("scheduler/{}", profile.id),
        AuditKind::JobProfileFired,
        payload,
        None,
    );
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::job_profile::{Cadence, Engagement, JobProfile};

    #[test]
    fn idempotency_key_per_day_for_daily_profile() {
        let p = JobProfile::new(
            "linkedin".into(),
            "post".into(),
            Cadence::Daily { hour_utc: 14, minute_utc: 0 },
            vec![Engagement::Post],
        );
        let t1 = chrono::Utc.with_ymd_and_hms(2026, 5, 11, 14, 5, 0).unwrap();
        let t2 = chrono::Utc.with_ymd_and_hms(2026, 5, 11, 18, 0, 0).unwrap();
        let t3 = chrono::Utc.with_ymd_and_hms(2026, 5, 12, 14, 5, 0).unwrap();
        // Same day → same key (bus deduplicates).
        assert_eq!(
            idempotency_key_for(&p, "draft_post", t1),
            idempotency_key_for(&p, "draft_post", t2),
        );
        // Next day → different key.
        assert_ne!(
            idempotency_key_for(&p, "draft_post", t1),
            idempotency_key_for(&p, "draft_post", t3),
        );
    }

    #[test]
    fn idempotency_key_differs_between_draft_and_publish() {
        // Critical: if FirstN ticks over and the same tick fires both
        // draft_post (from the user's prior approval) and post (the
        // freshly promoted gate), the two must not collide on the bus.
        let p = JobProfile::new(
            "linkedin".into(),
            "post".into(),
            Cadence::Daily { hour_utc: 14, minute_utc: 0 },
            vec![Engagement::Post],
        );
        let t = chrono::Utc.with_ymd_and_hms(2026, 5, 11, 14, 5, 0).unwrap();
        assert_ne!(
            idempotency_key_for(&p, "draft_post", t),
            idempotency_key_for(&p, "post", t),
        );
    }

    #[test]
    fn idempotency_key_per_hour_for_hourly_profile() {
        let p = JobProfile::new(
            "x".into(),
            "y".into(),
            Cadence::Hourly { every_n: 1 },
            vec![Engagement::Post],
        );
        let t1 = chrono::Utc.with_ymd_and_hms(2026, 5, 11, 14, 5, 0).unwrap();
        let t2 = chrono::Utc.with_ymd_and_hms(2026, 5, 11, 14, 55, 0).unwrap();
        let t3 = chrono::Utc.with_ymd_and_hms(2026, 5, 11, 15, 5, 0).unwrap();
        assert_eq!(
            idempotency_key_for(&p, "draft_post", t1),
            idempotency_key_for(&p, "draft_post", t2),
        );
        assert_ne!(
            idempotency_key_for(&p, "draft_post", t1),
            idempotency_key_for(&p, "draft_post", t3),
        );
    }

    #[test]
    fn engagement_suffix_is_stable() {
        assert_eq!(engagement_suffix(&Engagement::Post), "post");
        assert_eq!(engagement_suffix(&Engagement::Reply), "reply");
        assert_eq!(engagement_suffix(&Engagement::SendMessage), "send_message");
    }

    use chrono::TimeZone;
}
