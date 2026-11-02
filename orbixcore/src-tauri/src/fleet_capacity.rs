use serde::{Deserialize, Serialize};
use uuid::Uuid;
use chrono::Utc;

// ─────────────────────────────────────────────
//  Capacity Types
// ─────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CapacitySnapshot {
    pub id: String,
    pub device_id: String,
    pub device_name: String,
    pub cpu_percent: f32,
    pub memory_percent: f32,
    pub disk_percent: f32,
    pub workload_percent: f32,
    pub active_connections: u32,
    pub queue_depth: u32,
    pub timestamp: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum ThresholdLevel {
    Warning,
    Critical,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CapacityThreshold {
    pub id: String,
    pub name: String,
    pub resource: String,       // "cpu", "memory", "disk", "workload"
    pub level: ThresholdLevel,
    pub value: f32,             // percentage
    pub enabled: bool,
    pub created_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CapacityAlert {
    pub id: String,
    pub device_id: String,
    pub device_name: String,
    pub resource: String,
    pub current_value: f32,
    pub threshold_value: f32,
    pub level: ThresholdLevel,
    pub acknowledged: bool,
    pub timestamp: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FleetCapacitySummary {
    pub total_devices: u32,
    pub avg_cpu: f32,
    pub avg_memory: f32,
    pub avg_disk: f32,
    pub avg_workload: f32,
    pub devices_over_warning: u32,
    pub devices_over_critical: u32,
    pub unacknowledged_alerts: u32,
    pub highest_cpu_device: Option<String>,
    pub highest_memory_device: Option<String>,
}

// ─────────────────────────────────────────────
//  Store
// ─────────────────────────────────────────────

pub struct FleetCapacityStore {
    pub snapshots: Vec<CapacitySnapshot>,
    pub thresholds: Vec<CapacityThreshold>,
    pub alerts: Vec<CapacityAlert>,
}

impl Default for FleetCapacityStore {
    fn default() -> Self {
        let now = Utc::now().to_rfc3339();
        Self {
            snapshots: vec![
                CapacitySnapshot {
                    id: "snap-001".into(), device_id: "dev-001".into(),
                    device_name: "Mini PC - Sales Agent".into(),
                    cpu_percent: 42.5, memory_percent: 61.0, disk_percent: 38.2,
                    workload_percent: 55.0, active_connections: 12, queue_depth: 3,
                    timestamp: now.clone(),
                },
                CapacitySnapshot {
                    id: "snap-002".into(), device_id: "dev-002".into(),
                    device_name: "Mini PC - Support Bot".into(),
                    cpu_percent: 78.3, memory_percent: 72.5, disk_percent: 45.0,
                    workload_percent: 80.0, active_connections: 24, queue_depth: 8,
                    timestamp: now.clone(),
                },
                CapacitySnapshot {
                    id: "snap-003".into(), device_id: "dev-003".into(),
                    device_name: "Mini PC - Data Processor".into(),
                    cpu_percent: 15.0, memory_percent: 28.0, disk_percent: 52.1,
                    workload_percent: 20.0, active_connections: 4, queue_depth: 0,
                    timestamp: now.clone(),
                },
            ],
            thresholds: vec![
                CapacityThreshold {
                    id: "thr-cpu-warn".into(), name: "CPU Warning".into(),
                    resource: "cpu".into(), level: ThresholdLevel::Warning,
                    value: 70.0, enabled: true, created_at: now.clone(),
                },
                CapacityThreshold {
                    id: "thr-cpu-crit".into(), name: "CPU Critical".into(),
                    resource: "cpu".into(), level: ThresholdLevel::Critical,
                    value: 90.0, enabled: true, created_at: now.clone(),
                },
                CapacityThreshold {
                    id: "thr-mem-warn".into(), name: "Memory Warning".into(),
                    resource: "memory".into(), level: ThresholdLevel::Warning,
                    value: 75.0, enabled: true, created_at: now.clone(),
                },
                CapacityThreshold {
                    id: "thr-mem-crit".into(), name: "Memory Critical".into(),
                    resource: "memory".into(), level: ThresholdLevel::Critical,
                    value: 95.0, enabled: true, created_at: now.clone(),
                },
                CapacityThreshold {
                    id: "thr-disk-warn".into(), name: "Disk Warning".into(),
                    resource: "disk".into(), level: ThresholdLevel::Warning,
                    value: 80.0, enabled: true, created_at: now.clone(),
                },
                CapacityThreshold {
                    id: "thr-wl-warn".into(), name: "Workload Warning".into(),
                    resource: "workload".into(), level: ThresholdLevel::Warning,
                    value: 85.0, enabled: true, created_at: now,
                },
            ],
            alerts: vec![],
        }
    }
}

impl FleetCapacityStore {
    // ── Snapshots ────────────────────────────

    pub fn record_snapshot(&mut self, mut snap: CapacitySnapshot) -> CapacitySnapshot {
        if snap.id.is_empty() { snap.id = format!("snap-{}", Uuid::new_v4()); }
        if snap.timestamp.is_empty() { snap.timestamp = Utc::now().to_rfc3339(); }
        // Replace existing snapshot for same device (keep latest)
        self.snapshots.retain(|s| s.device_id != snap.device_id);
        self.snapshots.push(snap.clone());
        // Check thresholds
        self.check_thresholds(&snap);
        snap
    }

    pub fn list_snapshots(&self) -> Vec<CapacitySnapshot> {
        self.snapshots.clone()
    }

    pub fn get_device_snapshot(&self, device_id: &str) -> Option<CapacitySnapshot> {
        self.snapshots.iter().find(|s| s.device_id == device_id).cloned()
    }

    // ── Thresholds ───────────────────────────

    pub fn list_thresholds(&self) -> Vec<CapacityThreshold> {
        self.thresholds.clone()
    }

    pub fn create_threshold(&mut self, mut thr: CapacityThreshold) -> CapacityThreshold {
        if thr.id.is_empty() { thr.id = format!("thr-{}", Uuid::new_v4()); }
        if thr.created_at.is_empty() { thr.created_at = Utc::now().to_rfc3339(); }
        self.thresholds.push(thr.clone());
        thr
    }

    pub fn update_threshold(&mut self, thr: CapacityThreshold) -> Option<CapacityThreshold> {
        if let Some(t) = self.thresholds.iter_mut().find(|t| t.id == thr.id) {
            t.name = thr.name;
            t.resource = thr.resource;
            t.level = thr.level;
            t.value = thr.value;
            t.enabled = thr.enabled;
            Some(t.clone())
        } else { None }
    }

    pub fn toggle_threshold(&mut self, id: &str) -> Option<CapacityThreshold> {
        if let Some(t) = self.thresholds.iter_mut().find(|t| t.id == id) {
            t.enabled = !t.enabled;
            Some(t.clone())
        } else { None }
    }

    pub fn delete_threshold(&mut self, id: &str) -> bool {
        let len = self.thresholds.len();
        self.thresholds.retain(|t| t.id != id);
        self.thresholds.len() < len
    }

    // ── Alerts ───────────────────────────────

    fn check_thresholds(&mut self, snap: &CapacitySnapshot) {
        let now = Utc::now().to_rfc3339();
        for thr in &self.thresholds {
            if !thr.enabled { continue; }
            let current = match thr.resource.as_str() {
                "cpu" => snap.cpu_percent,
                "memory" => snap.memory_percent,
                "disk" => snap.disk_percent,
                "workload" => snap.workload_percent,
                _ => continue,
            };
            if current >= thr.value {
                // Don't duplicate alerts for same device+resource+level
                let exists = self.alerts.iter().any(|a| {
                    a.device_id == snap.device_id && a.resource == thr.resource
                        && a.level == thr.level && !a.acknowledged
                });
                if !exists {
                    self.alerts.push(CapacityAlert {
                        id: format!("calert-{}", Uuid::new_v4()),
                        device_id: snap.device_id.clone(),
                        device_name: snap.device_name.clone(),
                        resource: thr.resource.clone(),
                        current_value: current,
                        threshold_value: thr.value,
                        level: thr.level.clone(),
                        acknowledged: false,
                        timestamp: now.clone(),
                    });
                }
            }
        }
    }

    pub fn list_alerts(&self, unacked_only: bool) -> Vec<CapacityAlert> {
        self.alerts.iter()
            .filter(|a| !unacked_only || !a.acknowledged)
            .cloned()
            .collect()
    }

    pub fn acknowledge_alert(&mut self, id: &str) -> bool {
        if let Some(a) = self.alerts.iter_mut().find(|a| a.id == id) {
            a.acknowledged = true;
            true
        } else { false }
    }

    pub fn acknowledge_all_alerts(&mut self) {
        for a in &mut self.alerts { a.acknowledged = true; }
    }

    // ── Summary ──────────────────────────────

    pub fn get_summary(&self) -> FleetCapacitySummary {
        let total = self.snapshots.len() as u32;
        if total == 0 {
            return FleetCapacitySummary {
                total_devices: 0, avg_cpu: 0.0, avg_memory: 0.0, avg_disk: 0.0,
                avg_workload: 0.0, devices_over_warning: 0, devices_over_critical: 0,
                unacknowledged_alerts: 0, highest_cpu_device: None, highest_memory_device: None,
            };
        }

        let avg_cpu = self.snapshots.iter().map(|s| s.cpu_percent).sum::<f32>() / total as f32;
        let avg_memory = self.snapshots.iter().map(|s| s.memory_percent).sum::<f32>() / total as f32;
        let avg_disk = self.snapshots.iter().map(|s| s.disk_percent).sum::<f32>() / total as f32;
        let avg_workload = self.snapshots.iter().map(|s| s.workload_percent).sum::<f32>() / total as f32;

        let warn_thresh = self.thresholds.iter()
            .filter(|t| t.enabled && t.level == ThresholdLevel::Warning)
            .map(|t| t.value)
            .fold(f32::MAX, f32::min);
        let crit_thresh = self.thresholds.iter()
            .filter(|t| t.enabled && t.level == ThresholdLevel::Critical)
            .map(|t| t.value)
            .fold(f32::MAX, f32::min);

        let over_warning = self.snapshots.iter()
            .filter(|s| s.cpu_percent >= warn_thresh || s.memory_percent >= warn_thresh)
            .count() as u32;
        let over_critical = self.snapshots.iter()
            .filter(|s| s.cpu_percent >= crit_thresh || s.memory_percent >= crit_thresh)
            .count() as u32;

        let highest_cpu = self.snapshots.iter()
            .max_by(|a, b| a.cpu_percent.partial_cmp(&b.cpu_percent).unwrap())
            .map(|s| s.device_name.clone());
        let highest_mem = self.snapshots.iter()
            .max_by(|a, b| a.memory_percent.partial_cmp(&b.memory_percent).unwrap())
            .map(|s| s.device_name.clone());

        let unacked = self.alerts.iter().filter(|a| !a.acknowledged).count() as u32;

        FleetCapacitySummary {
            total_devices: total, avg_cpu, avg_memory, avg_disk, avg_workload,
            devices_over_warning: over_warning, devices_over_critical: over_critical,
            unacknowledged_alerts: unacked,
            highest_cpu_device: highest_cpu, highest_memory_device: highest_mem,
        }
    }
}
