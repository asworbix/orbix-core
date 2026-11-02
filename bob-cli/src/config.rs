//! Persistent CLI config (W8): chat model, app endpoint.
//!
//! Stored at ~/.config/orbix/cli.json so `orbix models use` and saved
//! tokens persist across invocations. An old "mode" key is ignored. Kept separate from the desktop
//! app's config; the CLI shares the app's model *servers* (8847/8848) and
//! its downloaded model files, but its own preferences live here.

use serde::{Deserialize, Serialize};
use std::path::PathBuf;

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct CliConfig {
    #[serde(default)]
    pub model: String,
    /// Orbix app server for `orbix handoff` (FABLE B3). Empty = default
    /// http://127.0.0.1:51231.
    #[serde(default)]
    pub app_url: String,
    /// Endpoint token from Orbix Settings > Models. Stored here by
    /// `orbix handoff --token <t> --save`.
    #[serde(default)]
    pub app_token: String,
}

fn config_path() -> PathBuf {
    dirs::config_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join("orbix")
        .join("cli.json")
}

/// Whether a config file exists yet. Used to detect the very first run.
pub fn exists() -> bool {
    config_path().exists()
}

impl CliConfig {
    pub fn load() -> Self {
        std::fs::read_to_string(config_path())
            .ok()
            .and_then(|s| serde_json::from_str(&s).ok())
            .unwrap_or_default()
    }

    /// Atomic write (tmp + rename) with owner-only permissions: since
    /// this file can hold the app's endpoint token.
    pub fn save(&self) -> Result<(), String> {
        let path = config_path();
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).map_err(|e| format!("create config dir: {}", e))?;
        }
        let json = serde_json::to_string_pretty(self).map_err(|e| format!("serialize: {}", e))?;
        let tmp = path.with_extension("json.tmp");
        std::fs::write(&tmp, &json).map_err(|e| format!("write config: {}", e))?;
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let _ = std::fs::set_permissions(&tmp, std::fs::Permissions::from_mode(0o600));
        }
        std::fs::rename(&tmp, &path).map_err(|e| format!("write config: {}", e))
    }
}

/// The Orbix Core app's downloaded-models directory (macOS app-support).
pub fn models_dir() -> PathBuf {
    dirs::data_local_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join("ai.orbixcore.app")
        .join("models")
}

/// The .gguf files the app has downloaded, by filename stem. This is what
/// `orbix models list` shows; the CLI shares the app's model files rather
/// than keeping its own.
pub fn downloaded_models() -> Vec<String> {
    let Ok(entries) = std::fs::read_dir(models_dir()) else { return Vec::new() };
    let mut out: Vec<String> = entries
        .flatten()
        .filter_map(|e| {
            let name = e.file_name().to_string_lossy().into_owned();
            name.strip_suffix(".gguf").map(|s| s.to_string())
        })
        .collect();
    out.sort();
    out
}
