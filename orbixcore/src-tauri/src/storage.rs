use aes_gcm::{
    aead::{Aead, KeyInit, OsRng},
    Aes256Gcm, Nonce,
};
use base64::{engine::general_purpose::STANDARD as B64, Engine};
use rand::RngCore;
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::PathBuf;

const NONCE_LEN: usize = 12;
const APP_DIR_NAME: &str = "ai.orbixcore.app";
const KEYCHAIN_SERVICE: &str = "ai.orbixcore.app";
const KEYCHAIN_ACCOUNT: &str = "master-key";

// ─────────────────────────────────────────────
//  Data directory
// ─────────────────────────────────────────────

/// The app's base directory - shared across every workspace profile.
/// Models, runtime binaries, auth, license and the profile registry
/// live here; everything user-facing lives under the active profile.
pub fn base_dir() -> PathBuf {
    // ORBIX_DATA_DIR relocates everything (a portable install, a second
    // instance, a CI job). Under `cargo test` the default is a per-process
    // temp folder, so the suite never writes into the real data folder:
    // it used to stamp audit_log.json, runs.json, and the vault with every
    // run (Windows session, 2026-09-17).
    if let Some(dir) = std::env::var_os("ORBIX_DATA_DIR").filter(|v| !v.is_empty()) {
        return PathBuf::from(dir);
    }
    #[cfg(test)]
    {
        return test_base_dir();
    }
    #[allow(unreachable_code)]
    dirs::data_local_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join(APP_DIR_NAME)
}

/// Orbix Core had accounts and license keys until 2026-11; the app no longer
/// reads either file. Delete what older builds left behind: auth.json held
/// an (encrypted) account token. license.json lived under the roaming data
/// dir, which differs from base_dir() on Windows.
pub fn remove_retired_account_files() {
    let _ = std::fs::remove_file(base_dir().join("auth.json"));
    if let Some(dir) = dirs::data_dir() {
        let _ = std::fs::remove_file(dir.join(APP_DIR_NAME).join("license.json"));
    }
}

#[cfg(test)]
fn test_base_dir() -> PathBuf {
    static DIR: std::sync::OnceLock<PathBuf> = std::sync::OnceLock::new();
    DIR.get_or_init(|| {
        let dir = std::env::temp_dir().join(format!("orbixcore-tests-{}", std::process::id()));
        fs::create_dir_all(&dir).ok();
        dir
    })
    .clone()
}

// ─────────────────────────────────────────────
//  Workspace profiles
//
//  A profile is a named data directory: base/profiles/<id>/. The
//  "default" profile maps to the base dir itself, so existing installs
//  keep every byte of their data with zero migration. The active
//  profile is resolved ONCE per process - switching restarts the app,
//  which is what keeps every store consistent by construction.
// ─────────────────────────────────────────────

pub const DEFAULT_PROFILE_ID: &str = "default";

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct WorkspaceProfile {
    pub id: String,
    pub name: String,
    pub created_at: String,
}

#[derive(Serialize, Deserialize, Clone, Default, Debug)]
pub struct ProfileRegistry {
    pub profiles: Vec<WorkspaceProfile>,
    pub active: Option<String>,
}

fn registry_path() -> PathBuf {
    base_dir().join("profiles.json")
}

pub fn load_profile_registry() -> ProfileRegistry {
    let mut reg: ProfileRegistry = fs::read_to_string(registry_path())
        .ok()
        .and_then(|s| serde_json::from_str(&s).ok())
        .unwrap_or_default();
    // The default profile always exists, listed first.
    if !reg.profiles.iter().any(|p| p.id == DEFAULT_PROFILE_ID) {
        reg.profiles.insert(0, WorkspaceProfile {
            id: DEFAULT_PROFILE_ID.to_string(),
            name: "Default".to_string(),
            created_at: chrono::Utc::now().to_rfc3339(),
        });
    }
    reg
}

pub fn save_profile_registry(reg: &ProfileRegistry) -> Result<(), String> {
    fs::create_dir_all(base_dir()).map_err(|e| e.to_string())?;
    fs::write(
        registry_path(),
        serde_json::to_string_pretty(reg).map_err(|e| e.to_string())?,
    )
    .map_err(|e| e.to_string())
}

/// The active profile id, resolved once per process.
pub fn active_profile_id() -> &'static str {
    static ACTIVE: std::sync::OnceLock<String> = std::sync::OnceLock::new();
    ACTIVE.get_or_init(|| {
        load_profile_registry()
            .active
            .unwrap_or_else(|| DEFAULT_PROFILE_ID.to_string())
    })
}

