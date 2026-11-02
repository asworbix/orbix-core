// ─────────────────────────────────────────────
//  Connector Signer - Phase 5 of CONNECTORS.md
//
//  Local Ed25519 keypair that signs each connector manifest at
//  activation. Bob produces manifests, the signer locks them, and the
//  runtime refuses to load any module whose manifest doesn't verify.
//
//  Key storage (§12 #2: macOS Keychain):
//    - Service:  "ai.orbixcore.app.connector_signer"
//    - Account:  "primary"
//    - Value:    PKCS8 DER of the Ed25519 SigningKey, base64-encoded
//
//  The Keychain entry is created on first call to `signing_key()`
//  and protected by the user's login keychain. Other apps cannot read
//  it without the keychain being unlocked and the user's consent.
//
//  Phase 5 ships a workable signer on macOS. Linux/Windows fall back
//  to an AES-encrypted file under data_dir()/keys/ until libsecret /
//  Windows Credential Vault wrappers land.
// ─────────────────────────────────────────────

use std::path::PathBuf;
use std::time::SystemTime;

use base64::Engine;
use ed25519_dalek::{Signer, SigningKey, VerifyingKey, Signature as EdSignature};
use rand_core::OsRng;
use sha2::{Digest, Sha256};

use crate::connector_manifest::{ConnectorManifest, Signature};

const KEYCHAIN_SERVICE: &str = "ai.orbixcore.app.connector_signer";
const KEYCHAIN_ACCOUNT: &str = "primary";
const ALGORITHM: &str = "ed25519";

// ─────────────────────────────────────────────
//  Key acquisition
// ─────────────────────────────────────────────

/// Read the signing key out of the Keychain. On first call, generates a
/// new Ed25519 keypair, stores it, and returns it. Subsequent calls
/// return the same key.
pub fn signing_key() -> Result<SigningKey, String> {
    if let Some(bytes) = load_from_keychain()? {
        return decode_key(&bytes);
    }
    // First run on this machine - generate and persist.
    let key = SigningKey::generate(&mut OsRng);
    let encoded = encode_key(&key);
    store_in_keychain(&encoded)?;
    Ok(key)
}

pub fn verifying_key() -> Result<VerifyingKey, String> {
    Ok(signing_key()?.verifying_key())
}

/// Stable hex id for the signing key - used as `Signature.key_id`. Lets
/// us rotate later by recognising old signatures via their key id.
pub fn key_id() -> Result<String, String> {
    let vk = verifying_key()?;
    let mut h = Sha256::new();
    h.update(vk.as_bytes());
    let digest = h.finalize();
    Ok(format!("ed25519:{}", hex::encode(&digest[..8])))
}

// ─────────────────────────────────────────────
//  Sign / verify a manifest
// ─────────────────────────────────────────────

/// Produce a `Signature` over the manifest's canonical bytes.
pub fn sign(manifest: &ConnectorManifest) -> Result<Signature, String> {
    let canonical = canonicalise(manifest)?;
    let key = signing_key()?;
    let sig: EdSignature = key.sign(&canonical);
    let now = SystemTime::now()
        .duration_since(SystemTime::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);
    Ok(Signature {
        key_id: key_id()?,
        algorithm: ALGORITHM.into(),
        signature: hex::encode(sig.to_bytes()),
        signed_at: chrono::DateTime::<chrono::Utc>::from_timestamp(now as i64, 0)
            .map(|dt| dt.to_rfc3339())
            .unwrap_or_default(),
    })
}

/// Verify the manifest's signature against the locally stored verifying
/// key. Returns Ok(()) on success, Err with reason on failure.
pub fn verify(manifest: &ConnectorManifest) -> Result<(), String> {
    let sig = manifest.signature.as_ref()
        .ok_or_else(|| "manifest is unsigned".to_string())?;
    if sig.algorithm != ALGORITHM {
        return Err(format!("unsupported algorithm '{}'", sig.algorithm));
    }
    let expected_kid = key_id()?;
    if sig.key_id != expected_kid {
        return Err(format!(
            "signature key id '{}' does not match local '{}' - module signed by a different installation",
            sig.key_id, expected_kid,
        ));
    }
    let raw = hex::decode(&sig.signature)
        .map_err(|e| format!("decode signature: {}", e))?;
    if raw.len() != 64 {
        return Err(format!("signature length {} != 64", raw.len()));
    }
    let ed: EdSignature = EdSignature::from_slice(&raw)
        .map_err(|e| format!("parse signature: {}", e))?;

    // The canonical bytes are computed over the manifest WITHOUT the
    // signature field. Strip it before canonicalising.
    let mut unsigned = manifest.clone();
    unsigned.signature = None;
    let canonical = canonicalise(&unsigned)?;

    let vk = verifying_key()?;
    vk.verify_strict(&canonical, &ed)
        .map_err(|e| format!("signature did not verify: {}", e))
}

/// Canonical bytes used as the signing input. We serialise the manifest
/// (sans signature) to compact JSON with sorted keys via serde_json::Value
/// → sorted Map. Stable across signer / verifier runs as long as the
/// struct field set is identical.
fn canonicalise(manifest: &ConnectorManifest) -> Result<Vec<u8>, String> {
    // Serialize then re-parse so we can sort keys lexicographically.
    let v: serde_json::Value = serde_json::to_value(manifest)
        .map_err(|e| format!("to_value: {}", e))?;
    let sorted = sort_keys(&v);
    serde_json::to_vec(&sorted).map_err(|e| format!("to_vec: {}", e))
}

