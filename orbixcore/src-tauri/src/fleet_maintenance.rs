use serde::{Deserialize, Serialize};
use uuid::Uuid;
use chrono::Utc;

// ─────────────────────────────────────────────
//  Maintenance Types
// ─────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum MaintenanceStatus {
    Scheduled,
    Active,
    Completed,
    Cancelled,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum RecurrenceType {
    None,
    Daily,
    Weekly,
    Monthly,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MaintenanceWindow {
    pub id: String,
    pub title: String,
    pub description: String,
    pub status: MaintenanceStatus,
    pub start_time: String,           // RFC3339
    pub end_time: String,             // RFC3339
    pub recurrence: RecurrenceType,
    pub affected_device_ids: Vec<String>,
    pub affected_group_ids: Vec<String>,
    pub suppress_alerts: bool,
    pub pause_sla: bool,
    pub created_by: String,
    pub notes: Vec<MaintenanceNote>,
    pub created_at: String,
    pub updated_at: String,
    pub completed_at: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MaintenanceNote {
    pub id: String,
    pub message: String,
    pub author: String,
    pub timestamp: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MaintenanceSummary {
    pub total_windows: u32,
    pub scheduled: u32,
    pub active: u32,
    pub completed: u32,
    pub cancelled: u32,
    pub total_affected_devices: u32,
    pub upcoming_next_24h: u32,
}

// ─────────────────────────────────────────────
//  Store
// ─────────────────────────────────────────────

pub struct FleetMaintenanceStore {
    pub windows: Vec<MaintenanceWindow>,
}

impl Default for FleetMaintenanceStore {
    fn default() -> Self {
        let now = Utc::now();
        let now_str = now.to_rfc3339();
        // Create a demo scheduled window 2 hours from now
        let start = (now + chrono::Duration::hours(2)).to_rfc3339();
        let end = (now + chrono::Duration::hours(4)).to_rfc3339();

        Self {
            windows: vec![
                MaintenanceWindow {
                    id: "maint-001".into(),
                    title: "Production node firmware update".into(),
                    description: "Rolling firmware update across production mini PCs. Expected 15min downtime per device.".into(),
                    status: MaintenanceStatus::Scheduled,
                    start_time: start,
                    end_time: end,
                    recurrence: RecurrenceType::None,
                    affected_device_ids: vec!["dev-001".into(), "dev-002".into()],
                    affected_group_ids: vec!["grp-production".into()],
                    suppress_alerts: true,
                    pause_sla: true,
                    created_by: "admin".into(),
                    notes: vec![],
                    created_at: now_str.clone(),
                    updated_at: now_str.clone(),
                    completed_at: None,
                },
                MaintenanceWindow {
                    id: "maint-002".into(),
                    title: "Weekly staging reset".into(),
                    description: "Automated weekly reset of staging environment databases and caches.".into(),
                    status: MaintenanceStatus::Scheduled,
                    start_time: (now + chrono::Duration::days(1)).to_rfc3339(),
                    end_time: (now + chrono::Duration::days(1) + chrono::Duration::hours(1)).to_rfc3339(),
                    recurrence: RecurrenceType::Weekly,
                    affected_device_ids: vec![],
                    affected_group_ids: vec!["grp-staging".into()],
                    suppress_alerts: true,
                    pause_sla: false,
                    created_by: "admin".into(),
                    notes: vec![],
                    created_at: now_str.clone(),
                    updated_at: now_str,
                    completed_at: None,
                },
            ],
        }
    }
}

impl FleetMaintenanceStore {
    // ── CRUD ──────────────────────────────────

    pub fn list_windows(&self, status_filter: Option<&str>) -> Vec<MaintenanceWindow> {
        self.windows.iter()
            .filter(|w| {
                if let Some(s) = status_filter {
                    format!("{:?}", w.status) == s
                } else {
                    true
                }
            })
            .cloned()
            .collect()
    }

    pub fn list_upcoming(&self) -> Vec<MaintenanceWindow> {
        let mut upcoming: Vec<MaintenanceWindow> = self.windows.iter()
            .filter(|w| matches!(w.status, MaintenanceStatus::Scheduled))
            .cloned()
            .collect();
        upcoming.sort_by(|a, b| a.start_time.cmp(&b.start_time));
        upcoming
    }

    pub fn list_active(&self) -> Vec<MaintenanceWindow> {
        self.windows.iter()
            .filter(|w| matches!(w.status, MaintenanceStatus::Active))
            .cloned()
            .collect()
    }

    pub fn list_completed(&self, limit: usize) -> Vec<MaintenanceWindow> {
        let mut completed: Vec<MaintenanceWindow> = self.windows.iter()
            .filter(|w| matches!(w.status, MaintenanceStatus::Completed | MaintenanceStatus::Cancelled))
            .cloned()
            .collect();
        completed.sort_by(|a, b| b.completed_at.cmp(&a.completed_at));
        completed.truncate(limit);
        completed
    }

    pub fn get_window(&self, id: &str) -> Option<MaintenanceWindow> {
        self.windows.iter().find(|w| w.id == id).cloned()
    }

    pub fn create_window(&mut self, mut window: MaintenanceWindow) -> MaintenanceWindow {
        if window.id.is_empty() { window.id = format!("maint-{}", Uuid::new_v4()); }
        let now = Utc::now().to_rfc3339();
        if window.created_at.is_empty() { window.created_at = now.clone(); }
        window.updated_at = now;
        if window.status != MaintenanceStatus::Active {
            window.status = MaintenanceStatus::Scheduled;
        }
        self.windows.push(window.clone());
        window
    }

    pub fn update_window(&mut self, window: MaintenanceWindow) -> Option<MaintenanceWindow> {
        if let Some(w) = self.windows.iter_mut().find(|w| w.id == window.id) {
            w.title = window.title;
            w.description = window.description;
            w.start_time = window.start_time;
            w.end_time = window.end_time;
            w.recurrence = window.recurrence;
            w.affected_device_ids = window.affected_device_ids;
            w.affected_group_ids = window.affected_group_ids;
            w.suppress_alerts = window.suppress_alerts;
            w.pause_sla = window.pause_sla;
            w.updated_at = Utc::now().to_rfc3339();
            Some(w.clone())
        } else {
            None
        }
    }

    pub fn delete_window(&mut self, id: &str) -> bool {
        let len = self.windows.len();
        self.windows.retain(|w| w.id != id);
        self.windows.len() < len
    }

    // ── Status Transitions ───────────────────

    pub fn activate_window(&mut self, id: &str) -> Option<MaintenanceWindow> {
        if let Some(w) = self.windows.iter_mut().find(|w| w.id == id) {
            w.status = MaintenanceStatus::Active;
            w.updated_at = Utc::now().to_rfc3339();
            Some(w.clone())
        } else {
            None
        }
    }

    pub fn complete_window(&mut self, id: &str) -> Option<MaintenanceWindow> {
        if let Some(w) = self.windows.iter_mut().find(|w| w.id == id) {
            let now = Utc::now().to_rfc3339();
            w.status = MaintenanceStatus::Completed;
            w.completed_at = Some(now.clone());
            w.updated_at = now;
            Some(w.clone())
        } else {
            None
        }
    }

    pub fn cancel_window(&mut self, id: &str) -> Option<MaintenanceWindow> {
        if let Some(w) = self.windows.iter_mut().find(|w| w.id == id) {
            let now = Utc::now().to_rfc3339();
            w.status = MaintenanceStatus::Cancelled;
            w.completed_at = Some(now.clone());
            w.updated_at = now;
            Some(w.clone())
        } else {
            None
        }
    }

    pub fn add_note(&mut self, id: &str, message: String, author: String) -> Option<MaintenanceWindow> {
        if let Some(w) = self.windows.iter_mut().find(|w| w.id == id) {
            w.notes.push(MaintenanceNote {
                id: Uuid::new_v4().to_string(),
                message,
                author,
                timestamp: Utc::now().to_rfc3339(),
            });
            w.updated_at = Utc::now().to_rfc3339();
            Some(w.clone())
        } else {
            None
        }
    }

    // ── Queries ──────────────────────────────

    pub fn is_device_in_maintenance(&self, device_id: &str) -> bool {
        self.windows.iter().any(|w| {
            w.status == MaintenanceStatus::Active
                && (w.affected_device_ids.contains(&device_id.to_string()))
        })
    }

    pub fn get_summary(&self) -> MaintenanceSummary {
        let now = Utc::now();
        let next_24h = (now + chrono::Duration::hours(24)).to_rfc3339();
        let now_str = now.to_rfc3339();

        let upcoming_24h = self.windows.iter()
            .filter(|w| w.status == MaintenanceStatus::Scheduled && w.start_time <= next_24h && w.start_time >= now_str)
            .count() as u32;

        let active_devices: std::collections::HashSet<&String> = self.windows.iter()
            .filter(|w| w.status == MaintenanceStatus::Active)
            .flat_map(|w| &w.affected_device_ids)
            .collect();

        MaintenanceSummary {
            total_windows: self.windows.len() as u32,
            scheduled: self.windows.iter().filter(|w| w.status == MaintenanceStatus::Scheduled).count() as u32,
            active: self.windows.iter().filter(|w| w.status == MaintenanceStatus::Active).count() as u32,
            completed: self.windows.iter().filter(|w| w.status == MaintenanceStatus::Completed).count() as u32,
            cancelled: self.windows.iter().filter(|w| w.status == MaintenanceStatus::Cancelled).count() as u32,
            total_affected_devices: active_devices.len() as u32,
            upcoming_next_24h: upcoming_24h,
        }
    }
}
