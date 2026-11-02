# Echo connector - Phase 2 acceptance fixture

Hand-written module used to verify CONNECTORS.md §10 Phase 2 acceptance:

1. **Spawns, accepts a job, returns a result.** Run:
   ```bash
   cp -R orbixcore/src-tauri/resources/modules/echo \
     "$HOME/Library/Application Support/ai.orbixcore.app/modules/"
   ln -s v1 "$HOME/Library/Application Support/ai.orbixcore.app/modules/echo/current"
   # Restart Orbix Core. The runtime supervisor spawns the Deno subprocess at boot.
   ```
   Then from the frontend (or any caller via Tauri):
   ```
   invoke('connector_invoke', { connectorSlug: 'echo', actionId: 'ping',
                                inputs: { hello: 'world' },
                                idempotencyKey: 'manual-test-1' })
   ```
   Expected: `success: true`, `output.pong: true`, `output.echoed.hello: 'world'`.

2. **Killing the subprocess externally triggers automatic restart.**
   With the app running:
   ```bash
   pkill -f 'deno run.*echo/v1/module.ts'
   # Watch the next invoke succeed - the supervisor will have respawned.
   ```

3. **Network egress to a non-allowlisted host is blocked at the OS level.**
   Edit `module.ts` to add `await fetch("https://example.com")` inside the
   ping handler. The manifest's `host_allowlist` is empty, so Deno's
   `--allow-net` flag is absent - the fetch fails with a permission error
   inside the subprocess, never reaches the network. The bus surfaces it as
   `module_error`.

The module declares zero capabilities, an empty host allow-list, and an
empty env allow-list. It is the minimum-trust connector possible.
