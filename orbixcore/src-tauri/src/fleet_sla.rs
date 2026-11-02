use serde::{Deserialize, Serialize};
use uuid::Uuid;
use chrono::Utc;

// ─────────────────────────────────────────────
//  SLA Policies
// ─────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SlaPolicy {
    pub id: String,
    pub name: String,
    pub description: String,
    pub priority_filter: Option<String>,     // "Low", "Normal", "High", "Urgent" or None = all
    pub role_filter: Option<String>,         // specific role or None = all
    pub response_minutes: u32,               // max time to acknowledge/assign
    pub resolution_minutes: u32,             // max time to complete
    pub warning_pct: f32,                    // warn when this % of time elapsed (e.g. 75)
    pub enabled: bool,
    pub is_builtin: bool,
    pub created_at: String,
}

// ─────────────────────────────────────────────
//  SLA Tracking
// ─────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum SlaStatus {
    OnTrack,
    AtRisk,
    Breached,
    Met,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SlaTracker {
    pub id: String,
    pub policy_id: String,
    pub policy_name: String,
    pub work_item_id: String,
    pub work_item_title: String,
    pub device_id: Option<String>,
    pub device_name: Option<String>,
    pub role: Option<String>,
    pub priority: String,
    pub status: SlaStatus,
    pub response_deadline: String,
    pub resolution_deadline: String,
    pub responded_at: Option<String>,
    pub resolved_at: Option<String>,
    pub response_breached: bool,
    pub resolution_breached: bool,
    pub remaining_minutes: i64,             // negative = overdue
    pub started_at: String,
}

// ─────────────────────────────────────────────
//  SLA Breaches
// ─────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SlaBreach {
    pub id: String,
    pub tracker_id: String,
    pub policy_id: String,
    pub policy_name: String,
    pub work_item_id: String,
    pub work_item_title: String,
    pub device_name: Option<String>,
    pub breach_type: BreachType,
    pub severity: BreachSeverity,
    pub overdue_minutes: i64,
    pub acknowledged: bool,
    pub breached_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum BreachType {
    Response,
    Resolution,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum BreachSeverity {
    Minor,      // < 30 min overdue
    Major,      // 30-120 min overdue
    Critical,   // > 120 min overdue
}

// ─────────────────────────────────────────────
//  Dashboard
// ─────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SlaDashboard {
    pub total_active: u32,
    pub on_track: u32,
    pub at_risk: u32,
    pub breached: u32,
    pub met_today: u32,
    pub compliance_rate: f32,               // % of SLAs met
    pub avg_resolution_mins: f32,
    pub total_breaches: u32,
    pub unacknowledged_breaches: u32,
}

// ─────────────────────────────────────────────
//  Store
// ─────────────────────────────────────────────

pub struct FleetSlaStore {
    pub policies: Vec<SlaPolicy>,
    pub trackers: Vec<SlaTracker>,
    pub breaches: Vec<SlaBreach>,
}

impl Default for FleetSlaStore {
    fn default() -> Self {
        Self {
            policies: Self::builtin_policies(),
            trackers: Vec::new(),
            breaches: Vec::new(),
        }
    }
}

impl FleetSlaStore {
    fn builtin_policies() -> Vec<SlaPolicy> {
        vec![
            SlaPolicy {
                id: "sla-urgent".into(),
                name: "Urgent Priority".into(),
                description: "Urgent items must be responded to in 5 min and resolved in 30 min".into(),
                priority_filter: Some("Urgent".into()),
                role_filter: None,
                response_minutes: 5,
                resolution_minutes: 30,
                warning_pct: 70.0,
                enabled: true,
                is_builtin: true,
                created_at: Utc::now().to_rfc3339(),
            },
            SlaPolicy {
                id: "sla-high".into(),
                name: "High Priority".into(),
                description: "High priority items: 15 min response, 60 min resolution".into(),
                priority_filter: Some("High".into()),
                role_filter: None,
                response_minutes: 15,
                resolution_minutes: 60,
                warning_pct: 75.0,
                enabled: true,
                is_builtin: true,
                created_at: Utc::now().to_rfc3339(),
            },
            SlaPolicy {
                id: "sla-normal".into(),
                name: "Normal Priority".into(),
                description: "Normal items: 30 min response, 120 min resolution".into(),
                priority_filter: Some("Normal".into()),
                role_filter: None,
                response_minutes: 30,
                resolution_minutes: 120,
                warning_pct: 75.0,
                enabled: true,
                is_builtin: true,
                created_at: Utc::now().to_rfc3339(),
            },
            SlaPolicy {
                id: "sla-low".into(),
                name: "Low Priority".into(),
                description: "Low priority items: 60 min response, 480 min resolution".into(),
                priority_filter: Some("Low".into()),
                role_filter: None,
                response_minutes: 60,
                resolution_minutes: 480,
                warning_pct: 80.0,
                enabled: false,
                is_builtin: true,
                created_at: Utc::now().to_rfc3339(),
            },
        ]
    }

    // ── Policies ───────────────────────────────

    pub fn list_policies(&self) -> Vec<SlaPolicy> {
        self.policies.clone()
    }

    pub fn create_policy(&mut self, mut policy: SlaPolicy) -> SlaPolicy {
        if policy.id.is_empty() { policy.id = Uuid::new_v4().to_string(); }
        if policy.created_at.is_empty() { policy.created_at = Utc::now().to_rfc3339(); }
        policy.is_builtin = false;
        self.policies.push(policy.clone());
        policy
    }

