// Build the Tauri updater manifest from a published GitHub Release and POST
// it to orbixcore.ai/api/update, so installed apps are offered the version.
//
// Runs from .github/workflows/publish-update.yml on `release: published`,
// or by hand:
//   RELEASE_TAG=v0.1.0 ORBIX_ADMIN_SECRET=... node scripts/publish-update-manifest.mjs
//
// The updater installs only signed artifacts, so a release built without
// TAURI_SIGNING_PRIVATE_KEY (no .sig assets) is skipped with a notice rather
// than published as an update nobody could install.

const repo = process.env.GITHUB_REPOSITORY || 'asworbix/orbix-core';
const tag = process.env.RELEASE_TAG;
const endpoint = process.env.UPDATE_ENDPOINT || 'https://orbixcore.ai/api/update';
const secret = process.env.ORBIX_ADMIN_SECRET;
const token = process.env.GITHUB_TOKEN;

if (!tag) {
  console.error('RELEASE_TAG is required (e.g. v0.1.0)');
  process.exit(1);
}
if (!secret) {
  console.log('ORBIX_ADMIN_SECRET is not set; skipping the manifest publish (forks and dry runs).');
  process.exit(0);
}

// Updater artifact per platform key the app's updater asks for
// ("{{target}}-{{arch}}"). Tauri names bundles "<product>_<version>_<arch>"
// and GitHub swaps the product's space for a dot, so match on the tail.
const PLATFORMS = {
  'darwin-aarch64': [/_aarch64\.app\.tar\.gz$/i],
  'darwin-x86_64': [/_x64\.app\.tar\.gz$/i],
  // v2 updater format signs the NSIS installer itself; v1-compatible builds
  // ship a .nsis.zip instead. Accept either.
  'windows-x86_64': [/_x64-setup\.exe$/i, /_x64-setup\.nsis\.zip$/i],
};

const headers = { 'User-Agent': 'orbix-core release', Accept: 'application/vnd.github+json' };
if (token) headers.Authorization = `Bearer ${token}`;

const relRes = await fetch(`https://api.github.com/repos/${repo}/releases/tags/${tag}`, { headers });
if (!relRes.ok) {
  console.error(`GitHub responded ${relRes.status} for release ${tag}`);
  process.exit(1);
}
const rel = await relRes.json();
const assets = rel.assets || [];
const byName = (re) => assets.find((a) => re.test(a.name));

const platforms = {};
for (const [key, patterns] of Object.entries(PLATFORMS)) {
  for (const re of patterns) {
    const artifact = byName(re);
    if (!artifact) continue;
    const sig = assets.find((a) => a.name === `${artifact.name}.sig`);
    if (!sig) {
      console.log(`${key}: ${artifact.name} has no .sig; not offered to the updater.`);
      break;
    }
    const sigRes = await fetch(sig.browser_download_url, { headers: { 'User-Agent': headers['User-Agent'] } });
    if (!sigRes.ok) {
      console.error(`${key}: could not read ${sig.name} (${sigRes.status})`);
      process.exit(1);
    }
    platforms[key] = { url: artifact.browser_download_url, signature: (await sigRes.text()).trim() };
    console.log(`${key}: ${artifact.name}`);
    break;
  }
}

if (Object.keys(platforms).length === 0) {
  console.log('No signed updater artifacts in this release; nothing published.');
  process.exit(0);
}

const version = String(rel.tag_name || tag).replace(/^v/, '');
const manifest = {
  version,
  notes: (rel.body || '').trim().slice(0, 4000) || `Orbix Core ${version}`,
  pub_date: rel.published_at || new Date().toISOString(),
  platforms,
};

const post = await fetch(endpoint, {
  method: 'POST',
  headers: { 'Content-Type': 'application/json', 'x-admin-secret': secret },
  body: JSON.stringify(manifest),
});
const text = await post.text();
if (!post.ok) {
  console.error(`Manifest publish failed: ${post.status} ${text}`);
  process.exit(1);
}
console.log(`Published update manifest ${version} for ${Object.keys(platforms).join(', ')}: ${text}`);
