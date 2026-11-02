//! On-demand Deno runtime.
//!
//! Deno is ~40 MB per architecture. Bundling it in the DMG more than tripled
//! the download (a ~20 MB app became ~59 MB), and most users never touch a
//! connector. So instead of shipping it as a Tauri `externalBin`, we fetch the
//! pinned version into the app data dir the first time a connector actually
//! needs it, and verify the official published SHA-256 (baked in below) before
//! we ever mark the binary executable.
//!
//! macOS and Windows fetch on demand; other platforms keep the previous
//! behaviour (a dev-built sidecar, or `deno` on PATH).

use std::path::{Path, PathBuf};
use std::sync::OnceLock;

use tokio::sync::Mutex;

/// Keep in lockstep with `scripts/download-deno.sh` (DENO_VERSION) and the two
/// pinned checksums below.
const DENO_VERSION: &str = "v2.5.4";

/// SHA-256 of each release `.zip`, from the official
/// `deno-<triple>.zip.sha256sum`. Baked in (not fetched) so a tampered or
/// truncated download is rejected regardless of what the server returns. Bump
/// all the constants together when moving Deno versions.
const SHA_AARCH64_DARWIN: &str =
    "ff301a55e9f51306c02eb6da76ceba6ac98109fd11dbd7a1f5b71f5794ad725c";
const SHA_X86_64_DARWIN: &str =
    "bda20c2edb39d614830b9dc4a7dcbfaa0043ae5c246bae2f90b82e943a735e66";
const SHA_X86_64_WINDOWS: &str =
    "1c40c73193960fcbed9341764d43bbd35e381eddbcf7d6cba7c2f3c0cabe96fb";

/// Serialize first-use downloads so several connectors starting at once don't
/// each fetch the same 40 MB.
fn download_lock() -> &'static Mutex<()> {
    static L: OnceLock<Mutex<()>> = OnceLock::new();
    L.get_or_init(|| Mutex::new(()))
}

/// Where the managed Deno binary lives once downloaded:
/// `~/Library/Application Support/ai.orbixcore.app/bin/deno`.
pub fn managed_deno_path() -> PathBuf {
    crate::storage::base_dir()
        .join("bin")
        .join(if cfg!(windows) { "deno.exe" } else { "deno" })
}

/// The release asset name and its pinned zip checksum for this build's target,
/// or None on platforms we don't fetch for.
fn asset_for_target() -> Option<(&'static str, &'static str)> {
    if cfg!(all(target_os = "macos", target_arch = "aarch64")) {
        Some(("deno-aarch64-apple-darwin.zip", SHA_AARCH64_DARWIN))
    } else if cfg!(all(target_os = "macos", target_arch = "x86_64")) {
        Some(("deno-x86_64-apple-darwin.zip", SHA_X86_64_DARWIN))
    } else if cfg!(all(target_os = "windows", target_arch = "x86_64")) {
        Some(("deno-x86_64-pc-windows-msvc.zip", SHA_X86_64_WINDOWS))
    } else {
        None
    }
}

/// A real (non-placeholder) executable Deno is tens of MB; anything smaller is
/// a stub or a partial write.
fn is_usable(path: &Path) -> bool {
    match std::fs::metadata(path) {
        Ok(m) => m.is_file() && m.len() > 1_000_000,
        Err(_) => false,
    }
}

/// The managed Deno path if it is already present, else None. Cheap and
/// network-free - the sync resolver in connector_runtime uses this.
pub fn existing_managed() -> Option<PathBuf> {
    let p = managed_deno_path();
    if is_usable(&p) {
        Some(p)
    } else {
        None
    }
}