pub fn profile_dir(id: &str) -> PathBuf {
    if id == DEFAULT_PROFILE_ID {
        base_dir()
    } else {
        base_dir().join("profiles").join(id)
    }
}

pub fn data_dir() -> PathBuf {
    profile_dir(active_profile_id())
}

fn ensure_data_dir() -> Result<PathBuf, String> {
    let dir = data_dir();
    fs::create_dir_all(&dir).map_err(|e| format!("Failed to create data dir: {}", e))?;
    fs::create_dir_all(dir.join("conversations")).ok();
    Ok(dir)
}

// ─────────────────────────────────────────────
//  Master key: macOS Keychain / Windows DPAPI
// ─────────────────────────────────────────────

/// Process-lifetime cache of the master key. Without it every encrypt and
/// decrypt shells out to `/usr/bin/security` - including once per agent inside
/// load_agents() and on every file-watcher reload - which is slow and spams the
/// process table. The key never changes within a run, so one read is enough.
static MASTER_KEY: std::sync::OnceLock<[u8; 32]> = std::sync::OnceLock::new();

fn get_or_create_master_key() -> Result<[u8; 32], String> {
    if let Some(k) = MASTER_KEY.get() {
        return Ok(*k);
    }
    // Serialize first-time creation: concurrent first callers must never
    // each mint a key and clobber the other's stored copy (reachable in
    // parallel tests on Windows, where the key is a DPAPI-wrapped file).
    static CREATE: std::sync::Mutex<()> = std::sync::Mutex::new(());
    let _g = CREATE.lock().unwrap_or_else(|e| e.into_inner());
    if let Some(k) = MASTER_KEY.get() {
        return Ok(*k);
    }
    let key = obtain_master_key()?;
    Ok(*MASTER_KEY.get_or_init(|| key))
}

fn obtain_master_key() -> Result<[u8; 32], String> {
    #[cfg(target_os = "windows")]
    {
        get_or_create_master_key_dpapi()
    }
    #[cfg(not(target_os = "windows"))]
    {
        // Try to read from Keychain
        if let Ok(key) = read_keychain_key() {
            return Ok(key);
        }
        // Generate a new random key and store it in the Keychain
        let mut key = [0u8; 32];
        OsRng.fill_bytes(&mut key);
        store_keychain_key(&key)?;
        Ok(key)
    }
}

/// Windows: the master key lives in a file in the BASE dir (shared by
/// every workspace profile, like the macOS Keychain entry), wrapped with
/// DPAPI (CryptProtectData, per-user scope). Only this Windows user
/// account can unwrap it.
#[cfg(target_os = "windows")]
fn get_or_create_master_key_dpapi() -> Result<[u8; 32], String> {
    let path = base_dir().join(".master.key");
    if let Ok(blob) = fs::read(&path) {
        let bytes = dpapi_unprotect(&blob)?;
        if bytes.len() == 32 {
            let mut key = [0u8; 32];
            key.copy_from_slice(&bytes);
            return Ok(key);
        }
    }

    let mut key = [0u8; 32];
    OsRng.fill_bytes(&mut key);
    let blob = dpapi_protect(&key)?;
    fs::create_dir_all(base_dir()).map_err(|e| format!("create base dir: {}", e))?;
    if let Err(e) = atomic_write(&path, blob) {
        // Another process may have won the race to create the key file;
        // their key is the canonical one - use it.
        if let Ok(existing) = fs::read(&path) {
            let bytes = dpapi_unprotect(&existing)?;
            if bytes.len() == 32 {
                let mut k = [0u8; 32];
                k.copy_from_slice(&bytes);
                return Ok(k);
            }
        }
        return Err(format!("write master key: {}", e));
    }
    Ok(key)
}

#[cfg(target_os = "windows")]
fn dpapi_protect(data: &[u8]) -> Result<Vec<u8>, String> {
    use windows::Win32::Foundation::{LocalFree, HLOCAL};
    use windows::Win32::Security::Cryptography::{CryptProtectData, CRYPT_INTEGER_BLOB};
    unsafe {
        let input = CRYPT_INTEGER_BLOB {
            cbData: data.len() as u32,
            pbData: data.as_ptr() as *mut u8,
        };
        let mut output = CRYPT_INTEGER_BLOB::default();
        CryptProtectData(&input, None, None, None, None, 0, &mut output)
            .map_err(|e| format!("DPAPI protect failed: {}", e))?;
        let out = std::slice::from_raw_parts(output.pbData, output.cbData as usize).to_vec();
        let _ = LocalFree(HLOCAL(output.pbData as *mut core::ffi::c_void));
        Ok(out)
    }
}