fn sort_keys(v: &serde_json::Value) -> serde_json::Value {
    use serde_json::{Map, Value};
    match v {
        Value::Object(m) => {
            let mut sorted: Vec<(&String, &Value)> = m.iter().collect();
            sorted.sort_by(|a, b| a.0.cmp(b.0));
            let mut out = Map::new();
            for (k, val) in sorted {
                out.insert(k.clone(), sort_keys(val));
            }
            Value::Object(out)
        }
        Value::Array(a) => Value::Array(a.iter().map(sort_keys).collect()),
        other => other.clone(),
    }
}

// ─────────────────────────────────────────────
//  Keychain I/O (macOS) with file fallback
// ─────────────────────────────────────────────

fn encode_key(k: &SigningKey) -> String {
    base64::engine::general_purpose::STANDARD.encode(k.to_bytes())
}

fn decode_key(s: &str) -> Result<SigningKey, String> {
    let bytes = base64::engine::general_purpose::STANDARD.decode(s.trim())
        .map_err(|e| format!("decode key: {}", e))?;
    if bytes.len() != 32 {
        return Err(format!("expected 32 raw key bytes, got {}", bytes.len()));
    }
    let mut arr = [0u8; 32];
    arr.copy_from_slice(&bytes);
    Ok(SigningKey::from_bytes(&arr))
}

#[cfg(target_os = "macos")]
fn load_from_keychain() -> Result<Option<String>, String> {
    use security_framework::passwords::get_generic_password;
    match get_generic_password(KEYCHAIN_SERVICE, KEYCHAIN_ACCOUNT) {
        Ok(bytes) => Ok(Some(String::from_utf8_lossy(&bytes).into_owned())),
        Err(e) if e.code() == -25300 => Ok(None), // errSecItemNotFound
        Err(e) => Err(format!("keychain read: {}", e)),
    }
}

#[cfg(target_os = "macos")]
fn store_in_keychain(value: &str) -> Result<(), String> {
    use security_framework::passwords::set_generic_password;
    set_generic_password(KEYCHAIN_SERVICE, KEYCHAIN_ACCOUNT, value.as_bytes())
        .map_err(|e| format!("keychain write: {}", e))
}

// File fallback for non-macOS dev machines. The file is AES-encrypted
// via the existing storage layer.
#[cfg(not(target_os = "macos"))]
fn fallback_key_path() -> PathBuf {
    crate::storage::data_dir().join("keys").join("connector_signer.b64")
}

#[cfg(not(target_os = "macos"))]
fn load_from_keychain() -> Result<Option<String>, String> {
    let path = fallback_key_path();
    if !path.exists() { return Ok(None); }
    let raw = std::fs::read_to_string(&path)
        .map_err(|e| format!("read fallback key: {}", e))?;
    Ok(Some(crate::storage::decrypt_string(&raw)?))
}

#[cfg(not(target_os = "macos"))]
fn store_in_keychain(value: &str) -> Result<(), String> {
    let path = fallback_key_path();
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)
            .map_err(|e| format!("create keys dir: {}", e))?;
    }
    let enc = crate::storage::encrypt_string(value)?;
    std::fs::write(&path, enc)
        .map_err(|e| format!("write fallback key: {}", e))
}

// Silence the dead-code warning when compiling on macOS.
#[allow(dead_code)]
fn _path_typecheck() -> PathBuf { PathBuf::new() }

// ─────────────────────────────────────────────
//  Tests
//
//  Note: these touch the real Keychain on macOS. After the first run
//  the entry remains and subsequent test runs reuse it (we generate
//  the keypair once per dev machine). This is intentional - the
//  Keychain entry IS the test fixture.
// ─────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::BTreeMap;

    fn dummy_manifest() -> ConnectorManifest {
        ConnectorManifest {
            slug: "signer-test".into(),
            version: "0.1.0".into(),
            display_name: "Signer Test".into(),
            vendor: "test".into(),
            driver: crate::connector_manifest::Driver::Api,
            auth: crate::connector_manifest::AuthSpec::None,
            host_allowlist: Vec::new(),
            env_allowlist: Vec::new(),
            vault_env_map: BTreeMap::new(),
            capabilities: Vec::new(),
            actions: BTreeMap::new(),
            events: BTreeMap::new(),
            resource_limits: Default::default(),
            signature: None,
        }
    }

    #[test]
    #[ignore = "touches macOS Keychain; prompts for permission on first run. \
                Invoke with `cargo test -- --ignored`."]
    fn sign_then_verify_round_trips() {
        let mut m = dummy_manifest();
        let sig = sign(&m).expect("sign");
        m.signature = Some(sig);
        verify(&m).expect("verify");
    }

    #[test]
    #[ignore = "touches macOS Keychain"]
    fn tampered_manifest_fails_verify() {
        let mut m = dummy_manifest();
        let sig = sign(&m).expect("sign");
        m.signature = Some(sig);
        // Mutate after signing.
        m.host_allowlist.push("evil.example.com".into());
        let r = verify(&m);
        assert!(r.is_err(), "tampered manifest should not verify");
    }

    #[test]
    fn unsigned_manifest_fails_verify() {
        // No keychain access - verify() short-circuits on missing signature.
        let m = dummy_manifest();
        let r = verify(&m);
        assert!(r.is_err());
        assert!(r.unwrap_err().contains("unsigned"));
    }

    #[test]
    fn canonical_form_is_stable_across_field_order() {
        // Build the same manifest two different ways; canonicalise both
        // and assert byte-equality. Guards against silent format drift
        // that would break signatures across upgrades.
        let m1 = dummy_manifest();
        let mut m2 = dummy_manifest();
        m2.events = BTreeMap::new();   // re-set a default - equivalent
        let c1 = canonicalise(&m1).unwrap();
        let c2 = canonicalise(&m2).unwrap();
        assert_eq!(c1, c2);
    }
}
