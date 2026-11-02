use serde::{Deserialize, Serialize};
use uuid::Uuid;
use chrono::Utc;

// ─────────────────────────────────────────────
//  Incident Types
// ─────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum IncidentSeverity {
    Critical,
    High,
    Medium,
    Low,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum IncidentStatus {
    Open,
    Investigating,
    Mitigating,
    Resolved,
    Closed,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IncidentUpdate {
    pub id: String,
    pub status: IncidentStatus,
    pub message: String,
    pub author: String,
    pub timestamp: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Incident {
    pub id: String,
    pub title: String,
    pub description: String,
    pub severity: IncidentSeverity,
    pub status: IncidentStatus,
    pub assigned_device_id: Option<String>,
    pub assigned_group_id: Option<String>,
    pub source: String,                   // "manual", "automation", "sla_breach", "alert"
    pub source_id: Option<String>,        // related alert/event ID
    pub affected_device_ids: Vec<String>,
    pub tags: Vec<String>,
    pub updates: Vec<IncidentUpdate>,
    pub created_at: String,
    pub updated_at: String,
    pub resolved_at: Option<String>,
    pub resolution_notes: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IncidentMetrics {
    pub total_incidents: u32,
    pub open_incidents: u32,
    pub investigating: u32,
    pub mitigating: u32,
    pub resolved_today: u32,
    pub by_severity: Vec<(String, u32)>,
    pub avg_resolution_mins: f64,
    pub mttr_mins: f64, // mean time to resolve
}

// ─────────────────────────────────────────────
//  Store
// ─────────────────────────────────────────────

pub struct FleetIncidentStore {
    pub incidents: Vec<Incident>,
}

impl Default for FleetIncidentStore {
    fn default() -> Self {
        let now = Utc::now().to_rfc3339();
        Self {
            incidents: vec![
                Incident {
                    id: "inc-001".into(),
                    title: "High latency on payment processing node".into(),
                    description: "Payment processing device reporting >500ms response times, affecting order throughput.".into(),
                    severity: IncidentSeverity::High,
                    status: IncidentStatus::Open,
                    assigned_device_id: None,
                    assigned_group_id: Some("grp-production".into()),
                    source: "alert".into(),
                    source_id: None,
                    affected_device_ids: vec!["dev-001".into()],
                    tags: vec!["performance".into(), "payment".into()],
                    updates: vec![],
                    created_at: now.clone(),
                    updated_at: now.clone(),
                    resolved_at: None,
                    resolution_notes: None,
                },
                Incident {
                    id: "inc-002".into(),
                    title: "Staging environment unreachable".into(),
                    description: "All staging devices are offline after network configuration change.".into(),
                    severity: IncidentSeverity::Medium,
                    status: IncidentStatus::Investigating,
                    assigned_device_id: None,
                    assigned_group_id: Some("grp-staging".into()),
                    source: "manual".into(),
                    source_id: None,
                    affected_device_ids: vec!["dev-003".into(), "dev-004".into()],
                    tags: vec!["network".into(), "staging".into()],
                    updates: vec![
                        IncidentUpdate {
                            id: "upd-001".into(),
                            status: IncidentStatus::Investigating,
                            message: "Network team investigating firewall rule changes.".into(),
                            author: "system".into(),
                            timestamp: now.clone(),
                        },
                    ],
                    created_at: now.clone(),
                    updated_at: now,
                    resolved_at: None,
                    resolution_notes: None,
                },
            ],
        }
    }
}

impl FleetIncidentStore {
    // ── CRUD ──────────────────────────────────

    pub fn list_incidents(&self, status_filter: Option<&str>, severity_filter: Option<&str>) -> Vec<Incident> {
        self.incidents.iter()
            .filter(|i| {
                if let Some(s) = status_filter {
                    if format!("{:?}", i.status) != s { return false; }
                }
                if let Some(sv) = severity_filter {
                    if format!("{:?}", i.severity) != sv { return false; }
                }
                true
            })
            .cloned()
            .collect()
    }

    pub fn list_active_incidents(&self) -> Vec<Incident> {
        self.incidents.iter()
            .filter(|i| !matches!(i.status, IncidentStatus::Resolved | IncidentStatus::Closed))
            .cloned()
            .collect()
    }

    pub fn list_resolved_incidents(&self, limit: usize) -> Vec<Incident> {
        let mut resolved: Vec<Incident> = self.incidents.iter()
            .filter(|i| matches!(i.status, IncidentStatus::Resolved | IncidentStatus::Closed))
            .cloned()
            .collect();
        resolved.sort_by(|a, b| b.resolved_at.cmp(&a.resolved_at));
        resolved.truncate(limit);
        resolved
    }

    pub fn get_incident(&self, id: &str) -> Option<Incident> {
        self.incidents.iter().find(|i| i.id == id).cloned()
    }

    pub fn create_incident(&mut self, mut incident: Incident) -> Incident {
        if incident.id.is_empty() { incident.id = format!("inc-{}", Uuid::new_v4()); }
        let now = Utc::now().to_rfc3339();
        if incident.created_at.is_empty() { incident.created_at = now.clone(); }
        incident.updated_at = now;
        self.incidents.push(incident.clone());
        incident
    }

    pub fn update_incident_status(&mut self, id: &str, status: IncidentStatus, message: String, author: String) -> Option<Incident> {
        if let Some(inc) = self.incidents.iter_mut().find(|i| i.id == id) {
            let now = Utc::now().to_rfc3339();
            let update = IncidentUpdate {
                id: Uuid::new_v4().to_string(),
                status: status.clone(),
                message,
                author,
                timestamp: now.clone(),
            };
            inc.updates.push(update);
            inc.status = status.clone();
            inc.updated_at = now.clone();
            if matches!(status, IncidentStatus::Resolved | IncidentStatus::Closed) {
                inc.resolved_at = Some(now);
            }
            Some(inc.clone())
        } else {
            None
        }
    }

    pub fn assign_incident(&mut self, id: &str, device_id: Option<String>, group_id: Option<String>) -> Option<Incident> {
        if let Some(inc) = self.incidents.iter_mut().find(|i| i.id == id) {
            inc.assigned_device_id = device_id;
            inc.assigned_group_id = group_id;
            inc.updated_at = Utc::now().to_rfc3339();
            Some(inc.clone())
        } else {
            None
        }
    }

    pub fn resolve_incident(&mut self, id: &str, notes: String) -> Option<Incident> {
        self.update_incident_status(id, IncidentStatus::Resolved, notes.clone(), "system".into())?;
        if let Some(inc) = self.incidents.iter_mut().find(|i| i.id == id) {
            inc.resolution_notes = Some(notes);
            Some(inc.clone())
        } else {
            None
        }
    }

    pub fn close_incident(&mut self, id: &str) -> Option<Incident> {
        self.update_incident_status(id, IncidentStatus::Closed, "Incident closed.".into(), "system".into())
    }

    pub fn delete_incident(&mut self, id: &str) -> bool {
        let len = self.incidents.len();
        self.incidents.retain(|i| i.id != id);
        self.incidents.len() < len
    }

    pub fn add_affected_device(&mut self, id: &str, device_id: &str) -> bool {
        if let Some(inc) = self.incidents.iter_mut().find(|i| i.id == id) {
            if !inc.affected_device_ids.contains(&device_id.to_string()) {
                inc.affected_device_ids.push(device_id.to_string());
                inc.updated_at = Utc::now().to_rfc3339();
            }
            true
        } else {
            false
        }
    }

    // ── Metrics ──────────────────────────────

    pub fn get_metrics(&self) -> IncidentMetrics {
        let total = self.incidents.len() as u32;
        let open = self.incidents.iter().filter(|i| i.status == IncidentStatus::Open).count() as u32;
        let investigating = self.incidents.iter().filter(|i| i.status == IncidentStatus::Investigating).count() as u32;
        let mitigating = self.incidents.iter().filter(|i| i.status == IncidentStatus::Mitigating).count() as u32;

        let today = Utc::now().format("%Y-%m-%d").to_string();
        let resolved_today = self.incidents.iter()
            .filter(|i| i.resolved_at.as_ref().map(|r| r.starts_with(&today)).unwrap_or(false))
            .count() as u32;

        let severities = ["Critical", "High", "Medium", "Low"];
        let by_severity: Vec<(String, u32)> = severities.iter().map(|s| {
            let count = self.incidents.iter().filter(|i| format!("{:?}", i.severity) == *s).count() as u32;
            (s.to_string(), count)
        }).collect();

        // MTTR: average time from created_at to resolved_at for resolved incidents
        let resolved: Vec<&Incident> = self.incidents.iter()
            .filter(|i| i.resolved_at.is_some())
            .collect();

        let avg_resolution_mins = if resolved.is_empty() {
            0.0
        } else {
            let total_mins: f64 = resolved.iter().filter_map(|i| {
                let created = chrono::DateTime::parse_from_rfc3339(&i.created_at).ok()?;
                let resolved = chrono::DateTime::parse_from_rfc3339(i.resolved_at.as_ref()?).ok()?;
                Some((resolved - created).num_minutes() as f64)
            }).sum();
            total_mins / resolved.len() as f64
        };

        IncidentMetrics {
            total_incidents: total,
            open_incidents: open,
            investigating,
            mitigating,
            resolved_today,
            by_severity,
            avg_resolution_mins,
            mttr_mins: avg_resolution_mins,
        }
    }
}
