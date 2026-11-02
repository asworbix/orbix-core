use serde::{Deserialize, Serialize};
use uuid::Uuid;
use chrono::Utc;
use std::path::{Path, PathBuf};

// ─────────────────────────────────────────────
//  Data Structures
// ─────────────────────────────────────────────

#[derive(Clone, Serialize, Deserialize, Debug, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum AccessLevel {
    ReadOnly,
    ReadWrite,
}

#[derive(Clone, Serialize, Deserialize, Debug)]
pub struct ConfidentialFolder {
    pub id: String,
    pub name: String,
    pub path: String,
    pub allowed_agents: Vec<String>,  // agent IDs
    pub access_level: AccessLevel,
    pub created_at: String,
    pub last_accessed: Option<String>,
}

#[derive(Clone, Serialize, Deserialize, Debug)]
pub struct FolderAccessLog {
    pub id: String,
    pub folder_id: String,
    pub agent_id: String,
    pub agent_name: String,
    pub action: String,
    pub file_path: String,
    pub timestamp: String,
}

#[derive(Clone, Serialize, Deserialize, Debug)]
pub struct FolderContents {
    pub folder_id: String,
    pub folder_name: String,
    pub path: String,
    pub files: Vec<FileEntry>,
}

#[derive(Clone, Serialize, Deserialize, Debug)]
pub struct FileEntry {
    pub name: String,
    pub path: String,
    pub is_dir: bool,
    pub size_bytes: u64,
    pub modified: Option<String>,
}

// ─────────────────────────────────────────────
//  Store
// ─────────────────────────────────────────────

pub struct DataControlStore {
    pub folders: Vec<ConfidentialFolder>,
    pub access_log: Vec<FolderAccessLog>,
}

impl Default for DataControlStore {
    fn default() -> Self {
        Self::load()
    }
}

impl DataControlStore {
    fn data_path() -> PathBuf {
        crate::storage::data_dir().join("data_control.json")
    }

    fn log_path() -> PathBuf {
        crate::storage::data_dir().join("access_log.json")
    }

    pub fn load() -> Self {
        let folders: Vec<ConfidentialFolder> = std::fs::read_to_string(Self::data_path())
            .ok()
            .and_then(|s| serde_json::from_str(&s).ok())
            .unwrap_or_default();

        let access_log: Vec<FolderAccessLog> = std::fs::read_to_string(Self::log_path())
            .ok()
            .and_then(|s| serde_json::from_str(&s).ok())
            .unwrap_or_default();

        Self { folders, access_log }
    }

    pub fn save(&self) {
        if let Ok(json) = serde_json::to_string_pretty(&self.folders) {
            crate::storage::atomic_write(Self::data_path(), json).ok();
        }
    }

    fn save_log(&self) {
        // Keep log capped at 5000 entries
        let log: Vec<_> = if self.access_log.len() > 5000 {
            self.access_log[self.access_log.len() - 5000..].to_vec()
        } else {
            self.access_log.clone()
        };
        if let Ok(json) = serde_json::to_string_pretty(&log) {
            crate::storage::atomic_write(Self::log_path(), json).ok();
        }
    }

    // ── CRUD ──

    pub fn add_folder(&mut self, name: String, path: String, allowed_agents: Vec<String>, access_level: AccessLevel) -> ConfidentialFolder {
        let folder = ConfidentialFolder {
            id: Uuid::new_v4().to_string(),
            name,
            path,
            allowed_agents,
            access_level,
            created_at: Utc::now().to_rfc3339(),
            last_accessed: None,
        };
        self.folders.push(folder.clone());
        self.save();
        folder
    }

    pub fn update_folder(&mut self, id: &str, name: Option<String>, allowed_agents: Option<Vec<String>>, access_level: Option<AccessLevel>) -> Option<ConfidentialFolder> {
        let idx = self.folders.iter().position(|f| f.id == id)?;
        if let Some(n) = name { self.folders[idx].name = n; }
        if let Some(agents) = allowed_agents { self.folders[idx].allowed_agents = agents; }
        if let Some(level) = access_level { self.folders[idx].access_level = level; }
        self.save();
        Some(self.folders[idx].clone())
    }

    pub fn remove_folder(&mut self, id: &str) -> bool {
        let before = self.folders.len();
        self.folders.retain(|f| f.id != id);
        if self.folders.len() < before {
            self.save();
            true
        } else {
            false
        }
    }

    // ── Permission Checks ──

