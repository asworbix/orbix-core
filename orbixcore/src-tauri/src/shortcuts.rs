use serde::{Deserialize, Serialize};

// ─────────────────────────────────────────────
//  Keyboard Shortcuts
// ─────────────────────────────────────────────

#[derive(Clone, Serialize, Deserialize, Debug)]
pub struct ShortcutBinding {
    pub id: String,
    pub label: String,
    pub keys: String,           // e.g. "Cmd+K", "Cmd+Shift+V"
    pub category: String,
    pub enabled: bool,
}

#[derive(Clone, Serialize, Deserialize, Debug)]
pub struct ShortcutPreferences {
    pub bindings: Vec<ShortcutBinding>,
    pub vim_mode: bool,
    pub show_hints: bool,
}

impl Default for ShortcutPreferences {
    fn default() -> Self {
        Self {
            bindings: default_bindings(),
            vim_mode: false,
            show_hints: true,
        }
    }
}

fn default_bindings() -> Vec<ShortcutBinding> {
    vec![
        ShortcutBinding { id: "command_palette".into(), label: "Command Palette".into(), keys: "Cmd+K".into(), category: "General".into(), enabled: true },
        ShortcutBinding { id: "new_conversation".into(), label: "New Conversation".into(), keys: "Cmd+N".into(), category: "General".into(), enabled: true },
        ShortcutBinding { id: "focus_input".into(), label: "Focus Chat Input".into(), keys: "Cmd+/".into(), category: "General".into(), enabled: true },
        ShortcutBinding { id: "voice_mode".into(), label: "Toggle Voice Mode".into(), keys: "Cmd+Shift+V".into(), category: "General".into(), enabled: true },
        ShortcutBinding { id: "close_panel".into(), label: "Close Panel".into(), keys: "Escape".into(), category: "General".into(), enabled: true },
        ShortcutBinding { id: "search".into(), label: "Search Conversations".into(), keys: "Cmd+F".into(), category: "General".into(), enabled: true },
        ShortcutBinding { id: "panel_1".into(), label: "Panel 1 (Agents)".into(), keys: "Cmd+1".into(), category: "Panels".into(), enabled: true },
        ShortcutBinding { id: "panel_2".into(), label: "Panel 2 (Tools)".into(), keys: "Cmd+2".into(), category: "Panels".into(), enabled: true },
        ShortcutBinding { id: "panel_3".into(), label: "Panel 3 (Workflows)".into(), keys: "Cmd+3".into(), category: "Panels".into(), enabled: true },
        ShortcutBinding { id: "panel_4".into(), label: "Panel 4 (Memory)".into(), keys: "Cmd+4".into(), category: "Panels".into(), enabled: true },
        ShortcutBinding { id: "panel_5".into(), label: "Panel 5 (Browser)".into(), keys: "Cmd+5".into(), category: "Panels".into(), enabled: true },
        ShortcutBinding { id: "panel_6".into(), label: "Panel 6 (Screen)".into(), keys: "Cmd+6".into(), category: "Panels".into(), enabled: true },
        ShortcutBinding { id: "panel_7".into(), label: "Panel 7 (Connectors)".into(), keys: "Cmd+7".into(), category: "Panels".into(), enabled: true },
        ShortcutBinding { id: "panel_8".into(), label: "Panel 8 (Triggers)".into(), keys: "Cmd+8".into(), category: "Panels".into(), enabled: true },
        ShortcutBinding { id: "panel_9".into(), label: "Panel 9 (Inbox)".into(), keys: "Cmd+9".into(), category: "Panels".into(), enabled: true },
    ]
}

// ─────────────────────────────────────────────
//  Shortcuts Store
// ─────────────────────────────────────────────

pub struct ShortcutsStore {
    pub preferences: ShortcutPreferences,
    pub recent_commands: Vec<String>,
}

impl Default for ShortcutsStore {
    fn default() -> Self {
        Self {
            preferences: ShortcutPreferences::default(),
            recent_commands: Vec::new(),
        }
    }
}

impl ShortcutsStore {
    pub fn get_preferences(&self) -> ShortcutPreferences {
        self.preferences.clone()
    }

    pub fn update_preferences(&mut self, prefs: ShortcutPreferences) {
        self.preferences = prefs;
    }

    pub fn update_binding(&mut self, id: &str, keys: Option<String>, enabled: Option<bool>) -> Option<ShortcutBinding> {
        if let Some(b) = self.preferences.bindings.iter_mut().find(|b| b.id == id) {
            if let Some(k) = keys { b.keys = k; }
            if let Some(e) = enabled { b.enabled = e; }
            Some(b.clone())
        } else {
            None
        }
    }

    pub fn add_recent_command(&mut self, command: String) {
        self.recent_commands.retain(|c| c != &command);
        self.recent_commands.insert(0, command);
        if self.recent_commands.len() > 10 {
            self.recent_commands.truncate(10);
        }
    }

    pub fn get_recent_commands(&self) -> Vec<String> {
        self.recent_commands.clone()
    }

    pub fn reset_to_defaults(&mut self) {
        self.preferences = ShortcutPreferences::default();
    }
}
