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
    run: (checksum = hash, extra = {}) => spawnSync('bash', ['apps/android/app/src/main/assets/install-termux.sh'], {
      cwd: path.resolve(new URL('../../..', import.meta.url).pathname),
      env: { ...process.env, HOME: home, PREFIX: path.join(root, 'tools'), FIXTURE_ARCHIVE: archive,
        OPENAIDE_RUNTIME_URL: 'https://downloads.example/runtime.tar.gz', OPENAIDE_RUNTIME_SHA256: checksum, ...extra },
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

// Replaces the fixture's agents with what the Claude install drives: npm, Termux's glibc files and a shell.
// The stand-in binary is a shell script, so the step that rewrites a real binary's loader path is a stand-in too.
function claudeTools(installation, starts, termuxExec = false) {
  const prefix = path.join(installation.runtimeRoot, '../../../../tools');
  const tools = path.join(prefix, 'bin');
  fs.rmSync(path.join(tools, 'claude'));
  fs.symlinkSync('/bin/sh', path.join(tools, 'sh'));
  fs.mkdirSync(path.join(prefix, 'glibc/lib'), { recursive: true });
  fs.writeFileSync(path.join(prefix, 'glibc/lib/ld-linux-aarch64.so.1'), '', { mode: 0o700 });
  fs.writeFileSync(path.join(prefix, 'glibc/lib/libtermux-exec.so'), '');
  if (termuxExec) { fs.mkdirSync(path.join(prefix, 'lib')); fs.writeFileSync(path.join(prefix, 'lib/libtermux-exec.so'), ''); }
  fs.writeFileSync(path.join(tools, 'node'), `#!${process.execPath}\nprocess.exit(process.argv[2] === '-' ? 0 : 1);\n`, { mode: 0o700 });
  const preload = termuxExec ? `test "$LD_PRELOAD" = '${prefix}/opt/openaide-claude/preload/$LIB/exec.so' || exit 1` : 'test -z "${LD_PRELOAD:-}" || exit 1';
  fs.writeFileSync(path.join(tools, 'npm'), `#!${process.execPath}
const fs = require('node:fs'), path = require('node:path'), args = process.argv.slice(2);
if (args[0] === 'view') { console.log(args[1] === '@openaide/claude-agent-acp@1.5.0' ? '0.3.293' : ''); process.exit(0); }
if (!args.includes('@anthropic-ai/claude-agent-sdk-linux-arm64@0.3.293')) process.exit(1);
const target = path.join(args[args.indexOf('--prefix') + 1], 'node_modules/@anthropic-ai/claude-agent-sdk-linux-arm64');
fs.mkdirSync(target, { recursive: true });
fs.writeFileSync(path.join(target, 'claude'), ${JSON.stringify(`#!/bin/sh\n${preload}\ntest "$DISABLE_AUTOUPDATER" = 1 || exit 1\nexit ${starts ? 0 : 1}\n`)});
`, { mode: 0o700 });
  // A Claude installed on the machine running the tests must not count as the phone's.
  const systemPath = ['/usr/bin', '/bin'];
  if (!fs.existsSync(path.join(path.dirname(process.execPath), 'claude'))) systemPath.unshift(path.dirname(process.execPath));
  return { command: path.join(tools, 'claude'), binary: path.join(prefix, 'opt/openaide-claude/claude-0.3.293'),
    env: { OPENAIDE_CLAUDE_ACP_VERSION: '1.5.0', PATH: systemPath.join(':'), LD_PRELOAD: '' } };
}

test('installs the Claude binary of the adapter behind a command that starts it', context => {
  const installation = fixture(context);
  const { command, binary, env } = claudeTools(installation, true);
  assert.equal(installation.run(undefined, env).status, 0);
  assert.ok(fs.existsSync(binary));
  assert.equal(spawnSync(command, ['--version'], { env: { PATH: env.PATH, LD_PRELOAD: 'libtermux-exec.so' } }).status, 0);
  assert.match(fs.readFileSync(path.join(installation.runtimeRoot, 'install.log'), 'utf8'), /claude=installed sdk=0\.3\.293/);
});

test('Claude and the Android programs it starts each find their own exec library', context => {
  const installation = fixture(context);
  const { command, env } = claudeTools(installation, true, true);
  assert.equal(installation.run(undefined, env).status, 0);
  assert.equal(spawnSync(command, ['--version'], { env: { PATH: env.PATH } }).status, 0);
  const preload = path.join(path.dirname(command), '../opt/openaide-claude/preload');
  assert.match(fs.readlinkSync(path.join(preload, '$LIB/exec.so')), /\/lib\/libtermux-exec\.so$/);
  assert.match(fs.readlinkSync(path.join(preload, 'lib/exec.so')), /\/glibc\/lib\/libtermux-exec\.so$/);
});

test('a Claude binary that cannot start is removed and setup continues with Codex', context => {
  const installation = fixture(context);
  const { command, binary, env } = claudeTools(installation, false);
  assert.equal(installation.run(undefined, env).status, 0);
  assert.equal(fs.existsSync(command), false);
  assert.equal(fs.existsSync(binary), false);
  assert.match(fs.readFileSync(path.join(installation.runtimeRoot, 'install.log'), 'utf8'), /claude=unavailable step=start/);
});

// The loader path step is a Node program inside the install script; these run it on a small made-up binary.
function loaderPath(context, padding) {
  const script = fs.readFileSync(new URL('../app/src/main/assets/install-termux.sh', import.meta.url), 'utf8');
  const program = script.slice(script.indexOf("<<'ELF_LOADER_PATH'"), script.indexOf('\nELF_LOADER_PATH\n')).split('\n').slice(1).join('\n');
  const root = fs.mkdtempSync(path.join(os.tmpdir(), 'openaide-elf-'));
  context.after(() => fs.rmSync(root, { recursive: true, force: true }));
  const segmentSize = 4096 - padding;
  const file = Buffer.alloc(8192);
  file.writeUInt32BE(0x7f454c46, 0); file[4] = 2; file[5] = 1;
  file.writeBigUInt64LE(64n, 0x20); file.writeUInt16LE(56, 0x36); file.writeUInt16LE(2, 0x38);
  const header = (index, type, flags, offset, address, size) => {
    const at = 64 + index * 56;
    file.writeUInt32LE(type, at); file.writeUInt32LE(flags, at + 4); file.writeBigUInt64LE(BigInt(offset), at + 8);
    file.writeBigUInt64LE(BigInt(address), at + 16); file.writeBigUInt64LE(BigInt(address), at + 24);
    file.writeBigUInt64LE(BigInt(size), at + 32); file.writeBigUInt64LE(BigInt(size), at + 40);
  };
  file.write('/lib/ld-linux-aarch64.so.1\0', 200);
  header(0, 3, 4, 200, 0x200000 + 200, 27);
  header(1, 1, 4, 0, 0x200000, segmentSize);
  file.fill(1, 256, segmentSize);
  const target = path.join(root, 'binary');
  fs.writeFileSync(target, file);
  const loader = '/data/data/com.termux/files/usr/glibc/lib/ld-linux-aarch64.so.1';
  const result = spawnSync(process.execPath, ['-', target, loader], { input: program, encoding: 'utf8' });
  return { result, before: file, after: fs.readFileSync(target), loader, segmentSize };
}

test('the loader path is written into padding without moving the rest of the binary', context => {
  const { result, before, after, loader, segmentSize } = loaderPath(context, 128);
  assert.equal(result.status, 0, result.stderr);
  assert.equal(after.length, before.length);
  assert.equal(Number(after.readBigUInt64LE(64 + 8)), segmentSize);
  assert.equal(Number(after.readBigUInt64LE(64 + 16)), 0x200000 + segmentSize);
  assert.equal(Number(after.readBigUInt64LE(64 + 32)), loader.length + 1);
  assert.equal(after.toString('latin1', segmentSize, segmentSize + loader.length + 1), loader + '\0');
  assert.ok(after.subarray(256, segmentSize).equals(before.subarray(256, segmentSize)));
});

test('a binary without room for the loader path is left unchanged', context => {
  const { result, before, after } = loaderPath(context, 16);
  assert.notEqual(result.status, 0);
  assert.ok(after.equals(before));
});
