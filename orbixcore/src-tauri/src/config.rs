use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::RwLock;

// ─────────────────────────────────────────────
//  Config Value
// ─────────────────────────────────────────────

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(untagged)]
pub enum ConfigValue {
    String(String),
    Integer(i64),
    Float(f64),
    Bool(bool),
    Null,
}

impl ConfigValue {
    pub fn as_str(&self) -> Option<&str> {
        match self {
            ConfigValue::String(s) => Some(s),
            _ => None,
        }
    }

    pub fn as_i64(&self) -> Option<i64> {
        match self {
            ConfigValue::Integer(i) => Some(*i),
            ConfigValue::String(s) => s.parse().ok(),
            _ => None,
        }
    }

    pub fn as_f64(&self) -> Option<f64> {
        match self {
            ConfigValue::Float(f) => Some(*f),
            ConfigValue::Integer(i) => Some(*i as f64),
            ConfigValue::String(s) => s.parse().ok(),
            _ => None,
        }
    }

    pub fn as_bool(&self) -> Option<bool> {
        match self {
            ConfigValue::Bool(b) => Some(*b),
            ConfigValue::String(s) => match s.to_lowercase().as_str() {
                "true" | "1" | "yes" => Some(true),
                "false" | "0" | "no" => Some(false),
                _ => None,
            },
            _ => None,
        }
    }

    pub fn as_usize(&self) -> Option<usize> {
        self.as_i64().and_then(|i| usize::try_from(i).ok())
    }
}

impl From<String> for ConfigValue {
    fn from(s: String) -> Self { ConfigValue::String(s) }
}
impl From<&str> for ConfigValue {
    fn from(s: &str) -> Self { ConfigValue::String(s.to_string()) }
}
impl From<i64> for ConfigValue {
    fn from(i: i64) -> Self { ConfigValue::Integer(i) }
}
impl From<f64> for ConfigValue {
    fn from(f: f64) -> Self { ConfigValue::Float(f) }
}
impl From<bool> for ConfigValue {
    fn from(b: bool) -> Self { ConfigValue::Bool(b) }
}

// ─────────────────────────────────────────────
//  Config Source Priority
// ─────────────────────────────────────────────

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
enum SourcePriority {
    Default = 0,
    File = 1,
    Environment = 2,
    Runtime = 3,
}

#[derive(Clone, Debug)]
struct ConfigEntry {
    value: ConfigValue,
    source: SourcePriority,
}

// ─────────────────────────────────────────────
//  Unified Config
// ─────────────────────────────────────────────

pub struct UnifiedConfig {
    entries: RwLock<HashMap<String, ConfigEntry>>,
}

impl UnifiedConfig {
    pub fn new() -> Self {
        let config = Self {
            entries: RwLock::new(HashMap::new()),
        };
        config.load_defaults();
        config.load_env_overrides();
        config
    }

    /// Load from a JSON config file (config.json). Since we can't add YAML deps, use JSON.
    pub fn load_file(&self, path: &PathBuf) -> Result<(), String> {
        let content = std::fs::read_to_string(path)
            .map_err(|e| format!("Failed to read config file: {}", e))?;
        let json: serde_json::Value = serde_json::from_str(&content)
            .map_err(|e| format!("Failed to parse config: {}", e))?;
        self.flatten_json("", &json, SourcePriority::File);
        Ok(())
    }

    /// Flatten nested JSON into dot-separated keys.
    fn flatten_json(&self, prefix: &str, value: &serde_json::Value, source: SourcePriority) {
        match value {
            serde_json::Value::Object(map) => {
                for (key, val) in map {
                    let full_key = if prefix.is_empty() {
                        key.clone()
                    } else {
                        format!("{}.{}", prefix, key)
                    };
                    self.flatten_json(&full_key, val, source.clone());
                }
            }
            serde_json::Value::String(s) => {
                self.set_internal(prefix, ConfigValue::String(s.clone()), source);
            }
            serde_json::Value::Number(n) => {
                let cv = if let Some(i) = n.as_i64() {
                    ConfigValue::Integer(i)
                } else if let Some(f) = n.as_f64() {
                    ConfigValue::Float(f)
                } else {
                    return;
                };
                self.set_internal(prefix, cv, source);
            }
            serde_json::Value::Bool(b) => {
                self.set_internal(prefix, ConfigValue::Bool(*b), source);
            }
            serde_json::Value::Null => {
                self.set_internal(prefix, ConfigValue::Null, source);
            }
            _ => {}
        }
    }

    fn set_internal(&self, key: &str, value: ConfigValue, source: SourcePriority) {
        let mut entries = self.entries.write().unwrap();
        let existing_priority = entries.get(key).map(|e| e.source.clone());

        // Only override if new source has equal or higher priority
        if existing_priority.map_or(true, |p| source >= p) {
            entries.insert(key.to_string(), ConfigEntry { value, source });
        }
    }

    /// Load environment variable overrides.
    /// Pattern: ORBIX_GATEWAY_PORT -> gateway.port
    fn load_env_overrides(&self) {
        for (key, value) in std::env::vars() {
            if let Some(stripped) = key.strip_prefix("ORBIX_") {
                let config_key = stripped.to_lowercase().replace('_', ".");
                // Try to parse as typed value
                let cv = if let Ok(i) = value.parse::<i64>() {
                    ConfigValue::Integer(i)
                } else if let Ok(f) = value.parse::<f64>() {
                    ConfigValue::Float(f)
                } else if value == "true" || value == "false" {
                    ConfigValue::Bool(value == "true")
                } else {
                    ConfigValue::String(value)
                };
                self.set_internal(&config_key, cv, SourcePriority::Environment);
            }
        }
    }

