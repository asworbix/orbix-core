// Write the Tauri config overrides for a release build from the
// environment, so the same `tauri build --config release-overrides.json`
// line serves signed and unsigned builds, in CI and locally. Nothing set
// means an empty override and an unsigned build, exactly as before.
//
//   TAURI_SIGNING_PRIVATE_KEY   updater artifacts on (Tauri hard-fails the
//                               flag without a key, so it follows the key)
//   WINDOWS_SIGN_COMMAND        Authenticode through a cloud signer CLI.
//                               "%1" is the file to sign. Either a plain
//                               string (split on spaces) or, for arguments
//                               with spaces, JSON: {"cmd":"x","args":["-d","Orbix Core","%1"]}
//   WINDOWS_CERT_THUMBPRINT     Authenticode through signtool with a
//                               certificate in the Windows store (a
//                               hardware token on the build PC).
//   WINDOWS_TIMESTAMP_URL       RFC 3161 timestamp server for the
//                               thumbprint route (default: DigiCert).
//
// Usage: node src-tauri/scripts/release-config.mjs [out.json]
// See SIGNING.md for the routes and their secrets.
import { writeFileSync } from 'node:fs';
import { pathToFileURL } from 'node:url';

export function releaseConfig(env, platform) {
  const bundle = {};
  if (env.TAURI_SIGNING_PRIVATE_KEY) bundle.createUpdaterArtifacts = true;

  if (platform === 'win32') {
    const command = (env.WINDOWS_SIGN_COMMAND ?? '').trim();
    const thumbprint = (env.WINDOWS_CERT_THUMBPRINT ?? '').replace(/\s+/g, '');
    if (command && thumbprint) {
      throw new Error('Set WINDOWS_SIGN_COMMAND or WINDOWS_CERT_THUMBPRINT, not both.');
    }
    if (command) {
      const signCommand = command.startsWith('{') ? JSON.parse(command) : command;
      const text = typeof signCommand === 'string' ? signCommand : [signCommand.cmd, ...(signCommand.args ?? [])].join(' ');
      if (!text.includes('%1')) {
        throw new Error('WINDOWS_SIGN_COMMAND needs a "%1" where the file to sign goes.');
      }
      bundle.windows = { signCommand };
    } else if (thumbprint) {
      bundle.windows = {
        certificateThumbprint: thumbprint,
        digestAlgorithm: 'sha256',
        timestampUrl: env.WINDOWS_TIMESTAMP_URL || 'http://timestamp.digicert.com',
        tsp: true,
      };
    }
  }
  return Object.keys(bundle).length ? { bundle } : {};
}

if (import.meta.url === pathToFileURL(process.argv[1]).href) {
  const out = process.argv[2] ?? 'release-overrides.json';
  const config = releaseConfig(process.env, process.platform);
  writeFileSync(out, JSON.stringify(config, null, 2) + '\n');
  const windows = config.bundle?.windows;
  console.log(
    `[release-config] ${out}: updater artifacts ${config.bundle?.createUpdaterArtifacts ? 'on' : 'off'}, ` +
      `Windows signing ${windows ? (windows.signCommand ? 'through the sign command' : 'through the store certificate') : 'off'}`,
  );
}
