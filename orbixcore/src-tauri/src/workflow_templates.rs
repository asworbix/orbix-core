use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use uuid::Uuid;
use chrono::Utc;

// ─────────────────────────────────────────────
//  Workflow Template Models
// ─────────────────────────────────────────────

#[derive(Clone, Serialize, Deserialize, Debug)]
pub struct TemplateStep {
    pub step_type: String,
    pub label: String,
    pub config: HashMap<String, serde_json::Value>,
    /// If set, this step should bind to a universal connector
    pub connector_id: Option<String>,
    pub endpoint_key: Option<String>,
}

#[derive(Clone, Serialize, Deserialize, Debug)]
pub struct WorkflowTemplate {
    pub id: String,
    pub name: String,
    pub description: String,
    pub category: String,
    pub icon: String,
    pub steps: Vec<TemplateStep>,
    pub is_builtin: bool,
    pub created_at: String,
    /// Suggested schedule (cron, local time) applied when instantiated.
    #[serde(default)]
    pub schedule: Option<String>,
}

#[derive(Clone, Serialize, Deserialize, Debug)]
pub struct TemplateCategory {
    pub id: String,
    pub label: String,
    pub icon: String,
}

// ─────────────────────────────────────────────
//  Template Store
// ─────────────────────────────────────────────

pub struct TemplateStore {
    pub templates: Vec<WorkflowTemplate>,
}

impl Default for TemplateStore {
    fn default() -> Self {
        Self {
            templates: builtin_templates(),
        }
    }
}

impl TemplateStore {
    pub fn load() -> Self {
        let mut store = Self::default();
        // Load user-created templates from disk and merge
        if let Ok(user_templates) = crate::storage::load_json::<Vec<WorkflowTemplate>>("workflow_templates") {
            for t in user_templates {
                if !t.is_builtin {
                    store.templates.push(t);
                }
            }
        }
        store
    }

    pub fn save(&self) {
        let user_templates: Vec<&WorkflowTemplate> = self.templates.iter().filter(|t| !t.is_builtin).collect();
        crate::storage::save_json("workflow_templates", &user_templates).ok();
    }

    pub fn list(&self) -> Vec<WorkflowTemplate> {
        self.templates.clone()
    }

    pub fn get(&self, id: &str) -> Option<WorkflowTemplate> {
        self.templates.iter().find(|t| t.id == id).cloned()
    }

    pub fn categories(&self) -> Vec<TemplateCategory> {
        vec![
            TemplateCategory { id: "automation".into(), label: "Every day".into(), icon: "zap".into() },
            TemplateCategory { id: "monitoring".into(), label: "Inbox & messages".into(), icon: "activity".into() },
            TemplateCategory { id: "reporting".into(), label: "Notes & tasks".into(), icon: "bar-chart".into() },
            TemplateCategory { id: "data".into(), label: "Files & web".into(), icon: "database".into() },
            TemplateCategory { id: "custom".into(), label: "My templates".into(), icon: "user".into() },
        ]
    }

    /// Create a workflow template from an existing workflow (save-as-template)
    pub fn save_as_template(
        &mut self,
        name: String,
        description: String,
        category: String,
        steps: Vec<TemplateStep>,
    ) -> WorkflowTemplate {
        let template = WorkflowTemplate {
            id: Uuid::new_v4().to_string(),
            name,
            description,
            category,
            icon: "file-text".into(),
            steps,
            is_builtin: false,
            created_at: Utc::now().to_rfc3339(),
            schedule: None,
        };
        self.templates.push(template.clone());
        self.save();
        template
    }

    pub fn delete(&mut self, id: &str) -> bool {
        let len = self.templates.len();
        self.templates.retain(|t| t.id != id || t.is_builtin);
        let removed = self.templates.len() < len;
        if removed { self.save(); }
        removed
    }

