import test from 'node:test';
import assert from 'node:assert/strict';
import fs from 'node:fs';
import os from 'node:os';
import path from 'node:path';
import { execFileSync } from 'node:child_process';
import { checkViewFiles, localAddress, verifySource } from './run.mjs';
import { renderViews } from './views.mjs';

test('both views must cover the same source files regardless of display order', () => {
  assert.equal(checkViewFiles({ app: { files: [{ p: 'a.rs' }, { p: 'b.rs' }] }, layers: { files: [{ p: 'b.rs' }, { p: 'a.rs' }] } }), 2);
  assert.throws(() => checkViewFiles({ app: { files: [{ p: 'a.rs' }] }, layers: { files: [{ p: 'b.rs' }] } }), /different files/);
});

test('cached server addresses stay on loopback without credentials or paths', () => {
  assert.equal(localAddress('http://127.0.0.1:4378/'), 'http://127.0.0.1:4378');
  for (const url of ['https://127.0.0.1:4378', 'http://example.com:4378', 'http://user:pass@127.0.0.1:4378', 'http://127.0.0.1:4378/other', 'http://127.0.0.1:4378/?token=x']) {
    assert.throws(() => localAddress(url), /Invalid local/);
  }
});

test('switcher uses the actual allocated ports and defaults to app relationships', () => {
  const html = renderViews('http://127.0.0.1:4393', 'http://127.0.0.1:4398');
  assert.match(html, /127\.0\.0\.1:4393/);
  assert.match(html, /127\.0\.0\.1:4398/);
  assert.match(html, /App relationships/);
  assert.match(html, /Architecture layers/);
  assert.match(html, /get\('view'\)\|\|'app'/);
  assert.doesNotMatch(html, /\/home\/|\.codex\/plugins/);
});

test('dependency override rejects a wrong revision or edited tool even in a path with spaces', t => {
  const folder = fs.mkdtempSync(path.join(os.tmpdir(), 'floorplan source '));
  t.after(() => fs.rmSync(folder, { recursive: true }));
  const git = (...args) => execFileSync('git', ['-C', folder, ...args], { encoding: 'utf8', stdio: ['ignore', 'pipe', 'pipe'] }).trim();
  git('init', '--quiet');
  fs.mkdirSync(path.join(folder, '.claude-plugin'));
  fs.writeFileSync(path.join(folder, '.claude-plugin/plugin.json'), JSON.stringify({ version: 'test' }));
  fs.mkdirSync(path.join(folder, 'skills/floorplan/scripts'), { recursive: true });
  for (const name of ['scan', 'build', 'describe']) fs.writeFileSync(path.join(folder, `skills/floorplan/scripts/${name}.mjs`), '// fixture\n');
  git('add', '.');
  git('-c', 'user.name=Floorplan test', '-c', 'user.email=floorplan@example.invalid', 'commit', '--quiet', '-m', 'fixture');
  const revision = git('rev-parse', 'HEAD');
  assert.equal(verifySource(folder, { revision, version: 'test' }), path.join(folder, 'skills/floorplan'));
  assert.throws(() => verifySource(folder, { revision: '0'.repeat(40), version: 'test' }), /revision mismatch/);
  assert.throws(() => verifySource(folder, { revision, version: 'wrong' }), /version mismatch/);
  fs.appendFileSync(path.join(folder, 'skills/floorplan/scripts/scan.mjs'), '// modified\n');
  assert.throws(() => verifySource(folder, { revision, version: 'test' }), /modified tracked files/);
});
