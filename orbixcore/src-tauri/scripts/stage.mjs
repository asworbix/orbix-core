// Stage the bundle resources tauri.conf.json expects before `tauri build`,
// picking the platform's scripts: the orbix/bob CLI binaries everywhere,
// plus the Swift speech helper on macOS. One entry point so package.json
// and both CI runners call the same thing: `npm run stage`.
import { spawnSync } from 'node:child_process';
import { dirname, join } from 'node:path';
import { fileURLToPath } from 'node:url';

const here = dirname(fileURLToPath(import.meta.url));
const run = (cmd, args) => {
  const r = spawnSync(cmd, args, { stdio: 'inherit' });
  if (r.status !== 0) process.exit(r.status ?? 1);
};

if (process.platform === 'win32') {
  run('powershell', ['-NoProfile', '-ExecutionPolicy', 'Bypass', '-File', join(here, 'build-bob.ps1')]);
} else {
  run('bash', [join(here, 'build-bob.sh')]);
  if (process.platform === 'darwin') run('bash', [join(here, 'build-swift.sh')]);
}
