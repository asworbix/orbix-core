// ─────────────────────────────────────────────
//  Connector Vault - Phase 4 of CONNECTORS.md
//
//  Slug-keyed credential store for connector modules. Separate from the
//  legacy instance-keyed vault in connectors.rs so built-ins and modules
//  cannot collide.
//
//  Invariants:
//    - Values are AES-encrypted at rest via storage::encrypt_string.
//    - Tokens are injected into the connector-agent's env at spawn time
//      and never returned across the bus (the brief, §2 / §7).
//    - Module slug is the only public key surface. No "vault_key" guess
//      from a different slug ever yields another module's secret.
//    - Filenames are slug+key sanitised so an attacker controlling the
//      slug string cannot path-traverse out of the vault dir.
// ─────────────────────────────────────────────

use std::path::PathBuf;

use serde::{Deserialize, Serialize};

use crate::storage;

const VAULT_SUBDIR: &str = "connector_vault";

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExpiryHint {
    /// Unix epoch seconds at which the stored token is considered stale.
    /// Modules SHOULD refresh shortly before this. The L-line never sees
    /// the token itself.
    pub expires_at_unix: Option<u64>,
    pub has_refresh_token: bool,
}

fn vault_dir() -> PathBuf { storage::data_dir().join(VAULT_SUBDIR) }

/// Filename-safe key: lowercase, only [a-z0-9_-]. Anything else becomes
/// '_'. Prevents path traversal and case collisions on macOS HFS+.
fn sanitise(part: &str) -> String {
    part.chars()
        .map(|c| match c {
            'a'..='z' | '0'..='9' | '-' | '_' => c,
            'A'..='Z' => c.to_ascii_lowercase(),
            _ => '_',
        })
        .collect()
}

fn entry_path(slug: &str, key: &str) -> PathBuf {
    let filename = format!("{}__{}.enc", sanitise(slug), sanitise(key));
    vault_dir().join(filename)
}

/// Write a value into the vault. Existing entries are overwritten.
pub fn put(slug: &str, key: &str, value: &str) -> Result<(), String> {
    let dir = vault_dir();
    std::fs::create_dir_all(&dir)
        .map_err(|e| format!("create vault dir: {}", e))?;
    let encrypted = storage::encrypt_string(value)?;
    crate::storage::atomic_write(entry_path(slug, key), encrypted)
        .map_err(|e| format!("write vault entry: {}", e))
}

/// Read a value out of the vault. Missing entries return Err so callers
/// can distinguish "no token" from "decrypt failure".
pub fn get(slug: &str, key: &str) -> Result<String, String> {
    let path = entry_path(slug, key);
    if !path.exists() {
        return Err(format!("vault entry missing: {}/{}", slug, key));
    }
    let encrypted = std::fs::read_to_string(&path)
        .map_err(|e| format!("read vault entry: {}", e))?;
    storage::decrypt_string(&encrypted)
}

/// True if the entry exists, even if it cannot be decrypted (so callers
/// can decide whether to re-OAuth).
pub fn exists(slug: &str, key: &str) -> bool {
    entry_path(slug, key).exists()
}

/// Drop a single entry. Idempotent.
pub fn delete(slug: &str, key: &str) {
    let _ = std::fs::remove_file(entry_path(slug, key));
}

/// Drop every entry belonging to a slug. Used at uninstall.
pub fn delete_slug(slug: &str) {
    let prefix = format!("{}__", sanitise(slug));
    let Ok(entries) = std::fs::read_dir(vault_dir()) else { return };
    for entry in entries.flatten() {
        if entry.file_name().to_string_lossy().starts_with(&prefix) {
            let _ = std::fs::remove_file(entry.path());
        }
    }
}

/// Keys present for a slug. Values are never returned by this function -
/// only the vault keys (e.g. "access_token", "refresh_token") so the UI
/// can show "what's stored" without revealing the secret.
pub fn list_keys(slug: &str) -> Vec<String> {
    let prefix = format!("{}__", sanitise(slug));
    let Ok(entries) = std::fs::read_dir(vault_dir()) else { return Vec::new() };
    let mut out: Vec<String> = entries
        .filter_map(|e| e.ok())
        .filter_map(|e| e.file_name().into_string().ok())
        .filter(|n| n.starts_with(&prefix) && n.ends_with(".enc"))
        .map(|n| n[prefix.len()..n.len() - 4].to_string())
        .collect();
    out.sort();
    out
}

