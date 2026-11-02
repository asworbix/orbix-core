// ─────────────────────────────────────────────
//  Job Profiles - Phase 7 of CONNECTORS.md
//
//  Per §9: a connector lets Bob act, but it does not tell him *what*
//  to do. The JobProfile is the "what" - the cadence, review gate,
//  engagement scope, and content sources for a recurring use of a
//  connector. The user fills this in once per (connector, intent);
//  Bob runs it forever after.
//
//  Critical defaults: ReviewGate::AutoPublish is opt-in, never the
//  default. Engagement scope is explicit, not inherited from the
//  connector's full capability set.
// ─────────────────────────────────────────────

use std::collections::HashMap;

use chrono::{DateTime, Datelike, Timelike, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::storage;

const STORE_FILE: &str = "job_profiles.json";

// ─────────────────────────────────────────────
//  Sub-types
// ─────────────────────────────────────────────

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "snake_case", tag = "kind")]
pub enum ContentSource {
    Rss { url: String },
    GitHub { owner: String, repo: Option<String> },
    Calendar,
    Manual,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "snake_case", tag = "kind")]
pub enum Cadence {
    /// Every N hours, anchored to the profile's creation minute.
    Hourly { every_n: u32 },
    /// Once a day at `hour_utc:minute_utc`.
    Daily { hour_utc: u8, minute_utc: u8 },
    /// Once a week on `weekday` (Mon=0..Sun=6) at `hour_utc:minute_utc`.
    Weekly { weekday: u8, hour_utc: u8, minute_utc: u8 },
    /// Free-form cron string; resolution is up to the scheduler.
    Custom { cron: String },
}

/// Review gate. `AutoPublish` is opt-in and only after N drafts have
/// been approved by the user - never a default for new profiles.
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "snake_case", tag = "kind")]
pub enum ReviewGate {
    /// Every output routes to the review queue.
    Always,
    /// First `remaining` outputs route to review; on each user approval,
    /// `remaining` decreases. When it hits zero the gate flips to
    /// AutoPublish for that profile.
    FirstN { remaining: u32 },
    /// No review - outputs hit the connector immediately. Only allowed
    /// to be set if the profile has at least N prior approvals; the
    /// frontend enforces this and the backend rejects a fresh profile
    /// trying to start in AutoPublish.
    AutoPublish,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq, Hash)]
#[serde(rename_all = "snake_case")]
pub enum Engagement {
    Post,
    Reply,
    Comment,
    Like,
    AcceptRequest,
    SendMessage,
}

// ─────────────────────────────────────────────
//  JobProfile
// ─────────────────────────────────────────────

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct JobProfile {
    pub id: String,
    pub connector_slug: String,
    /// "post daily to LinkedIn", "reply to GitHub issues" - natural
    /// language goal Bob uses to keep its outputs consistent.
    pub goal: String,
    /// Voice profile id is filled by a separate flow; empty until then.
    pub voice_profile_id: String,
    pub sources: Vec<ContentSource>,
    pub cadence: Cadence,
    pub review_gate: ReviewGate,
    pub engagement_scope: Vec<Engagement>,
    pub created_at: String,
    pub updated_at: String,
    pub last_run_at: Option<String>,
    /// Monotonic counter of user-approved outputs from this profile.
    /// Used to allow promotion to AutoPublish without re-asking.
    pub approved_count: u32,
    /// True if the profile is active (scheduler ticks it).
    pub enabled: bool,
}

impl JobProfile {
    pub fn new(
        connector_slug: String,
        goal: String,
        cadence: Cadence,
        engagement_scope: Vec<Engagement>,
    ) -> Self {
        let now = Utc::now().to_rfc3339();
        Self {
            id: Uuid::new_v4().to_string(),
            connector_slug,
            goal,
            voice_profile_id: String::new(),
            sources: Vec::new(),
            cadence,
            // Always start in Always; user promotes later.
            review_gate: ReviewGate::Always,
            engagement_scope,
            created_at: now.clone(),
            updated_at: now,
            last_run_at: None,
            approved_count: 0,
            enabled: true,
        }
    }