#[cfg(target_os = "windows")]
fn dpapi_unprotect(blob: &[u8]) -> Result<Vec<u8>, String> {
    use windows::Win32::Foundation::{LocalFree, HLOCAL};
    use windows::Win32::Security::Cryptography::{CryptUnprotectData, CRYPT_INTEGER_BLOB};
    unsafe {
        let input = CRYPT_INTEGER_BLOB {
            cbData: blob.len() as u32,
            pbData: blob.as_ptr() as *mut u8,
        };
        let mut output = CRYPT_INTEGER_BLOB::default();
        CryptUnprotectData(&input, None, None, None, None, 0, &mut output)
            .map_err(|e| format!("DPAPI unprotect failed: {}", e))?;
        let out = std::slice::from_raw_parts(output.pbData, output.cbData as usize).to_vec();
        let _ = LocalFree(HLOCAL(output.pbData as *mut core::ffi::c_void));
        Ok(out)
    }
}

#[cfg(not(target_os = "windows"))]
fn read_keychain_key() -> Result<[u8; 32], String> {
    let output = std::process::Command::new("security")
        .args([
            "find-generic-password",
            "-s", KEYCHAIN_SERVICE,
            "-a", KEYCHAIN_ACCOUNT,
            "-w",
        ])
        .output()
        .map_err(|e| format!("Keychain read failed: {}", e))?;

    if !output.status.success() {
        return Err("Key not found in Keychain".to_string());
    }

    let encoded = String::from_utf8_lossy(&output.stdout).trim().to_string();
    let bytes = B64.decode(&encoded).map_err(|e| format!("Base64 decode failed: {}", e))?;

    if bytes.len() != 32 {
        return Err("Invalid key length".to_string());
    }

    let mut key = [0u8; 32];
    key.copy_from_slice(&bytes);
    Ok(key)
}

#[cfg(not(target_os = "windows"))]
fn store_keychain_key(key: &[u8; 32]) -> Result<(), String> {
    let encoded = B64.encode(key);

    // Delete existing entry (ignore errors)
    let _ = std::process::Command::new("security")
        .args([
            "delete-generic-password",
            "-s", KEYCHAIN_SERVICE,
            "-a", KEYCHAIN_ACCOUNT,
        ])
        .output();

    let output = std::process::Command::new("security")
        .args([
            "add-generic-password",
            "-s", KEYCHAIN_SERVICE,
            "-a", KEYCHAIN_ACCOUNT,
            "-w", &encoded,
            "-U",
        ])
        .output()
        .map_err(|e| format!("Keychain store failed: {}", e))?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(format!("Keychain store failed: {}", stderr));
    }
    Ok(())
}

// ─────────────────────────────────────────────
//  Encrypt / Decrypt
// ─────────────────────────────────────────────

pub fn encrypt_string(plaintext: &str) -> Result<String, String> {
    let key_bytes = get_or_create_master_key()?;
    let cipher = Aes256Gcm::new_from_slice(&key_bytes)
        .map_err(|e| format!("Cipher init failed: {}", e))?;

    let mut nonce_bytes = [0u8; NONCE_LEN];
    OsRng.fill_bytes(&mut nonce_bytes);
    let nonce = Nonce::from_slice(&nonce_bytes);

    let ciphertext = cipher
        .encrypt(nonce, plaintext.as_bytes())
        .map_err(|e| format!("Encryption failed: {}", e))?;

    // Format: base64(nonce + ciphertext)
    let mut combined = Vec::with_capacity(NONCE_LEN + ciphertext.len());
    combined.extend_from_slice(&nonce_bytes);
    combined.extend_from_slice(&ciphertext);

    Ok(B64.encode(&combined))
}

