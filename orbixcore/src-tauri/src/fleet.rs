use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use uuid::Uuid;
use chrono::Utc;

// ─────────────────────────────────────────────
//  Fleet Device
// ─────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum DeviceStatus {
    Online,
    Offline,
    Busy,
    Error,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FleetDevice {
    pub id: String,
    pub name: String,
    pub hostname: String,
    pub role: String,           // e.g. "Sales Agent", "Support Bot"
    pub role_description: String,
    pub device_type: String,    // "mac_mini", "macbook", "mac_pro", "ios"
    pub ip_address: String,
    pub port: u16,              // mobile API port
    pub parent_id: Option<String>,
    pub status: DeviceStatus,
    pub current_workload: u32,
    pub max_workload: u32,
    pub active_chains: Vec<String>,   // chain IDs running
    pub active_workflows: Vec<String>,
    pub tags: Vec<String>,
    pub registered_at: String,
    pub last_heartbeat: String,
    pub metadata: HashMap<String, String>,
}

// ─────────────────────────────────────────────
//  Work Items
// ─────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum WorkItemStatus {
    Queued,
    Assigned,
    InProgress,
    HandedOff,
    Completed,
    Failed,
    Blocked,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum WorkItemPriority {
    Low,
    Normal,
    High,
    Urgent,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WorkItemEvent {
    pub timestamp: String,
    pub event_type: String,   // "created", "assigned", "started", "handed_off", "completed", "failed"
    pub device_id: Option<String>,
    pub details: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WorkItem {
    pub id: String,
    pub title: String,
    pub description: String,
    pub source: String,           // "trigger", "manual", "chain", "handoff"
    pub source_id: Option<String>,
    pub assigned_device: Option<String>,
    pub assigned_role: Option<String>,
    pub status: WorkItemStatus,
    pub priority: WorkItemPriority,
    pub chain_id: Option<String>,
    pub workflow_id: Option<String>,
    pub parent_item_id: Option<String>,   // for sub-items
    pub tags: Vec<String>,
    pub sla_deadline: Option<String>,
    pub timeline: Vec<WorkItemEvent>,
    pub result: Option<String>,
    pub created_at: String,
    pub updated_at: String,
}

// ─────────────────────────────────────────────
//  Fleet Orchestration View
// ─────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DeviceActivity {
    pub device_id: String,
    pub device_name: String,
    pub role: String,
    pub status: DeviceStatus,
    pub running_chains: Vec<ChainSummary>,
    pub running_workflows: Vec<WorkflowSummary>,
    pub work_items_active: u32,
    pub work_items_completed_today: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChainSummary {
    pub chain_id: String,
    pub chain_name: String,
    pub current_step: String,
    pub progress_pct: u8,
    pub started_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WorkflowSummary {
    pub workflow_id: String,
    pub workflow_name: String,
    pub status: String,
    pub started_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FleetOverview {
    pub total_devices: usize,
    pub online_devices: usize,
    pub busy_devices: usize,
    pub total_work_items: usize,
    pub items_in_progress: usize,
    pub items_completed_today: usize,
    pub items_queued: usize,
    pub items_blocked: usize,
    pub active_chains: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HierarchyNode {
    pub device: FleetDevice,
    pub children: Vec<HierarchyNode>,
    pub active_items: usize,
}

// ─────────────────────────────────────────────
//  Fleet Store
// ─────────────────────────────────────────────

pub struct FleetStore {
    pub devices: Vec<FleetDevice>,
    pub work_items: Vec<WorkItem>,
}

impl Default for FleetStore {
    fn default() -> Self {
        Self {
            devices: Vec::new(),
            work_items: Vec::new(),
        }
    }
}

impl FleetStore {
    // ── Device Management ───────────────────

    pub fn list_devices(&self) -> Vec<FleetDevice> {
        self.devices.clone()
    }

    pub fn get_device(&self, id: &str) -> Option<FleetDevice> {
        self.devices.iter().find(|d| d.id == id).cloned()
    }

    pub fn register_device(&mut self, mut device: FleetDevice) -> FleetDevice {
        if device.id.is_empty() {
            device.id = Uuid::new_v4().to_string();
        }
        if device.registered_at.is_empty() {
            device.registered_at = Utc::now().to_rfc3339();
        }
        device.last_heartbeat = Utc::now().to_rfc3339();
        // Update if exists, else add
        if let Some(pos) = self.devices.iter().position(|d| d.id == device.id) {
            self.devices[pos] = device.clone();
        } else {
            self.devices.push(device.clone());
        }
        device
    }

    pub fn remove_device(&mut self, id: &str) -> bool {
        let len = self.devices.len();
        self.devices.retain(|d| d.id != id);
        // Also unparent any children
        for d in &mut self.devices {
            if d.parent_id.as_deref() == Some(id) {
                d.parent_id = None;
            }
        }
        self.devices.len() < len
    }

    pub fn update_device_status(&mut self, id: &str, status: DeviceStatus) -> Option<FleetDevice> {
        if let Some(dev) = self.devices.iter_mut().find(|d| d.id == id) {
            dev.status = status;
            dev.last_heartbeat = Utc::now().to_rfc3339();
            Some(dev.clone())
        } else {
            None
        }
    }

    pub fn heartbeat(&mut self, id: &str, workload: u32, chains: Vec<String>, workflows: Vec<String>) -> Option<FleetDevice> {
        if let Some(dev) = self.devices.iter_mut().find(|d| d.id == id) {
            dev.last_heartbeat = Utc::now().to_rfc3339();
            dev.current_workload = workload;
            dev.active_chains = chains;
            dev.active_workflows = workflows;
            if dev.status == DeviceStatus::Offline {
                dev.status = DeviceStatus::Online;
            }
            Some(dev.clone())
        } else {
            None
        }
    }

    pub fn set_device_parent(&mut self, id: &str, parent_id: Option<&str>) -> Option<FleetDevice> {
        if let Some(dev) = self.devices.iter_mut().find(|d| d.id == id) {
            dev.parent_id = parent_id.map(|s| s.to_string());
            Some(dev.clone())
        } else {
            None
        }
    }

    pub fn get_hierarchy(&self) -> Vec<HierarchyNode> {
        // Build tree from root nodes (no parent)
        let roots: Vec<&FleetDevice> = self.devices.iter()
            .filter(|d| d.parent_id.is_none())
            .collect();

        roots.iter().map(|d| self.build_node(d)).collect()
    }

    fn build_node(&self, device: &FleetDevice) -> HierarchyNode {
        let children: Vec<HierarchyNode> = self.devices.iter()
            .filter(|d| d.parent_id.as_deref() == Some(&device.id))
            .map(|d| self.build_node(d))
            .collect();
        let active = self.work_items.iter()
            .filter(|w| w.assigned_device.as_deref() == Some(&device.id)
                && matches!(w.status, WorkItemStatus::InProgress | WorkItemStatus::Assigned))
            .count();
        HierarchyNode {
            device: device.clone(),
            children,
            active_items: active,
        }
    }

    // ── Work Items ──────────────────────────

    pub fn list_work_items(&self, status: Option<&str>, device_id: Option<&str>, role: Option<&str>) -> Vec<WorkItem> {
        self.work_items.iter().filter(|w| {
            if let Some(s) = status {
                let ws = format!("{:?}", w.status);
                if !ws.eq_ignore_ascii_case(s) { return false; }
            }
            if let Some(did) = device_id {
                if w.assigned_device.as_deref() != Some(did) { return false; }
            }
            if let Some(r) = role {
                if w.assigned_role.as_deref() != Some(r) { return false; }
            }
            true
        }).cloned().collect()
    }

    pub fn get_work_item(&self, id: &str) -> Option<WorkItem> {
        self.work_items.iter().find(|w| w.id == id).cloned()
    }

    pub fn create_work_item(&mut self, mut item: WorkItem) -> WorkItem {
        if item.id.is_empty() {
            item.id = Uuid::new_v4().to_string();
        }
        let now = Utc::now().to_rfc3339();
        if item.created_at.is_empty() {
            item.created_at = now.clone();
        }
        item.updated_at = now.clone();
        item.timeline.push(WorkItemEvent {
            timestamp: now,
            event_type: "created".to_string(),
            device_id: None,
            details: "Work item created".to_string(),
        });
        self.work_items.push(item.clone());
        item
    }

    pub fn assign_work_item(&mut self, item_id: &str, device_id: &str) -> Option<WorkItem> {
        let device_name = self.devices.iter()
            .find(|d| d.id == device_id)
            .map(|d| d.name.clone())
            .unwrap_or_default();
        let device_role = self.devices.iter()
            .find(|d| d.id == device_id)
            .map(|d| d.role.clone());

        if let Some(item) = self.work_items.iter_mut().find(|w| w.id == item_id) {
            item.assigned_device = Some(device_id.to_string());
            item.assigned_role = device_role;
            item.status = WorkItemStatus::Assigned;
            item.updated_at = Utc::now().to_rfc3339();
            item.timeline.push(WorkItemEvent {
                timestamp: Utc::now().to_rfc3339(),
                event_type: "assigned".to_string(),
                device_id: Some(device_id.to_string()),
                details: format!("Assigned to {}", device_name),
            });
            Some(item.clone())
        } else {
            None
        }
    }

    pub fn update_work_item_status(&mut self, item_id: &str, status: WorkItemStatus, details: Option<&str>) -> Option<WorkItem> {
        if let Some(item) = self.work_items.iter_mut().find(|w| w.id == item_id) {
            let event_type = format!("{:?}", status).to_lowercase();
            item.status = status;
            item.updated_at = Utc::now().to_rfc3339();
            item.timeline.push(WorkItemEvent {
                timestamp: Utc::now().to_rfc3339(),
                event_type,
                device_id: item.assigned_device.clone(),
                details: details.unwrap_or("Status updated").to_string(),
            });
            Some(item.clone())
        } else {
            None
        }
    }

    pub fn handoff_work_item(&mut self, item_id: &str, from_device: &str, to_device: &str) -> Option<WorkItem> {
        let to_name = self.devices.iter()
            .find(|d| d.id == to_device)
            .map(|d| d.name.clone())
            .unwrap_or_default();
        let to_role = self.devices.iter()
            .find(|d| d.id == to_device)
            .map(|d| d.role.clone());

        if let Some(item) = self.work_items.iter_mut().find(|w| w.id == item_id) {
            item.status = WorkItemStatus::HandedOff;
            item.updated_at = Utc::now().to_rfc3339();
            item.timeline.push(WorkItemEvent {
                timestamp: Utc::now().to_rfc3339(),
                event_type: "handed_off".to_string(),
                device_id: Some(from_device.to_string()),
                details: format!("Handed off to {} ({})", to_name, to_device),
            });
            // Re-assign
            item.assigned_device = Some(to_device.to_string());
            item.assigned_role = to_role;
            item.status = WorkItemStatus::Assigned;
            item.timeline.push(WorkItemEvent {
                timestamp: Utc::now().to_rfc3339(),
                event_type: "assigned".to_string(),
                device_id: Some(to_device.to_string()),
                details: format!("Received handoff from {}", from_device),
            });
            Some(item.clone())
        } else {
            None
        }
    }

    pub fn complete_work_item(&mut self, item_id: &str, result: Option<&str>) -> Option<WorkItem> {
        if let Some(item) = self.work_items.iter_mut().find(|w| w.id == item_id) {
            item.status = WorkItemStatus::Completed;
            item.result = result.map(|s| s.to_string());
            item.updated_at = Utc::now().to_rfc3339();
            item.timeline.push(WorkItemEvent {
                timestamp: Utc::now().to_rfc3339(),
                event_type: "completed".to_string(),
                device_id: item.assigned_device.clone(),
                details: result.unwrap_or("Completed").to_string(),
            });
            Some(item.clone())
        } else {
            None
        }
    }

    // ── Fleet Overview ──────────────────────

    pub fn get_overview(&self) -> FleetOverview {
        let today = Utc::now().format("%Y-%m-%d").to_string();
        FleetOverview {
            total_devices: self.devices.len(),
            online_devices: self.devices.iter().filter(|d| d.status == DeviceStatus::Online || d.status == DeviceStatus::Busy).count(),
            busy_devices: self.devices.iter().filter(|d| d.status == DeviceStatus::Busy).count(),
            total_work_items: self.work_items.len(),
            items_in_progress: self.work_items.iter().filter(|w| w.status == WorkItemStatus::InProgress).count(),
            items_completed_today: self.work_items.iter()
                .filter(|w| w.status == WorkItemStatus::Completed && w.updated_at.starts_with(&today))
                .count(),
            items_queued: self.work_items.iter().filter(|w| w.status == WorkItemStatus::Queued).count(),
            items_blocked: self.work_items.iter().filter(|w| w.status == WorkItemStatus::Blocked).count(),
            active_chains: self.devices.iter().map(|d| d.active_chains.len()).sum(),
        }
    }

    pub fn get_device_activity(&self) -> Vec<DeviceActivity> {
        let today = Utc::now().format("%Y-%m-%d").to_string();
        self.devices.iter().map(|d| {
            let active = self.work_items.iter()
                .filter(|w| w.assigned_device.as_deref() == Some(&d.id)
                    && matches!(w.status, WorkItemStatus::InProgress | WorkItemStatus::Assigned))
                .count() as u32;
            let completed = self.work_items.iter()
                .filter(|w| w.assigned_device.as_deref() == Some(&d.id)
                    && w.status == WorkItemStatus::Completed
                    && w.updated_at.starts_with(&today))
                .count() as u32;
            DeviceActivity {
                device_id: d.id.clone(),
                device_name: d.name.clone(),
                role: d.role.clone(),
                status: d.status.clone(),
                running_chains: d.active_chains.iter().map(|cid| ChainSummary {
                    chain_id: cid.clone(),
                    chain_name: cid.clone(),
                    current_step: "Running".to_string(),
                    progress_pct: 50,
                    started_at: d.last_heartbeat.clone(),
                }).collect(),
                running_workflows: d.active_workflows.iter().map(|wid| WorkflowSummary {
                    workflow_id: wid.clone(),
                    workflow_name: wid.clone(),
                    status: "running".to_string(),
                    started_at: d.last_heartbeat.clone(),
                }).collect(),
                work_items_active: active,
                work_items_completed_today: completed,
            }
        }).collect()
    }
}
