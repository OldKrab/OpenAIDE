import assert from 'node:assert/strict';
import { createHash } from 'node:crypto';
import { spawnSync } from 'node:child_process';
import fs from 'node:fs';
import os from 'node:os';
import path from 'node:path';
import test from 'node:test';

function fixture(context, symlink = false, versioned = true) {
  const root = fs.mkdtempSync(path.join(os.tmpdir(), 'openaide-install-'));
  context.after(() => fs.rmSync(root, { recursive: true, force: true }));
  const source = path.join(root, 'source/runtime');
  const tools = path.join(root, 'tools/bin');
  fs.mkdirSync(path.join(source, 'bin'), { recursive: true });
  fs.mkdirSync(tools, { recursive: true });
  fs.writeFileSync(path.join(source, 'bin/openaide-app-server'), 'fixture', { mode: 0o700 });
  if (versioned) fs.writeFileSync(path.join(source, 'VERSION'), '1.2.3\n');
  if (symlink) fs.symlinkSync('../../outside', path.join(source, 'escape'));
  const archive = path.join(root, 'runtime.tar.gz');
  assert.equal(spawnSync('tar', ['-czf', archive, '-C', path.join(root, 'source'), 'runtime']).status, 0);
  for (const command of ['pkg', 'codex', 'claude', 'pkill', 'pgrep']) fs.writeFileSync(path.join(tools, command), `#!${process.execPath}\nprocess.exit(0);\n`, { mode: 0o700 });
  fs.writeFileSync(path.join(tools, 'curl'), `#!${process.execPath}\nrequire('node:fs').copyFileSync(process.env.FIXTURE_ARCHIVE, process.argv[process.argv.indexOf('--output') + 1]);\n`, { mode: 0o700 });
  const home = path.join(root, 'home');
  fs.mkdirSync(home);
  const runtimeRoot = path.join(home, '.local/share/openaide-android');
  const hash = createHash('sha256').update(fs.readFileSync(archive)).digest('hex');
  return {
    runtimeRoot,
    run: (checksum = hash) => spawnSync('bash', ['apps/android/app/src/main/assets/install-termux.sh'], {
      cwd: path.resolve(new URL('../../..', import.meta.url).pathname),
      env: { ...process.env, HOME: home, PREFIX: path.join(root, 'tools'), FIXTURE_ARCHIVE: archive,
        OPENAIDE_RUNTIME_URL: 'https://downloads.example/runtime.tar.gz', OPENAIDE_RUNTIME_SHA256: checksum },
      encoding: 'utf8',
    }),
  };
}

test('installs a verified runtime without downloading through a real network', context => {
  const installation = fixture(context);
  assert.equal(installation.run().status, 0);
  assert.ok(fs.existsSync(path.join(installation.runtimeRoot, 'runtime/bin/openaide-app-server')));
});

test('checksum mismatch does not install a runtime', context => {
  const installation = fixture(context);
  assert.notEqual(installation.run('0'.repeat(64)).status, 0);
  assert.equal(fs.existsSync(path.join(installation.runtimeRoot, 'runtime')), false);
});

test('an update replaces the runtime and keeps task state', context => {
  const installation = fixture(context);
  fs.mkdirSync(path.join(installation.runtimeRoot, 'runtime'), { recursive: true });
  fs.mkdirSync(path.join(installation.runtimeRoot, 'state'));
  fs.writeFileSync(path.join(installation.runtimeRoot, 'runtime/old'), 'old');
  fs.writeFileSync(path.join(installation.runtimeRoot, 'state/keep'), 'history');
  assert.equal(installation.run().status, 0);
  assert.equal(fs.existsSync(path.join(installation.runtimeRoot, 'runtime/old')), false);
  assert.equal(fs.readFileSync(path.join(installation.runtimeRoot, 'runtime/VERSION'), 'utf8'), '1.2.3\n');
  assert.equal(fs.readFileSync(path.join(installation.runtimeRoot, 'state/keep'), 'utf8'), 'history');
  assert.equal(fs.existsSync(path.join(installation.runtimeRoot, 'runtime.previous')), false);
});

test('a runtime without a version is not installed', context => {
  const installation = fixture(context, false, false);
  assert.notEqual(installation.run().status, 0);
  assert.equal(fs.existsSync(path.join(installation.runtimeRoot, 'runtime')), false);
});

test('archive links cannot escape the staging directory', context => {
  const installation = fixture(context, true);
  assert.notEqual(installation.run().status, 0);
  assert.equal(fs.existsSync(path.join(installation.runtimeRoot, 'runtime')), false);
});