/// Ensure a checksum-verified Deno exists in the app data dir, downloading it
/// once on first call, and return its path. Returns Err on non-macOS targets
/// (the caller then falls back to a sidecar / PATH) and on any download or
/// verification failure.
pub async fn ensure_deno() -> Result<PathBuf, String> {
    let dest = managed_deno_path();
    if is_usable(&dest) {
        return Ok(dest);
    }
    let Some((asset, want_sha)) = asset_for_target() else {
        return Err("managed Deno download is not available on this platform".to_string());
    };

    // One downloader at a time; the rest wait and then find it present.
    let _guard = download_lock().lock().await;
    if is_usable(&dest) {
        return Ok(dest);
    }

    let bin_dir = dest
        .parent()
        .ok_or_else(|| "no parent for managed deno path".to_string())?
        .to_path_buf();
    std::fs::create_dir_all(&bin_dir).map_err(|e| format!("create bin dir: {e}"))?;

    let url = format!(
        "https://github.com/denoland/deno/releases/download/{DENO_VERSION}/{asset}"
    );
    log::info!("[deno] fetching {DENO_VERSION} ({asset}) - first connector use");
    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(300))
        .build()
        .map_err(|e| format!("http client: {e}"))?;
    let resp = client
        .get(&url)
        .send()
        .await
        .map_err(|e| format!("download deno: {e}"))?;
    if !resp.status().is_success() {
        return Err(format!("download deno: {url} returned HTTP {}", resp.status()));
    }
    let bytes = resp
        .bytes()
        .await
        .map_err(|e| format!("read deno download: {e}"))?;

    // Verify against the pinned checksum BEFORE anything touches disk as an
    // executable. A mismatch means a corrupted or tampered download.
    use sha2::{Digest, Sha256};
    let got = hex::encode(Sha256::digest(&bytes));
    if !got.eq_ignore_ascii_case(want_sha) {
        return Err(format!(
            "Deno download failed its checksum (expected {want_sha}, got {got}); refusing to run it"
        ));
    }

    // Extract via the OS unzip (always present on macOS; matches how the
    // llama-server archive is unpacked), then move the single `deno` file into
    // place and mark it executable.
    let tmp_zip = bin_dir.join("deno-download.zip");
    crate::storage::atomic_write(&tmp_zip, &bytes)
        .map_err(|e| format!("save deno zip: {e}"))?;
    let extract_dir = bin_dir.join("deno-extract");
    let _ = std::fs::remove_dir_all(&extract_dir);
    std::fs::create_dir_all(&extract_dir).map_err(|e| format!("mkdir extract: {e}"))?;

    // unzip on macOS; tar.exe (Windows 10 1803+, reads ZIPs natively) on
    // Windows - both always present, matching how llama-server unpacks.
    #[cfg(windows)]
    let out = crate::process_ext::tokio_command("tar.exe")
        .arg("-xf")
        .arg(&tmp_zip)
        .arg("-C")
        .arg(&extract_dir)
        .output()
        .await
        .map_err(|e| format!("extract deno: {e}"))?;
    #[cfg(not(windows))]
    let out = tokio::process::Command::new("unzip")
        .arg("-oq")
        .arg(&tmp_zip)
        .arg("-d")
        .arg(&extract_dir)
        .output()
        .await
        .map_err(|e| format!("unzip deno: {e}"))?;
    if !out.status.success() {
        let _ = std::fs::remove_file(&tmp_zip);
        let _ = std::fs::remove_dir_all(&extract_dir);
        return Err(format!(
            "unzip deno failed: {}",
            String::from_utf8_lossy(&out.stderr)
        ));
    }

    let extracted = extract_dir.join(if cfg!(windows) { "deno.exe" } else { "deno" });
    if !is_usable(&extracted) {
        let _ = std::fs::remove_file(&tmp_zip);
        let _ = std::fs::remove_dir_all(&extract_dir);
        return Err("deno archive did not contain the expected binary".to_string());
    }
    // Same filesystem (both under bin_dir), so this rename is atomic.
    std::fs::rename(&extracted, &dest).map_err(|e| format!("install deno: {e}"))?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let _ = std::fs::set_permissions(&dest, std::fs::Permissions::from_mode(0o755));
    }
    let _ = std::fs::remove_file(&tmp_zip);
    let _ = std::fs::remove_dir_all(&extract_dir);
    log::info!("[deno] installed {}", dest.display());
    Ok(dest)
}
