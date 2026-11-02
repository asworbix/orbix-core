// ─────────────────────────────────────────────
//  Workspace setting - Step 2 of ORBIX_HANDOFF.md
//
//  When Bob runs `claude` in real agentic mode, the subprocess needs
//  a working dir to act in. There was no general workspace concept in
//  Orbix Core today - only a hardcoded ~/Documents/GitHub/... path in
//  open_claude_terminal. This module is the persisted user setting
//  that replaces that hardcode.
//
//  Persistence: workspace.json in data_dir() via storage::save_json,
//  matching the pattern used by every other persisted store in the app.
// ─────────────────────────────────────────────

use std::path::PathBuf;

use serde::{Deserialize, Serialize};

use crate::storage;

const STORE_FILE: &str = "workspace.json";

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct WorkspaceSettings {
    /// Absolute path to the user's chosen workspace directory.
    /// None means Bob's agentic mode is unavailable and we fall back
    /// to chat-only on technical requests.
    pub path: Option<String>,
}

impl WorkspaceSettings {
    pub fn load() -> Self {
        storage::load_json::<Self>(STORE_FILE).unwrap_or_default()
    }

    pub fn save(&self) -> Result<(), String> {
        storage::save_json(STORE_FILE, self)
    }

    /// Best-effort path the agent should `cd` into. Returns None if
    /// unset OR if the configured path no longer exists (a user moved
    /// their repo). The agent must surface this to the UI rather than
    /// spawn into a missing dir and produce confusing errors.
    pub fn usable_path(&self) -> Option<PathBuf> {
        let p = self.path.as_ref()?;
        let pb = PathBuf::from(p);
        if pb.is_dir() { Some(pb) } else { None }
    }
}

// ─────────────────────────────────────────────
//  Tests
// ─────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_is_unset() {
        let s = WorkspaceSettings::default();
        assert!(s.path.is_none());
        assert!(s.usable_path().is_none());
    }

    #[test]
    fn usable_returns_none_for_nonexistent_path() {
        let s = WorkspaceSettings {
            path: Some("/this/path/does/not/exist/anywhere".into()),
        };
        assert!(s.usable_path().is_none());
    }

    #[test]
    fn usable_returns_path_when_dir_exists() {
        // A directory guaranteed to exist on the platform running the tests.
        let dir = std::env::temp_dir();
        let s = WorkspaceSettings { path: Some(dir.to_string_lossy().into_owned()) };
        assert_eq!(s.usable_path(), Some(dir));
    }

    #[test]
    fn usable_returns_none_when_path_is_a_file_not_a_dir() {
        // Create a temp file; passing its path must not pass the dir check.
        let tmp = tempfile::NamedTempFile::new().unwrap();
        let s = WorkspaceSettings { path: Some(tmp.path().to_string_lossy().into_owned()) };
        assert!(s.usable_path().is_none());
    }
}
