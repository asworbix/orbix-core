use serde::{Deserialize, Serialize};
use uuid::Uuid;
use chrono::Utc;

// ─────────────────────────────────────────────
//  Suggestion Models
// ─────────────────────────────────────────────

#[derive(Clone, Serialize, Deserialize, Debug, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum SuggestionCategory {
    Pipeline,       // Stale items, unassigned items
    Workflow,       // Matching patterns, automation opportunities
    FollowUp,       // Items needing attention
    Optimization,   // Automation recommendations
    Scheduling,     // Time-based patterns
}

#[derive(Clone, Serialize, Deserialize, Debug, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum SuggestionPriority {
    Low,
    Medium,
    High,
}

#[derive(Clone, Serialize, Deserialize, Debug, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum SuggestionStatus {
    Active,
    Accepted,
    Dismissed,
    Expired,
}

#[derive(Clone, Serialize, Deserialize, Debug, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum SuggestionAction {
    RunWorkflow { workflow_id: String },
    MoveItem { item_id: String, stage_id: String },
    CreateRule { pattern: String },
    CreateSchedule { workflow_id: String, cron: String },
    OpenPanel { panel: String },
    Custom { label: String, command: String },
}

#[derive(Clone, Serialize, Deserialize, Debug)]
pub struct Suggestion {
    pub id: String,
    pub category: SuggestionCategory,
    pub priority: SuggestionPriority,
    pub title: String,
    pub description: String,
    pub action: SuggestionAction,
    pub action_label: String,
    pub status: SuggestionStatus,
    pub context_key: String,          // Dedup key - don't re-suggest same thing
    pub created_at: String,
    pub resolved_at: Option<String>,
}

// ─────────────────────────────────────────────
//  Suggestion Preferences
// ─────────────────────────────────────────────

#[derive(Clone, Serialize, Deserialize, Debug)]
pub struct SuggestionPreferences {
    pub enabled: bool,
    pub check_interval_mins: u32,
    pub categories: CategoryPrefs,
    pub stale_threshold_hours: u32,     // Hours before an item is "stale"
    pub followup_threshold_hours: u32,  // Hours before suggesting follow-up
}

#[derive(Clone, Serialize, Deserialize, Debug)]
pub struct CategoryPrefs {
    pub pipeline: bool,
    pub workflow: bool,
    pub follow_up: bool,
    pub optimization: bool,
    pub scheduling: bool,
}

impl Default for SuggestionPreferences {
    fn default() -> Self {
        Self {
            enabled: true,
            check_interval_mins: 5,
            categories: CategoryPrefs {
                pipeline: true,
                workflow: true,
                follow_up: true,
                optimization: true,
                scheduling: true,
            },
            stale_threshold_hours: 48,
            followup_threshold_hours: 120, // 5 days
        }
    }
}

// ─────────────────────────────────────────────
//  Suggestion Store
// ─────────────────────────────────────────────

pub struct SuggestionStore {
    pub suggestions: Vec<Suggestion>,
    pub dismissed_keys: Vec<String>,    // Context keys user has dismissed
    pub preferences: SuggestionPreferences,
    pub action_log: Vec<ActionLogEntry>,
}

#[derive(Clone, Serialize, Deserialize, Debug)]
pub struct ActionLogEntry {
    pub action: String,
    pub category: String,
    pub timestamp: String,
}

impl Default for SuggestionStore {
    fn default() -> Self {
        Self {
            suggestions: Vec::new(),
            dismissed_keys: Vec::new(),
            preferences: SuggestionPreferences::default(),
            action_log: Vec::new(),
        }
    }
}

impl SuggestionStore {
    // ── Queries ──

    pub fn list_active(&self) -> Vec<Suggestion> {
        self.suggestions.iter()
            .filter(|s| s.status == SuggestionStatus::Active)
            .cloned()
            .collect()
    }

    pub fn list_history(&self, limit: usize) -> Vec<Suggestion> {
        self.suggestions.iter()
            .filter(|s| s.status != SuggestionStatus::Active)
            .take(limit)
            .cloned()
            .collect()
    }

    pub fn get(&self, id: &str) -> Option<&Suggestion> {
        self.suggestions.iter().find(|s| s.id == id)
    }

    pub fn active_count(&self) -> usize {
        self.suggestions.iter().filter(|s| s.status == SuggestionStatus::Active).count()
    }

    // ── Actions ──

    pub fn accept(&mut self, id: &str) -> Option<Suggestion> {
        if let Some(s) = self.suggestions.iter_mut().find(|s| s.id == id) {
            s.status = SuggestionStatus::Accepted;
            s.resolved_at = Some(Utc::now().to_rfc3339());
            self.action_log.push(ActionLogEntry {
                action: "accepted".into(),
                category: format!("{:?}", s.category),
                timestamp: Utc::now().to_rfc3339(),
            });
            Some(s.clone())
        } else {
            None
        }
    }

    pub fn dismiss(&mut self, id: &str) -> Option<Suggestion> {
        if let Some(s) = self.suggestions.iter_mut().find(|s| s.id == id) {
            s.status = SuggestionStatus::Dismissed;
            s.resolved_at = Some(Utc::now().to_rfc3339());
            self.dismissed_keys.push(s.context_key.clone());
            Some(s.clone())
        } else {
            None
        }
    }