    pub fn update_policy(&mut self, policy: SlaPolicy) -> Option<SlaPolicy> {
        if let Some(p) = self.policies.iter_mut().find(|p| p.id == policy.id) {
            p.name = policy.name;
            p.description = policy.description;
            p.priority_filter = policy.priority_filter;
            p.role_filter = policy.role_filter;
            p.response_minutes = policy.response_minutes;
            p.resolution_minutes = policy.resolution_minutes;
            p.warning_pct = policy.warning_pct;
            p.enabled = policy.enabled;
            Some(p.clone())
        } else {
            None
        }
    }

    pub fn delete_policy(&mut self, id: &str) -> bool {
        let len = self.policies.len();
        self.policies.retain(|p| p.id != id || p.is_builtin);
        self.policies.len() < len
    }

    pub fn toggle_policy(&mut self, id: &str) -> Option<SlaPolicy> {
        if let Some(p) = self.policies.iter_mut().find(|p| p.id == id) {
            p.enabled = !p.enabled;
            Some(p.clone())
        } else {
            None
        }
    }

    // ── Tracking ───────────────────────────────

    pub fn start_tracking(&mut self, mut tracker: SlaTracker) -> SlaTracker {
        if tracker.id.is_empty() { tracker.id = Uuid::new_v4().to_string(); }
        if tracker.started_at.is_empty() { tracker.started_at = Utc::now().to_rfc3339(); }
        self.trackers.push(tracker.clone());
        tracker
    }

    pub fn list_active_trackers(&self) -> Vec<SlaTracker> {
        self.trackers.iter()
            .filter(|t| t.status == SlaStatus::OnTrack || t.status == SlaStatus::AtRisk || t.status == SlaStatus::Breached)
            .cloned()
            .collect()
    }

    pub fn list_all_trackers(&self, limit: usize) -> Vec<SlaTracker> {
        self.trackers.iter().rev().take(limit).cloned().collect()
    }

    pub fn mark_responded(&mut self, tracker_id: &str) -> Option<SlaTracker> {
        if let Some(t) = self.trackers.iter_mut().find(|t| t.id == tracker_id) {
            t.responded_at = Some(Utc::now().to_rfc3339());
            Some(t.clone())
        } else {
            None
        }
    }

    pub fn mark_resolved(&mut self, tracker_id: &str) -> Option<SlaTracker> {
        if let Some(t) = self.trackers.iter_mut().find(|t| t.id == tracker_id) {
            t.resolved_at = Some(Utc::now().to_rfc3339());
            t.status = if t.resolution_breached { SlaStatus::Breached } else { SlaStatus::Met };
            Some(t.clone())
        } else {
            None
        }
    }

    // ── Breaches ───────────────────────────────

    pub fn record_breach(&mut self, mut breach: SlaBreach) -> SlaBreach {
        if breach.id.is_empty() { breach.id = Uuid::new_v4().to_string(); }
        if breach.breached_at.is_empty() { breach.breached_at = Utc::now().to_rfc3339(); }
        breach.severity = if breach.overdue_minutes > 120 {
            BreachSeverity::Critical
        } else if breach.overdue_minutes > 30 {
            BreachSeverity::Major
        } else {
            BreachSeverity::Minor
        };
        self.breaches.push(breach.clone());
        breach
    }

    pub fn list_breaches(&self, unacked_only: bool) -> Vec<SlaBreach> {
        if unacked_only {
            self.breaches.iter().filter(|b| !b.acknowledged).cloned().collect()
        } else {
            let mut breaches = self.breaches.clone();
            breaches.sort_by(|a, b| b.breached_at.cmp(&a.breached_at));
            breaches
        }
    }

    pub fn acknowledge_breach(&mut self, id: &str) -> bool {
        if let Some(b) = self.breaches.iter_mut().find(|b| b.id == id) {
            b.acknowledged = true;
            true
        } else {
            false
        }
    }

    pub fn acknowledge_all_breaches(&mut self) {
        for b in &mut self.breaches {
            b.acknowledged = true;
        }
    }

    // ── Dashboard ──────────────────────────────

    pub fn get_dashboard(&self) -> SlaDashboard {
        let active: Vec<&SlaTracker> = self.trackers.iter()
            .filter(|t| t.status != SlaStatus::Met)
            .collect();

        let on_track = active.iter().filter(|t| t.status == SlaStatus::OnTrack).count() as u32;
        let at_risk = active.iter().filter(|t| t.status == SlaStatus::AtRisk).count() as u32;
        let breached = active.iter().filter(|t| t.status == SlaStatus::Breached).count() as u32;
        let met = self.trackers.iter().filter(|t| t.status == SlaStatus::Met).count() as u32;

        let total_completed = met + breached;
        let compliance_rate = if total_completed > 0 { met as f32 / total_completed as f32 * 100.0 } else { 100.0 };

        let resolved_times: Vec<f32> = self.trackers.iter()
            .filter_map(|t| {
                t.resolved_at.as_ref().and_then(|_| {
                    Some(t.remaining_minutes.abs() as f32) // simplified
                })
            })
            .collect();
        let avg_resolution = if resolved_times.is_empty() { 0.0 } else {
            resolved_times.iter().sum::<f32>() / resolved_times.len() as f32
        };

        let unacked = self.breaches.iter().filter(|b| !b.acknowledged).count() as u32;

        SlaDashboard {
            total_active: active.len() as u32,
            on_track,
            at_risk,
            breached,
            met_today: met,
            compliance_rate,
            avg_resolution_mins: avg_resolution,
            total_breaches: self.breaches.len() as u32,
            unacknowledged_breaches: unacked,
        }
    }
}