    /// True if the profile is due for a run at `now` based on its
    /// cadence and last_run_at. The scheduler polls this each tick.
    pub fn is_due(&self, now: DateTime<Utc>) -> bool {
        if !self.enabled { return false; }
        let last = self.last_run_at
            .as_deref()
            .and_then(|s| DateTime::parse_from_rfc3339(s).ok())
            .map(|t| t.with_timezone(&Utc));
        match &self.cadence {
            Cadence::Hourly { every_n } => {
                let every = (*every_n).max(1);
                match last {
                    None => true,
                    Some(t) => (now - t).num_hours() >= every as i64,
                }
            }
            Cadence::Daily { hour_utc, minute_utc } => {
                // Due if we're past today's scheduled minute and we haven't
                // run today yet.
                let target_today = now
                    .date_naive()
                    .and_hms_opt(*hour_utc as u32, *minute_utc as u32, 0)
                    .map(|d| d.and_utc())
                    .unwrap_or(now);
                if now < target_today { return false; }
                match last {
                    None => true,
                    Some(t) => t < target_today,
                }
            }
            Cadence::Weekly { weekday, hour_utc, minute_utc } => {
                if now.weekday().num_days_from_monday() as u8 != *weekday { return false; }
                if now.hour() < *hour_utc as u32 { return false; }
                if now.hour() == *hour_utc as u32 && now.minute() < *minute_utc as u32 {
                    return false;
                }
                match last {
                    None => true,
                    Some(t) => (now - t).num_days() >= 7,
                }
            }
            // Cron parsing is out of scope here; the scheduler crate
            // owns it. For now Custom is never auto-due.
            Cadence::Custom { .. } => false,
        }
    }
}

// ─────────────────────────────────────────────
//  Store
// ─────────────────────────────────────────────

#[derive(Default, Clone, Debug, Serialize, Deserialize)]
pub struct JobProfileStore {
    pub profiles: HashMap<String, JobProfile>,
}

impl JobProfileStore {
    pub fn load() -> Self {
        storage::load_json::<Self>(STORE_FILE).unwrap_or_default()
    }

    pub fn save(&self) -> Result<(), String> {
        storage::save_json(STORE_FILE, self)
    }

    /// Insert a freshly-built profile. Refuses to accept an
    /// AutoPublish review gate on first save - that gate is only
    /// reachable via promotion after `approved_count` reaches its
    /// threshold (the frontend's flow).
    pub fn create(&mut self, profile: JobProfile) -> Result<JobProfile, String> {
        if matches!(profile.review_gate, ReviewGate::AutoPublish) {
            return Err(
                "ReviewGate::AutoPublish is not a valid starting gate; \
                 begin with Always or FirstN and let the user promote it"
                    .into(),
            );
        }
        self.profiles.insert(profile.id.clone(), profile.clone());
        self.save()?;
        Ok(profile)
    }

    pub fn list(&self) -> Vec<&JobProfile> {
        let mut out: Vec<&JobProfile> = self.profiles.values().collect();
        out.sort_by(|a, b| a.created_at.cmp(&b.created_at));
        out
    }

    pub fn get(&self, id: &str) -> Option<&JobProfile> {
        self.profiles.get(id)
    }

    pub fn update(&mut self, id: &str, mutate: impl FnOnce(&mut JobProfile)) -> Result<JobProfile, String> {
        let profile = self.profiles.get_mut(id)
            .ok_or_else(|| format!("no such profile: {}", id))?;
        mutate(profile);
        profile.updated_at = Utc::now().to_rfc3339();
        let cloned = profile.clone();
        self.save()?;
        Ok(cloned)
    }

    pub fn delete(&mut self, id: &str) -> Result<(), String> {
        self.profiles.remove(id);
        self.save()
    }

    /// Every profile whose cadence says "run me now". The scheduler
    /// polls this each tick.
    pub fn due(&self, now: DateTime<Utc>) -> Vec<&JobProfile> {
        self.profiles.values().filter(|p| p.is_due(now)).collect()
    }

    /// Mark a profile as having just run. Called by the scheduler after
    /// the job dispatch succeeds.
    pub fn record_run(&mut self, id: &str, at: DateTime<Utc>) -> Result<(), String> {
        self.update(id, |p| { p.last_run_at = Some(at.to_rfc3339()); })?;
        Ok(())
    }

    /// Record a user-approved output and possibly promote the gate.
    /// If the gate is FirstN, decrement; when it hits zero, flip to
    /// AutoPublish. If the gate is Always, just bump the counter so the
    /// frontend can show progress toward promotion.
    pub fn record_approval(&mut self, id: &str) -> Result<JobProfile, String> {
        self.update(id, |p| {
            p.approved_count = p.approved_count.saturating_add(1);
            if let ReviewGate::FirstN { remaining } = &mut p.review_gate {
                if *remaining > 0 { *remaining -= 1; }
                if *remaining == 0 { p.review_gate = ReviewGate::AutoPublish; }
            }
        })
    }

    /// Explicit promotion to AutoPublish by the user. Refused unless
    /// the profile has at least one prior approval.
    pub fn promote_to_autopublish(&mut self, id: &str) -> Result<JobProfile, String> {
        let prior_approvals = self.get(id).map(|p| p.approved_count).unwrap_or(0);
        if prior_approvals < 1 {
            return Err(
                "refusing to promote to AutoPublish without any approved drafts on record"
                    .into(),
            );
        }
        self.update(id, |p| { p.review_gate = ReviewGate::AutoPublish; })
    }
}

