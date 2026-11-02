// In-app updates. Thin wrapper over the Tauri updater/process plugins so the
// UI can stay simple: check once, and if something is waiting, let the user
// install it. Everything is best-effort - in dev, with no published release,
// or before the signing key is set up, check() just resolves to no update and
// we stay quiet.

import { check, type Update } from '@tauri-apps/plugin-updater';
import { relaunch } from '@tauri-apps/plugin-process';

export type UpdateInfo = { version: string; notes: string; update: Update };

/// Ask the update endpoint whether a newer signed build exists. Returns null
/// when up to date; throws when the endpoint can't be reached or the updater
/// isn't configured. For a person pressing Check for Updates.
export async function checkForUpdateOrThrow(): Promise<UpdateInfo | null> {
  const update = await check();
  if (!update) return null;
  return { version: update.version, notes: update.body ?? '', update };
}

/// The quiet variant for the background check: also null when offline or
/// when the updater isn't configured yet.
export async function checkForUpdate(): Promise<UpdateInfo | null> {
  try {
    return await checkForUpdateOrThrow();
  } catch (e) {
    // No endpoint/manifest yet, dev build, or a transient network error.
    console.warn('[updater] check failed', e);
    return null;
  }
}

/// Download + install the update, reporting coarse progress, then relaunch into
/// the new version. Throws on failure so the caller can surface it.
export async function installUpdate(
  info: UpdateInfo,
  onProgress?: (phase: 'downloading' | 'installing' | 'done', pct?: number) => void,
): Promise<void> {
  let total = 0;
  let received = 0;
  await info.update.downloadAndInstall((event) => {
    switch (event.event) {
      case 'Started':
        total = event.data.contentLength ?? 0;
        onProgress?.('downloading', 0);
        break;
      case 'Progress':
        received += event.data.chunkLength;
        onProgress?.('downloading', total ? Math.round((received / total) * 100) : undefined);
        break;
      case 'Finished':
        onProgress?.('installing');
        break;
    }
  });
  onProgress?.('done');
  await relaunch();
}