    /// Instantiate a template into workflow steps with fresh IDs
    pub fn instantiate(&self, template_id: &str) -> Result<(String, String, Vec<crate::workflow::WorkflowStep>), String> {
        let template = self.get(template_id).ok_or("Template not found")?;
        let steps = template.steps.iter().enumerate().map(|(i, ts)| {
            let mut config = ts.config.clone();
            // Inject connector bindings into config
            if let Some(ref cid) = ts.connector_id {
                config.insert("connector_id".into(), serde_json::Value::String(cid.clone()));
            }
            if let Some(ref ek) = ts.endpoint_key {
                config.insert("endpoint_key".into(), serde_json::Value::String(ek.clone()));
            }
            let step_type = match ts.step_type.as_str() {
                "action" => crate::workflow::StepType::Action,
                "condition" => crate::workflow::StepType::Condition,
                "wait" => crate::workflow::StepType::Wait,
                "loop" => crate::workflow::StepType::Loop,
                _ => crate::workflow::StepType::Prompt,
            };
            crate::workflow::WorkflowStep {
                id: Uuid::new_v4().to_string(),
                order: i,
                step_type,
                label: ts.label.clone(),
                config,
            }
        }).collect();
        Ok((template.name.clone(), template.description.clone(), steps))
    }
}

// ─────────────────────────────────────────────
//  Built-in Templates
// ─────────────────────────────────────────────

