use serde::{Deserialize, Serialize};
use uuid::Uuid;
use chrono::Utc;
use std::path::PathBuf;

// ─────────────────────────────────────────────
//  Plugin System v2 - Capabilities & Lifecycle
// ─────────────────────────────────────────────

#[derive(Clone, Serialize, Deserialize, Debug, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum Capability {
    NetworkOutbound,
    NetworkInbound,
    StorageRead,
    StorageWrite,
    FileRead,
    FileWrite,
    ShellExecute,
    SystemInfo,
    MemoryAccess,
    ConversationRead,
    ConversationWrite,
    WorkflowExecute,
    EventPublish,
    EventSubscribe,
}

#[derive(Clone, Serialize, Deserialize, Debug)]
pub struct ResourceLimits {
    pub memory_limit_mb: u32,
    pub cpu_limit_percent: u8,
    pub timeout_seconds: u32,
    pub max_concurrent: u8,
}

impl Default for ResourceLimits {
    fn default() -> Self {
        Self {
            memory_limit_mb: 256,
            cpu_limit_percent: 25,
            timeout_seconds: 30,
            max_concurrent: 4,
        }
    }
}

// ─────────────────────────────────────────────
//  Plugin Models
// ─────────────────────────────────────────────

#[derive(Clone, Serialize, Deserialize, Debug, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum PluginType {
    Tool,
    Connector,
    WorkflowStep,
    Widget,
}

/// Plugin lifecycle states (v2):
///   Registered → Initialized → Active → Deactivated → Unloaded
///   Any state may transition to Error (quarantine).
#[derive(Clone, Serialize, Deserialize, Debug, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum PluginStatus {
    Registered,
    Initialized,
    Active,
    Deactivated,
    Unloaded,
    Error,
    // Legacy aliases kept for serde backwards-compat deserialization
    #[serde(alias = "installed")]
    Installed,
    #[serde(alias = "disabled")]
    Disabled,
}

#[derive(Clone, Serialize, Deserialize, Debug)]
pub struct PluginPermission {
    pub id: String,
    pub label: String,
    pub granted: bool,
}

#[derive(Clone, Serialize, Deserialize, Debug)]
pub struct PluginManifest {
    pub name: String,
    pub version: String,
    pub author: String,
    pub description: String,
    pub plugin_type: PluginType,
    pub min_app_version: Option<String>,
    pub permissions: Vec<String>,
    pub entry_point: String,
    pub settings_schema: Option<serde_json::Value>,
    // v2 fields
    #[serde(default = "default_api_version")]
    pub api_version: String,
    #[serde(default)]
    pub capabilities: Vec<Capability>,
    #[serde(default)]
    pub resource_limits: Option<ResourceLimits>,
}

fn default_api_version() -> String {
    "2".to_string()
}

#[derive(Clone, Serialize, Deserialize, Debug)]
pub struct Plugin {
    pub id: String,
    pub manifest: PluginManifest,
    pub status: PluginStatus,
    pub permissions: Vec<PluginPermission>,
    pub settings: serde_json::Value,
    pub install_path: String,
    pub installed_at: String,
    pub updated_at: String,
    pub error: Option<String>,
}

// ─────────────────────────────────────────────
//  Plugin Store
// ─────────────────────────────────────────────

#[derive(Clone, Serialize, Deserialize, Debug, Default)]
pub struct PluginStore {
    pub plugins: Vec<Plugin>,
}

impl PluginStore {
    pub fn plugins_dir() -> PathBuf {
        let dir = crate::storage::data_dir().join("plugins");
        let _ = std::fs::create_dir_all(&dir);
        dir
    }

    pub fn load() -> Self {
        let path = Self::plugins_dir().join("registry.json");
        if let Ok(data) = std::fs::read_to_string(&path) {
            serde_json::from_str(&data).unwrap_or_default()
        } else {
            Self::default()
        }
    }

    pub fn save(&self) {
        let path = Self::plugins_dir().join("registry.json");
        if let Ok(json) = serde_json::to_string_pretty(self) {
            let _ = crate::storage::atomic_write(&path, json);
        }
    }

    pub fn list(&self) -> Vec<Plugin> {
        self.plugins.clone()
    }

    pub fn get(&self, id: &str) -> Option<&Plugin> {
        self.plugins.iter().find(|p| p.id == id)
    }

    // ── Install (creates plugin in Registered state) ──

