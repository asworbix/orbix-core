use serde::{Deserialize, Serialize};
use uuid::Uuid;
use chrono::Utc;

// ─────────────────────────────────────────────
//  Webhook Types
// ─────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum WebhookEventType {
    DeviceOnline,
    DeviceOffline,
    WorkItemCreated,
    WorkItemCompleted,
    WorkItemFailed,
    IncidentCreated,
    IncidentResolved,
    SlaBreach,
    CapacityAlert,
    MaintenanceStarted,
    MaintenanceCompleted,
    CommandIssued,
    CommandCompleted,
    All,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum DeliveryStatus {
    Pending,
    Success,
    Failed,
    Retrying,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Webhook {
    pub id: String,
    pub name: String,
    pub url: String,
    pub secret: Option<String>,
    pub event_types: Vec<WebhookEventType>,
    pub enabled: bool,
    pub retry_count: u32,         // max retries
    pub timeout_secs: u32,
    pub created_at: String,
    pub updated_at: String,
    pub last_triggered: Option<String>,
    pub total_deliveries: u32,
    pub successful_deliveries: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WebhookDelivery {
    pub id: String,
    pub webhook_id: String,
    pub webhook_name: String,
    pub event_type: WebhookEventType,
    pub status: DeliveryStatus,
    pub response_code: Option<u16>,
    pub response_body: Option<String>,
    pub attempt: u32,
    pub max_attempts: u32,
    pub error: Option<String>,
    pub payload_preview: String,
    pub timestamp: String,
    pub duration_ms: Option<u32>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WebhookStats {
    pub total_webhooks: u32,
    pub active_webhooks: u32,
    pub total_deliveries: u32,
    pub successful_deliveries: u32,
    pub failed_deliveries: u32,
    pub pending_deliveries: u32,
    pub success_rate: f32,
    pub deliveries_today: u32,
}

// ─────────────────────────────────────────────
//  Store
// ─────────────────────────────────────────────

pub struct FleetWebhookStore {
    pub webhooks: Vec<Webhook>,
    pub deliveries: Vec<WebhookDelivery>,
}

impl Default for FleetWebhookStore {
    fn default() -> Self {
        let now = Utc::now().to_rfc3339();
        Self {
            webhooks: vec![
                Webhook {
                    id: "wh-slack".into(),
                    name: "Slack Notifications".into(),
                    url: "https://hooks.slack.com/services/example".into(),
                    secret: Some("whsec_example123".into()),
                    event_types: vec![WebhookEventType::IncidentCreated, WebhookEventType::SlaBreach, WebhookEventType::CapacityAlert],
                    enabled: true,
                    retry_count: 3,
                    timeout_secs: 10,
                    created_at: now.clone(),
                    updated_at: now.clone(),
                    last_triggered: None,
                    total_deliveries: 0,
                    successful_deliveries: 0,
                },
                Webhook {
                    id: "wh-monitor".into(),
                    name: "Monitoring Dashboard".into(),
                    url: "https://monitor.example.com/api/events".into(),
                    secret: None,
                    event_types: vec![WebhookEventType::All],
                    enabled: false,
                    retry_count: 2,
                    timeout_secs: 5,
                    created_at: now.clone(),
                    updated_at: now,
                    last_triggered: None,
                    total_deliveries: 0,
                    successful_deliveries: 0,
                },
            ],
            deliveries: vec![],
        }
    }
}

impl FleetWebhookStore {
    // ── Webhooks ─────────────────────────────

    pub fn list_webhooks(&self) -> Vec<Webhook> {
        self.webhooks.clone()
    }

    pub fn get_webhook(&self, id: &str) -> Option<Webhook> {
        self.webhooks.iter().find(|w| w.id == id).cloned()
    }

    pub fn create_webhook(&mut self, mut wh: Webhook) -> Webhook {
        if wh.id.is_empty() { wh.id = format!("wh-{}", Uuid::new_v4()); }
        let now = Utc::now().to_rfc3339();
        if wh.created_at.is_empty() { wh.created_at = now.clone(); }
        wh.updated_at = now;
        wh.total_deliveries = 0;
        wh.successful_deliveries = 0;
        self.webhooks.push(wh.clone());
        wh
    }

    pub fn update_webhook(&mut self, wh: Webhook) -> Option<Webhook> {
        if let Some(w) = self.webhooks.iter_mut().find(|w| w.id == wh.id) {
            w.name = wh.name;
            w.url = wh.url;
            w.secret = wh.secret;
            w.event_types = wh.event_types;
            w.retry_count = wh.retry_count;
            w.timeout_secs = wh.timeout_secs;
            w.updated_at = Utc::now().to_rfc3339();
            Some(w.clone())
        } else { None }
    }

    pub fn toggle_webhook(&mut self, id: &str) -> Option<Webhook> {
        if let Some(w) = self.webhooks.iter_mut().find(|w| w.id == id) {
            w.enabled = !w.enabled;
            w.updated_at = Utc::now().to_rfc3339();
            Some(w.clone())
        } else { None }
    }

    pub fn delete_webhook(&mut self, id: &str) -> bool {
        let len = self.webhooks.len();
        self.webhooks.retain(|w| w.id != id);
        self.deliveries.retain(|d| d.webhook_id != id);
        self.webhooks.len() < len
    }

    pub fn record_test_result(&mut self, id: &str, delivery: WebhookDelivery) {
        if let Some(w) = self.webhooks.iter_mut().find(|w| w.id == id) {
            w.total_deliveries += 1;
            if delivery.status == DeliveryStatus::Success {
                w.successful_deliveries += 1;
            }
            w.last_triggered = Some(Utc::now().to_rfc3339());
        }
        self.deliveries.push(delivery);
    }

    #[allow(dead_code)]
    pub async fn test_webhook(&mut self, id: &str) -> Option<WebhookDelivery> {
        let wh = self.webhooks.iter().find(|w| w.id == id)?.clone();
        let test_payload = serde_json::json!({
            "type": "test",
            "source": "orbixcore",
            "message": "Webhook test from OrbixCore",
            "timestamp": Utc::now().to_rfc3339(),
        });

        let client = reqwest::Client::new();
        let start = std::time::Instant::now();
        let result = client
            .post(&wh.url)
            .header("Content-Type", "application/json")
            .header("User-Agent", "OrbixCore/1.0")
            .json(&test_payload)
            .timeout(std::time::Duration::from_secs(10))
            .send()
            .await;
        let duration = start.elapsed().as_millis() as u64;

        let (status, code, body, error) = match result {
            Ok(resp) => {
                let code = resp.status().as_u16();
                let body = resp.text().await.unwrap_or_default();
                if code >= 200 && code < 300 {
                    (DeliveryStatus::Success, Some(code), Some(body), None)
                } else {
                    (DeliveryStatus::Failed, Some(code), Some(body), Some(format!("HTTP {}", code)))
                }
            }
            Err(e) => {
                (DeliveryStatus::Failed, None, None, Some(e.to_string()))
            }
        };

        let delivery = WebhookDelivery {
            id: format!("del-{}", Uuid::new_v4()),
            webhook_id: wh.id.clone(),
            webhook_name: wh.name.clone(),
            event_type: WebhookEventType::All,
            status: status.clone(),
            response_code: code.map(|c| c as u16),
            response_body: body,
            attempt: 1,
            max_attempts: 1,
            error,
            payload_preview: serde_json::to_string(&test_payload).unwrap_or_default(),
            timestamp: Utc::now().to_rfc3339(),
            duration_ms: Some(duration as u32),
        };

        if let Some(w) = self.webhooks.iter_mut().find(|w| w.id == id) {
            w.total_deliveries += 1;
            if status == DeliveryStatus::Success {
                w.successful_deliveries += 1;
            }
            w.last_triggered = Some(Utc::now().to_rfc3339());
        }
        self.deliveries.push(delivery.clone());
        Some(delivery)
    }

    // ── Deliveries ───────────────────────────

    pub fn log_delivery(&mut self, mut del: WebhookDelivery) -> WebhookDelivery {
        if del.id.is_empty() { del.id = format!("del-{}", Uuid::new_v4()); }
        if del.timestamp.is_empty() { del.timestamp = Utc::now().to_rfc3339(); }
        // Update webhook stats
        if let Some(w) = self.webhooks.iter_mut().find(|w| w.id == del.webhook_id) {
            w.total_deliveries += 1;
            if del.status == DeliveryStatus::Success { w.successful_deliveries += 1; }
            w.last_triggered = Some(del.timestamp.clone());
        }
        self.deliveries.push(del.clone());
        del
    }

    pub fn list_deliveries(&self, webhook_id: Option<&str>, limit: usize) -> Vec<WebhookDelivery> {
        let mut dels: Vec<WebhookDelivery> = self.deliveries.iter()
            .filter(|d| {
                if let Some(wid) = webhook_id { d.webhook_id == wid } else { true }
            })
            .cloned()
            .collect();
        dels.sort_by(|a, b| b.timestamp.cmp(&a.timestamp));
        dels.truncate(limit);
        dels
    }

    pub fn clear_deliveries(&mut self, webhook_id: Option<&str>) {
        if let Some(wid) = webhook_id {
            self.deliveries.retain(|d| d.webhook_id != wid);
        } else {
            self.deliveries.clear();
        }
    }

    // ── Stats ────────────────────────────────

    pub fn get_stats(&self) -> WebhookStats {
        let total_wh = self.webhooks.len() as u32;
        let active_wh = self.webhooks.iter().filter(|w| w.enabled).count() as u32;
        let total_del = self.deliveries.len() as u32;
        let success = self.deliveries.iter().filter(|d| d.status == DeliveryStatus::Success).count() as u32;
        let failed = self.deliveries.iter().filter(|d| d.status == DeliveryStatus::Failed).count() as u32;
        let pending = self.deliveries.iter().filter(|d| matches!(d.status, DeliveryStatus::Pending | DeliveryStatus::Retrying)).count() as u32;

        let finished = success + failed;
        let success_rate = if finished > 0 { success as f32 / finished as f32 * 100.0 } else { 100.0 };

        let today = Utc::now().format("%Y-%m-%d").to_string();
        let today_count = self.deliveries.iter().filter(|d| d.timestamp.starts_with(&today)).count() as u32;

        WebhookStats {
            total_webhooks: total_wh, active_webhooks: active_wh,
            total_deliveries: total_del, successful_deliveries: success,
            failed_deliveries: failed, pending_deliveries: pending,
            success_rate, deliveries_today: today_count,
        }
    }
}
