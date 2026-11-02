# LinkedIn - reference API connector

The canonical "what a Bob-authored API connector looks like." Three
actions, ~80 lines of TypeScript, declared end-to-end via the
manifest's allow-lists.

## Trust surface

- **host_allowlist:** `api.linkedin.com`, `www.linkedin.com`. Deno's
  `--allow-net=` is set from these at spawn time; any other host is
  blocked by the OS kernel, not by JS.
- **env_allowlist:** `LINKEDIN_TOKEN` only. `--allow-env=` is set from
  this; the module cannot read `HOME`, `PATH`, or anything else.
- **vault_env_map:** `LINKEDIN_TOKEN ← vault.access_token`. The token
  reaches this subprocess by env injection at spawn; it is never
  carried across the bus.
- **capabilities:** `network_outbound` only. `validate_capability_grant`
  refuses to install this manifest under any other PluginType.
- **resource_limits:** 128MB / 15% CPU / 15s job timeout. Tight enough
  that a runaway connector cannot starve the rest of the app.

## Actions

| id | side effect | review gate behaviour |
|---|---|---|
| `userinfo` | none - GET only | bypass; used as a smoke test |
| `draft_post` | none - composes a payload | route to review queue |
| `post` | publishes to LinkedIn | only called when gate is AutoPublish |

The connector itself is unaware of the review gate - that lives in the
scheduler and the approval flow. `draft_post` returns the body that a
later `post` call would submit; the same body round-trips through the
review UI and back into a `post` invocation on approval.

## Auth flow (Phase 4 + 5)

1. Frontend hits `connector_complete_oauth_for_module("linkedin", access_token, refresh_token?, expires_in?)` after the OAuth callback. The token enters the vault here and never leaves Rust again.
2. Activator (`connector_activate_module`) runs the scanner, signs the manifest with the Keychain key, and atomically installs `current → v1/`.
3. The runtime spawns the Deno subprocess. The vault's `access_token` is exported as the `LINKEDIN_TOKEN` env var. The Rust side never reads it.
4. On 401 from the LinkedIn API, the module throws `OrbixError("auth_expired", …)`. The bus surfaces that classified code, the UI re-runs the OAuth flow, and the new token replaces the old one in the vault. Module restarts on next supervisor cycle pick up the new env transparently.

## Smoke test

```bash
# After cp into ~/Library/Application Support/ai.orbixcore.app/modules/linkedin/v1
# and dropping the SDK shim alongside:
invoke('connector_invoke', {
  connectorSlug: 'linkedin',
  actionId: 'userinfo',
  inputs: {},
  idempotencyKey: 'smoke-test-1'
})
```

Expect `success: true` with `output.sub`, `output.name`, `output.email`.
