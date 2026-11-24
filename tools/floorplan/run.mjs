#!/usr/bin/env node
// Project-owned orchestration; upstream Floorplan remains unmodified.
import fs from 'node:fs';
import path from 'node:path';
import crypto from 'node:crypto';
import { execFileSync, spawn } from 'node:child_process';
import { fileURLToPath } from 'node:url';
import { renderViews } from './views.mjs';

const directory = path.dirname(fileURLToPath(import.meta.url));
const root = path.resolve(directory, '../..');
const pin = readJson(path.join(directory, 'floorplan.lock.json'));
const output = path.join(root, 'floorplan');
const layouts = ['app', 'layers'];
// Native servers resolve the codebase as their output folder's parent.
const destination = view => view === 'app' ? output : path.join(root, 'floorplan-layers');
const authored = view => path.join(directory, 'views', view);
const sleep = ms => new Promise(resolve => setTimeout(resolve, ms));

function readJson(file) { return JSON.parse(fs.readFileSync(file, 'utf8')); }
function optionalJson(file) { try { return readJson(file); } catch { return null; } }
function git(cwd, ...args) {
  return execFileSync('git', ['--no-optional-locks', '-C', cwd, ...args], { encoding: 'utf8', stdio: ['ignore', 'pipe', 'pipe'] }).trim();
}
function writeChanged(file, content) {
  if (fs.existsSync(file) && fs.readFileSync(file, 'utf8') === content) return;
  fs.mkdirSync(path.dirname(file), { recursive: true });
  const pending = `${file}.${process.pid}.tmp`;
  fs.writeFileSync(pending, content);
  fs.renameSync(pending, file);
}
function writeJson(file, data) { writeChanged(file, JSON.stringify(data, null, 2) + '\n'); }
function copyIfPresent(source, target) {
  if (fs.existsSync(source)) writeChanged(target, fs.readFileSync(source, 'utf8'));
}

export function verifySource(source, expected) {
  const revision = git(source, 'rev-parse', 'HEAD');
  if (revision !== expected.revision) throw new Error(`Floorplan revision mismatch: expected ${expected.revision}, found ${revision}`);
  if (readJson(path.join(source, '.claude-plugin/plugin.json')).version !== expected.version) throw new Error('Floorplan version mismatch');
  if (git(source, 'status', '--porcelain', '--untracked-files=no')) throw new Error('Floorplan has modified tracked files; use a clean pinned checkout');
  for (const script of ['scan', 'build', 'describe']) {
    if (!fs.existsSync(path.join(source, 'skills/floorplan/scripts', `${script}.mjs`))) throw new Error(`Missing Floorplan ${script} script`);
  }
  return path.join(source, 'skills/floorplan');
}

function ensureSource() {
  const override = process.env.FLOORPLAN_SOURCE_DIR;
  if (override) return verifySource(path.resolve(override), pin);
  const source = path.join(output, '.cache/tool', pin.revision);
  if (!fs.existsSync(path.join(source, '.git'))) {
    fs.mkdirSync(source, { recursive: true });
    git(source, 'init', '--quiet');
    git(source, 'remote', 'add', 'origin', pin.repository);
  }
  let head;
  try { head = git(source, 'rev-parse', 'HEAD'); } catch { /* Resume an interrupted first fetch. */ }
  if (!head) {
    console.log(`Fetching Floorplan ${pin.version} (${pin.revision.slice(0, 12)})…`);
    git(source, 'fetch', '--depth=1', 'origin', pin.revision);
    git(source, 'checkout', '--detach', pin.revision);
  }
  return verifySource(source, pin);
}

function upstream(skill, script, args) {
  const stdout = execFileSync(process.execPath, [path.join(skill, 'scripts', `${script}.mjs`), ...args, '--json', '--unattended'], {
    cwd: root, encoding: 'utf8', maxBuffer: 32 * 1024 * 1024,
    env: { ...process.env, FLOORPLAN_UNATTENDED: '1' }, stdio: ['ignore', 'pipe', 'inherit'],
  });
  const result = JSON.parse(stdout);
  if (!result.ok) throw new Error(result.errors?.join('\n') || `${script} failed`);
  return result;
}

export function checkViewFiles(graphs) {
  const app = graphs.app.files.map(file => file.p).sort();
  const layers = graphs.layers.files.map(file => file.p).sort();
  if (JSON.stringify(app) !== JSON.stringify(layers)) throw new Error('Views cover different files; review both architecture configurations');
  return app.length;
}

