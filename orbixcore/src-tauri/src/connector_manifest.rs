// ─────────────────────────────────────────────
//  Connector Manifest - Phase 2 of CONNECTORS.md (schema only)
//
//  Loaded from
//    <modules_dir>/<slug>/<version>/manifest.json
//  Active version is selected via the `current` symlink in
//    <modules_dir>/<slug>/current → <modules_dir>/<slug>/v3
//
//  The Phase 2 runtime parses this, derives Deno spawn flags from
//  host_allowlist + env_allowlist, and validates that the module's
//  static scan never reaches outside those allow-lists (Phase 5).
//
//  Signature verification is ENFORCED at load (W5.1): load_all_active
//  refuses any module whose signature does not verify against the local
//  key. activate_from_staging signs every module it installs, so a
//  legitimately-activated module always passes; an unsigned or tampered
//  module dropped into the modules dir is refused unless the developer
//  override ORBIX_ALLOW_UNSIGNED_MODULES is set.
// ─────────────────────────────────────────────

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::plugins::Capability;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConnectorManifest {
    pub slug: String,
    pub version: String,
    pub display_name: String,
    pub vendor: String,
    pub driver: Driver,
    pub auth: AuthSpec,
    /// Hosts the module is allowed to reach. Becomes `--allow-net=h1,h2,…`
    /// for Deno. Enforced at the OS level, not in JS.
    #[serde(default)]
    pub host_allowlist: Vec<String>,
    /// Env vars the module is allowed to read. Becomes `--allow-env=K1,K2,…`.
    #[serde(default)]
    pub env_allowlist: Vec<String>,
    /// Map of env var name → vault key. The runtime reads each value out of
    /// connector_vault::get(slug, vault_key) at spawn time and exports it
    /// as the named env var. Tokens never come back across the bus.
    /// Example: { "LINKEDIN_TOKEN": "access_token" }
    #[serde(default)]
    pub vault_env_map: std::collections::BTreeMap<String, String>,
    /// Declared capabilities. Validated against plugins::validate_capability_grant
    /// at activation time (Phase 1 invariant - NetworkOutbound only for Connectors).
    #[serde(default)]
    pub capabilities: Vec<Capability>,
    pub actions: BTreeMap<String, ActionSpec>,
    #[serde(default)]
    pub events: BTreeMap<String, EventSpec>,
    #[serde(default)]
    pub resource_limits: ResourceLimits,
    /// Populated by Bob's signer in Phase 5; unsigned manifests still
    /// load in Phase 2 so hand-authored modules can be tested.
    #[serde(default)]
    pub signature: Option<Signature>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case", tag = "kind")]
pub enum Driver {
    Api,
    BrowserAttach { profile_hint: Option<String> },
    /// Tier-1 app bridge: the connector reads a desktop app through its
    /// AppleScript dictionary via an app_bridge manifest (app_bridge.rs).
    /// No OAuth, no app registration - the user's existing login is the
    /// session.
    NativeScript {
        /// Name of the app_bridge manifest this connector drives,
        /// e.g. "Outlook".
        bridge: String,
    },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case", tag = "kind")]
pub enum AuthSpec {
    #[serde(rename = "oauth2")]
    OAuth2 {
        authorize_url: String,
        token_url: String,
        #[serde(default)]
        scopes: Vec<String>,
        #[serde(default)]
        refresh_supported: bool,
    },
    BrowserSession {
        #[serde(default)]
        cookie_domains: Vec<String>,
    },
    ApiKey {
        header: String,
    },
    None,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ActionSpec {
    pub description: String,
    /// Free-form JSON Schema for inputs. Validated by the bus before
    /// the job reaches the runtime.
    #[serde(default)]
    pub inputs: serde_json::Value,
    #[serde(default)]
    pub outputs: serde_json::Value,
    /// Classified error codes the module may return (e.g. "rate_limited",
    /// "upstream_error"). Used by the bus to drive circuit-breaker logic.
    #[serde(default)]
    pub errors: Vec<String>,
    #[serde(default)]
    pub idempotent: bool,
    #[serde(default)]
    pub rate_limit: Option<RateLimit>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EventSpec {
    pub description: String,
    /// "webhook" or "polled"
    pub kind: String,
    #[serde(default)]
    pub payload: serde_json::Value,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RateLimit {
    pub per_second: Option<u32>,
    pub per_minute: Option<u32>,
}

/// Connector-specific resource limits - tighter than the generic
/// plugins::ResourceLimits because connectors are network-bound,
/// not compute. Decision locked 2026-05-11.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ResourceLimits {
    pub memory_limit_mb: u32,
    pub cpu_limit_percent: u8,
    pub job_timeout_seconds: u32,
    pub max_concurrent: u8,
}

impl Default for ResourceLimits {
    fn default() -> Self {
        Self {
            memory_limit_mb: 128,
            cpu_limit_percent: 15,
            job_timeout_seconds: 15,
            max_concurrent: 4,
        }
    }
}

/// Phase 5 will fill these in. Phase 2 just parses them through.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Signature {
    pub key_id: String,
    pub algorithm: String,
    pub signature: String,
    pub signed_at: String,
}

// ─────────────────────────────────────────────
//  Loading
// ─────────────────────────────────────────────

/// Resolve the active version directory for a module: follows the
/// `current` symlink if present, otherwise picks the highest-named
/// version directory (lexicographic - fine for semver "v1", "v2", "v3").
pub fn resolve_active_version(slug_dir: &Path) -> Option<PathBuf> {
    let current = slug_dir.join("current");
    if current.exists() {
        // canonicalize resolves the symlink
        if let Ok(resolved) = std::fs::canonicalize(&current) {
            if resolved.is_dir() { return Some(resolved); }
        }
    }
    // Fallback: pick the highest-named dir.
    let entries = std::fs::read_dir(slug_dir).ok()?;
    let mut versions: Vec<PathBuf> = entries
        .filter_map(|e| e.ok())
        .filter(|e| e.file_type().map(|t| t.is_dir()).unwrap_or(false))
        .map(|e| e.path())
        .filter(|p| p.file_name().and_then(|n| n.to_str()).map(|s| s != "current").unwrap_or(false))
        .collect();
    versions.sort();
    versions.pop()
}

/// Parse a manifest at `<version_dir>/manifest.json`.
pub fn load_manifest(version_dir: &Path) -> Result<ConnectorManifest, String> {
    let manifest_path = version_dir.join("manifest.json");
    let raw = std::fs::read_to_string(&manifest_path)
        .map_err(|e| format!("read {}: {}", manifest_path.display(), e))?;
    serde_json::from_str(&raw)
        .map_err(|e| format!("parse {}: {}", manifest_path.display(), e))
}

/// Scan `<modules_dir>` and return (slug, version_dir, manifest) for every
/// module whose active version has a parseable manifest. Errors are
/// collected per-module and logged; one bad module never blocks the others.
pub fn load_all_active(modules_dir: &Path) -> Vec<LoadedModule> {
    load_all_active_with(modules_dir, allow_unsigned())
}

/// Selection + signature-gated load. `allow_unsigned` is threaded in so
/// selection tests can exercise version/slug logic without the process-
/// global env override racing other tests.
pub fn load_all_active_with(modules_dir: &Path, allow_unsigned: bool) -> Vec<LoadedModule> {
    let Ok(entries) = std::fs::read_dir(modules_dir) else { return Vec::new() };
    let mut out = Vec::new();
    for entry in entries.flatten() {
        if !entry.file_type().map(|t| t.is_dir()).unwrap_or(false) { continue; }
        let slug_dir = entry.path();
        let name = entry.file_name();
        let slug = match name.to_str() {
            Some(s) if !s.starts_with('.') => s.to_string(),
            _ => continue,
        };
        let Some(version_dir) = resolve_active_version(&slug_dir) else {
            eprintln!("[connector_manifest] {}: no active version", slug);
            continue;
        };
        match load_manifest(&version_dir) {
            Ok(m) if m.slug == slug => {
                // W5.1: refuse to load a module whose signature does not
                // verify. Signing was implemented but never enforced, so an
                // unsigned or tampered module dropped into the modules dir
                // loaded and ran. activate_from_staging signs every module it
                // installs, so a legitimately-activated module always passes.
                // ORBIX_ALLOW_UNSIGNED_MODULES=1 is a developer override that
                // ships off.
                if let Err(e) = crate::connector_signer::verify(&m) {
                    if allow_unsigned {
                        eprintln!(
                            "[connector_manifest] {}: signature check failed ({}); loading anyway (dev override)",
                            slug, e,
                        );
                    } else {
                        eprintln!(
                            "[connector_manifest] {}: REFUSED, signature check failed: {}",
                            slug, e,
                        );
                        continue;
                    }
                }
                out.push(LoadedModule { slug, version_dir, manifest: m });
            }
            Ok(m) => eprintln!(
                "[connector_manifest] {}: slug mismatch (manifest says '{}')",
                slug, m.slug,
            ),
            Err(e) => eprintln!("[connector_manifest] {}: {}", slug, e),
        }
    }
    out
}

/// Developer escape hatch for loading unsigned/foreign-signed modules.
/// Off unless ORBIX_ALLOW_UNSIGNED_MODULES is set to a truthy value.
pub fn allow_unsigned() -> bool {
    matches!(
        std::env::var("ORBIX_ALLOW_UNSIGNED_MODULES").ok().as_deref(),
        Some("1") | Some("true") | Some("yes")
    )
}

#[derive(Debug, Clone)]
pub struct LoadedModule {
    pub slug: String,
    pub version_dir: PathBuf,
    pub manifest: ConnectorManifest,
}

// ─────────────────────────────────────────────
//  Deno spawn args from manifest
// ─────────────────────────────────────────────

/// Compute the Deno flags from the manifest's allow-lists. The runtime
/// passes these as-is to `deno run` so the OS kernel (not JS) enforces
/// host and env restrictions.
pub fn deno_spawn_args(manifest: &ConnectorManifest, module_ts: &Path) -> Vec<String> {
    let mut args: Vec<String> = vec!["run".into()];

    // --allow-net: empty list disables network entirely; non-empty list
    // pins to exactly those hosts (Deno enforces). For BrowserAttach,
    // 127.0.0.1:<cdp-port> is added by the runtime layer, not here.
    if !manifest.host_allowlist.is_empty() {
        args.push(format!("--allow-net={}", manifest.host_allowlist.join(",")));
    }

    if !manifest.env_allowlist.is_empty() {
        args.push(format!("--allow-env={}", manifest.env_allowlist.join(",")));
    }

    // Modules may read files inside their own version directory - the
    // SDK shim, fixtures/, helper TS files. Nothing outside. The brief
    // §2 limits modules to "read-only module dir only".
    if let Some(version_dir) = module_ts.parent() {
        args.push(format!("--allow-read={}", version_dir.to_string_lossy()));
    }

    // Never grant write, run, or general fs/shell.
    args.push("--no-prompt".into());
    args.push("--quiet".into());
    args.push(module_ts.to_string_lossy().into_owned());
    args
}

// ─────────────────────────────────────────────
//  Tests
// ─────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;

    fn write_manifest(dir: &Path, body: &str) {
        std::fs::create_dir_all(dir).unwrap();
        let mut f = std::fs::File::create(dir.join("manifest.json")).unwrap();
        f.write_all(body.as_bytes()).unwrap();
    }

    #[test]
    fn parses_minimal_manifest() {
        let m: ConnectorManifest = serde_json::from_str(r#"{
            "slug": "echo",
            "version": "0.1.0",
            "display_name": "Echo",
            "vendor": "orbixcore.ai",
            "driver": {"kind": "api"},
            "auth": {"kind": "none"},
            "actions": {
                "ping": {
                    "description": "Echo back the inputs",
                    "idempotent": true
                }
            }
        }"#).unwrap();
        assert_eq!(m.slug, "echo");
        assert!(m.actions.contains_key("ping"));
        assert_eq!(m.resource_limits.memory_limit_mb, 128);
        assert!(m.signature.is_none());
    }

    #[test]
    fn deno_args_pin_hosts_and_env() {
        let m: ConnectorManifest = serde_json::from_str(r#"{
            "slug": "linkedin",
            "version": "0.1.0",
            "display_name": "LinkedIn",
            "vendor": "linkedin.com",
            "driver": {"kind": "api"},
            "auth": {"kind": "none"},
            "host_allowlist": ["api.linkedin.com"],
            "env_allowlist": ["LINKEDIN_TOKEN"],
            "actions": {"post": {"description": "x"}}
        }"#).unwrap();
        let args = deno_spawn_args(&m, Path::new("/tmp/module-dir/m.ts"));
        assert_eq!(args[0], "run");
        assert!(args.iter().any(|a| a == "--allow-net=api.linkedin.com"));
        assert!(args.iter().any(|a| a == "--allow-env=LINKEDIN_TOKEN"));
        assert!(args.iter().any(|a| a == "--no-prompt"));
        // Read is scoped to the module's own directory only - SDK shim
        // and fixtures need to be readable, nothing else does.
        assert!(args.iter().any(|a| a == "--allow-read=/tmp/module-dir"));
        // Critical: write and run never granted.
        assert!(!args.iter().any(|a| a.starts_with("--allow-write")));
        assert!(!args.iter().any(|a| a.starts_with("--allow-run")));
    }

    #[test]
    fn deno_args_with_empty_allowlist_grants_no_network() {
        let m: ConnectorManifest = serde_json::from_str(r#"{
            "slug": "offline",
            "version": "0.1.0",
            "display_name": "Offline",
            "vendor": "local",
            "driver": {"kind": "api"},
            "auth": {"kind": "none"},
            "actions": {"go": {"description": "x"}}
        }"#).unwrap();
        let args = deno_spawn_args(&m, Path::new("/tmp/m.ts"));
        assert!(!args.iter().any(|a| a.starts_with("--allow-net")));
    }

    #[test]
    #[cfg(unix)] // symlink-based activation is unix-only (see connector_activate)
    fn load_all_active_picks_current_symlink() {
        let tmp = tempfile::tempdir().unwrap();
        let slug_dir = tmp.path().join("echo");
        let v1 = slug_dir.join("v1");
        let v2 = slug_dir.join("v2");
        std::fs::create_dir_all(&v1).unwrap();
        std::fs::create_dir_all(&v2).unwrap();
        // Make v2 the active one via symlink.
        std::os::unix::fs::symlink(&v2, slug_dir.join("current")).unwrap();
        // v1 should be ignored even though its manifest is valid.
        write_manifest(&v1, r#"{"slug":"echo","version":"0.1.0","display_name":"v1","vendor":"x","driver":{"kind":"api"},"auth":{"kind":"none"},"actions":{}}"#);
        write_manifest(&v2, r#"{"slug":"echo","version":"0.2.0","display_name":"v2","vendor":"x","driver":{"kind":"api"},"auth":{"kind":"none"},"actions":{}}"#);
        let loaded = load_all_active_with(tmp.path(), true);
        assert_eq!(loaded.len(), 1);
        assert_eq!(loaded[0].manifest.version, "0.2.0");
    }

    /// W5.1: an unsigned module is refused by default and only loads under
    /// the explicit dev override.
    #[test]
    fn unsigned_module_refused_by_default() {
        let tmp = tempfile::tempdir().unwrap();
        let v1 = tmp.path().join("echo").join("v1");
        write_manifest(&v1, r#"{"slug":"echo","version":"0.1.0","display_name":"x","vendor":"x","driver":{"kind":"api"},"auth":{"kind":"none"},"actions":{}}"#);
        // Default policy: unsigned -> refused.
        assert!(load_all_active_with(tmp.path(), false).is_empty());
        // Dev override: unsigned -> loaded.
        assert_eq!(load_all_active_with(tmp.path(), true).len(), 1);
    }

    #[test]
    fn load_all_active_ignores_module_with_slug_mismatch() {
        let tmp = tempfile::tempdir().unwrap();
        let v1 = tmp.path().join("echo").join("v1");
        write_manifest(&v1, r#"{"slug":"NOT_ECHO","version":"0.1.0","display_name":"x","vendor":"x","driver":{"kind":"api"},"auth":{"kind":"none"},"actions":{}}"#);
        let loaded = load_all_active_with(tmp.path(), true);
        assert!(loaded.is_empty());
    }
}
