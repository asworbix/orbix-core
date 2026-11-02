use serde::{Deserialize, Serialize};
use uuid::Uuid;
use chrono::Utc;

// ─────────────────────────────────────────────
//  Routing Rules
// ─────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum RoutingCondition {
    Always,
    StatusEquals(String),          // "Completed", "Failed"
    PriorityAtLeast(String),       // "High", "Urgent"
    HasTag(String),
    SourceEquals(String),          // "trigger", "manual", "chain"
    SlaBreached,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum RoutingStrategy {
    FirstAvailable,   // first device with target role that's online
    LeastBusy,        // device with lowest workload
    RoundRobin,       // rotate through devices
    Specific(String), // specific device id
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RoutingRule {
    pub id: String,
    pub name: String,
    pub description: String,
    pub enabled: bool,
    pub from_role: String,
    pub to_role: String,
    pub condition: RoutingCondition,
    pub strategy: RoutingStrategy,
    pub fallback_role: Option<String>,
    pub auto_start: bool,           // auto-set to InProgress on assign
    pub created_at: String,
    pub last_triggered: Option<String>,
    pub trigger_count: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EscalationRule {
    pub id: String,
    pub name: String,
    pub enabled: bool,
    pub condition: EscalationCondition,
    pub escalate_to_role: String,
    pub priority_boost: bool,        // bump priority up one level
    pub alert: bool,                 // generate alert
    pub created_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum EscalationCondition {
    SlaBreached,
    StuckMinutes(u32),               // item in same status for N minutes
    DeviceOffline,
    FailedAttempts(u32),
}

// ─────────────────────────────────────────────
//  Inter-Device Messages
// ─────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum MessageType {
    Request,
    Response,
    Alert,
    Handoff,
    Broadcast,
    Info,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FleetMessage {
    pub id: String,
    pub channel: String,          // "sales->support", "broadcast", device_id
    pub from_device: Option<String>,
    pub from_role: String,
    pub to_role: Option<String>,  // None = broadcast
    pub to_device: Option<String>,
    pub message_type: MessageType,
    pub subject: String,
    pub body: String,
    pub ref_item_id: Option<String>,   // related work item
    pub read: bool,
    pub created_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FleetChannel {
    pub id: String,
    pub name: String,
    pub role_a: String,
    pub role_b: String,
    pub message_count: usize,
    pub last_message_at: Option<String>,
}

// ─────────────────────────────────────────────
//  Fleet Alerts
// ─────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FleetAlert {
    pub id: String,
    pub alert_type: String,       // "device_offline", "sla_breach", "stuck_item", "escalation"
    pub severity: String,         // "info", "warning", "critical"
    pub title: String,
    pub details: String,
    pub device_id: Option<String>,
    pub item_id: Option<String>,
    pub acknowledged: bool,
    pub created_at: String,
}

// ─────────────────────────────────────────────
//  Routing Store
// ─────────────────────────────────────────────

const MAX_MESSAGES: usize = 500;
const MAX_ALERTS: usize = 200;

pub struct FleetRoutingStore {
    pub routing_rules: Vec<RoutingRule>,
    pub escalation_rules: Vec<EscalationRule>,
    pub messages: Vec<FleetMessage>,
    pub alerts: Vec<FleetAlert>,
    round_robin_index: std::collections::HashMap<String, usize>,  // role -> index
}

impl Default for FleetRoutingStore {
    fn default() -> Self {
        Self {
            routing_rules: Vec::new(),
            escalation_rules: Vec::new(),
            messages: Vec::new(),
            alerts: Vec::new(),
            round_robin_index: std::collections::HashMap::new(),
        }
    }
}

impl FleetRoutingStore {
    // ── Routing Rules ───────────────────────

    pub fn list_routing_rules(&self) -> Vec<RoutingRule> {
        self.routing_rules.clone()
    }

    pub fn create_routing_rule(&mut self, mut rule: RoutingRule) -> RoutingRule {
        if rule.id.is_empty() {
            rule.id = Uuid::new_v4().to_string();
        }
        if rule.created_at.is_empty() {
            rule.created_at = Utc::now().to_rfc3339();
        }
        self.routing_rules.push(rule.clone());
        rule
    }

    pub fn update_routing_rule(&mut self, rule: RoutingRule) -> Option<RoutingRule> {
        if let Some(pos) = self.routing_rules.iter().position(|r| r.id == rule.id) {
            self.routing_rules[pos] = rule.clone();
            Some(rule)
        } else {
            None
        }
    }

    pub fn delete_routing_rule(&mut self, id: &str) -> bool {
        let len = self.routing_rules.len();
        self.routing_rules.retain(|r| r.id != id);
        self.routing_rules.len() < len
    }

    pub fn toggle_routing_rule(&mut self, id: &str) -> Option<RoutingRule> {
        if let Some(rule) = self.routing_rules.iter_mut().find(|r| r.id == id) {
            rule.enabled = !rule.enabled;
            Some(rule.clone())
        } else {
            None
        }
    }

    /// Evaluate routing rules and return the target role for a work item
    pub fn evaluate_route(
        &mut self,
        from_role: &str,
        item_status: &str,
        item_priority: &str,
        item_tags: &[String],
        item_source: &str,
    ) -> Option<(String, RoutingStrategy, bool)> {
        // (target_role, strategy, auto_start)
        for rule in &mut self.routing_rules {
            if !rule.enabled || rule.from_role != from_role {
                continue;
            }
            let matches = match &rule.condition {
                RoutingCondition::Always => true,
                RoutingCondition::StatusEquals(s) => s.eq_ignore_ascii_case(item_status),
                RoutingCondition::PriorityAtLeast(p) => {
                    let pri_ord = |s: &str| match s.to_lowercase().as_str() {
                        "low" => 0, "normal" => 1, "high" => 2, "urgent" => 3, _ => 1,
                    };
                    pri_ord(item_priority) >= pri_ord(p)
                }
                RoutingCondition::HasTag(t) => item_tags.iter().any(|tag| tag == t),
                RoutingCondition::SourceEquals(s) => s.eq_ignore_ascii_case(item_source),
                RoutingCondition::SlaBreached => false, // checked separately
            };
            if matches {
                rule.last_triggered = Some(Utc::now().to_rfc3339());
                rule.trigger_count += 1;
                return Some((rule.to_role.clone(), rule.strategy.clone(), rule.auto_start));
            }
        }
        None
    }

    /// Pick a target device from available devices using the routing strategy
    pub fn pick_device(
        &mut self,
        target_role: &str,
        strategy: &RoutingStrategy,
        devices: &[(String, String, u32, bool)], // (id, role, workload, is_online)
    ) -> Option<String> {
        let candidates: Vec<&(String, String, u32, bool)> = devices.iter()
            .filter(|(_, role, _, online)| role == target_role && *online)
            .collect();

        if candidates.is_empty() {
            return None;
        }

        match strategy {
            RoutingStrategy::FirstAvailable => Some(candidates[0].0.clone()),
            RoutingStrategy::LeastBusy => {
                candidates.iter()
                    .min_by_key(|(_, _, workload, _)| workload)
                    .map(|(id, _, _, _)| id.clone())
            }
            RoutingStrategy::RoundRobin => {
                let idx = self.round_robin_index.entry(target_role.to_string()).or_insert(0);
                let pick = *idx % candidates.len();
                *idx = pick + 1;
                Some(candidates[pick].0.clone())
            }
            RoutingStrategy::Specific(id) => {
                if candidates.iter().any(|(did, _, _, _)| did == id) {
                    Some(id.clone())
                } else {
                    None
                }
            }
        }
    }

    // ── Escalation Rules ────────────────────

    pub fn list_escalation_rules(&self) -> Vec<EscalationRule> {
        self.escalation_rules.clone()
    }

    pub fn create_escalation_rule(&mut self, mut rule: EscalationRule) -> EscalationRule {
        if rule.id.is_empty() {
            rule.id = Uuid::new_v4().to_string();
        }
        if rule.created_at.is_empty() {
            rule.created_at = Utc::now().to_rfc3339();
        }
        self.escalation_rules.push(rule.clone());
        rule
    }

    pub fn delete_escalation_rule(&mut self, id: &str) -> bool {
        let len = self.escalation_rules.len();
        self.escalation_rules.retain(|r| r.id != id);
        self.escalation_rules.len() < len
    }

    pub fn toggle_escalation_rule(&mut self, id: &str) -> Option<EscalationRule> {
        if let Some(rule) = self.escalation_rules.iter_mut().find(|r| r.id == id) {
            rule.enabled = !rule.enabled;
            Some(rule.clone())
        } else {
            None
        }
    }

    // ── Messages ────────────────────────────

    pub fn send_message(&mut self, mut msg: FleetMessage) -> FleetMessage {
        if msg.id.is_empty() {
            msg.id = Uuid::new_v4().to_string();
        }
        if msg.created_at.is_empty() {
            msg.created_at = Utc::now().to_rfc3339();
        }
        if msg.channel.is_empty() {
            msg.channel = match (&msg.from_role, &msg.to_role) {
                (from, Some(to)) => format!("{}→{}", from, to),
                (from, None) => format!("{}→broadcast", from),
            };
        }
        self.messages.push(msg.clone());
        if self.messages.len() > MAX_MESSAGES {
            self.messages.drain(0..self.messages.len() - MAX_MESSAGES);
        }
        msg
    }

    pub fn list_messages(&self, channel: Option<&str>, limit: usize) -> Vec<FleetMessage> {
        let iter = self.messages.iter().rev();
        let filtered: Vec<FleetMessage> = match channel {
            Some(ch) => iter.filter(|m| m.channel == ch).take(limit).cloned().collect(),
            None => iter.take(limit).cloned().collect(),
        };
        filtered
    }

    pub fn list_channels(&self) -> Vec<FleetChannel> {
        let mut channels: std::collections::HashMap<String, FleetChannel> = std::collections::HashMap::new();
        for msg in &self.messages {
            let entry = channels.entry(msg.channel.clone()).or_insert_with(|| {
                let parts: Vec<&str> = msg.channel.split('→').collect();
                FleetChannel {
                    id: msg.channel.clone(),
                    name: msg.channel.clone(),
                    role_a: parts.first().unwrap_or(&"").to_string(),
                    role_b: parts.get(1).unwrap_or(&"").to_string(),
                    message_count: 0,
                    last_message_at: None,
                }
            });
            entry.message_count += 1;
            entry.last_message_at = Some(msg.created_at.clone());
        }
        let mut result: Vec<FleetChannel> = channels.into_values().collect();
        result.sort_by(|a, b| b.last_message_at.cmp(&a.last_message_at));
        result
    }

    pub fn mark_messages_read(&mut self, channel: &str) {
        for msg in &mut self.messages {
            if msg.channel == channel {
                msg.read = true;
            }
        }
    }

    pub fn unread_count(&self) -> usize {
        self.messages.iter().filter(|m| !m.read).count()
    }

    // ── Alerts ──────────────────────────────

    pub fn add_alert(&mut self, mut alert: FleetAlert) -> FleetAlert {
        if alert.id.is_empty() {
            alert.id = Uuid::new_v4().to_string();
        }
        if alert.created_at.is_empty() {
            alert.created_at = Utc::now().to_rfc3339();
        }
        self.alerts.push(alert.clone());
        if self.alerts.len() > MAX_ALERTS {
            self.alerts.drain(0..self.alerts.len() - MAX_ALERTS);
        }
        alert
    }

    pub fn list_alerts(&self, unacked_only: bool) -> Vec<FleetAlert> {
        if unacked_only {
            self.alerts.iter().filter(|a| !a.acknowledged).cloned().collect()
        } else {
            self.alerts.clone()
        }
    }

    pub fn acknowledge_alert(&mut self, id: &str) -> bool {
        if let Some(a) = self.alerts.iter_mut().find(|a| a.id == id) {
            a.acknowledged = true;
            true
        } else {
            false
        }
    }

    pub fn acknowledge_all_alerts(&mut self) {
        for a in &mut self.alerts {
            a.acknowledged = true;
        }
    }
}