pub fn decrypt_string(encoded: &str) -> Result<String, String> {
    let key_bytes = get_or_create_master_key()?;
    let cipher = Aes256Gcm::new_from_slice(&key_bytes)
        .map_err(|e| format!("Cipher init failed: {}", e))?;

    let combined = B64.decode(encoded).map_err(|e| format!("Base64 decode failed: {}", e))?;

    if combined.len() < NONCE_LEN {
        return Err("Invalid encrypted data".to_string());
    }

    let (nonce_bytes, ciphertext) = combined.split_at(NONCE_LEN);
    let nonce = Nonce::from_slice(nonce_bytes);

    let plaintext = cipher
        .decrypt(nonce, ciphertext)
        .map_err(|_| "Decryption failed - key may have changed".to_string())?;

    String::from_utf8(plaintext).map_err(|e| format!("UTF-8 decode failed: {}", e))
}

// ─────────────────────────────────────────────
//  JSON file persistence
// ─────────────────────────────────────────────

/// Crash-safe replacement for `fs::write`: writes to a temp file in the
/// same directory, fsyncs, then atomically renames over the target.
/// A `kill -9` mid-save leaves either the old file or the new file,
/// never a truncated one. Drop-in signature match with `fs::write`.
pub fn atomic_write<P: AsRef<std::path::Path>, C: AsRef<[u8]>>(
    path: P,
    contents: C,
) -> std::io::Result<()> {
    use std::io::Write;
    let path = path.as_ref();
    let parent = match path.parent() {
        Some(p) if !p.as_os_str().is_empty() => p.to_path_buf(),
        _ => PathBuf::from("."),
    };
    let mut tmp = tempfile::NamedTempFile::new_in(&parent)?;
    tmp.write_all(contents.as_ref())?;
    tmp.as_file().sync_all()?;
    tmp.persist(path).map_err(|e| e.error)?;
    Ok(())
}

pub fn save_json<T: Serialize + ?Sized>(filename: &str, data: &T) -> Result<(), String> {
    let dir = ensure_data_dir()?;
    let path = dir.join(filename);
    let json = serde_json::to_string_pretty(data)
        .map_err(|e| format!("JSON serialize failed: {}", e))?;
    atomic_write(&path, json).map_err(|e| format!("File write failed: {}", e))?;
    Ok(())
}

pub fn load_json<T: for<'de> Deserialize<'de>>(filename: &str) -> Result<T, String> {
    let dir = data_dir();
    let path = dir.join(filename);
    if !path.exists() {
        return Err("File not found".to_string());
    }
    let json = fs::read_to_string(&path).map_err(|e| format!("File read failed: {}", e))?;
    serde_json::from_str(&json).map_err(|e| format!("JSON parse failed: {}", e))
}

pub fn delete_file(filename: &str) -> Result<(), String> {
    let dir = data_dir();
    let path = dir.join(filename);
    if path.exists() {
        fs::remove_file(&path).map_err(|e| format!("Delete failed: {}", e))?;
    }
    Ok(())
}

// ─────────────────────────────────────────────
//  Agent persistence (encrypts API keys)
// ─────────────────────────────────────────────

#[derive(Serialize, Deserialize)]
struct StoredAgent {
    id: String,
    name: String,
    provider: String,
    model: String,
    endpoint: String,
    api_key_encrypted: String,
    enabled: bool,
    is_default: bool,
    created_at: String,
    #[serde(default)]
    connector_ids: Vec<String>,
    #[serde(default)]
    workflow_ids: Vec<String>,
    #[serde(default)]
    charter: String,
    #[serde(default)]
    granted_tools: Vec<String>,
    #[serde(default)]
    schedule_minutes: Option<u64>,
    #[serde(default)]
    last_run_at: Option<String>,
}

pub fn save_agents(agents: &[super::Agent]) -> Result<(), String> {
    let stored: Vec<StoredAgent> = agents
        .iter()
        .map(|a| {
            let encrypted_key = if a.api_key.is_empty() {
                String::new()
            } else {
                encrypt_string(&a.api_key).unwrap_or_default()
            };
            StoredAgent {
                id: a.id.clone(),
                name: a.name.clone(),
                provider: serde_json::to_string(&a.provider).unwrap_or_default().trim_matches('"').to_string(),
                model: a.model.clone(),
                endpoint: a.endpoint.clone(),
                api_key_encrypted: encrypted_key,
                enabled: a.enabled,
                is_default: a.is_default,
                created_at: a.created_at.clone(),
                connector_ids: a.connector_ids.clone(),
                workflow_ids: a.workflow_ids.clone(),
                charter: a.charter.clone(),
                granted_tools: a.granted_tools.clone(),
                schedule_minutes: a.schedule_minutes,
                last_run_at: a.last_run_at.clone(),
            }
        })
        .collect();
    save_json("agents.json", &stored)
}