    pub fn dismiss_all(&mut self) {
        let now = Utc::now().to_rfc3339();
        for s in &mut self.suggestions {
            if s.status == SuggestionStatus::Active {
                s.status = SuggestionStatus::Dismissed;
                s.resolved_at = Some(now.clone());
                self.dismissed_keys.push(s.context_key.clone());
            }
        }
    }

    // ── Adding suggestions ──

    pub fn add(&mut self, suggestion: Suggestion) -> bool {
        // Don't add if already dismissed or duplicate active
        if self.dismissed_keys.contains(&suggestion.context_key) {
            return false;
        }
        if self.suggestions.iter().any(|s|
            s.context_key == suggestion.context_key && s.status == SuggestionStatus::Active
        ) {
            return false;
        }
        self.suggestions.insert(0, suggestion);
        // Cap at 100
        if self.suggestions.len() > 100 {
            self.suggestions.truncate(100);
        }
        true
    }

    /// Expire old active suggestions (older than 24h)
    pub fn expire_old(&mut self) {
        let cutoff = Utc::now() - chrono::Duration::hours(24);
        for s in &mut self.suggestions {
            if s.status == SuggestionStatus::Active {
                if let Ok(created) = s.created_at.parse::<chrono::DateTime<Utc>>() {
                    if created < cutoff {
                        s.status = SuggestionStatus::Expired;
                        s.resolved_at = Some(Utc::now().to_rfc3339());
                    }
                }
            }
        }
    }

    // ── Preferences ──

    pub fn get_preferences(&self) -> SuggestionPreferences {
        self.preferences.clone()
    }

    pub fn update_preferences(&mut self, prefs: SuggestionPreferences) {
        self.preferences = prefs;
    }

    // ── Analysis helpers ──

    /// Generate pipeline suggestions from stale item data.
    pub fn generate_pipeline_suggestions(
        &mut self,
        stale_items: Vec<(String, String, String, u64)>, // (item_id, title, stage_name, hours_stale)
    ) -> Vec<Suggestion> {
        if !self.preferences.categories.pipeline { return vec![]; }

        let mut added = Vec::new();
        // Group by stage
        let count = stale_items.len();
        if count > 0 {
            let key = format!("stale_items_{}", Utc::now().format("%Y-%m-%d"));
            let suggestion = Suggestion {
                id: Uuid::new_v4().to_string(),
                category: SuggestionCategory::Pipeline,
                priority: if count > 5 { SuggestionPriority::High } else { SuggestionPriority::Medium },
                title: format!("{} items need attention", count),
                description: format!(
                    "{} pipeline items have been stale for {}+ hours. Consider reviewing or moving them forward.",
                    count, self.preferences.stale_threshold_hours,
                ),
                action: SuggestionAction::OpenPanel { panel: "inbox".into() },
                action_label: "Open Inbox".into(),
                status: SuggestionStatus::Active,
                context_key: key,
                created_at: Utc::now().to_rfc3339(),
                resolved_at: None,
            };
            if self.add(suggestion.clone()) {
                added.push(suggestion);
            }
        }

        // Individual high-priority items (>5 days)
        for (item_id, title, stage, hours) in &stale_items {
            if *hours >= self.preferences.followup_threshold_hours as u64 {
                let key = format!("followup_{}_{}", item_id, Utc::now().format("%Y-%m-%d"));
                let suggestion = Suggestion {
                    id: Uuid::new_v4().to_string(),
                    category: SuggestionCategory::FollowUp,
                    priority: SuggestionPriority::High,
                    title: format!("Follow up: {}", title),
                    description: format!(
                        "\"{}\" has been in \"{}\" for {} hours without update.",
                        title, stage, hours,
                    ),
                    action: SuggestionAction::OpenPanel { panel: "inbox".into() },
                    action_label: "Review Item".into(),
                    status: SuggestionStatus::Active,
                    context_key: key,
                    created_at: Utc::now().to_rfc3339(),
                    resolved_at: None,
                };
                if self.add(suggestion.clone()) {
                    added.push(suggestion);
                }
            }
        }

        added
    }

    /// Generate workflow suggestions from unautomated patterns.
    pub fn generate_workflow_suggestions(
        &mut self,
        manual_workflow_runs: Vec<(String, String, u32)>, // (workflow_id, name, run_count_last_week)
    ) -> Vec<Suggestion> {
        if !self.preferences.categories.optimization { return vec![]; }

        let mut added = Vec::new();
        for (wf_id, name, runs) in &manual_workflow_runs {
            if *runs >= 3 {
                let key = format!("automate_{}_{}", wf_id, Utc::now().format("%Y-W%V"));
                let suggestion = Suggestion {
                    id: Uuid::new_v4().to_string(),
                    category: SuggestionCategory::Optimization,
                    priority: SuggestionPriority::Medium,
                    title: format!("Automate \"{}\"?", name),
                    description: format!(
                        "You've run \"{}\" {} times this week. Consider adding a schedule or trigger.",
                        name, runs,
                    ),
                    action: SuggestionAction::CreateSchedule {
                        workflow_id: wf_id.clone(),
                        cron: "0 9 * * 1-5".into(),
                    },
                    action_label: "Create Schedule".into(),
                    status: SuggestionStatus::Active,
                    context_key: key,
                    created_at: Utc::now().to_rfc3339(),
                    resolved_at: None,
                };
                if self.add(suggestion.clone()) {
                    added.push(suggestion);
                }
            }
        }
        added
    }
}