function build(skill) {
  fs.mkdirSync(output, { recursive: true });
  writeChanged(path.join(output, '.gitignore'), '*\n');
  const scan = upstream(skill, 'scan', [root]);
  console.log(scan.summary.split('\n')[0]);
  const stage = fs.mkdtempSync(path.join(output, '.cache/build-'));
  const graphs = {}, reports = {};
  // Validate both candidates before replacing either served map.
  for (const view of layouts) {
    const pending = path.join(stage, view);
    fs.mkdirSync(pending, { recursive: true });
    fs.cpSync(path.join(directory, 'descriptions'), path.join(pending, 'descriptions'), { recursive: true });
    fs.copyFileSync(path.join(authored(view), 'architecture.json'), path.join(pending, 'architecture.json'));
    for (const name of ['custom-flows.json', 'health-dismissed.json']) {
      const source = path.join(authored(view), name);
      copyIfPresent(fs.existsSync(source) ? source : path.join(destination(view), name), path.join(pending, name));
    }
    reports[view] = upstream(skill, 'build', [root, '--out', pending, '--scan', path.join(output, '.cache/scan.json')]);
    const report = reports[view];
    if (report.files.unassigned || !report.descriptions.complete) {
      throw new Error(`${view}: ${report.files.unassigned} unassigned files; descriptions complete=${report.descriptions.complete}. Review the map with $floorplan and run capture; see tools/floorplan/README.md.`);
    }
    graphs[view] = readJson(path.join(pending, 'graph.json'));
  }
  const files = checkViewFiles(graphs);
  const findings = health => { const { dismissed, ...checks } = health; return checks; };
  if (JSON.stringify(findings(graphs.app.health)) !== JSON.stringify(findings(graphs.layers.health))) throw new Error('View Health findings disagree');
  for (const view of layouts) {
    const pending = path.join(stage, view), target = destination(view);
    fs.mkdirSync(target, { recursive: true });
    writeChanged(path.join(target, '.gitignore'), '*\n');
    for (const name of ['architecture.json', 'graph.json', 'query.mjs', 'serve.mjs', 'page-safety.mjs', 'custom-flows.json', 'health-dismissed.json']) copyIfPresent(path.join(pending, name), path.join(target, name));
    fs.cpSync(path.join(directory, 'descriptions'), path.join(target, 'descriptions'), { recursive: true });
    // Publishing index last makes upstream's watcher reload matching graph/query files.
    copyIfPresent(path.join(pending, 'index.html'), path.join(target, 'index.html'));
  }
  const warnings = [...new Set(layouts.flatMap(view => reports[view].warnings))].sort();
  writeJson(path.join(directory, 'health.json'), { schema: 'cyoa-floorplan-health@1', generator: pin, health: graphs.app.health, buildWarnings: warnings });
  writeJson(path.join(output, '.cache/build.json'), { files, views: layouts, warnings });
  fs.rmSync(stage, { recursive: true });
  console.log(`Built both views: ${files} files each; all assigned and described.`);
  for (const warning of warnings) console.log(`Warning: ${warning}`);
  console.log(`Health: ${JSON.stringify(graphs.app.health.summary)}; tools/floorplan/health.json updated only when its contents change.`);
}

