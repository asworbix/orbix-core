# In-app updates

Orbix Core checks `https://orbixcore.ai/api/update` shortly after sign-in and,
if a newer signed build is published, offers a click-to-install toast that
downloads, installs, and relaunches into the new version.

Two independent pieces:

- **Runtime** (already wired): `tauri-plugin-updater` + `tauri-plugin-process`,
  the `plugins.updater` block in `tauri.conf.json` (endpoint + public key), and
  the frontend check in `src/updater.ts`. No secrets needed for the app to run;
  with nothing published the check just returns "no update" and stays quiet.
- **Release signing** (needs a key, like Apple signing): the updater only trusts
  artifacts signed with the private key whose public half is in the config.

## The signing key

A keypair was already generated. The **public** key is committed in
`tauri.conf.json` (`plugins.updater.pubkey`) - safe to share. The **private**
key is on this machine at:

```
~/.orbix/orbix-updater.key          (chmod 600 - keep it secret, back it up)
~/.orbix/orbix-updater.key.pub      (matches the committed pubkey)
```

It was generated with no password. Add it as CI secrets so release builds can
sign updater artifacts:

| Secret | Value |
|---|---|
| `TAURI_SIGNING_PRIVATE_KEY` | the full contents of `~/.orbix/orbix-updater.key` |
| `TAURI_SIGNING_PRIVATE_KEY_PASSWORD` | empty string (this key has no password) |

Prefer your own key? Run `npm run tauri signer generate -- -w orbix-updater.key`,
replace the `pubkey` in `tauri.conf.json`, and use the new file. **If you lose
the private key you can never ship another update to installed apps** - back it
up.

When `TAURI_SIGNING_PRIVATE_KEY` is present the release workflow builds with
`--config '{"bundle":{"createUpdaterArtifacts":true}}'`, producing the
`.app.tar.gz` + `.sig`. Without it the build is unsigned and emits no updater
artifacts (dev/fork builds keep working) - Tauri hard-fails that flag if the
key is missing, which is why it is gated rather than always on.

## Publishing a release to the updater

The `/api/update` endpoint serves the manifest from Upstash KV, so shipping an
update is a single authenticated POST - no site redeploy:

1. Build a signed release (CI, with the secrets above). Collect per arch:
   - the `.app.tar.gz` URL (upload it to Vercel Blob next to the DMG), and
   - the matching `.sig` file's contents.
2. POST the manifest (admin only):

```bash
curl -X POST https://orbixcore.ai/api/update \
  -H "x-admin-secret: $ORBIX_ADMIN_SECRET" \
  -H "Content-Type: application/json" \
  -d '{
    "version": "0.2.0",
    "notes": "What changed",
    "platforms": {
      "darwin-aarch64": { "url": "https://<blob>/Orbix_0.2.0_aarch64.app.tar.gz", "signature": "<contents of .sig>" },
      "darwin-x86_64":  { "url": "https://<blob>/Orbix_0.2.0_x64.app.tar.gz",     "signature": "<contents of .sig>" }
    }
  }'
```

Clients older than `version` are offered the update; clients on `version` or
newer get `204 No Content`. POST with no `version` clears the manifest (stops
offering updates).

## Bump the app version

The offered version comes from the published manifest, but the running app's
version (what it compares against) is `tauri.conf.json` `version` and
`package.json` `version` - bump both before cutting a release.
