use serde::{Deserialize, Serialize};
use uuid::Uuid;
use chrono::Utc;

// ─────────────────────────────────────────────
//  Device Groups
// ─────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DeviceGroup {
    pub id: String,
    pub name: String,
    pub description: String,
    pub color: String,
    pub icon: String,                       // emoji or icon name
    pub auto_rule: Option<GroupAutoRule>,    // smart group rule
    pub member_ids: Vec<String>,            // device IDs
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GroupAutoRule {
    pub match_type: GroupMatchType,
    pub value: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum GroupMatchType {
    Role,
    Tag,
    Status,
    NameContains,
}

// ─────────────────────────────────────────────
//  Group Metrics
// ─────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GroupMetrics {
    pub group_id: String,
    pub group_name: String,
    pub color: String,
    pub total_devices: u32,
    pub online_devices: u32,
    pub busy_devices: u32,
    pub total_work_items: u32,
    pub items_completed: u32,
    pub items_failed: u32,
    pub avg_workload: f32,
}

// ─────────────────────────────────────────────
//  Store
// ─────────────────────────────────────────────

pub struct FleetGroupStore {
    pub groups: Vec<DeviceGroup>,
}

impl Default for FleetGroupStore {
    fn default() -> Self {
        let now = Utc::now().to_rfc3339();
        Self {
            groups: vec![
                DeviceGroup {
                    id: "grp-production".into(),
                    name: "Production".into(),
                    description: "All production environment devices".into(),
                    color: "#10b981".into(),
                    icon: "server".into(),
                    auto_rule: None,
                    member_ids: Vec::new(),
                    created_at: now.clone(),
                    updated_at: now.clone(),
                },
                DeviceGroup {
                    id: "grp-staging".into(),
                    name: "Staging".into(),
                    description: "Staging and test devices".into(),
                    color: "#fbbf24".into(),
                    icon: "flask".into(),
                    auto_rule: None,
                    member_ids: Vec::new(),
                    created_at: now.clone(),
                    updated_at: now.clone(),
                },
                DeviceGroup {
                    id: "grp-critical".into(),
                    name: "Critical Path".into(),
                    description: "Devices on the critical processing path".into(),
                    color: "#ef4444".into(),
                    icon: "alert".into(),
                    auto_rule: None,
                    member_ids: Vec::new(),
                    created_at: now.clone(),
                    updated_at: now,
                },
            ],
        }
    }
}

impl FleetGroupStore {
    // ── Group CRUD ─────────────────────────────

    pub fn list_groups(&self) -> Vec<DeviceGroup> {
        self.groups.clone()
    }

    pub fn get_group(&self, id: &str) -> Option<DeviceGroup> {
        self.groups.iter().find(|g| g.id == id).cloned()
    }

    pub fn create_group(&mut self, mut group: DeviceGroup) -> DeviceGroup {
        if group.id.is_empty() { group.id = Uuid::new_v4().to_string(); }
        let now = Utc::now().to_rfc3339();
        if group.created_at.is_empty() { group.created_at = now.clone(); }
        group.updated_at = now;
        self.groups.push(group.clone());
        group
    }

    pub fn update_group(&mut self, group: DeviceGroup) -> Option<DeviceGroup> {
        if let Some(g) = self.groups.iter_mut().find(|g| g.id == group.id) {
            g.name = group.name;
            g.description = group.description;
            g.color = group.color;
            g.icon = group.icon;
            g.auto_rule = group.auto_rule;
            g.updated_at = Utc::now().to_rfc3339();
            Some(g.clone())
        } else {
            None
        }
    }

    pub fn delete_group(&mut self, id: &str) -> bool {
        let len = self.groups.len();
        self.groups.retain(|g| g.id != id);
        self.groups.len() < len
    }

    // ── Membership ─────────────────────────────

    pub fn add_device_to_group(&mut self, group_id: &str, device_id: &str) -> bool {
        if let Some(g) = self.groups.iter_mut().find(|g| g.id == group_id) {
            if !g.member_ids.contains(&device_id.to_string()) {
                g.member_ids.push(device_id.to_string());
                g.updated_at = Utc::now().to_rfc3339();
            }
            true
        } else {
            false
        }
    }

    pub fn remove_device_from_group(&mut self, group_id: &str, device_id: &str) -> bool {
        if let Some(g) = self.groups.iter_mut().find(|g| g.id == group_id) {
            let len = g.member_ids.len();
            g.member_ids.retain(|d| d != device_id);
            if g.member_ids.len() < len {
                g.updated_at = Utc::now().to_rfc3339();
                return true;
            }
        }
        false
    }

    pub fn add_devices_to_group(&mut self, group_id: &str, device_ids: &[String]) -> u32 {
        let mut count = 0u32;
        for did in device_ids {
            if self.add_device_to_group(group_id, did) { count += 1; }
        }
        count
    }

    pub fn get_device_groups(&self, device_id: &str) -> Vec<DeviceGroup> {
        self.groups.iter()
            .filter(|g| g.member_ids.contains(&device_id.to_string()))
            .cloned()
            .collect()
    }

    pub fn get_group_members(&self, group_id: &str) -> Vec<String> {
        self.groups.iter()
            .find(|g| g.id == group_id)
            .map(|g| g.member_ids.clone())
            .unwrap_or_default()
    }

    // ── Metrics ────────────────────────────────

    pub fn compute_group_metrics(
        &self,
        group_id: &str,
        devices: &[(String, String, u32, u32, u32, u32, u32)], // (id, status, workload, items_total, completed, failed, max_workload)
    ) -> Option<GroupMetrics> {
        let group = self.groups.iter().find(|g| g.id == group_id)?;
        let members: Vec<&(String, String, u32, u32, u32, u32, u32)> = devices.iter()
            .filter(|(id, _, _, _, _, _, _)| group.member_ids.contains(id))
            .collect();

        let total = members.len() as u32;
        let online = members.iter().filter(|(_, s, _, _, _, _, _)| s == "Online").count() as u32;
        let busy = members.iter().filter(|(_, s, _, _, _, _, _)| s == "Busy").count() as u32;
        let total_items: u32 = members.iter().map(|(_, _, _, t, _, _, _)| t).sum();
        let completed: u32 = members.iter().map(|(_, _, _, _, c, _, _)| c).sum();
        let failed: u32 = members.iter().map(|(_, _, _, _, _, f, _)| f).sum();
        let avg_wl = if total > 0 {
            members.iter().map(|(_, _, w, _, _, _, m)| if *m > 0 { *w as f32 / *m as f32 * 100.0 } else { 0.0 }).sum::<f32>() / total as f32
        } else { 0.0 };

        Some(GroupMetrics {
            group_id: group.id.clone(),
            group_name: group.name.clone(),
            color: group.color.clone(),
            total_devices: total,
            online_devices: online,
            busy_devices: busy,
            total_work_items: total_items,
            items_completed: completed,
            items_failed: failed,
            avg_workload: avg_wl,
        })
    }

    pub fn compute_all_metrics(
        &self,
        devices: &[(String, String, u32, u32, u32, u32, u32)],
    ) -> Vec<GroupMetrics> {
        self.groups.iter()
            .filter_map(|g| self.compute_group_metrics(&g.id, devices))
            .collect()
    }
}
