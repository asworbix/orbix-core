use serde::{Deserialize, Serialize};
use uuid::Uuid;
use chrono::Utc;

// ─────────────────────────────────────────────
//  Command Types
// ─────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum CommandStatus {
    Pending,
    Running,
    Completed,
    Failed,
    Cancelled,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum CommandTarget {
    SingleDevice,
    Group,
    AllDevices,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FleetCommand {
    pub id: String,
    pub name: String,
    pub command: String,             // the actual command/action
    pub args: Vec<String>,
    pub target_type: CommandTarget,
    pub target_id: Option<String>,   // device_id or group_id
    pub target_name: Option<String>, // display name
    pub status: CommandStatus,
    pub result: Option<String>,
    pub error: Option<String>,
    pub issued_by: String,
    pub created_at: String,
    pub started_at: Option<String>,
    pub completed_at: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CommandTemplate {
    pub id: String,
    pub name: String,
    pub description: String,
    pub command: String,
    pub default_args: Vec<String>,
    pub category: String,
    pub created_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CommandStats {
    pub total_commands: u32,
    pub pending: u32,
    pub running: u32,
    pub completed: u32,
    pub failed: u32,
    pub success_rate: f32,
    pub commands_today: u32,
}

// ─────────────────────────────────────────────
//  Store
// ─────────────────────────────────────────────

pub struct FleetCommandStore {
    pub commands: Vec<FleetCommand>,
    pub templates: Vec<CommandTemplate>,
}

impl Default for FleetCommandStore {
    fn default() -> Self {
        let now = Utc::now().to_rfc3339();
        Self {
            commands: vec![],
            templates: vec![
                CommandTemplate {
                    id: "tpl-restart".into(),
                    name: "Restart Service".into(),
                    description: "Restart the main application service on the target device.".into(),
                    command: "service_restart".into(),
                    default_args: vec!["orbixcore-agent".into()],
                    category: "System".into(),
                    created_at: now.clone(),
                },
                CommandTemplate {
                    id: "tpl-health".into(),
                    name: "Health Check".into(),
                    description: "Run a comprehensive health check and report device status.".into(),
                    command: "health_check".into(),
                    default_args: vec!["--verbose".into()],
                    category: "Diagnostics".into(),
                    created_at: now.clone(),
                },
                CommandTemplate {
                    id: "tpl-update".into(),
                    name: "Update Agent".into(),
                    description: "Pull and apply the latest agent software update.".into(),
                    command: "agent_update".into(),
                    default_args: vec!["--channel".into(), "stable".into()],
                    category: "System".into(),
                    created_at: now.clone(),
                },
                CommandTemplate {
                    id: "tpl-logs".into(),
                    name: "Collect Logs".into(),
                    description: "Gather and upload recent log files for analysis.".into(),
                    command: "collect_logs".into(),
                    default_args: vec!["--hours".into(), "24".into()],
                    category: "Diagnostics".into(),
                    created_at: now.clone(),
                },
                CommandTemplate {
                    id: "tpl-clear-cache".into(),
                    name: "Clear Cache".into(),
                    description: "Clear application caches and temporary files.".into(),
                    command: "clear_cache".into(),
                    default_args: vec![],
                    category: "Maintenance".into(),
                    created_at: now.clone(),
                },
                CommandTemplate {
                    id: "tpl-pause-work".into(),
                    name: "Pause Work Items".into(),
                    description: "Pause all active work item processing on the device.".into(),
                    command: "pause_processing".into(),
                    default_args: vec![],
                    category: "Operations".into(),
                    created_at: now,
                },
            ],
        }
    }
}

impl FleetCommandStore {
    // ── Commands ─────────────────────────────

    pub fn issue_command(&mut self, mut cmd: FleetCommand) -> FleetCommand {
        if cmd.id.is_empty() { cmd.id = format!("cmd-{}", Uuid::new_v4()); }
        let now = Utc::now().to_rfc3339();
        if cmd.created_at.is_empty() { cmd.created_at = now.clone(); }
        cmd.status = CommandStatus::Pending;
        self.commands.push(cmd.clone());
        cmd
    }

    pub fn start_command(&mut self, id: &str) -> Option<FleetCommand> {
        if let Some(cmd) = self.commands.iter_mut().find(|c| c.id == id) {
            cmd.status = CommandStatus::Running;
            cmd.started_at = Some(Utc::now().to_rfc3339());
            Some(cmd.clone())
        } else { None }
    }

    pub fn complete_command(&mut self, id: &str, result: String) -> Option<FleetCommand> {
        if let Some(cmd) = self.commands.iter_mut().find(|c| c.id == id) {
            cmd.status = CommandStatus::Completed;
            cmd.result = Some(result);
            cmd.completed_at = Some(Utc::now().to_rfc3339());
            Some(cmd.clone())
        } else { None }
    }

    pub fn fail_command(&mut self, id: &str, error: String) -> Option<FleetCommand> {
        if let Some(cmd) = self.commands.iter_mut().find(|c| c.id == id) {
            cmd.status = CommandStatus::Failed;
            cmd.error = Some(error);
            cmd.completed_at = Some(Utc::now().to_rfc3339());
            Some(cmd.clone())
        } else { None }
    }

    pub fn cancel_command(&mut self, id: &str) -> Option<FleetCommand> {
        if let Some(cmd) = self.commands.iter_mut().find(|c| c.id == id) {
            if matches!(cmd.status, CommandStatus::Pending | CommandStatus::Running) {
                cmd.status = CommandStatus::Cancelled;
                cmd.completed_at = Some(Utc::now().to_rfc3339());
                return Some(cmd.clone());
            }
        }
        None
    }

    pub fn list_commands(&self, status_filter: Option<&str>, limit: usize) -> Vec<FleetCommand> {
        let mut cmds: Vec<FleetCommand> = self.commands.iter()
            .filter(|c| {
                if let Some(s) = status_filter {
                    format!("{:?}", c.status) == s
                } else { true }
            })
            .cloned()
            .collect();
        cmds.sort_by(|a, b| b.created_at.cmp(&a.created_at));
        cmds.truncate(limit);
        cmds
    }

    pub fn list_active_commands(&self) -> Vec<FleetCommand> {
        self.commands.iter()
            .filter(|c| matches!(c.status, CommandStatus::Pending | CommandStatus::Running))
            .cloned()
            .collect()
    }

    pub fn get_command(&self, id: &str) -> Option<FleetCommand> {
        self.commands.iter().find(|c| c.id == id).cloned()
    }

    pub fn delete_command(&mut self, id: &str) -> bool {
        let len = self.commands.len();
        self.commands.retain(|c| c.id != id);
        self.commands.len() < len
    }

    // ── Templates ────────────────────────────

    pub fn list_templates(&self) -> Vec<CommandTemplate> {
        self.templates.clone()
    }

    pub fn get_template(&self, id: &str) -> Option<CommandTemplate> {
        self.templates.iter().find(|t| t.id == id).cloned()
    }

    pub fn create_template(&mut self, mut tpl: CommandTemplate) -> CommandTemplate {
        if tpl.id.is_empty() { tpl.id = format!("tpl-{}", Uuid::new_v4()); }
        if tpl.created_at.is_empty() { tpl.created_at = Utc::now().to_rfc3339(); }
        self.templates.push(tpl.clone());
        tpl
    }

    pub fn delete_template(&mut self, id: &str) -> bool {
        let len = self.templates.len();
        self.templates.retain(|t| t.id != id);
        self.templates.len() < len
    }

    // ── Stats ────────────────────────────────

    pub fn get_stats(&self) -> CommandStats {
        let total = self.commands.len() as u32;
        let pending = self.commands.iter().filter(|c| c.status == CommandStatus::Pending).count() as u32;
        let running = self.commands.iter().filter(|c| c.status == CommandStatus::Running).count() as u32;
        let completed = self.commands.iter().filter(|c| c.status == CommandStatus::Completed).count() as u32;
        let failed = self.commands.iter().filter(|c| c.status == CommandStatus::Failed).count() as u32;

        let finished = completed + failed;
        let success_rate = if finished > 0 { completed as f32 / finished as f32 * 100.0 } else { 100.0 };

        let today = Utc::now().format("%Y-%m-%d").to_string();
        let commands_today = self.commands.iter()
            .filter(|c| c.created_at.starts_with(&today))
            .count() as u32;

        CommandStats { total_commands: total, pending, running, completed, failed, success_rate, commands_today }
    }
}
