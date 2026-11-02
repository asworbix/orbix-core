use serde::{Deserialize, Serialize};
use uuid::Uuid;
use chrono::Utc;
use std::collections::HashMap;

// ─────────────────────────────────────────────
//  Activity Feed
// ─────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum ActivitySeverity {
    Info,
    Success,
    Warning,
    Error,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ActivityEvent {
    pub id: String,
    pub event_type: String,      // "work_created", "work_assigned", "work_completed", "work_failed",
                                  // "handoff", "device_online", "device_offline", "route_triggered",
                                  // "escalation", "sla_breach"
    pub severity: ActivitySeverity,
    pub title: String,
    pub details: String,
    pub device_id: Option<String>,
    pub device_name: Option<String>,
    pub role: Option<String>,
    pub item_id: Option<String>,
    pub timestamp: String,
}

// ─────────────────────────────────────────────
//  Fleet Map
// ─────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MapNode {
    pub device_id: String,
    pub name: String,
    pub role: String,
    pub status: String,         // "Online", "Offline", "Busy", "Error"
    pub active_items: u32,
    pub completed_today: u32,
    pub x: f32,                 // layout position
    pub y: f32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MapEdge {
    pub from_role: String,
    pub to_role: String,
    pub rule_name: String,
    pub items_routed: u32,
    pub active: bool,           // has items currently flowing
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FleetMap {
    pub nodes: Vec<MapNode>,
    pub edges: Vec<MapEdge>,
}

// ─────────────────────────────────────────────
//  Fleet Health
// ─────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DeviceHealth {
    pub device_id: String,
    pub device_name: String,
    pub role: String,
    pub uptime_pct: f32,
    pub items_processed_today: u32,
    pub items_per_hour: f32,
    pub avg_processing_mins: f32,
    pub active_items: u32,
    pub failed_items: u32,
    pub is_bottleneck: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FleetHealthSummary {
    pub total_throughput_today: u32,
    pub avg_items_per_hour: f32,
    pub avg_processing_mins: f32,
    pub bottleneck_role: Option<String>,
    pub bottleneck_device: Option<String>,
    pub devices: Vec<DeviceHealth>,
}

// ─────────────────────────────────────────────
//  Fleet Presets
// ─────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FleetPresetRole {
    pub name: String,
    pub description: String,
    pub device_type: String,
    pub max_workload: u32,
    pub tags: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FleetPresetRoute {
    pub from_role: String,
    pub to_role: String,
    pub condition: String,    // serialized condition description
    pub strategy: String,     // "LeastBusy", "RoundRobin", etc.
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FleetPreset {
    pub id: String,
    pub name: String,
    pub description: String,
    pub category: String,     // "sales", "support", "dev", "custom"
    pub roles: Vec<FleetPresetRole>,
    pub routes: Vec<FleetPresetRoute>,
    pub is_builtin: bool,
    pub created_at: String,
}

// ─────────────────────────────────────────────
//  Store
// ─────────────────────────────────────────────

const MAX_EVENTS: usize = 500;

pub struct FleetMonitorStore {
    pub events: Vec<ActivityEvent>,
    pub presets: Vec<FleetPreset>,
    pub device_uptime: HashMap<String, (u64, u64)>, // device_id -> (online_secs, total_secs)
}

impl Default for FleetMonitorStore {
    fn default() -> Self {
        Self {
            events: Vec::new(),
            presets: Self::builtin_presets(),
            device_uptime: HashMap::new(),
        }
    }
}

impl FleetMonitorStore {
    fn builtin_presets() -> Vec<FleetPreset> {
        vec![
            FleetPreset {
                id: "preset-sales-team".to_string(),
                name: "Sales Team".to_string(),
                description: "Lead generation, qualification, and account management pipeline".to_string(),
                category: "sales".to_string(),
                roles: vec![
                    FleetPresetRole { name: "Lead Generator".to_string(), description: "Finds and captures inbound leads".to_string(), device_type: "mac_mini".to_string(), max_workload: 20, tags: vec!["sales".to_string(), "inbound".to_string()] },
                    FleetPresetRole { name: "Qualifier".to_string(), description: "Qualifies leads and scores opportunities".to_string(), device_type: "mac_mini".to_string(), max_workload: 15, tags: vec!["sales".to_string(), "qualification".to_string()] },
                    FleetPresetRole { name: "Account Manager".to_string(), description: "Manages qualified accounts and closes deals".to_string(), device_type: "mac_mini".to_string(), max_workload: 10, tags: vec!["sales".to_string(), "closing".to_string()] },
                ],
                routes: vec![
                    FleetPresetRoute { from_role: "Lead Generator".to_string(), to_role: "Qualifier".to_string(), condition: "Always".to_string(), strategy: "LeastBusy".to_string() },
                    FleetPresetRoute { from_role: "Qualifier".to_string(), to_role: "Account Manager".to_string(), condition: "Status = Completed".to_string(), strategy: "LeastBusy".to_string() },
                ],
                is_builtin: true,
                created_at: "2025-01-01T00:00:00Z".to_string(),
            },
            FleetPreset {
                id: "preset-support-desk".to_string(),
                name: "Support Desk".to_string(),
                description: "Tiered support with triage, L1, and L2 escalation".to_string(),
                category: "support".to_string(),
                roles: vec![
                    FleetPresetRole { name: "Triage Bot".to_string(), description: "Classifies and prioritizes incoming tickets".to_string(), device_type: "mac_mini".to_string(), max_workload: 50, tags: vec!["support".to_string(), "triage".to_string()] },
                    FleetPresetRole { name: "L1 Support".to_string(), description: "Handles common issues and FAQ responses".to_string(), device_type: "mac_mini".to_string(), max_workload: 20, tags: vec!["support".to_string(), "l1".to_string()] },
                    FleetPresetRole { name: "L2 Specialist".to_string(), description: "Handles complex technical issues".to_string(), device_type: "mac_mini".to_string(), max_workload: 8, tags: vec!["support".to_string(), "l2".to_string()] },
                ],
                routes: vec![
                    FleetPresetRoute { from_role: "Triage Bot".to_string(), to_role: "L1 Support".to_string(), condition: "Always".to_string(), strategy: "RoundRobin".to_string() },
                    FleetPresetRoute { from_role: "L1 Support".to_string(), to_role: "L2 Specialist".to_string(), condition: "Priority ≥ High".to_string(), strategy: "LeastBusy".to_string() },
                ],
                is_builtin: true,
                created_at: "2025-01-01T00:00:00Z".to_string(),
            },
            FleetPreset {
                id: "preset-dev-pipeline".to_string(),
                name: "Dev Pipeline".to_string(),
                description: "Code review, testing, and deployment automation".to_string(),
                category: "dev".to_string(),
                roles: vec![
                    FleetPresetRole { name: "Code Reviewer".to_string(), description: "Reviews PRs and provides feedback".to_string(), device_type: "mac_mini".to_string(), max_workload: 15, tags: vec!["dev".to_string(), "review".to_string()] },
                    FleetPresetRole { name: "Test Runner".to_string(), description: "Runs test suites and reports results".to_string(), device_type: "mac_mini".to_string(), max_workload: 25, tags: vec!["dev".to_string(), "testing".to_string()] },
                    FleetPresetRole { name: "Deploy Agent".to_string(), description: "Handles staging and production deployments".to_string(), device_type: "mac_mini".to_string(), max_workload: 5, tags: vec!["dev".to_string(), "deploy".to_string()] },
                ],
                routes: vec![
                    FleetPresetRoute { from_role: "Code Reviewer".to_string(), to_role: "Test Runner".to_string(), condition: "Status = Completed".to_string(), strategy: "FirstAvailable".to_string() },
                    FleetPresetRoute { from_role: "Test Runner".to_string(), to_role: "Deploy Agent".to_string(), condition: "Status = Completed".to_string(), strategy: "FirstAvailable".to_string() },
                ],
                is_builtin: true,
                created_at: "2025-01-01T00:00:00Z".to_string(),
            },
            FleetPreset {
                id: "preset-content-factory".to_string(),
                name: "Content Factory".to_string(),
                description: "Content creation, editing, and publishing pipeline".to_string(),
                category: "marketing".to_string(),
                roles: vec![
                    FleetPresetRole { name: "Research Agent".to_string(), description: "Gathers data and research for content".to_string(), device_type: "mac_mini".to_string(), max_workload: 10, tags: vec!["content".to_string(), "research".to_string()] },
                    FleetPresetRole { name: "Writer".to_string(), description: "Creates drafts from research briefs".to_string(), device_type: "mac_mini".to_string(), max_workload: 8, tags: vec!["content".to_string(), "writing".to_string()] },
                    FleetPresetRole { name: "Editor".to_string(), description: "Reviews and polishes content".to_string(), device_type: "mac_mini".to_string(), max_workload: 12, tags: vec!["content".to_string(), "editing".to_string()] },
                    FleetPresetRole { name: "Publisher".to_string(), description: "Formats and publishes to channels".to_string(), device_type: "mac_mini".to_string(), max_workload: 15, tags: vec!["content".to_string(), "publish".to_string()] },
                ],
                routes: vec![
                    FleetPresetRoute { from_role: "Research Agent".to_string(), to_role: "Writer".to_string(), condition: "Always".to_string(), strategy: "LeastBusy".to_string() },
                    FleetPresetRoute { from_role: "Writer".to_string(), to_role: "Editor".to_string(), condition: "Status = Completed".to_string(), strategy: "RoundRobin".to_string() },
                    FleetPresetRoute { from_role: "Editor".to_string(), to_role: "Publisher".to_string(), condition: "Status = Completed".to_string(), strategy: "FirstAvailable".to_string() },
                ],
                is_builtin: true,
                created_at: "2025-01-01T00:00:00Z".to_string(),
            },
        ]
    }

    // ── Activity Feed ───────────────────────

    pub fn log_event(&mut self, mut event: ActivityEvent) -> ActivityEvent {
        if event.id.is_empty() {
            event.id = Uuid::new_v4().to_string();
        }
        if event.timestamp.is_empty() {
            event.timestamp = Utc::now().to_rfc3339();
        }
        self.events.push(event.clone());
        if self.events.len() > MAX_EVENTS {
            self.events.drain(0..self.events.len() - MAX_EVENTS);
        }
        event
    }

    pub fn list_events(
        &self,
        device_id: Option<&str>,
        role: Option<&str>,
        event_type: Option<&str>,
        severity: Option<&str>,
        limit: usize,
    ) -> Vec<ActivityEvent> {
        self.events.iter().rev()
            .filter(|e| {
                if let Some(did) = device_id {
                    if e.device_id.as_deref() != Some(did) { return false; }
                }
                if let Some(r) = role {
                    if e.role.as_deref() != Some(r) { return false; }
                }
                if let Some(et) = event_type {
                    if e.event_type != et { return false; }
                }
                if let Some(s) = severity {
                    let sev = format!("{:?}", e.severity);
                    if !sev.eq_ignore_ascii_case(s) { return false; }
                }
                true
            })
            .take(limit)
            .cloned()
            .collect()
    }

    pub fn clear_events(&mut self) {
        self.events.clear();
    }

    // ── Fleet Map ───────────────────────────

    pub fn build_map(
        &self,
        devices: &[(String, String, String, String, u32, u32)], // (id, name, role, status, active, completed)
        routes: &[(String, String, String, u32, bool)],          // (from_role, to_role, rule_name, items_routed, active)
    ) -> FleetMap {
        let node_count = devices.len();
        let nodes: Vec<MapNode> = devices.iter().enumerate().map(|(i, (id, name, role, status, active, completed))| {
            // Arrange in a circle
            let angle = (i as f32 / node_count as f32) * std::f32::consts::PI * 2.0;
            let cx = 200.0;
            let cy = 150.0;
            let radius = 120.0;
            MapNode {
                device_id: id.clone(),
                name: name.clone(),
                role: role.clone(),
                status: status.clone(),
                active_items: *active,
                completed_today: *completed,
                x: cx + angle.cos() * radius,
                y: cy + angle.sin() * radius,
            }
        }).collect();

        let edges: Vec<MapEdge> = routes.iter().map(|(from, to, name, count, active)| {
            MapEdge {
                from_role: from.clone(),
                to_role: to.clone(),
                rule_name: name.clone(),
                items_routed: *count,
                active: *active,
            }
        }).collect();

        FleetMap { nodes, edges }
    }

    // ── Fleet Health ────────────────────────

    pub fn compute_health(
        &self,
        devices: &[(String, String, String, u32, u32, u32, f32)], // (id, name, role, active, completed, failed, avg_mins)
    ) -> FleetHealthSummary {
        let total: u32 = devices.iter().map(|d| d.4).sum();
        let total_per_hour: f32 = if devices.is_empty() { 0.0 } else {
            devices.iter().map(|d| d.4 as f32 / 8.0).sum() // assume 8-hour workday
        };
        let avg_proc: f32 = if devices.is_empty() { 0.0 } else {
            devices.iter().map(|d| d.6).sum::<f32>() / devices.len() as f32
        };

        let mut health_devs: Vec<DeviceHealth> = devices.iter().map(|(id, name, role, active, completed, failed, avg_mins)| {
            DeviceHealth {
                device_id: id.clone(),
                device_name: name.clone(),
                role: role.clone(),
                uptime_pct: self.device_uptime.get(id)
                    .map(|(on, total)| if *total > 0 { (*on as f32 / *total as f32) * 100.0 } else { 100.0 })
                    .unwrap_or(100.0),
                items_processed_today: *completed,
                items_per_hour: *completed as f32 / 8.0,
                avg_processing_mins: *avg_mins,
                active_items: *active,
                failed_items: *failed,
                is_bottleneck: false,
            }
        }).collect();

        // Find bottleneck: device with highest avg processing time and active items
        let mut bottleneck_role = None;
        let mut bottleneck_device = None;
        if let Some(worst) = health_devs.iter_mut()
            .filter(|d| d.active_items > 0)
            .max_by(|a, b| a.avg_processing_mins.partial_cmp(&b.avg_processing_mins).unwrap_or(std::cmp::Ordering::Equal))
        {
            worst.is_bottleneck = true;
            bottleneck_role = Some(worst.role.clone());
            bottleneck_device = Some(worst.device_name.clone());
        }

        FleetHealthSummary {
            total_throughput_today: total,
            avg_items_per_hour: total_per_hour,
            avg_processing_mins: avg_proc,
            bottleneck_role,
            bottleneck_device,
            devices: health_devs,
        }
    }

    // ── Presets ──────────────────────────────

    pub fn list_presets(&self) -> Vec<FleetPreset> {
        self.presets.clone()
    }

    pub fn get_preset(&self, id: &str) -> Option<FleetPreset> {
        self.presets.iter().find(|p| p.id == id).cloned()
    }

    pub fn create_preset(&mut self, mut preset: FleetPreset) -> FleetPreset {
        if preset.id.is_empty() {
            preset.id = Uuid::new_v4().to_string();
        }
        if preset.created_at.is_empty() {
            preset.created_at = Utc::now().to_rfc3339();
        }
        preset.is_builtin = false;
        self.presets.push(preset.clone());
        preset
    }

    pub fn delete_preset(&mut self, id: &str) -> bool {
        let len = self.presets.len();
        self.presets.retain(|p| p.id != id || p.is_builtin);
        self.presets.len() < len
    }
}
