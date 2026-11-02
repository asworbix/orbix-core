// ─────────────────────────────────────────────
//  Connector modules - Phase 1 of CONNECTORS.md
//
//  On disk layout (decided 2026-05-11):
//
//    ~/Library/Application Support/ai.orbixcore.app/modules/
//      <slug>/
//        <version>/
//          manifest.json
//          module.ts
//          tests.ts
//          fixtures/
//          README.md
//        current -> v3   (active version symlink)
//
//  Phase 1 only opens the directory and lists slugs. Manifest parsing,
//  signing, runtime supervision arrive in Phase 2.
// ─────────────────────────────────────────────

use std::path::PathBuf;

use crate::storage;

/// Root directory that holds every installed connector module.
pub fn modules_dir() -> PathBuf {
    storage::data_dir().join("modules")
}

/// Slugs of every directory under `modules/`. The result is sorted so
/// callers don't have to care about filesystem iteration order.
pub fn list_module_slugs() -> Vec<String> {
    let dir = modules_dir();
    let Ok(entries) = std::fs::read_dir(&dir) else { return Vec::new() };
    let mut out: Vec<String> = entries
        .filter_map(|e| e.ok())
        .filter(|e| e.file_type().map(|t| t.is_dir()).unwrap_or(false))
        .filter_map(|e| e.file_name().into_string().ok())
        // Hidden / dotfiles are not modules.
        .filter(|s| !s.starts_with('.'))
        .collect();
    out.sort();
    out
}

/// Best-effort count of installed connector modules. Missing dir = zero.
pub fn module_count() -> usize {
    list_module_slugs().len()
}

/// Ensure the modules root exists. Idempotent.
pub fn ensure_modules_dir() -> std::io::Result<PathBuf> {
    let dir = modules_dir();
    std::fs::create_dir_all(&dir)?;
    Ok(dir)
}
