use serde::{Deserialize, Serialize};
use uuid::Uuid;
use chrono::Utc;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum EnvTier {
    Production,
    Staging,
    Development,
    Testing,
    Custom,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FleetEnvironment {
    pub id: String,
    pub name: String,
    pub tier: EnvTier,
    pub description: String,
    pub color: String,
    pub device_ids: Vec<String>,
    pub locked: bool,
    pub created_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EnvVariable {
    pub key: String,
    pub value: String,
    pub secret: bool,
    pub updated_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EnvPromotion {
    pub id: String,
    pub from_env_id: String,
    pub from_env_name: String,
    pub to_env_id: String,
    pub to_env_name: String,
    pub promoted_by: String,
    pub notes: String,
    pub promoted_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EnvSummary {
    pub total_environments: u32,
    pub total_devices_assigned: u32,
    pub recent_promotions: u32,
    pub environments: Vec<EnvSummaryEntry>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EnvSummaryEntry {
    pub name: String,
    pub tier: EnvTier,
    pub device_count: u32,
}

pub struct FleetEnvironmentStore {
    pub environments: Vec<FleetEnvironment>,
    pub variables: std::collections::HashMap<String, Vec<EnvVariable>>, // env_id -> vars
    pub promotions: Vec<EnvPromotion>,
}

impl Default for FleetEnvironmentStore {
    fn default() -> Self {
        let now = Utc::now().to_rfc3339();
        Self {
            environments: vec![
                FleetEnvironment {
                    id: Uuid::new_v4().to_string(),
                    name: "Production".into(),
                    tier: EnvTier::Production,
                    description: "Live production environment".into(),
                    color: "#ef4444".into(),
                    device_ids: vec![],
                    locked: true,
                    created_at: now.clone(),
                },
                FleetEnvironment {
                    id: Uuid::new_v4().to_string(),
                    name: "Staging".into(),
                    tier: EnvTier::Staging,
                    description: "Pre-production staging environment".into(),
                    color: "#f59e0b".into(),
                    device_ids: vec![],
                    locked: false,
                    created_at: now.clone(),
                },
                FleetEnvironment {
                    id: Uuid::new_v4().to_string(),
                    name: "Development".into(),
                    tier: EnvTier::Development,
                    description: "Development and testing environment".into(),
                    color: "#10b981".into(),
                    device_ids: vec![],
                    locked: false,
                    created_at: now,
                },
            ],
            variables: std::collections::HashMap::new(),
            promotions: vec![],
        }
    }
}

impl FleetEnvironmentStore {
    pub fn list_environments(&self) -> Vec<FleetEnvironment> {
        self.environments.clone()
    }

    pub fn create_environment(&mut self, mut env: FleetEnvironment) -> FleetEnvironment {
        env.id = Uuid::new_v4().to_string();
        env.created_at = Utc::now().to_rfc3339();
        self.environments.push(env.clone());
        env
    }

    pub fn update_environment(&mut self, env: FleetEnvironment) -> Option<FleetEnvironment> {
        if let Some(e) = self.environments.iter_mut().find(|e| e.id == env.id) {
            e.name = env.name;
            e.description = env.description;
            e.tier = env.tier;
            e.color = env.color;
            e.locked = env.locked;
            return Some(e.clone());
        }
        None
    }

    pub fn delete_environment(&mut self, id: &str) -> bool {
        let len = self.environments.len();
        self.environments.retain(|e| e.id != id);
        self.variables.remove(id);
        self.environments.len() < len
    }

    pub fn assign_device(&mut self, env_id: &str, device_id: &str) -> Option<FleetEnvironment> {
        // Remove device from any other environment first
        for env in &mut self.environments {
            env.device_ids.retain(|d| d != device_id);
        }
        if let Some(env) = self.environments.iter_mut().find(|e| e.id == env_id) {
            if !env.device_ids.contains(&device_id.to_string()) {
                env.device_ids.push(device_id.to_string());
            }
            return Some(env.clone());
        }
        None
    }

    pub fn remove_device(&mut self, env_id: &str, device_id: &str) -> Option<FleetEnvironment> {
        if let Some(env) = self.environments.iter_mut().find(|e| e.id == env_id) {
            env.device_ids.retain(|d| d != device_id);
            return Some(env.clone());
        }
        None
    }

    pub fn list_variables(&self, env_id: &str) -> Vec<EnvVariable> {
        self.variables.get(env_id).cloned().unwrap_or_default()
    }

    pub fn set_variable(&mut self, env_id: &str, mut variable: EnvVariable) -> EnvVariable {
        variable.updated_at = Utc::now().to_rfc3339();
        let vars = self.variables.entry(env_id.to_string()).or_default();
        if let Some(v) = vars.iter_mut().find(|v| v.key == variable.key) {
            *v = variable.clone();
        } else {
            vars.push(variable.clone());
        }
        variable
    }

    pub fn delete_variable(&mut self, env_id: &str, key: &str) -> bool {
        if let Some(vars) = self.variables.get_mut(env_id) {
            let len = vars.len();
            vars.retain(|v| v.key != key);
            return vars.len() < len;
        }
        false
    }

    pub fn promote(&mut self, from_env_id: &str, to_env_id: &str, notes: &str) -> Option<EnvPromotion> {
        let from = self.environments.iter().find(|e| e.id == from_env_id)?;
        let to = self.environments.iter().find(|e| e.id == to_env_id)?;

        let promotion = EnvPromotion {
            id: Uuid::new_v4().to_string(),
            from_env_id: from_env_id.into(),
            from_env_name: from.name.clone(),
            to_env_id: to_env_id.into(),
            to_env_name: to.name.clone(),
            promoted_by: "user".into(),
            notes: notes.into(),
            promoted_at: Utc::now().to_rfc3339(),
        };

        // Copy variables from source to target
        if let Some(source_vars) = self.variables.get(from_env_id).cloned() {
            self.variables.insert(to_env_id.to_string(), source_vars);
        }

        self.promotions.push(promotion.clone());
        Some(promotion)
    }

    pub fn list_promotions(&self, limit: usize) -> Vec<EnvPromotion> {
        let mut promos = self.promotions.clone();
        promos.reverse();
        promos.truncate(limit);
        promos
    }

    pub fn get_summary(&self) -> EnvSummary {
        let total_devices: usize = self.environments.iter().map(|e| e.device_ids.len()).sum();
        EnvSummary {
            total_environments: self.environments.len() as u32,
            total_devices_assigned: total_devices as u32,
            recent_promotions: self.promotions.len() as u32,
            environments: self.environments.iter().map(|e| EnvSummaryEntry {
                name: e.name.clone(),
                tier: e.tier.clone(),
                device_count: e.device_ids.len() as u32,
            }).collect(),
        }
    }
}
