# Connector modules

Reference modules and the SDK shim. The activator
(`connector_activate_module`) copies these into
`~/Library/Application Support/ai.orbixcore.app/modules/<slug>/<version>/`
after running the static scanner, signing the manifest with the Ed25519
key in the Keychain, and atomically repointing the `current` symlink.

## Layout

```
resources/modules/
  _sdk/orbix-sdk.ts          ← copied alongside each module at activation
  echo/v1/                   ← raw NDJSON test fixture (no SDK)
  echo_sdk/v1/               ← SDK shim demo (zero auth, zero network)
  linkedin/v1/               ← canonical OAuth API connector
```

## Producing a new module

1. Author `manifest.json` + `module.ts` under a fresh `<slug>/<version>/` dir.
2. Hand the dir to the wizard (Connectors → +) or call
   `connector_activate_module(<staging_dir>)` directly.
3. The pipeline runs: capability check → AST scan → Ed25519 sign → atomic install.
4. On any violation the activator returns a clear error and leaves the
   modules root untouched.

## Building the production app

Before `npm run tauri build`, fetch the Deno sidecar:

```bash
cd orbixcore
npm run deno:host       # current platform only
# or
npm run deno:all        # every supported target (CI uses this)
```

Use `npm run tauri:build` to chain `deno:host` + `tauri build`.

For CI builds, copy `src-tauri/scripts/release-workflow.yml` to
`.github/workflows/release.yml` once. The workflow matrix-builds
macOS arm64 + x86_64 with the sidecar bundled in.