// ─────────────────────────────────────────────
//  Tests
// ─────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::TimeZone;

    fn fresh() -> JobProfile {
        JobProfile::new(
            "linkedin".into(),
            "post daily".into(),
            Cadence::Daily { hour_utc: 14, minute_utc: 0 },
            vec![Engagement::Post],
        )
    }

    #[test]
    fn new_profile_defaults_review_gate_to_always() {
        let p = fresh();
        assert!(matches!(p.review_gate, ReviewGate::Always));
        assert!(p.enabled);
        assert_eq!(p.approved_count, 0);
    }

    #[test]
    fn store_refuses_initial_autopublish() {
        let mut s = JobProfileStore::default();
        let mut p = fresh();
        p.review_gate = ReviewGate::AutoPublish;
        let err = s.create(p).unwrap_err();
        assert!(err.contains("AutoPublish"));
    }

    #[test]
    fn promote_refuses_without_approvals() {
        let mut s = JobProfileStore::default();
        // Use a placeholder save: in-memory only by avoiding the
        // filesystem path - we just test the validation rule, so we
        // call promote_to_autopublish on a manually inserted profile.
        let p = fresh();
        let id = p.id.clone();
        s.profiles.insert(id.clone(), p);
        let err = s.promote_to_autopublish(&id).unwrap_err();
        assert!(err.contains("AutoPublish"));
    }

    #[test]
    fn first_n_decrements_and_promotes_on_zero() {
        let mut s = JobProfileStore::default();
        let mut p = fresh();
        p.review_gate = ReviewGate::FirstN { remaining: 2 };
        let id = p.id.clone();
        s.profiles.insert(id.clone(), p);

        let after = s.record_approval(&id).unwrap();
        match after.review_gate {
            ReviewGate::FirstN { remaining: 1 } => {}
            ref g => panic!("expected remaining=1, got {:?}", g),
        }
        let after = s.record_approval(&id).unwrap();
        // remaining hit zero → flip to AutoPublish
        assert!(matches!(after.review_gate, ReviewGate::AutoPublish));
        assert_eq!(after.approved_count, 2);
    }

    #[test]
    fn daily_is_due_after_scheduled_time_first_run() {
        let p = JobProfile {
            cadence: Cadence::Daily { hour_utc: 14, minute_utc: 0 },
            last_run_at: None,
            ..fresh()
        };
        // Before 14:00 UTC - not due.
        let early = Utc.with_ymd_and_hms(2026, 5, 11, 13, 30, 0).unwrap();
        assert!(!p.is_due(early));
        // After 14:00 UTC, no prior run - due.
        let later = Utc.with_ymd_and_hms(2026, 5, 11, 14, 30, 0).unwrap();
        assert!(p.is_due(later));
    }

    #[test]
    fn daily_not_due_again_same_day() {
        let already_today = Utc.with_ymd_and_hms(2026, 5, 11, 14, 5, 0).unwrap();
        let p = JobProfile {
            cadence: Cadence::Daily { hour_utc: 14, minute_utc: 0 },
            last_run_at: Some(already_today.to_rfc3339()),
            ..fresh()
        };
        let same_day_evening = Utc.with_ymd_and_hms(2026, 5, 11, 20, 0, 0).unwrap();
        assert!(!p.is_due(same_day_evening));
        let next_day_after_target = Utc.with_ymd_and_hms(2026, 5, 12, 14, 30, 0).unwrap();
        assert!(p.is_due(next_day_after_target));
    }

    #[test]
    fn hourly_respects_every_n() {
        let now = Utc.with_ymd_and_hms(2026, 5, 11, 10, 0, 0).unwrap();
        let four_hours_ago = Utc.with_ymd_and_hms(2026, 5, 11, 6, 0, 0).unwrap();
        let p = JobProfile {
            cadence: Cadence::Hourly { every_n: 6 },
            last_run_at: Some(four_hours_ago.to_rfc3339()),
            ..fresh()
        };
        assert!(!p.is_due(now));

        let seven_hours_ago = Utc.with_ymd_and_hms(2026, 5, 11, 3, 0, 0).unwrap();
        let p2 = JobProfile {
            last_run_at: Some(seven_hours_ago.to_rfc3339()),
            ..p
        };
        assert!(p2.is_due(now));
    }

    #[test]
    fn disabled_profile_never_due() {
        let mut p = fresh();
        p.enabled = false;
        let now = Utc.with_ymd_and_hms(2026, 5, 11, 23, 0, 0).unwrap();
        assert!(!p.is_due(now));
    }

    #[test]
    fn custom_cron_never_auto_due_in_phase_7() {
        let p = JobProfile {
            cadence: Cadence::Custom { cron: "*/5 * * * *".into() },
            last_run_at: None,
            ..fresh()
        };
        assert!(!p.is_due(Utc::now()));
    }
}
