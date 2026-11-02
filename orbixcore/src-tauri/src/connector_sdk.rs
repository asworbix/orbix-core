// ─────────────────────────────────────────────
//  Embedded SDK shim - single source of truth for orbix-sdk.ts
//
//  Embedding the SDK at compile time means three things:
//
//    1. The activator writes the SAME bytes the scanner reasoned
//       about. No race where a tampered file on disk slips past
//       activation and into the module dir.
//    2. The shipped binary is self-contained for module activation;
//       a stripped install without resources/ still works.
//    3. The scanner can identify modules that use the SDK by their
//       import specifier (./orbix-sdk.ts) and grant the SDK's
//       trusted helpers (env, OrbixError) without scanning into them.
//
//  Build-time include_str! pulls from resources/modules/_sdk/. If
//  that file changes, the binary is recompiled.
// ─────────────────────────────────────────────

use sha2::{Digest, Sha256};

pub const EMBEDDED_SDK: &str = include_str!("../resources/modules/_sdk/orbix-sdk.ts");

/// Stable hex digest of the embedded SDK bytes. Lets the activator
/// audit-anchor "what SDK shipped" so a chain reader can correlate a
/// module activation with the SDK source it was paired with.
pub fn sdk_sha256() -> String {
    let mut h = Sha256::new();
    h.update(EMBEDDED_SDK.as_bytes());
    hex::encode(h.finalize())
}

/// Filename the activator writes alongside module.ts.
pub const SDK_FILENAME: &str = "orbix-sdk.ts";

/// Conventional relative-import specifier modules use to pick up the SDK.
pub const SDK_IMPORT_SPEC: &str = "./orbix-sdk.ts";

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn embedded_sdk_is_nonempty_and_exports_define_connector() {
        assert!(!EMBEDDED_SDK.is_empty());
        assert!(EMBEDDED_SDK.contains("export function defineConnector"));
        assert!(EMBEDDED_SDK.contains("export function env"));
        assert!(EMBEDDED_SDK.contains("export class OrbixError"));
        assert!(EMBEDDED_SDK.contains("export async function vaultPut"));
        assert!(EMBEDDED_SDK.contains("export async function refreshOauth"));
    }

    #[test]
    fn sha256_is_deterministic() {
        let a = sdk_sha256();
        let b = sdk_sha256();
        assert_eq!(a, b);
        assert_eq!(a.len(), 64);
    }
}