    pub fn install(&mut self, manifest: PluginManifest) -> Result<Plugin, String> {
        // Check for duplicate
        if self.plugins.iter().any(|p| p.manifest.name == manifest.name) {
            return Err(format!("Plugin '{}' is already installed", manifest.name));
        }

        // Check version compatibility
        if let Some(ref min_ver) = manifest.min_app_version {
            // Simple check - in production would use semver
            let _ = min_ver; // Accept for now
        }

        let now = Utc::now().to_rfc3339();
        let id = Uuid::new_v4().to_string();

        let permissions: Vec<PluginPermission> = manifest.permissions.iter()
            .map(|p| PluginPermission {
                id: p.clone(),
                label: permission_label(p),
                granted: false,
            })
            .collect();

        let install_path = Self::plugins_dir()
            .join(&manifest.name)
            .to_string_lossy()
            .to_string();

        let plugin = Plugin {
            id: id.clone(),
            manifest,
            status: PluginStatus::Registered,
            permissions,
            settings: serde_json::json!({}),
            install_path,
            installed_at: now.clone(),
            updated_at: now,
            error: None,
        };

        self.plugins.push(plugin.clone());
        self.save();
        Ok(plugin)
    }

    pub fn uninstall(&mut self, id: &str) -> bool {
        let len = self.plugins.len();
        self.plugins.retain(|p| p.id != id);
        if self.plugins.len() != len {
            self.save();
            true
        } else {
            false
        }
    }

    // ── Lifecycle transitions (v2) ──

    /// Registered → Initialized
    pub fn initialize(&mut self, id: &str) -> Option<Plugin> {
        if let Some(p) = self.plugins.iter_mut().find(|p| p.id == id) {
            if p.status != PluginStatus::Registered && p.status != PluginStatus::Installed {
                return None;
            }
            p.status = PluginStatus::Initialized;
            p.updated_at = Utc::now().to_rfc3339();
            let result = p.clone();
            self.save();
            Some(result)
        } else {
            None
        }
    }

    /// Transitions to Active (from Initialized, Registered, Installed, or Deactivated).
    pub fn enable(&mut self, id: &str) -> Option<Plugin> {
        if let Some(p) = self.plugins.iter_mut().find(|p| p.id == id) {
            p.status = PluginStatus::Active;
            p.updated_at = Utc::now().to_rfc3339();
            p.error = None;
            let result = p.clone();
            self.save();
            Some(result)
        } else {
            None
        }
    }

    /// Active → Deactivated
    pub fn disable(&mut self, id: &str) -> Option<Plugin> {
        if let Some(p) = self.plugins.iter_mut().find(|p| p.id == id) {
            p.status = PluginStatus::Deactivated;
            p.updated_at = Utc::now().to_rfc3339();
            let result = p.clone();
            self.save();
            Some(result)
        } else {
            None
        }
    }

    /// Active → Deactivated (explicit lifecycle alias)
    pub fn deactivate(&mut self, id: &str) -> Option<Plugin> {
        self.disable(id)
    }

    /// Any → Error (quarantine with error message)
    pub fn quarantine(&mut self, id: &str, error: String) -> Option<Plugin> {
        if let Some(p) = self.plugins.iter_mut().find(|p| p.id == id) {
            p.status = PluginStatus::Error;
            p.error = Some(error);
            p.updated_at = Utc::now().to_rfc3339();
            let result = p.clone();
            self.save();
            Some(result)
        } else {
            None
        }
    }

    /// Error → Active (clears error, reactivates)
    pub fn reactivate(&mut self, id: &str) -> Option<Plugin> {
        if let Some(p) = self.plugins.iter_mut().find(|p| p.id == id) {
            if p.status != PluginStatus::Error {
                return None;
            }
            p.status = PluginStatus::Active;
            p.error = None;
            p.updated_at = Utc::now().to_rfc3339();
            let result = p.clone();
            self.save();
            Some(result)
        } else {
            None
        }
    }

    // ── Capabilities (v2) ──

    /// Get the list of capabilities declared by a plugin.
    pub fn get_capabilities(&self, id: &str) -> Option<Vec<Capability>> {
        self.get(id).map(|p| p.manifest.capabilities.clone())
    }

    /// Check whether a plugin declares a specific capability.
    pub fn has_capability(&self, id: &str, cap: &Capability) -> bool {
        self.get(id)
            .map(|p| p.manifest.capabilities.contains(cap))
            .unwrap_or(false)
    }

    // ── Permissions & settings ──

