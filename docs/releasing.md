# Releasing

How a version gets from this repository to a download button and an in-app update. Everything after the tag is automated; the two manual steps are deliberate.

## 1. Bump and tag

1. Set the same version in `orbixcore/src-tauri/tauri.conf.json` and `orbixcore/package.json`. The running app compares its own version against the published one, so they must agree.
2. Commit, then tag and push the tag:

```bash
git tag v0.1.0
git push origin v0.1.0
```

## 2. The build (automatic)

`.github/workflows/release.yml` builds on the tag:

| Runner | Artifacts |
|---|---|
| `macos-14` (arm64) | `Orbix Core_<v>_aarch64.dmg`, plus `.app.tar.gz` and `.sig` when the updater key is set |
| `macos-13` (x86_64) | the same for Intel |
| `windows-latest` | `Orbix Core_<v>_x64-setup.exe`, `Orbix Core_<v>_x64_en-US.msi`, plus `.sig` files when the updater key is set |

A final job writes `SHA256SUMS.txt` and opens a **draft** GitHub Release with generated notes and every artifact attached.

Secrets that change what the build produces (all optional; absent means unsigned):

- `TAURI_SIGNING_PRIVATE_KEY` and `TAURI_SIGNING_PRIVATE_KEY_PASSWORD`: sign the updater artifacts. Without them there are no `.sig` files and installed apps are not offered the release. See `orbixcore/src-tauri/scripts/UPDATER.md`.
- Apple secrets (`APPLE_CERTIFICATE` and friends): sign and notarize the macOS app. See `orbixcore/src-tauri/scripts/SIGNING.md`.
- Windows Authenticode signing is wired but off until a certificate is enrolled (`orbixcore/src-tauri/scripts/SIGNING.md`); until then SmartScreen warns on the installer.

## 3. Review and publish (manual)

Open the draft on the Releases page, read the notes, spot-check the artifact list and checksums, then **Publish**. Nothing is public until this click.

## 4. What publishing triggers (automatic)

- **The website.** `orbixcore.ai/download` resolves the latest release through the GitHub API (cached ten minutes) and offers the DMGs and the Windows installer, with the checksum and release-notes links. No site deploy is involved.
- **The in-app updater.** `.github/workflows/publish-update.yml` reads the published release, pairs each updater artifact with its `.sig`, and POSTs the manifest to `orbixcore.ai/api/update` (needs the `ORBIX_ADMIN_SECRET` repository secret). Apps on an older version get the update toast on their next check. An unsigned release is skipped with a notice in the job log.

To re-run the manifest publish by hand (for example after adding the secret): Actions > Publish update manifest > Run workflow, with the tag.

## Rolling back

- Website: delete or unpublish the release on GitHub; the site falls back to the previous latest release within ten minutes.
- Updater: `POST /api/update` with `{}` and the admin secret clears the manifest, so no app is offered anything until the next publish.
