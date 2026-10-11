import { readFileSync } from 'node:fs';
import assert from 'node:assert/strict';
import { test } from 'node:test';
import { JSDOM } from 'jsdom';

const root = new URL('../app/src/main/assets/setup/', import.meta.url);
const html = readFileSync(new URL('index.html', root), 'utf8');
const script = readFileSync(new URL('app.js', root), 'utf8');
const ready = Object.fromEntries(['arm64', 'storage', 'space', 'node', 'nodeVersion', 'npm', 'git', 'agent', 'agentVersion', 'runtime', 'authenticated'].map(key => [key, true]));

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

test('pairing asks for a code, never an address or password, and keeps a pasted code through progress', () => {
  const view = setup({ initial: 'remote' });
  try {
    assert.equal(view.document.querySelectorAll('input').length, 0);
    view.document.querySelector('[data-action="scan"]').click();
    assert.deepEqual(view.actions.at(-1), { action: 'scan' });
    view.document.getElementById('code').value = 'OAI1 ABCD';
    view.document.getElementById('pair-form').dispatchEvent(new view.window.Event('submit', { cancelable: true }));
    assert.deepEqual(view.actions.at(-1), { action: 'pair', code: 'OAI1 ABCD' });
    view.window.receive({ busy: true, notice: 'Pairing with your computer…' });
    assert.equal(view.document.getElementById('code').value, 'OAI1 ABCD');
    assert.equal(view.document.querySelector('button[type="submit"]').disabled, true);
    view.window.receive({ busy: false, notice: '<img src=x onerror=alert(1)>' });
    assert.equal(view.document.getElementById('code').value, 'OAI1 ABCD');
    assert.equal(view.document.querySelectorAll('img').length, 0);
  } finally { view.close(); }
});

test('this phone’s code is requested only while its screen is open', () => {
  const view = setup({ initial: 'remote' });
  try {
    view.document.querySelector('[data-action="join_screen"]').click();
    assert.deepEqual(view.actions.at(-1), { action: 'join' });
    assert.match(view.document.querySelector('main').textContent, /Preparing this phone/);
    view.window.receive({ join: { text: 'OAJ1 ABCD', qr: { size: 2, path: 'M0 0h1v1h-1z' } } });
    assert.equal(view.document.querySelector('.code').textContent, 'OAJ1 ABCD');
    assert.equal(view.document.querySelector('.qr path').getAttribute('d'), 'M0 0h1v1h-1z');
    view.window.back();
    assert.deepEqual(view.actions.at(-1), { action: 'join_stop' });
    assert.ok(view.document.getElementById('pair-form'));
  } finally { view.close(); }
});

test('a paired computer is named without interpreting its name as HTML', () => {
  const view = setup({ initial: 'remote', paired: true, remote: false, computer: '<img src=x onerror=alert(1)>' });
  try {
    assert.equal(view.document.querySelectorAll('img').length, 0);
    assert.match(view.document.querySelector('.panel h2').textContent, /<img/);
    view.document.querySelector('[data-action="paired"]').click();
    assert.deepEqual(view.actions.at(-1), { action: 'paired' });
  } finally { view.close(); }
});

test('unsupported phones offer a remote workspace instead of repeated installation', () => {
  const view = setup({ initial: 'local', checks: { ...ready, arm64: false } });
  try {
    view.document.querySelector('[data-action="remote_screen"]').click();
    assert.equal(view.document.querySelector('h1').textContent, 'Pair with your computer');
    assert.equal(view.document.querySelectorAll('[data-action="install"]').length, 0);
  } finally { view.close(); }
});

test('an incompatible existing agent gets an explicit recovery, not an install loop', () => {
  const view = setup({ initial: 'local', checks: { ...ready, agentVersion: false } });
  try {
    assert.match(view.document.querySelector('h1').textContent, /agent needs an update/);
    assert.equal(view.document.querySelectorAll('[data-action="install"]').length, 0);
    view.document.querySelector('[data-action="termux"]').click();
    assert.equal(view.actions.at(-1).action, 'termux');
  } finally { view.close(); }
});