fn builtin_templates() -> Vec<WorkflowTemplate> {
    // Everyday recipes that run out of the box on a Mac with Orbix: the
    // shipped app bridges (Calendar, Mail, Notes, Reminders), the web,
    // files, and Bob's own writing. No connector to bind, nothing to ask
    // the user at run time. Each one is a sentence a person would say.
    let bridge = |app: &str, action: &str, args: serde_json::Value| -> HashMap<String, serde_json::Value> {
        let mut m = HashMap::new();
        m.insert("tool".into(), serde_json::json!("app_bridge"));
        m.insert("tool_args".into(), serde_json::json!({ "app": app, "action": action, "args": args }));
        m
    };
    let tool = |name: &str, args: serde_json::Value| -> HashMap<String, serde_json::Value> {
        let mut m = HashMap::new();
        m.insert("tool".into(), serde_json::json!(name));
        m.insert("tool_args".into(), args);
        m
    };
    let step = |step_type: &str, label: &str, config: HashMap<String, serde_json::Value>| TemplateStep {
        step_type: step_type.into(),
        label: label.into(),
        config,
        connector_id: None,
        endpoint_key: None,
    };
    let tpl = |id: &str, name: &str, description: &str, category: &str, icon: &str, schedule: Option<&str>, steps: Vec<TemplateStep>| WorkflowTemplate {
        id: id.into(),
        name: name.into(),
        description: description.into(),
        category: category.into(),
        icon: icon.into(),
        steps,
        is_builtin: true,
        created_at: "2026-09-28T00:00:00Z".into(),
        schedule: schedule.map(String::from),
    };

    vec![
        tpl("tpl-morning-brief", "Morning briefing",
            "Reads today's calendar and your newest mail, then writes a short brief for the day.",
            "automation", "zap", Some("30 7 * * 1-5"), vec![
            step("action", "Read today's events in Calendar", bridge("Calendar", "upcoming_events", serde_json::json!({ "days": 1 }))),
            step("action", "Read the newest mail in Mail", bridge("Mail", "recent_messages", serde_json::json!({ "limit": 15 }))),
            step("prompt", "Write a short brief for the day", map_from(&[("prompt", "From the calendar events and the mail above, write a brief for today in under 120 words: what is on, what needs a reply, what can wait. Plain language, no headings.")])),
        ]),
        tpl("tpl-inbox-digest", "Inbox digest",
            "Groups your newest mail by what needs you and what can wait.",
            "monitoring", "activity", Some("0 12 * * 1-5"), vec![
            step("action", "Read the newest mail in Mail", bridge("Mail", "recent_messages", serde_json::json!({ "limit": 30 }))),
            step("prompt", "Sort it into needs a reply, worth reading, can wait", map_from(&[("prompt", "Group the messages above into three short lists: needs a reply from me, worth reading, can wait. One line per message with sender and subject. Skip newsletters unless they look important.")])),
        ]),
        tpl("tpl-meeting-prep", "Meeting prep",
            "Looks at today's meetings and prepares one line of context and two questions for each.",
            "automation", "zap", Some("0 8 * * 1-5"), vec![
            step("action", "Read today's events in Calendar", bridge("Calendar", "upcoming_events", serde_json::json!({ "days": 1 }))),
            step("action", "Read recent notes in Notes", bridge("Notes", "recent_notes", serde_json::json!({ "limit": 10 }))),
            step("prompt", "Prepare each meeting", map_from(&[("prompt", "For each meeting today, write its time, one line of context (use the notes if any relate), and two good questions to ask. Skip all-day events and reminders.")])),
        ]),
        tpl("tpl-open-tasks", "What is on my plate",
            "Lists your open reminders in order of urgency with a suggested first move.",
            "reporting", "bar-chart", Some("0 8 * * *"), vec![
            step("action", "Read open reminders", bridge("Reminders", "open_reminders", serde_json::json!({ "limit": 40 }))),
            step("prompt", "Order them and suggest a first move", map_from(&[("prompt", "Order the open reminders above by urgency (overdue first, then due today, then the rest). Show each with its due date and list, and end with the one thing to do first and why.")])),
        ]),
        tpl("tpl-weekly-status", "Weekly status",
            "Turns this week's calendar, notes and reminders into a status note you can send.",
            "reporting", "bar-chart", Some("0 16 * * 5"), vec![
            step("action", "Read this week's events in Calendar", bridge("Calendar", "upcoming_events", serde_json::json!({ "days": 7 }))),
            step("action", "Read recent notes in Notes", bridge("Notes", "recent_notes", serde_json::json!({ "limit": 15 }))),
            step("action", "Read open reminders", bridge("Reminders", "open_reminders", serde_json::json!({ "limit": 40 }))),
            step("prompt", "Write the weekly status", map_from(&[("prompt", "Write a weekly status note in three short sections: done, in progress, next week. Use the events, notes and reminders above. Under 200 words, ready to paste into a message.")])),
        ]),
        tpl("tpl-notes-to-actions", "Notes into action items",
            "Reads your latest notes and pulls out the action items with owners and dates where they are mentioned.",
            "reporting", "bar-chart", None, vec![
            step("action", "Read recent notes in Notes", bridge("Notes", "recent_notes", serde_json::json!({ "limit": 8 }))),
            step("prompt", "Extract the action items", map_from(&[("prompt", "From the notes above, list every action item as one line: what, who (if named), when (if mentioned), and which note it came from. Then list open questions separately.")])),
        ]),
        tpl("tpl-desktop-tidy", "Desktop tidy-up",
            "Looks at what is on your Desktop and suggests what to archive, delete or keep.",
            "data", "database", Some("0 17 * * 5"), vec![
            step("action", "List the files on the Desktop", tool("file_list", serde_json::json!({ "path": "~/Desktop" }))),
            step("prompt", "Suggest what to do with them", map_from(&[("prompt", "Group the files above into keep, archive and probably delete, with one reason each. Point out screenshots, downloads and duplicates. Do not move anything; this is a suggestion.")])),
        ]),
        tpl("tpl-topic-watch", "Watch a topic",
            "Searches the web for a topic and writes what is new. Change the topic by telling Bob.",
            "data", "database", Some("0 9 * * *"), vec![
            step("action", "Search the web for the topic", tool("web_search", serde_json::json!({ "query": "latest news on AI regulation this week" }))),
            step("prompt", "Write what is new", map_from(&[("prompt", "From the search results above, write five bullets on what is new and why it matters, each with the source name. Say plainly if the results are thin.")])),
        ]),
    ]
}

fn map_from(pairs: &[(&str, &str)]) -> HashMap<String, serde_json::Value> {
    let mut m = HashMap::new();
    for (k, v) in pairs {
        m.insert(k.to_string(), serde_json::Value::String(v.to_string()));
    }
    m
}

