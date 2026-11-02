use serde::{Deserialize, Serialize};
use uuid::Uuid;
use chrono::Utc;

/// Post a native OS notification attributed to the app bundle, so it shows the
/// Orbix Core icon. Uses the notification plugin rather than
/// `osascript display notification`, which macOS always attributes to Script
/// Editor (a generic icon) regardless of the title. Best-effort.
pub fn os_notify(app: &tauri::AppHandle, title: &str, body: &str) {
    use tauri_plugin_notification::NotificationExt;
    let _ = app.notification().builder().title(title).body(body).show();
}

// ─────────────────────────────────────────────
//  Notification Models
// ─────────────────────────────────────────────

#[derive(Clone, Serialize, Deserialize, Debug, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum NotificationSource {
    Trigger,
    Pipeline,
    Workflow,
    Schedule,
    Suggestion,
    System,
}

#[derive(Clone, Serialize, Deserialize, Debug, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum NotificationLevel {
    Info,
    Success,
    Warning,
    Error,
}

#[derive(Clone, Serialize, Deserialize, Debug)]
pub struct Notification {
    pub id: String,
    pub source: NotificationSource,
    pub level: NotificationLevel,
    pub title: String,
    pub message: String,
    pub action_panel: Option<String>,  // Panel to navigate to on click
    pub action_id: Option<String>,     // Specific item ID
    pub read: bool,
    pub created_at: String,
}

impl Notification {
    pub fn new(
        source: NotificationSource,
        level: NotificationLevel,
        title: String,
        message: String,
    ) -> Self {
        Self {
            id: Uuid::new_v4().to_string(),
            source,
            level,
            title,
            message,
            action_panel: None,
            action_id: None,
            read: false,
            created_at: Utc::now().to_rfc3339(),
        }
    }

    pub fn with_action(mut self, panel: &str, id: Option<&str>) -> Self {
        self.action_panel = Some(panel.into());
        self.action_id = id.map(|s| s.into());
        self
    }
}

// ─────────────────────────────────────────────
//  Notification Preferences
// ─────────────────────────────────────────────

#[derive(Clone, Serialize, Deserialize, Debug)]
pub struct NotificationPreferences {
    pub enabled: bool,
    pub do_not_disturb: bool,
    pub sound_enabled: bool,
    pub desktop_enabled: bool,
    pub sources: SourcePrefs,
}

#[derive(Clone, Serialize, Deserialize, Debug)]
pub struct SourcePrefs {
    pub trigger: bool,
    pub pipeline: bool,
    pub workflow: bool,
    pub schedule: bool,
    pub suggestion: bool,
    pub system: bool,
}

impl Default for NotificationPreferences {
    fn default() -> Self {
        Self {
            enabled: true,
            do_not_disturb: false,
            sound_enabled: true,
            desktop_enabled: true,
            sources: SourcePrefs {
                trigger: true,
                pipeline: true,
                workflow: true,
                schedule: true,
                suggestion: true,
                system: true,
            },
        }
    }
}

// ─────────────────────────────────────────────
//  Notification Store
// ─────────────────────────────────────────────

pub struct NotificationStore {
    pub notifications: Vec<Notification>,
    pub preferences: NotificationPreferences,
}

impl Default for NotificationStore {
    fn default() -> Self {
        Self {
            notifications: Vec::new(),
            preferences: NotificationPreferences::default(),
        }
    }
}

impl NotificationStore {
    pub fn add(&mut self, notif: Notification) -> bool {
        // Check if source is enabled
        if !self.preferences.enabled || self.preferences.do_not_disturb {
            return false;
        }
        let source_enabled = match notif.source {
            NotificationSource::Trigger => self.preferences.sources.trigger,
            NotificationSource::Pipeline => self.preferences.sources.pipeline,
            NotificationSource::Workflow => self.preferences.sources.workflow,
            NotificationSource::Schedule => self.preferences.sources.schedule,
            NotificationSource::Suggestion => self.preferences.sources.suggestion,
            NotificationSource::System => self.preferences.sources.system,
        };
        if !source_enabled { return false; }

        self.notifications.insert(0, notif);
        // Keep last 200
        if self.notifications.len() > 200 {
            self.notifications.truncate(200);
        }
        true
    }

    pub fn list(&self, limit: usize) -> Vec<Notification> {
        self.notifications.iter().take(limit).cloned().collect()
    }

    pub fn unread_count(&self) -> usize {
        self.notifications.iter().filter(|n| !n.read).count()
    }

    pub fn mark_read(&mut self, id: &str) -> bool {
        if let Some(n) = self.notifications.iter_mut().find(|n| n.id == id) {
            n.read = true;
            true
        } else {
            false
        }
    }

    pub fn mark_all_read(&mut self) {
        for n in &mut self.notifications {
            n.read = true;
        }
    }

    pub fn dismiss(&mut self, id: &str) -> bool {
        let len = self.notifications.len();
        self.notifications.retain(|n| n.id != id);
        self.notifications.len() != len
    }

    pub fn dismiss_all(&mut self) {
        self.notifications.clear();
    }

    pub fn get_preferences(&self) -> NotificationPreferences {
        self.preferences.clone()
    }

    pub fn update_preferences(&mut self, prefs: NotificationPreferences) {
        self.preferences = prefs;
    }
}