    /// Load default configuration values.
    fn load_defaults(&self) {
        let defaults: Vec<(&str, ConfigValue)> = vec![
            // Gateway
            ("gateway.port", ConfigValue::Integer(8080)),
            ("gateway.tls.enabled", ConfigValue::Bool(false)),
            ("gateway.rate_limit.rps", ConfigValue::Integer(1000)),
            ("gateway.timeout_ms", ConfigValue::Integer(30000)),
            // Session
            ("session.backend", ConfigValue::String("memory".into())),
            ("session.ttl_seconds", ConfigValue::Integer(3600)),
            ("session.idle_timeout_seconds", ConfigValue::Integer(900)),
            // Engine
            ("engine.worker_pool_size", ConfigValue::Integer(16)),
            ("engine.job_queue_max_size", ConfigValue::Integer(10000)),
            ("engine.dead_letter_retention_hours", ConfigValue::Integer(72)),
            // Event Bus
            ("event_bus.buffer_capacity", ConfigValue::Integer(1024)),
            // Observability
            ("observability.tracing.enabled", ConfigValue::Bool(false)),
            ("observability.metrics.enabled", ConfigValue::Bool(true)),
            ("observability.metrics.port", ConfigValue::Integer(9090)),
            ("observability.logging.level", ConfigValue::String("info".into())),
            ("observability.logging.format", ConfigValue::String("json".into())),
            // Data Residency (EU Boundary Mode)
            ("residency.mode", ConfigValue::String("eu_boundary".into())),
            ("residency.eu.primary_endpoint", ConfigValue::String("vertex_frankfurt".into())),
            ("residency.eu.fallback_endpoint", ConfigValue::String("bedrock_frankfurt".into())),
            ("residency.eu.gcp_region", ConfigValue::String("europe-west3".into())),
            ("residency.compliance_logging", ConfigValue::Bool(true)),
            // Plugins
            ("plugins.sandbox.enabled", ConfigValue::Bool(true)),
            ("plugins.sandbox.memory_limit_mb", ConfigValue::Integer(256)),
            ("plugins.sandbox.cpu_limit_percent", ConfigValue::Integer(25)),
        ];

        for (key, value) in defaults {
            self.set_internal(key, value, SourcePriority::Default);
        }
    }

    // ── Public Getters ──

    pub fn get(&self, key: &str) -> Option<ConfigValue> {
        self.entries.read().unwrap().get(key).map(|e| e.value.clone())
    }

    pub fn get_str(&self, key: &str) -> Option<String> {
        self.get(key).and_then(|v| v.as_str().map(|s| s.to_string()))
    }

    pub fn get_i64(&self, key: &str) -> Option<i64> {
        self.get(key).and_then(|v| v.as_i64())
    }

    pub fn get_bool(&self, key: &str) -> Option<bool> {
        self.get(key).and_then(|v| v.as_bool())
    }

    pub fn get_f64(&self, key: &str) -> Option<f64> {
        self.get(key).and_then(|v| v.as_f64())
    }

    pub fn get_usize(&self, key: &str) -> Option<usize> {
        self.get(key).and_then(|v| v.as_usize())
    }

    /// Get with a default fallback.
    pub fn get_or(&self, key: &str, default: ConfigValue) -> ConfigValue {
        self.get(key).unwrap_or(default)
    }

    /// Set a runtime override (highest priority).
    pub fn set(&self, key: &str, value: ConfigValue) {
        self.set_internal(key, value, SourcePriority::Runtime);
    }

    /// List all config keys with their values (for debugging/display).
    pub fn all(&self) -> HashMap<String, ConfigValue> {
        self.entries.read().unwrap()
            .iter()
            .map(|(k, e)| (k.clone(), e.value.clone()))
            .collect()
    }

    /// Get all keys under a prefix (e.g., "gateway" returns gateway.port, gateway.tls.enabled, etc.)
    pub fn get_section(&self, prefix: &str) -> HashMap<String, ConfigValue> {
        let prefix_dot = format!("{}.", prefix);
        self.entries.read().unwrap()
            .iter()
            .filter(|(k, _)| k.starts_with(&prefix_dot) || k == &prefix)
            .map(|(k, e)| (k.clone(), e.value.clone()))
            .collect()
    }

    /// Export current config as JSON.
    pub fn to_json(&self) -> serde_json::Value {
        let entries = self.entries.read().unwrap();
        let mut root = serde_json::Map::new();
        for (key, entry) in entries.iter() {
            let val = match &entry.value {
                ConfigValue::String(s) => serde_json::Value::String(s.clone()),
                ConfigValue::Integer(i) => serde_json::Value::Number((*i).into()),
                ConfigValue::Float(f) => serde_json::json!(*f),
                ConfigValue::Bool(b) => serde_json::Value::Bool(*b),
                ConfigValue::Null => serde_json::Value::Null,
            };
            // Reconstruct nested structure
            let parts: Vec<&str> = key.split('.').collect();
            set_nested(&mut root, &parts, val);
        }
        serde_json::Value::Object(root)
    }
}

impl Default for UnifiedConfig {
    fn default() -> Self {
        Self::new()
    }
}

/// Helper: set a value in a nested JSON map structure.
fn set_nested(map: &mut serde_json::Map<String, serde_json::Value>, path: &[&str], value: serde_json::Value) {
    if path.len() == 1 {
        map.insert(path[0].to_string(), value);
        return;
    }
    let child = map.entry(path[0].to_string())
        .or_insert_with(|| serde_json::Value::Object(serde_json::Map::new()));
    if let serde_json::Value::Object(ref mut child_map) = child {
        set_nested(child_map, &path[1..], value);
    }
}
