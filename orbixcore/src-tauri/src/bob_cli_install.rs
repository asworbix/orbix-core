// ─────────────────────────────────────────────
//  bob CLI install - bundled-resource → ~/.local/bin/bob
//
//  The .app ships the orbix binary as a signed sidecar next to the app
//  executable (Contents/MacOS/orbix on macOS, beside orbixcore.exe on Windows).
//  This module copies it to ~/.local/bin/bob and reports whether
//  that dir is on PATH so the user knows whether to update their
//  shell rc. The "Install bob CLI" button in Settings calls install().
// ─────────────────────────────────────────────

use std::path::PathBuf;

use serde::Serialize;
use tauri::Manager;

#[derive(Debug, Clone, Serialize)]
pub struct InstallReport {
    pub installed_at: String,
    pub on_path: bool,
    /// Detected user shell rc file (best-effort), so the UI can
    /// suggest the right append target if PATH needs updating.
    pub shell_rc_hint: Option<String>,
    pub version_bytes: u64,
}

pub fn install(app: &tauri::AppHandle) -> Result<InstallReport, String> {
    let src = locate_bundled_cli(app)?;
    // Refuse to install the build.rs placeholder. Real releases are ~2MB;
    // the placeholder is ~80 bytes.
    let src_size = std::fs::metadata(&src).map(|m| m.len()).unwrap_or(0);
    if src_size < 100_000 {
        return Err(format!(
            "bundled CLI binary at {} is only {} bytes: a build placeholder, not the real CLI. \
             Run `npm run stage` before `tauri build`.",
            src.display(), src_size,
        ));
    }
    // W8: orbix is the primary command; bob is a back-compat alias.
    let orbix_name = if cfg!(windows) { "orbix.exe" } else { "orbix" };
    let dest = install_dir().join(orbix_name);
    let install_dir = dest.parent().ok_or("install dir has no parent")?;
    std::fs::create_dir_all(install_dir)
        .map_err(|e| format!("create {}: {}", install_dir.display(), e))?;

    std::fs::copy(&src, &dest)
        .map_err(|e| format!("copy {} -> {}: {}", src.display(), dest.display(), e))?;

    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&dest, std::fs::Permissions::from_mode(0o755))
            .map_err(|e| format!("chmod {}: {}", dest.display(), e))?;
    }

    // Install the `bob` alias next to orbix. Prefer a symlink; if the FS
    // refuses (or a stale file exists), fall back to a copy so both names
    // always work.
    let bob_dest = install_dir.join(if cfg!(windows) { "bob.exe" } else { "bob" });
    let _ = std::fs::remove_file(&bob_dest);
    #[cfg(unix)]
    let aliased = std::os::unix::fs::symlink(&dest, &bob_dest).is_ok();
    #[cfg(not(unix))]
    let aliased = false;
    if !aliased {
        let _ = std::fs::copy(&dest, &bob_dest);
    }

    let size = std::fs::metadata(&dest).map(|m| m.len()).unwrap_or(0);
    Ok(InstallReport {
        installed_at: dest.to_string_lossy().into_owned(),
        on_path: is_on_path(install_dir),
        shell_rc_hint: shell_rc_hint(),
        version_bytes: size,
    })
}

/// Where the bundled binary lives at run time. Resolution order:
/// 1. Sidecar next to the app executable (bundle.externalBin): the bundler
///    puts it in Contents/MacOS on macOS and beside the .exe on Windows and
///    signs it with the app; `tauri dev` copies it next to the debug
///    executable too.
/// 2. Older bundles that staged it as a resource (Contents/Resources/binaries/).
/// 3. Dev fallbacks: the staged sidecar in the crate, then bob-cli's own
///    release build.
pub(crate) fn locate_bundled_cli(app: &tauri::AppHandle) -> Result<PathBuf, String> {
    let exe = if cfg!(windows) { ".exe" } else { "" };
    if let Some(dir) = std::env::current_exe().ok().and_then(|p| p.parent().map(|d| d.to_path_buf())) {
        for name in ["orbix", "bob"] {
            let p = dir.join(format!("{name}{exe}"));
            if is_real_binary(&p) { return Ok(p); }
        }
    }
    let resolver = app.path();
    for name in [format!("binaries/orbix{exe}"), format!("binaries/bob{exe}")] {
        if let Ok(p) = resolver.resolve(&name, tauri::path::BaseDirectory::Resource) {
            if is_real_binary(&p) { return Ok(p); }
        }
    }
    let crate_dir = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let triple = env!("ORBIX_TARGET_TRIPLE");
    let dev_candidates = [
        crate_dir.join(format!("binaries/orbix-{triple}{exe}")),
        crate_dir.join(format!("binaries/bob-{triple}{exe}")),
        crate_dir.join(format!("../../bob-cli/target/release/orbix{exe}")),
        crate_dir.join(format!("../../bob-cli/target/release/bob{exe}")),
    ];
    for p in &dev_candidates {
        if is_real_binary(p) { return Ok(p.clone()); }
    }
    Err("bundled orbix binary not found: was `npm run stage` run before tauri build?".into())
}

/// A file that is more than the build-time placeholder stub (a few dozen
/// bytes of shell). The size check that guards the install itself is above.
fn is_real_binary(p: &std::path::Path) -> bool {
    std::fs::metadata(p).map(|m| m.is_file() && m.len() > 1024).unwrap_or(false)
}

fn install_dir() -> PathBuf {
    #[cfg(windows)]
    {
        // Same location install.ps1 uses, so the Settings button and the
        // manual script agree on where the CLI lives.
        dirs::data_local_dir()
            .map(|d| d.join("Orbix").join("bin"))
            .unwrap_or_else(|| PathBuf::from("C:\\Orbix\\bin"))
    }
    #[cfg(not(windows))]
    {
        dirs::home_dir()
            .map(|h| h.join(".local").join("bin"))
            .unwrap_or_else(|| PathBuf::from("/usr/local/bin"))
    }
}

fn is_on_path(target: &std::path::Path) -> bool {
    let Ok(path) = std::env::var("PATH") else { return false };
    std::env::split_paths(&path).any(|p| p == target)
}

fn shell_rc_hint() -> Option<String> {
    // On Windows there is no shell rc; the UI should suggest adding the
    // install dir to the user PATH (or running bob-cli/install.ps1).
    if cfg!(windows) {
        return Some(install_dir().to_string_lossy().into_owned());
    }
    let home = dirs::home_dir()?;
    let shell = std::env::var("SHELL").unwrap_or_default();
    let candidate = if shell.ends_with("/zsh") {
        home.join(".zshrc")
    } else if shell.ends_with("/bash") {
        home.join(".bashrc")
    } else if shell.ends_with("/fish") {
        home.join(".config/fish/config.fish")
    } else {
        // Reasonable default on macOS where shell var may be unset.
        home.join(".zshrc")
    };
    Some(candidate.to_string_lossy().into_owned())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn install_dir_uses_home_when_set() {
        let dir = install_dir();
        let s = dir.to_string_lossy();
        #[cfg(windows)]
        assert!(s.ends_with("Orbix\\bin") || s.ends_with("Orbix/bin"));
        #[cfg(not(windows))]
        assert!(s.ends_with(".local/bin") || s.ends_with(".local\\bin")
            || s == "/usr/local/bin");
    }

    #[test]
    fn shell_rc_hint_returns_something() {
        // Should always return Some() on a system with HOME set.
        assert!(shell_rc_hint().is_some());
    }
}
