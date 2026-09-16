import { readFileSync } from 'node:fs';
import assert from 'node:assert/strict';
import { test } from 'node:test';
import { JSDOM } from 'jsdom';

const root = new URL('../app/src/main/assets/setup/', import.meta.url);
const html = readFileSync(new URL('index.html', root), 'utf8');
const script = readFileSync(new URL('app.js', root), 'utf8');
const ready = Object.fromEntries(['arm64', 'storage', 'space', 'node', 'nodeVersion', 'npm', 'git', 'codex', 'codexVersion', 'runtime', 'frontend', 'supervisor', 'authenticated'].map(key => [key, true]));

function setup(state = {}) {
  const dom = new JSDOM(html, { runScripts: 'outside-only', url: 'https://app.openaide.invalid/index.html' });
  const actions = [];
  dom.window.OpenAIDESetup = { send: message => actions.push(JSON.parse(message)) };
  dom.window.scrollTo = () => {};
  dom.window.eval(script);
  dom.window.receive({ initial: 'welcome', termux: true, permission: true, checks: ready, ...state });
  return { window: dom.window, document: dom.window.document, actions, close: () => dom.window.close() };
}

test('onboarding offers two understandable choices, not developer controls', () => {
  const view = setup();
  try {
    assert.match(view.document.querySelector('h1').textContent, /Where will youragents work/);
    assert.equal(view.document.querySelectorAll('.choice').length, 2);
    assert.doesNotMatch(view.document.querySelector('main').textContent, /checksum|supervisor|diagnostics/i);
    view.document.querySelector('[data-screen="local"]').click();
    assert.equal(view.actions.at(-1).action, 'check');
    assert.match(view.document.querySelector('h1').textContent, /Your phone is ready/);
  } finally { view.close(); }
});

test('missing tools offer installation without asking users for URLs or hashes', () => {
  const view = setup({ initial: 'local', checks: { ...ready, runtime: false } });
  try {
    view.document.querySelector('[data-action="install"]').click();
    assert.equal(view.actions.at(-1).action, 'install');
    assert.equal(view.document.querySelectorAll('input').length, 0);
  } finally { view.close(); }
});

test('permission and sign-in are separate guided steps', () => {
  const view = setup({ initial: 'local', permission: false });
  try {
    view.document.querySelector('[data-action="grant"]').click();
    assert.equal(view.actions.at(-1).action, 'grant');
    view.window.receive({ permission: true, termux: true, checks: { ...ready, authenticated: false } });
    view.document.querySelector('[data-action="signin"]').click();
    assert.equal(view.actions.at(-1).action, 'signin');
  } finally { view.close(); }
});

test('remote fields survive progress, errors and QR selection without interpreting HTML', () => {
  const view = setup({ initial: 'remote' });
  try {
    const form = view.document.getElementById('remote-form');
    for (const [key, value] of Object.entries({ address: 'https://server.example', username: 'user', password: 'test-secret' })) form.elements[key].value = value;
    form.dispatchEvent(new view.window.Event('submit', { cancelable: true }));
    assert.deepEqual(view.actions.at(-1), { action: 'remote', address: 'https://server.example', username: 'user', password: 'test-secret' });
    view.window.receive({ busy: true, notice: 'Connecting securely…' });
    assert.equal(view.document.getElementById('password').value, 'test-secret');
    assert.equal(view.document.querySelector('button[type="submit"]').disabled, true);
    view.window.receive({ busy: false, notice: 'Could not connect' });
    view.window.scanned('https://another.example');
    assert.equal(view.document.getElementById('username').value, 'user');
    assert.equal(view.document.getElementById('address').value, 'https://another.example');
    view.window.receive({ notice: '<img src=x onerror=alert(1)>' });
    assert.equal(view.document.querySelectorAll('img').length, 0);
  } finally { view.close(); }
});

test('opening settings and backing out never switches connections', () => {
  const view = setup({ initial: 'settings' });
  try {
    view.document.querySelector('[data-action="background_screen"]').click();
    view.window.back();
    assert.equal(view.document.querySelector('h1').textContent, 'Connection');
    view.window.back();
    assert.deepEqual(view.actions, [{ action: 'close' }]);
  } finally { view.close(); }
});

test('unsupported phones offer a remote workspace instead of repeated installation', () => {
  const view = setup({ initial: 'local', checks: { ...ready, arm64: false } });
  try {
    view.document.querySelector('[data-action="remote_screen"]').click();
    assert.equal(view.document.querySelector('h1').textContent, 'Bring your workspace');
    assert.equal(view.document.querySelectorAll('[data-action="install"]').length, 0);
  } finally { view.close(); }
});

test('an incompatible existing agent gets an explicit recovery, not an install loop', () => {
  const view = setup({ initial: 'local', checks: { ...ready, codexVersion: false } });
  try {
    assert.match(view.document.querySelector('h1').textContent, /agent needs an update/);
    assert.equal(view.document.querySelectorAll('[data-action="install"]').length, 0);
    view.document.querySelector('[data-action="termux"]').click();
    assert.equal(view.actions.at(-1).action, 'termux');
  } finally { view.close(); }
});