export function localAddress(raw) {
  const url = new URL(raw);
  if (url.protocol !== 'http:' || url.hostname !== '127.0.0.1' || !url.port || url.username || url.password || url.pathname !== '/' || url.search || url.hash) throw new Error('Invalid local Floorplan server address');
  return url.origin;
}
async function status(state) {
  if (!state?.url || !state?.token) return null;
  const url = localAddress(state.url);
  try {
    const response = await fetch(url + '/api/status', { headers: { 'x-floorplan-token': state.token }, signal: AbortSignal.timeout(1500) });
    const value = response.ok ? await response.json() : null;
    if (value && path.resolve(value.root) !== root) throw new Error('Server belongs to a different checkout');
    return value;
  } catch (error) {
    if (error.message === 'Server belongs to a different checkout') throw error;
    return null;
  }
}
function stopView(view) {
  const file = path.join(destination(view), 'serve.mjs');
  if (fs.existsSync(file)) execFileSync(process.execPath, [file, '--stop'], { cwd: root, stdio: 'inherit' });
}
async function ensureServer(view, port) {
  const folder = destination(view), stateFile = path.join(folder, '.cache/server.json');
  let state = optionalJson(stateFile), active = await status(state);
  const hash = crypto.createHash('sha1').update(fs.readFileSync(path.join(folder, 'serve.mjs'))).digest('hex').slice(0, 12);
  if (active && active.hash !== hash) { stopView(view); active = null; }
  if (!active) {
    fs.mkdirSync(path.join(folder, '.cache'), { recursive: true });
    const log = fs.openSync(path.join(folder, '.cache/server.log'), 'w');
    const child = spawn(process.execPath, [path.join(folder, 'serve.mjs'), '--no-open', '--port', String(port), '--idle-exit', '120'], { cwd: root, detached: true, stdio: ['ignore', log, log] });
    let failure; child.on('error', error => { failure = error; }); child.unref(); fs.closeSync(log);
    for (let i = 0; i < 100; i++) {
      await sleep(200);
      if (failure) throw failure;
      state = optionalJson(stateFile); active = await status(state);
      if (active && active.hash === hash) break;
    }
    if (!active || active.hash !== hash) throw new Error(`${view} server did not start; inspect ${path.relative(root, folder)}/.cache/server.log`);
  }
  return localAddress(state.url);
}
function browser(url) {
  const command = process.platform === 'darwin' ? ['open', [url]] : process.platform === 'win32' ? ['cmd', ['/c', 'start', '', url]] : ['xdg-open', [url]];
  const child = spawn(command[0], command[1], { stdio: 'ignore', detached: true });
  child.on('error', () => console.log(`Open ${url} in your browser.`)); child.unref();
}
async function serve(noOpen) {
  for (const view of layouts) if (!fs.existsSync(path.join(destination(view), 'index.html'))) throw new Error('Build both maps first');
  const app = await ensureServer('app', 4378), layers = await ensureServer('layers', 4380);
  writeChanged(path.join(output, 'views.html'), renderViews(app, layers));
  const url = app + '/views.html';
  if (!noOpen) browser(url);
  console.log(`Floorplan: ${url}`);
  return url;
}
function capture() {
  // The ordinary Floorplan skill edits its app map. Persist those authored inputs.
  const app = readJson(path.join(output, 'architecture.json'));
  writeJson(path.join(authored('app'), 'architecture.json'), app);
  for (const name of fs.readdirSync(path.join(output, 'descriptions')).filter(name => name.endsWith('.json'))) {
    const data = readJson(path.join(output, 'descriptions', name));
    writeJson(path.join(directory, 'descriptions', name), data);
  }
  for (const view of layouts) for (const name of ['custom-flows.json', 'health-dismissed.json']) {
    const file = path.join(destination(view), name);
    if (fs.existsSync(file)) writeJson(path.join(authored(view), name), readJson(file));
  }
  console.log('Captured app architecture, shared descriptions and per-view flows/Health decisions. Layer architecture stays at tools/floorplan/views/layers/architecture.json.');
}

export async function main(args = process.argv.slice(2)) {
  const action = args.find(arg => !arg.startsWith('--')) || 'open';
  if (args.includes('--help')) {
    console.log('Usage: node tools/floorplan/run.mjs [open|setup|build|serve|capture|stop] [--no-open]\nDefault: fetch the pinned tool if needed, rebuild both views and open them. No LLM call is made.'); return;
  }
  if (args.some(arg => arg.startsWith('--') && arg !== '--no-open')) throw new Error('Unknown option; use --help');
  if (args.filter(arg => !arg.startsWith('--')).length > 1) throw new Error('Expected one command; use --help');
  if (action === 'stop') { layouts.forEach(stopView); return; }
  if (action === 'capture') { capture(); return; }
  if (action === 'serve') { await serve(args.includes('--no-open')); return; }
  if (!['open', 'setup', 'build'].includes(action)) throw new Error('Unknown command; use --help');
  const skill = ensureSource();
  if (action === 'setup') { console.log(`Pinned Floorplan ready: ${pin.version} at ${pin.revision}`); return; }
  build(skill);
  if (action === 'open') await serve(args.includes('--no-open'));
}
if (process.argv[1] && path.resolve(process.argv[1]) === fileURLToPath(import.meta.url)) main().catch(error => {
  console.error(`Floorplan: ${error.message}`); process.exitCode = 1;
});