    pub fn grant_permission(&mut self, plugin_id: &str, permission_id: &str) -> bool {
        if let Some(p) = self.plugins.iter_mut().find(|p| p.id == plugin_id) {
            if let Some(perm) = p.permissions.iter_mut().find(|pm| pm.id == permission_id) {
                perm.granted = true;
                p.updated_at = Utc::now().to_rfc3339();
                self.save();
                return true;
            }
        }
        false
    }

    pub fn revoke_permission(&mut self, plugin_id: &str, permission_id: &str) -> bool {
        if let Some(p) = self.plugins.iter_mut().find(|p| p.id == plugin_id) {
            if let Some(perm) = p.permissions.iter_mut().find(|pm| pm.id == permission_id) {
                perm.granted = false;
                p.updated_at = Utc::now().to_rfc3339();
                self.save();
                return true;
            }
        }
        false
    }

    pub fn update_settings(&mut self, id: &str, settings: serde_json::Value) -> Option<Plugin> {
        if let Some(p) = self.plugins.iter_mut().find(|p| p.id == id) {
            p.settings = settings;
            p.updated_at = Utc::now().to_rfc3339();
            let result = p.clone();
            self.save();
            Some(result)
        } else {
            None
        }
    }

    pub fn active_plugins(&self) -> Vec<&Plugin> {
        self.plugins.iter().filter(|p| p.status == PluginStatus::Active).collect()
    }

    pub fn active_by_type(&self, plugin_type: &PluginType) -> Vec<&Plugin> {
        self.plugins.iter()
            .filter(|p| p.status == PluginStatus::Active && &p.manifest.plugin_type == plugin_type)
            .collect()
    }
}

// ─────────────────────────────────────────────
//  Capability grant validation (Phase 1, custom connector system)
//
//  Enforces the two-trust-line invariant from CONNECTORS.md §2:
//  NetworkOutbound is only grantable to PluginType::Connector. Any other
//  plugin type (Tool, WorkflowStep, Widget) running on the L-line must
//  not be able to reach the internet directly. They go through the bus.
// ─────────────────────────────────────────────

pub fn validate_capability_grant(
    plugin_type: &PluginType,
    caps: &[Capability],
) -> Result<(), String> {
    for cap in caps {
        match (plugin_type, cap) {
            (PluginType::Connector, _) => {} // Connectors may hold any cap.
            (_, Capability::NetworkOutbound) => {
                return Err(format!(
                    "NetworkOutbound is only grantable to PluginType::Connector \
                     (attempted for {:?})",
                    plugin_type
                ));
            }
            _ => {}
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn network_outbound_rejected_for_tool() {
        let err = validate_capability_grant(
            &PluginType::Tool,
            &[Capability::NetworkOutbound],
        ).unwrap_err();
        assert!(err.contains("NetworkOutbound"));
    }

    #[test]
    fn network_outbound_rejected_for_workflow_step() {
        assert!(validate_capability_grant(
            &PluginType::WorkflowStep,
            &[Capability::NetworkOutbound],
        ).is_err());
    }

    #[test]
    fn network_outbound_rejected_for_widget() {
        assert!(validate_capability_grant(
            &PluginType::Widget,
            &[Capability::NetworkOutbound],
        ).is_err());
    }

    #[test]
    fn network_outbound_allowed_for_connector() {
        assert!(validate_capability_grant(
            &PluginType::Connector,
            &[Capability::NetworkOutbound],
        ).is_ok());
    }

    #[test]
    fn non_network_caps_allowed_for_any_type() {
        for pt in [
            PluginType::Tool, PluginType::Connector,
            PluginType::WorkflowStep, PluginType::Widget,
        ] {
            assert!(validate_capability_grant(
                &pt,
                &[Capability::StorageRead, Capability::EventPublish],
            ).is_ok());
        }
    }

    #[test]
    fn rejects_when_network_is_one_of_many_caps_on_non_connector() {
        assert!(validate_capability_grant(
            &PluginType::Tool,
            &[Capability::StorageRead, Capability::NetworkOutbound],
        ).is_err());
    }
}

fn permission_label(id: &str) -> String {
    match id {
        "read_conversations" => "Read conversation history".into(),
        "write_conversations" => "Send messages".into(),
        "read_pipeline" => "Read pipeline items".into(),
        "write_pipeline" => "Create/modify pipeline items".into(),
        "read_memory" => "Access memory store".into(),
        "write_memory" => "Add/modify memories".into(),
        "run_workflows" => "Execute workflows".into(),
        "network_access" => "Make network requests".into(),
        "file_access" => "Read/write files".into(),
        "system_info" => "Read system information".into(),
        _ => id.replace('_', " ").to_string(),
    }
}