/// Store a full OAuth result atomically: access_token + optional
/// refresh_token + optional expiry. Returns ONLY an ExpiryHint; the
/// caller never sees the token bytes.
pub fn put_oauth_bundle(
    slug: &str,
    access_token: &str,
    refresh_token: Option<&str>,
    expires_in_seconds: Option<u64>,
) -> Result<ExpiryHint, String> {
    put(slug, "access_token", access_token)?;
    let has_refresh_token = match refresh_token {
        Some(rt) if !rt.is_empty() => {
            put(slug, "refresh_token", rt)?;
            true
        }
        _ => false,
    };
    let expires_at_unix = expires_in_seconds.map(|secs| {
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_secs())
            .unwrap_or(0);
        now + secs
    });
    if let Some(exp) = expires_at_unix {
        put(slug, "expires_at_unix", &exp.to_string())?;
    }
    Ok(ExpiryHint { expires_at_unix, has_refresh_token })
}

// ─────────────────────────────────────────────
//  Tests
//
//  Note: these touch the real data_dir() so they share state with the
//  app vault on dev machines. Each test uses a unique slug prefix to
//  avoid collisions.
// ─────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    fn unique_slug(tag: &str) -> String {
        format!("test-vault-{}-{}", tag, uuid::Uuid::new_v4())
    }

    #[test]
    fn round_trip_put_get() {
        let slug = unique_slug("roundtrip");
        put(&slug, "access_token", "sk-secret-abc123").unwrap();
        assert_eq!(get(&slug, "access_token").unwrap(), "sk-secret-abc123");
        delete_slug(&slug);
    }

    #[test]
    fn list_keys_does_not_leak_values() {
        let slug = unique_slug("listkeys");
        put(&slug, "access_token", "VALUE_A").unwrap();
        put(&slug, "refresh_token", "VALUE_B").unwrap();
        let keys = list_keys(&slug);
        assert_eq!(keys, vec!["access_token", "refresh_token"]);
        // None of the values appear in the listing - the function returns
        // key names only.
        for k in &keys {
            assert!(!k.contains("VALUE"));
        }
        delete_slug(&slug);
    }

    #[test]
    fn put_oauth_bundle_writes_token_and_returns_hint_only() {
        let slug = unique_slug("oauth");
        let hint = put_oauth_bundle(&slug, "TOK", Some("REF"), Some(3600)).unwrap();
        assert!(hint.has_refresh_token);
        assert!(hint.expires_at_unix.is_some());
        // The hint struct must NOT carry the token in any field.
        let serialised = serde_json::to_string(&hint).unwrap();
        assert!(!serialised.contains("TOK"));
        assert!(!serialised.contains("REF"));
        delete_slug(&slug);
    }

    #[test]
    fn delete_slug_removes_every_entry() {
        let slug = unique_slug("delete");
        put(&slug, "access_token", "x").unwrap();
        put(&slug, "refresh_token", "y").unwrap();
        assert_eq!(list_keys(&slug).len(), 2);
        delete_slug(&slug);
        assert!(list_keys(&slug).is_empty());
    }

    #[test]
    fn sanitise_blocks_path_traversal_in_slug() {
        let slug = "..//etc/passwd";
        let path = entry_path(slug, "access_token");
        // The dangerous characters get replaced. The file ends up inside
        // vault_dir(), not somewhere above it.
        let path_str = path.to_string_lossy();
        assert!(!path_str.contains(".."));
        assert!(!path_str.contains("//"));
    }

    #[test]
    fn isolation_two_slugs_cannot_read_each_other() {
        let a = unique_slug("iso-a");
        let b = unique_slug("iso-b");
        put(&a, "access_token", "A_SECRET").unwrap();
        put(&b, "access_token", "B_SECRET").unwrap();
        assert_eq!(get(&a, "access_token").unwrap(), "A_SECRET");
        assert_eq!(get(&b, "access_token").unwrap(), "B_SECRET");
        delete_slug(&a);
        delete_slug(&b);
    }
}