    /// Check if an agent has access to a specific file path.
    /// Returns the folder and access level if permitted.
    pub fn check_access(&self, agent_id: &str, file_path: &str) -> Option<(&ConfidentialFolder, &AccessLevel)> {
        let normalized = Path::new(file_path);
        for folder in &self.folders {
            let folder_path = Path::new(&folder.path);
            if normalized.starts_with(folder_path) {
                if folder.allowed_agents.contains(&agent_id.to_string()) {
                    return Some((folder, &folder.access_level));
                }
                // Path is in a confidential folder but agent doesn't have access
                return None;
            }
        }
        // Path is not in any confidential folder - allow (unrestricted)
        None // returning None means "not a confidential path" - caller decides
    }

    /// Check if a path falls within any confidential folder
    pub fn is_confidential_path(&self, file_path: &str) -> bool {
        let normalized = Path::new(file_path);
        self.folders.iter().any(|f| normalized.starts_with(Path::new(&f.path)))
    }

    /// Check agent permission and return result
    pub fn authorize(&self, agent_id: &str, file_path: &str, write: bool) -> Result<(), String> {
        let normalized = Path::new(file_path);
        for folder in &self.folders {
            let folder_path = Path::new(&folder.path);
            if normalized.starts_with(folder_path) {
                if !folder.allowed_agents.contains(&agent_id.to_string()) {
                    return Err(format!(
                        "Access denied: agent is not authorised for confidential folder \"{}\"",
                        folder.name
                    ));
                }
                if write && folder.access_level == AccessLevel::ReadOnly {
                    return Err(format!(
                        "Write denied: folder \"{}\" is read-only for this agent",
                        folder.name
                    ));
                }
                return Ok(());
            }
        }
        // Not in any confidential folder - unrestricted access
        Ok(())
    }

    // ── Access Logging ──

    pub fn log_access(&mut self, folder_id: &str, agent_id: &str, agent_name: &str, action: &str, file_path: &str) {
        let entry = FolderAccessLog {
            id: Uuid::new_v4().to_string(),
            folder_id: folder_id.to_string(),
            agent_id: agent_id.to_string(),
            agent_name: agent_name.to_string(),
            action: action.to_string(),
            file_path: file_path.to_string(),
            timestamp: Utc::now().to_rfc3339(),
        };
        self.access_log.push(entry);

        // Update last_accessed on the folder
        if let Some(folder) = self.folders.iter_mut().find(|f| f.id == folder_id) {
            folder.last_accessed = Some(Utc::now().to_rfc3339());
        }

        self.save();
        self.save_log();
    }

    // ── Folder Contents ──

    pub fn list_contents(&self, folder_id: &str) -> Result<FolderContents, String> {
        let folder = self.folders.iter().find(|f| f.id == folder_id)
            .ok_or_else(|| "Folder not found".to_string())?;

        let path = Path::new(&folder.path);
        if !path.exists() {
            return Err(format!("Folder path does not exist: {}", folder.path));
        }

        let mut files = Vec::new();
        let entries = std::fs::read_dir(path).map_err(|e| e.to_string())?;

        for entry in entries.flatten() {
            let metadata = entry.metadata().ok();
            let modified = metadata.as_ref()
                .and_then(|m| m.modified().ok())
                .map(|t| {
                    let dt: chrono::DateTime<Utc> = t.into();
                    dt.to_rfc3339()
                });

            files.push(FileEntry {
                name: entry.file_name().to_string_lossy().to_string(),
                path: entry.path().to_string_lossy().to_string(),
                is_dir: entry.path().is_dir(),
                size_bytes: metadata.as_ref().map(|m| m.len()).unwrap_or(0),
                modified,
            });
        }

        files.sort_by(|a, b| {
            b.is_dir.cmp(&a.is_dir).then(a.name.to_lowercase().cmp(&b.name.to_lowercase()))
        });

        Ok(FolderContents {
            folder_id: folder.id.clone(),
            folder_name: folder.name.clone(),
            path: folder.path.clone(),
            files,
        })
    }

    /// Build context string for agent system prompts
    pub fn build_agent_context(&self, agent_id: &str) -> String {
        let accessible: Vec<_> = self.folders.iter()
            .filter(|f| f.allowed_agents.contains(&agent_id.to_string()))
            .collect();

        if accessible.is_empty() {
            return String::new();
        }

        let items: Vec<String> = accessible.iter().map(|f| {
            let level = match f.access_level {
                AccessLevel::ReadOnly => "read-only",
                AccessLevel::ReadWrite => "read-write",
            };
            format!("- \"{}\" → {} ({})", f.name, f.path, level)
        }).collect();

        format!(
            "\nCONFIDENTIAL FOLDERS (you have access to these):\n{}\n\
            Use file_read with full paths to access files in these folders.\n",
            items.join("\n")
        )
    }
}