pub fn load_agents() -> Result<Vec<super::Agent>, String> {
    let stored: Vec<StoredAgent> = load_json("agents.json")?;
    let agents = stored
        .into_iter()
        .map(|s| {
            let api_key = if s.api_key_encrypted.is_empty() {
                String::new()
            } else {
                decrypt_string(&s.api_key_encrypted).unwrap_or_default()
            };
            let provider_str = format!("\"{}\"", s.provider);
            let provider: super::AgentProvider =
                serde_json::from_str(&provider_str).unwrap_or(super::AgentProvider::Custom);
            super::Agent {
                id: s.id,
                name: s.name,
                provider,
                model: s.model,
                endpoint: s.endpoint,
                api_key,
                enabled: s.enabled,
                is_default: s.is_default,
                created_at: s.created_at,
                connector_ids: s.connector_ids,
                workflow_ids: s.workflow_ids,
                capabilities: super::agent_router::AgentCapabilities::default(),
                charter: s.charter,
                granted_tools: s.granted_tools,
                schedule_minutes: s.schedule_minutes,
                last_run_at: s.last_run_at,
            }
        })
        .collect();
    Ok(agents)
}

// ─────────────────────────────────────────────
//  Conversation persistence
// ─────────────────────────────────────────────

#[derive(Serialize, Deserialize)]
pub struct ConversationSummary {
    pub id: String,
    pub title: String,
    pub message_count: usize,
    pub last_message_at: String,
    pub created_at: String,
}

#[derive(Serialize, Deserialize)]
struct StoredConversation {
    id: String,
    title: String,
    messages: Vec<super::Message>,
    created_at: String,
}

pub fn save_conversation(id: &str, title: &str, messages: &[super::Message]) -> Result<(), String> {
    let conv = StoredConversation {
        id: id.to_string(),
        title: title.to_string(),
        messages: messages.to_vec(),
        created_at: messages
            .first()
            .map(|m| m.timestamp.clone())
            .unwrap_or_else(|| chrono::Utc::now().to_rfc3339()),
    };
    save_json(&format!("conversations/{}.json", id), &conv)
}

pub fn load_conversation(id: &str) -> Result<Vec<super::Message>, String> {
    let conv: StoredConversation = load_json(&format!("conversations/{}.json", id))?;
    Ok(conv.messages)
}

pub fn rename_conversation(id: &str, new_title: &str) -> Result<(), String> {
    let mut conv: StoredConversation = load_json(&format!("conversations/{}.json", id))?;
    conv.title = new_title.to_string();
    save_json(&format!("conversations/{}.json", id), &conv)
}

pub fn list_conversations() -> Result<Vec<ConversationSummary>, String> {
    let dir = data_dir().join("conversations");
    if !dir.exists() {
        return Ok(Vec::new());
    }

    let mut summaries = Vec::new();
    let entries = fs::read_dir(&dir).map_err(|e| format!("Read dir failed: {}", e))?;

    for entry in entries.flatten() {
        let path = entry.path();
        if path.extension().map(|e| e == "json").unwrap_or(false) {
            if let Ok(json) = fs::read_to_string(&path) {
                if let Ok(conv) = serde_json::from_str::<StoredConversation>(&json) {
                    summaries.push(ConversationSummary {
                        id: conv.id,
                        title: conv.title,
                        message_count: conv.messages.len(),
                        last_message_at: conv
                            .messages
                            .last()
                            .map(|m| m.timestamp.clone())
                            .unwrap_or_default(),
                        created_at: conv.created_at,
                    });
                }
            }
        }
    }

    summaries.sort_by(|a, b| b.last_message_at.cmp(&a.last_message_at));
    Ok(summaries)
}

pub fn delete_conversation(id: &str) -> Result<(), String> {
    delete_file(&format!("conversations/{}.json", id))
}

// ─────────────────────────────────────────────
//  MCP server persistence
// ─────────────────────────────────────────────

pub fn save_mcp_servers(servers: &[super::McpServer]) -> Result<(), String> {
    save_json("mcp_servers.json", servers)
}

pub fn load_mcp_servers() -> Result<Vec<super::McpServer>, String> {
    load_json("mcp_servers.json")
}

// ─────────────────────────────────────────────
//  Role & Onboarding persistence (Epic T)
// ─────────────────────────────────────────────

pub fn save_onboarding(onb: &super::onboarding::UserOnboarding) -> Result<(), String> {
    save_json("onboarding.json", onb)
}

