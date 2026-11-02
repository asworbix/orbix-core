use serde::{Deserialize, Serialize};
use uuid::Uuid;
use chrono::Utc;
use std::collections::HashMap;

// ─────────────────────────────────────────────
//  Tags
// ─────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FleetTag {
    pub id: String,
    pub name: String,
    pub color: String,          // hex color e.g. "#6366f1"
    pub description: String,
    pub auto_rule: Option<String>,  // auto-tag rule expression
    pub created_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TagAssignment {
    pub tag_id: String,
    pub resource_type: ResourceType,    // what kind of resource
    pub resource_id: String,
    pub assigned_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum ResourceType {
    Device,
    WorkItem,
    Playbook,
    RoutingRule,
}

// ─────────────────────────────────────────────
//  Filters
// ─────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SavedFilter {
    pub id: String,
    pub name: String,
    pub description: String,
    pub criteria: Vec<FilterCriterion>,
    pub logic: FilterLogic,
    pub created_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum FilterCriterion {
    HasTag(String),                     // tag_id
    HasAnyTag(Vec<String>),             // any of these tag_ids
    Status(String),
    Role(String),
    TextContains(String),
    Priority(String),
    CreatedAfter(String),
    CreatedBefore(String),
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum FilterLogic {
    And,
    Or,
}

// ─────────────────────────────────────────────
//  Analytics
// ─────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TagUsage {
    pub tag_id: String,
    pub tag_name: String,
    pub color: String,
    pub device_count: u32,
    pub work_item_count: u32,
    pub playbook_count: u32,
    pub routing_rule_count: u32,
    pub total_count: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TagAnalytics {
    pub total_tags: u32,
    pub total_assignments: u32,
    pub most_used: Option<String>,      // tag name
    pub least_used: Option<String>,     // tag name
    pub untagged_devices: u32,
    pub usage: Vec<TagUsage>,
}

// ─────────────────────────────────────────────
//  Store
// ─────────────────────────────────────────────

pub struct FleetTagStore {
    pub tags: Vec<FleetTag>,
    pub assignments: Vec<TagAssignment>,
    pub saved_filters: Vec<SavedFilter>,
}

impl Default for FleetTagStore {
    fn default() -> Self {
        Self {
            tags: vec![
                FleetTag { id: "tag-prod".into(), name: "Production".into(), color: "#10b981".into(), description: "Production environment devices".into(), auto_rule: None, created_at: Utc::now().to_rfc3339() },
                FleetTag { id: "tag-staging".into(), name: "Staging".into(), color: "#fbbf24".into(), description: "Staging/test environment".into(), auto_rule: None, created_at: Utc::now().to_rfc3339() },
                FleetTag { id: "tag-critical".into(), name: "Critical".into(), color: "#ef4444".into(), description: "Critical priority resources".into(), auto_rule: None, created_at: Utc::now().to_rfc3339() },
                FleetTag { id: "tag-automated".into(), name: "Automated".into(), color: "#6366f1".into(), description: "Fully automated resources".into(), auto_rule: None, created_at: Utc::now().to_rfc3339() },
                FleetTag { id: "tag-review".into(), name: "Needs Review".into(), color: "#f97316".into(), description: "Resources flagged for review".into(), auto_rule: None, created_at: Utc::now().to_rfc3339() },
            ],
            assignments: Vec::new(),
            saved_filters: Vec::new(),
        }
    }
}

impl FleetTagStore {
    // ── Tag CRUD ───────────────────────────────

    pub fn list_tags(&self) -> Vec<FleetTag> {
        self.tags.clone()
    }

    pub fn create_tag(&mut self, mut tag: FleetTag) -> FleetTag {
        if tag.id.is_empty() {
            tag.id = Uuid::new_v4().to_string();
        }
        if tag.created_at.is_empty() {
            tag.created_at = Utc::now().to_rfc3339();
        }
        self.tags.push(tag.clone());
        tag
    }

    pub fn update_tag(&mut self, tag: FleetTag) -> Option<FleetTag> {
        if let Some(existing) = self.tags.iter_mut().find(|t| t.id == tag.id) {
            existing.name = tag.name;
            existing.color = tag.color;
            existing.description = tag.description;
            existing.auto_rule = tag.auto_rule;
            Some(existing.clone())
        } else {
            None
        }
    }

    pub fn delete_tag(&mut self, id: &str) -> bool {
        let len = self.tags.len();
        self.tags.retain(|t| t.id != id);
        self.assignments.retain(|a| a.tag_id != id);
        self.tags.len() < len
    }

    // ── Assignments ────────────────────────────

    pub fn assign_tag(&mut self, tag_id: &str, resource_type: ResourceType, resource_id: &str) -> bool {
        if !self.tags.iter().any(|t| t.id == tag_id) { return false; }
        // Prevent duplicates
        if self.assignments.iter().any(|a| a.tag_id == tag_id && a.resource_id == resource_id && a.resource_type == resource_type) {
            return true;
        }
        self.assignments.push(TagAssignment {
            tag_id: tag_id.into(),
            resource_type,
            resource_id: resource_id.into(),
            assigned_at: Utc::now().to_rfc3339(),
        });
        true
    }

    pub fn remove_tag_assignment(&mut self, tag_id: &str, resource_type: ResourceType, resource_id: &str) -> bool {
        let len = self.assignments.len();
        self.assignments.retain(|a| !(a.tag_id == tag_id && a.resource_id == resource_id && a.resource_type == resource_type));
        self.assignments.len() < len
    }

    pub fn get_resource_tags(&self, resource_type: ResourceType, resource_id: &str) -> Vec<FleetTag> {
        let tag_ids: Vec<&str> = self.assignments.iter()
            .filter(|a| a.resource_type == resource_type && a.resource_id == resource_id)
            .map(|a| a.tag_id.as_str())
            .collect();
        self.tags.iter().filter(|t| tag_ids.contains(&t.id.as_str())).cloned().collect()
    }

    pub fn get_resources_by_tag(&self, tag_id: &str, resource_type: Option<ResourceType>) -> Vec<String> {
        self.assignments.iter()
            .filter(|a| a.tag_id == tag_id && resource_type.as_ref().map(|rt| a.resource_type == *rt).unwrap_or(true))
            .map(|a| a.resource_id.clone())
            .collect()
    }

    pub fn bulk_assign_tag(&mut self, tag_id: &str, resource_type: ResourceType, resource_ids: &[String]) -> u32 {
        let mut count = 0u32;
        for rid in resource_ids {
            if self.assign_tag(tag_id, resource_type.clone(), rid) {
                count += 1;
            }
        }
        count
    }

    pub fn bulk_remove_tag(&mut self, tag_id: &str, resource_type: ResourceType, resource_ids: &[String]) -> u32 {
        let mut count = 0u32;
        for rid in resource_ids {
            if self.remove_tag_assignment(tag_id, resource_type.clone(), rid) {
                count += 1;
            }
        }
        count
    }

    // ── Saved Filters ──────────────────────────

    pub fn list_saved_filters(&self) -> Vec<SavedFilter> {
        self.saved_filters.clone()
    }

    pub fn create_saved_filter(&mut self, mut filter: SavedFilter) -> SavedFilter {
        if filter.id.is_empty() {
            filter.id = Uuid::new_v4().to_string();
        }
        if filter.created_at.is_empty() {
            filter.created_at = Utc::now().to_rfc3339();
        }
        self.saved_filters.push(filter.clone());
        filter
    }

    pub fn delete_saved_filter(&mut self, id: &str) -> bool {
        let len = self.saved_filters.len();
        self.saved_filters.retain(|f| f.id != id);
        self.saved_filters.len() < len
    }

    // ── Analytics ──────────────────────────────

    pub fn get_analytics(&self, total_devices: u32) -> TagAnalytics {
        let mut usage_map: HashMap<&str, (u32, u32, u32, u32)> = HashMap::new();
        for tag in &self.tags {
            usage_map.insert(&tag.id, (0, 0, 0, 0));
        }
        for a in &self.assignments {
            if let Some(counts) = usage_map.get_mut(a.tag_id.as_str()) {
                match a.resource_type {
                    ResourceType::Device => counts.0 += 1,
                    ResourceType::WorkItem => counts.1 += 1,
                    ResourceType::Playbook => counts.2 += 1,
                    ResourceType::RoutingRule => counts.3 += 1,
                }
            }
        }

        let usage: Vec<TagUsage> = self.tags.iter().map(|t| {
            let (d, w, p, r) = usage_map.get(t.id.as_str()).copied().unwrap_or((0, 0, 0, 0));
            TagUsage {
                tag_id: t.id.clone(),
                tag_name: t.name.clone(),
                color: t.color.clone(),
                device_count: d,
                work_item_count: w,
                playbook_count: p,
                routing_rule_count: r,
                total_count: d + w + p + r,
            }
        }).collect();

        let most_used = usage.iter().max_by_key(|u| u.total_count).map(|u| u.tag_name.clone());
        let least_used = usage.iter().filter(|u| u.total_count > 0).min_by_key(|u| u.total_count).map(|u| u.tag_name.clone());

        let tagged_devices: std::collections::HashSet<&str> = self.assignments.iter()
            .filter(|a| a.resource_type == ResourceType::Device)
            .map(|a| a.resource_id.as_str())
            .collect();
        let untagged_devices = total_devices.saturating_sub(tagged_devices.len() as u32);

        TagAnalytics {
            total_tags: self.tags.len() as u32,
            total_assignments: self.assignments.len() as u32,
            most_used,
            least_used,
            untagged_devices,
            usage,
        }
    }
}
