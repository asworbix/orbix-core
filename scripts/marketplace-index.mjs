#!/usr/bin/env node
// Builds marketplace/index.json, the catalog orbixcore.ai/marketplace shows.
//
// Two sources:
//   - marketplace/{skills,workflows,paired-apps,connectors}/: community
//     items, added by pull request.
//   - what ships inside the app (peer recipes, app bridges, workflow
//     templates, reference connectors), marked `included`, read from the
//     app's own resources so the catalog never drifts from the build.
//
// Small text items carry their file content so the site can offer a copy
// and a download without a second request.
//
//   node scripts/marketplace-index.mjs           write the index
//   node scripts/marketplace-index.mjs --check   exit 1 when it is stale (CI)

import fs from 'node:fs';
import path from 'node:path';
import { fileURLToPath } from 'node:url';

const ROOT = path.resolve(path.dirname(fileURLToPath(import.meta.url)), '..');
const MARKET = path.join(ROOT, 'marketplace');
const RES = path.join(ROOT, 'orbixcore/src-tauri/resources');
const TEMPLATES_RS = path.join(ROOT, 'orbixcore/src-tauri/src/workflow_templates.rs');
const OUT = path.join(MARKET, 'index.json');
const MAX_CONTENT = 20000;

const rel = (p) => path.relative(ROOT, p).split(path.sep).join('/');
const files = (dir, ext) =>
  fs.existsSync(dir)
    ? fs.readdirSync(dir).filter((f) => f.endsWith(ext) && !f.startsWith('_') && !f.startsWith('.')).sort()
    : [];
const problems = [];
// Skill names are slugs Bob looks up ("meeting-notes"); the site shows a title.
const humanize = (s) => (s || '').replace(/[-_]+/g, ' ').replace(/^./, (c) => c.toUpperCase());

function frontmatter(text, file) {
  const m = text.match(/^---\n([\s\S]*?)\n---\n?([\s\S]*)$/);
  if (!m) {
    problems.push(`${file}: missing frontmatter`);
    return { meta: {}, body: text };
  }
  const meta = {};
  for (const line of m[1].split('\n')) {
    const kv = line.match(/^([a-z_]+):\s*(.*)$/);
    if (kv) meta[kv[1]] = kv[2].replace(/^"(.*)"$/, '$1').trim();
  }
  return { meta, body: m[2].trim() };
}

function need(item, file, ...keys) {
  for (const k of keys) if (!item[k]) problems.push(`${file}: missing ${k}`);
  return item;
}

const items = [];

// ── Community items ──

for (const f of files(path.join(MARKET, 'skills'), '.md')) {
  const p = path.join(MARKET, 'skills', f);
  const text = fs.readFileSync(p, 'utf8');
  const { meta } = frontmatter(text, rel(p));
  items.push(need({
    kind: 'skill', slug: f.replace(/\.md$/, ''), name: meta.name, title: meta.title || humanize(meta.name), description: meta.description,
    author: meta.author, included: false, path: rel(p), filename: f, content: text,
  }, rel(p), 'name', 'description', 'author'));
}

for (const f of files(path.join(MARKET, 'workflows'), '.md')) {
  const p = path.join(MARKET, 'workflows', f);
  const { meta, body } = frontmatter(fs.readFileSync(p, 'utf8'), rel(p));
  items.push(need({
    kind: 'workflow', slug: f.replace(/\.md$/, ''), name: meta.name, description: meta.description,
    author: meta.author, included: false, schedule: meta.schedule || null, path: rel(p), content: body,
  }, rel(p), 'name', 'description', 'author'));
}

for (const f of files(path.join(MARKET, 'paired-apps'), '.json')) {
  const p = path.join(MARKET, 'paired-apps', f);
  const text = fs.readFileSync(p, 'utf8');
  let recipe = {};
  try { recipe = JSON.parse(text); } catch (e) { problems.push(`${rel(p)}: ${e.message}`); }
  items.push(need({
    kind: 'paired-app', slug: f.replace(/\.json$/, ''), name: recipe.app, description: recipe.description,
    author: recipe.author, included: false, path: rel(p), filename: f, content: text,
  }, rel(p), 'name', 'description', 'author'));
}

function connectorItem(dir, included) {
  const versions = fs.readdirSync(dir).filter((v) => fs.statSync(path.join(dir, v)).isDirectory()).sort();
  const latest = versions[versions.length - 1];
  if (!latest) return null;
  const mp = path.join(dir, latest, 'manifest.json');
  if (!fs.existsSync(mp)) { problems.push(`${rel(dir)}: no manifest.json in ${latest}`); return null; }
  const m = JSON.parse(fs.readFileSync(mp, 'utf8'));
  const actions = Object.keys(m.actions || {});
  return need({
    kind: 'connector', slug: m.slug || path.basename(dir), name: m.display_name || m.slug,
    description: m.description || `Connects ${m.display_name || m.slug}${actions.length ? `. Actions: ${actions.join(', ')}` : ''}.`,
    author: included ? 'Orbix' : m.author, included, version: m.version || latest,
    hosts: m.host_allowlist || [], path: rel(path.join(dir, latest)),
  }, rel(mp), 'name', 'author');
}

