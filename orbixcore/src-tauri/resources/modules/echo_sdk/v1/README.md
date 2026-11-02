# Echo (SDK) - reference module using orbix-sdk.ts

This module is the same contract as the raw `echo/` module but uses the SDK
shim. It's the shape every Bob-authored module follows.

What it demonstrates:

- Importing `defineConnector` from a relative `./orbix-sdk.ts` (the
  activator drops a copy of the bundled SDK into each module's version dir,
  so this import resolves without granting `--allow-read` outside the
  module dir)
- Returning a plain object from an action - the SDK frames the wire response
- Throwing `OrbixError("code", "msg")` so the bus returns
  `error_code = "forced_failure"` rather than the generic `"module_error"`

To install for end-to-end testing once `deno` is on PATH:

```bash
cp -R orbixcore/src-tauri/resources/modules/echo_sdk \
  "$HOME/Library/Application Support/ai.orbixcore.app/modules/"
# Drop the SDK alongside (the production path is the activator; for
# pre-activation manual install we copy by hand):
cp orbixcore/src-tauri/resources/modules/_sdk/orbix-sdk.ts \
   "$HOME/Library/Application Support/ai.orbixcore.app/modules/echo_sdk/v1/"
ln -s v1 "$HOME/Library/Application Support/ai.orbixcore.app/modules/echo_sdk/current"
# Restart Orbix Core. Then:
#   invoke('connector_invoke', { connectorSlug: 'echo_sdk', actionId: 'ping',
#                                 inputs: { hello: 'world' },
#                                 idempotencyKey: 'k1' })
```

Going through the proper activator (`connector_activate_module`) is the
sanctioned path - it scans, signs, and copies the SDK automatically. The
manual route above is only for early testing before the frontend wizard
lands.
