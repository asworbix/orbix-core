use serde::{Deserialize, Serialize};
use uuid::Uuid;
use chrono::Utc;

// ─────────────────────────────────────────────
//  Timeline Events
// ─────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum TimelineCategory {
    DeviceEvent,
    WorkItemLifecycle,
    SlaEvent,
    AutomationTrigger,
    ComplianceChange,
    RoutingActivity,
    SystemEvent,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum TimelineSeverity {
    Info,
    Success,
    Warning,
    Error,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TimelineEvent {
    pub id: String,
    pub category: TimelineCategory,
    pub severity: TimelineSeverity,
    pub title: String,
    pub description: String,
    pub source_system: String,          // "routing", "sla", "automation", "audit", etc.
    pub source_id: Option<String>,      // ID in the source system
    pub device_id: Option<String>,
    pub device_name: Option<String>,
    pub work_item_id: Option<String>,
    pub related_event_ids: Vec<String>,  // correlated events
    pub bookmarked: bool,
    pub tags: Vec<String>,
    pub timestamp: String,
}

// ─────────────────────────────────────────────
//  Timeline Stats
// ─────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TimelineStats {
    pub total_events: u32,
    pub events_today: u32,
    pub bookmarked: u32,
    pub by_category: Vec<(String, u32)>,
    pub by_severity: Vec<(String, u32)>,
    pub most_active_device: Option<String>,
}

// ─────────────────────────────────────────────
//  Store
// ─────────────────────────────────────────────

pub struct FleetTimelineStore {
    pub events: Vec<TimelineEvent>,
    pub max_events: usize,
}

impl Default for FleetTimelineStore {
    fn default() -> Self {
        Self {
            events: Vec::new(),
            max_events: 5000,
        }
    }
}

impl FleetTimelineStore {
    // ── Events ─────────────────────────────────

    pub fn push_event(&mut self, mut event: TimelineEvent) -> TimelineEvent {
        if event.id.is_empty() {
            event.id = Uuid::new_v4().to_string();
        }
        if event.timestamp.is_empty() {
            event.timestamp = Utc::now().to_rfc3339();
        }
        self.events.push(event.clone());
        if self.events.len() > self.max_events {
            self.events.drain(0..self.events.len() - self.max_events);
        }
        event
    }

    pub fn list_events(
        &self,
        category: Option<&str>,
        severity: Option<&str>,
        device_id: Option<&str>,
        source_system: Option<&str>,
        bookmarked_only: bool,
        limit: usize,
    ) -> Vec<TimelineEvent> {
        self.events.iter().rev()
            .filter(|e| {
                if let Some(cat) = category {
                    let cat_str = format!("{:?}", e.category);
                    if !cat_str.eq_ignore_ascii_case(cat) { return false; }
                }
                if let Some(sev) = severity {
                    let sev_str = format!("{:?}", e.severity);
                    if !sev_str.eq_ignore_ascii_case(sev) { return false; }
                }
                if let Some(did) = device_id {
                    if e.device_id.as_deref() != Some(did) { return false; }
                }
                if let Some(src) = source_system {
                    if e.source_system != src { return false; }
                }
                if bookmarked_only && !e.bookmarked { return false; }
                true
            })
            .take(limit)
            .cloned()
            .collect()
    }

    pub fn search_events(&self, query: &str, limit: usize) -> Vec<TimelineEvent> {
        let q = query.to_lowercase();
        self.events.iter().rev()
            .filter(|e| {
                e.title.to_lowercase().contains(&q)
                    || e.description.to_lowercase().contains(&q)
                    || e.source_system.to_lowercase().contains(&q)
                    || e.device_name.as_ref().map(|n| n.to_lowercase().contains(&q)).unwrap_or(false)
                    || e.tags.iter().any(|t| t.to_lowercase().contains(&q))
            })
            .take(limit)
            .cloned()
            .collect()
    }

    // ── Bookmarks ──────────────────────────────

    pub fn toggle_bookmark(&mut self, id: &str) -> Option<TimelineEvent> {
        if let Some(e) = self.events.iter_mut().find(|e| e.id == id) {
            e.bookmarked = !e.bookmarked;
            Some(e.clone())
        } else {
            None
        }
    }

    pub fn list_bookmarks(&self) -> Vec<TimelineEvent> {
        self.events.iter().filter(|e| e.bookmarked).rev().cloned().collect()
    }

    // ── Correlation ────────────────────────────

    pub fn correlate_events(&mut self, event_id: &str, related_id: &str) -> bool {
        let mut found = false;
        if let Some(e) = self.events.iter_mut().find(|e| e.id == event_id) {
            if !e.related_event_ids.contains(&related_id.to_string()) {
                e.related_event_ids.push(related_id.to_string());
            }
            found = true;
        }
        if let Some(e) = self.events.iter_mut().find(|e| e.id == related_id) {
            if !e.related_event_ids.contains(&event_id.to_string()) {
                e.related_event_ids.push(event_id.to_string());
            }
        }
        found
    }

    pub fn get_related_events(&self, event_id: &str) -> Vec<TimelineEvent> {
        if let Some(event) = self.events.iter().find(|e| e.id == event_id) {
            self.events.iter()
                .filter(|e| event.related_event_ids.contains(&e.id))
                .cloned()
                .collect()
        } else {
            Vec::new()
        }
    }

    // ── Stats ──────────────────────────────────

    pub fn get_stats(&self) -> TimelineStats {
        let today = Utc::now().format("%Y-%m-%d").to_string();
        let events_today = self.events.iter().filter(|e| e.timestamp.starts_with(&today)).count() as u32;
        let bookmarked = self.events.iter().filter(|e| e.bookmarked).count() as u32;

        let mut cat_counts = std::collections::HashMap::new();
        let mut sev_counts = std::collections::HashMap::new();
        let mut device_counts: std::collections::HashMap<String, u32> = std::collections::HashMap::new();

        for e in &self.events {
            *cat_counts.entry(format!("{:?}", e.category)).or_insert(0u32) += 1;
            *sev_counts.entry(format!("{:?}", e.severity)).or_insert(0u32) += 1;
            if let Some(name) = &e.device_name {
                *device_counts.entry(name.clone()).or_insert(0) += 1;
            }
        }

        let most_active = device_counts.iter().max_by_key(|(_, c)| *c).map(|(n, _)| n.clone());

        TimelineStats {
            total_events: self.events.len() as u32,
            events_today,
            bookmarked,
            by_category: cat_counts.into_iter().collect(),
            by_severity: sev_counts.into_iter().collect(),
            most_active_device: most_active,
        }
    }

    pub fn clear(&mut self) {
        self.events.clear();
    }
}