for (const d of fs.existsSync(path.join(MARKET, 'connectors')) ? fs.readdirSync(path.join(MARKET, 'connectors')).sort() : []) {
  const dir = path.join(MARKET, 'connectors', d);
  if (d.startsWith('.') || !fs.statSync(dir).isDirectory()) continue;
  const it = connectorItem(dir, false);
  if (it) items.push(it);
}

// ── Included with the app ──

const HOW = {
  bridge: 'reads it directly on your computer',
  service: 'uses the account you connected',
  mcp: 'talks to it over MCP',
  mcp_server: 'uses its MCP server',
  url: 'opens things through its links',
  devtools: 'drives it through its developer tools',
  accessibility: 'reads and types into its window',
};
const DISPLAY = { 'zoom.us': 'Zoom' };

for (const f of files(path.join(RES, 'peers'), '.json')) {
  const p = path.join(RES, 'peers', f);
  const r = JSON.parse(fs.readFileSync(p, 'utf8'));
  const ways = (r.transports || []).filter((t) => HOW[t]).map((t) => HOW[t]);
  items.push({
    kind: 'paired-app', slug: f.replace(/\.json$/, ''), name: DISPLAY[r.app] || r.app,
    description: `Bob ${ways.slice(0, 2).join(', or ')}.`, author: 'Orbix', included: true, path: rel(p),
  });
}

for (const f of files(path.join(RES, 'bridges'), '.json')) {
  const p = path.join(RES, 'bridges', f);
  const b = JSON.parse(fs.readFileSync(p, 'utf8'));
  const stem = f.replace(/\.json$/, '');
  if (items.some((i) => i.kind === 'paired-app' && i.included && (i.slug === stem || i.name === (b.display_name || b.app)))) {
    continue; // the app's peer recipe already lists it
  }
  const slug = `${stem}-bridge`;
  items.push({
    kind: 'paired-app', slug, name: b.display_name || b.app,
    description: 'Bob reads it directly on your computer. No account to connect.',
    author: 'Orbix', included: true, path: rel(p),
  });
}

const rs = fs.readFileSync(TEMPLATES_RS, 'utf8');
for (const m of rs.matchAll(/tpl\("([^"]+)",\s*"([^"]+)",\s*"([^"]+)",\s*"[^"]*",\s*"[^"]*",\s*(Some\("([^"]+)"\)|None)/g)) {
  items.push({
    kind: 'workflow', slug: m[1].replace(/^tpl-/, ''), name: m[2], description: m[3],
    author: 'Orbix', included: true, schedule: m[5] || null, path: rel(TEMPLATES_RS),
  });
}

for (const d of fs.readdirSync(path.join(RES, 'modules')).sort()) {
  const dir = path.join(RES, 'modules', d);
  // _sdk is the shim, echo and echo_sdk are test fixtures.
  if (d.startsWith('_') || d.startsWith('echo') || !fs.statSync(dir).isDirectory()) continue;
  const it = connectorItem(dir, true);
  if (it) items.push(it);
}

// ── Checks and output ──

const seen = new Set();
for (const it of items) {
  const key = `${it.kind}/${it.slug}`;
  if (seen.has(key)) problems.push(`duplicate ${key}`);
  seen.add(key);
  if (it.content && it.content.length > MAX_CONTENT) problems.push(`${it.path}: larger than ${MAX_CONTENT} characters`);
  for (const [k, v] of Object.entries(it)) {
    if (typeof v === 'string' && v.includes('—')) problems.push(`${it.path}: em dash in ${k}`);
  }
}
if (problems.length) {
  for (const p of problems) console.error('marketplace:', p);
  process.exit(1);
}

const order = { skill: 0, workflow: 1, 'paired-app': 2, connector: 3 };
for (const it of items) if (!it.title) it.title = it.name;
items.sort((a, b) => order[a.kind] - order[b.kind] || Number(a.included) - Number(b.included) || a.title.localeCompare(b.title));
const json = JSON.stringify({ version: 1, items }, null, 2) + '\n';

if (process.argv.includes('--check')) {
  const current = fs.existsSync(OUT) ? fs.readFileSync(OUT, 'utf8') : '';
  if (current !== json) {
    console.error('marketplace/index.json is out of date: run node scripts/marketplace-index.mjs and commit it.');
    process.exit(1);
  }
  console.log(`marketplace/index.json is current (${items.length} items)`);
} else {
  fs.writeFileSync(OUT, json);
  const counts = items.reduce((c, i) => ({ ...c, [i.kind]: (c[i.kind] || 0) + 1 }), {});
  console.log(`wrote marketplace/index.json: ${items.length} items`, counts);
}
