fn main() {
    // Speech / AVFoundation are Apple-only frameworks. Emitting these link
    // directives on non-Apple targets makes the crate fail to build with
    // "library kind `framework` is only supported on Apple targets".
    if std::env::var("CARGO_CFG_TARGET_OS").as_deref() == Ok("macos") {
        println!("cargo:rustc-link-lib=framework=Speech");
        println!("cargo:rustc-link-lib=framework=AVFoundation");
    }

    // The orbix/bob CLI and the macOS speech helper ship as sidecars
    // (bundle.externalBin, named <name>-<target-triple>): the bundler
    // copies them next to the app executable and code-signs them with it.
    // Deno is not bundled; it is downloaded on first connector use
    // (src/deno_sidecar.rs).
    //
    // tauri_build::build() below copies every sidecar into the target dir
    // and fails when one is missing, so a debug build gets a placeholder
    // and a release build must have the real binaries staged first
    // (`npm run stage`, run by `npm run tauri:build` and the release
    // workflow). The Tauri command bob_cli_install() refuses to copy a
    // placeholder onto the user's PATH because it checks the size at
    // run time.
    let target = std::env::var("TARGET").unwrap_or_default();
    println!("cargo:rustc-env=ORBIX_TARGET_TRIPLE={target}");
    if !target.is_empty() {
        let crate_dir = std::env::var("CARGO_MANIFEST_DIR").unwrap_or_default();
        let bin_dir = std::path::PathBuf::from(&crate_dir).join("binaries");
        let _ = std::fs::create_dir_all(&bin_dir);
        let profile = std::env::var("PROFILE").unwrap_or_default();
        let exe = if target.contains("windows") { ".exe" } else { "" };
        // orbix is the primary CLI; bob is the back-compat alias. The Swift
        // TTS helper is macOS-only; on Windows SAPI handles TTS.
        let mut sidecars = vec![format!("orbix-{target}{exe}"), format!("bob-{target}{exe}")];
        if target.contains("apple-darwin") {
            sidecars.push(format!("orbix-speak-{target}"));
        }
        for name in sidecars {
            let p = bin_dir.join(&name);
            if p.exists() {
                continue;
            }
            // A release build that reaches here with no real binary would
            // ship a 60-byte `exit 1` stub as the CLI. Fail loudly instead
            // of quietly bundling a broken sidecar.
            if profile == "release" {
                panic!(
                    "release build is missing src-tauri/binaries/{name}; \
                     run `npm run stage` before `tauri build` (or use \
                     `npm run tauri:build`, which stages it)."
                );
            }
            let _ = std::fs::write(
                &p,
                b"#!/bin/sh\n# placeholder; run `npm run stage`\nexit 1\n",
            );
            #[cfg(unix)]
            {
                use std::os::unix::fs::PermissionsExt;
                let _ = std::fs::set_permissions(&p, std::fs::Permissions::from_mode(0o755));
            }
        }
    }

    tauri_build::build()
}