pub fn load_onboarding() -> Result<super::onboarding::UserOnboarding, String> {
    load_json("onboarding.json")
}



// ─────────────────────────────────────────────
//  Workflow persistence
// ─────────────────────────────────────────────

pub fn save_workflows(workflows: &[super::workflow::Workflow]) -> Result<(), String> {
    save_json("workflows.json", workflows)
}

pub fn load_workflows() -> Result<Vec<super::workflow::Workflow>, String> {
    load_json("workflows.json")
}

// ─────────────────────────────────────────────
//  Twin persistence
// ─────────────────────────────────────────────

pub fn save_twin_graph(graph: &super::semantic_twin::SemanticGraph) -> Result<(), String> {
    save_json("twin_graph.json", graph)
}

pub fn load_twin_graph() -> Result<super::semantic_twin::SemanticGraph, String> {
    load_json("twin_graph.json")
}

pub fn save_twin_events(ledger: &super::event_ledger::EventLedger) -> Result<(), String> {
    save_json("twin_events.json", ledger)
}

pub fn load_twin_events() -> Result<super::event_ledger::EventLedger, String> {
    load_json("twin_events.json")
}

// ─────────────────────────────────────────────
//  Tests - atomic write crash safety (W0.5)
// ─────────────────────────────────────────────

#[cfg(test)]
mod atomic_tests {
    use super::*;
    use std::time::Duration;

    #[test]
    fn atomic_write_replaces_contents() {
        let dir = tempfile::tempdir().unwrap();
        let target = dir.path().join("store.json");
        atomic_write(&target, b"first").unwrap();
        atomic_write(&target, b"second").unwrap();
        assert_eq!(std::fs::read(&target).unwrap(), b"second");
        // no stray temp files left behind
        let leftovers: Vec<_> = std::fs::read_dir(dir.path())
            .unwrap()
            .filter_map(|e| e.ok())
            .filter(|e| e.file_name() != "store.json")
            .collect();
        assert!(leftovers.is_empty(), "temp files left: {:?}", leftovers);
    }

    /// Child mode for the kill -9 harness: loops atomic writes of large
    /// alternating payloads until killed. Not a real test on its own.
    #[test]
    fn crash_child_loop() {
        let Ok(target) = std::env::var("ORBIX_CRASH_TARGET") else {
            return; // normal test runs skip this
        };
        let a = vec![b'A'; 1_000_000];
        let b = vec![b'B'; 1_000_000];
        let mut i: u64 = 0;
        loop {
            let payload = if i % 2 == 0 { &a } else { &b };
            let _ = atomic_write(&target, payload);
            i += 1;
        }
    }

    /// W0 acceptance: kill -9 mid-save never corrupts a store.
    #[test]
    fn kill9_never_corrupts() {
        let dir = tempfile::tempdir().unwrap();
        let target = dir.path().join("store.bin");
        atomic_write(&target, vec![b'A'; 1_000_000]).unwrap();
        let exe = std::env::current_exe().unwrap();
        for round in 0u64..8 {
            let mut child = std::process::Command::new(&exe)
                .arg("crash_child_loop")
                .arg("--nocapture")
                .arg("--test-threads=1")
                .env("ORBIX_CRASH_TARGET", &target)
                .stdout(std::process::Stdio::null())
                .stderr(std::process::Stdio::null())
                .spawn()
                .unwrap();
            std::thread::sleep(Duration::from_millis(25 + round * 13));
            child.kill().unwrap(); // SIGKILL
            let _ = child.wait();
            let data = std::fs::read(&target).unwrap();
            assert_eq!(data.len(), 1_000_000, "round {}: truncated store", round);
            let first = data[0];
            assert!(first == b'A' || first == b'B', "round {}: garbage byte", round);
            assert!(
                data.iter().all(|&c| c == first),
                "round {}: mixed content - torn write",
                round
            );
        }
    }
}

#[cfg(test)]
mod profile_tests {
    use super::*;

    #[test]
    fn default_profile_maps_to_base_dir_and_always_exists() {
        assert_eq!(profile_dir(DEFAULT_PROFILE_ID), base_dir());
        let reg = load_profile_registry();
        assert!(reg.profiles.iter().any(|p| p.id == DEFAULT_PROFILE_ID));
        // Named profiles nest strictly under base/profiles.
        let p = profile_dir("some-uuid");
        assert!(p.starts_with(base_dir().join("profiles")));
        assert_ne!(p, base_dir());
    }
}
