import assert from 'node:assert/strict';
import { setTimeout as delay } from 'node:timers/promises';

const endpoint = 'http://127.0.0.1:9222';
async function targets() { return (await fetch(`${endpoint}/json/list`)).json(); }
async function connect(target) {
  const socket = new WebSocket(target.webSocketDebuggerUrl);
  await new Promise((resolve, reject) => {
    socket.addEventListener('open', resolve, { once: true });
    socket.addEventListener('error', reject, { once: true });
  });
  let sequence = 0;
  const pending = new Map();
  socket.addEventListener('message', ({ data }) => {
    const message = JSON.parse(data);
    pending.get(message.id)?.(message);
  });
  return {
    close: () => socket.close(),
    call(method, params = {}) {
      return new Promise((resolve, reject) => {
        const id = ++sequence;
        const timeout = setTimeout(() => { pending.delete(id); reject(new Error(`${method} timed out`)); }, 15000);
        pending.set(id, message => {
          clearTimeout(timeout);
          pending.delete(id);
          if (message.error) reject(new Error(`${method} failed`));
          else resolve(message.result);
        });
        socket.send(JSON.stringify({ id, method, params }));
      });
    },
  };
}

async function evaluate(client, expression, userGesture = false) {
  const result = await client.call('Runtime.evaluate', { expression, returnByValue: true, awaitPromise: true, userGesture });
  if (result.exceptionDetails) throw new Error('Device UI evaluation failed');
  return result.result.value;
}

async function waitFor(read, description) {
  const deadline = Date.now() + 12000;
  while (Date.now() < deadline) {
    const value = await read();
    if (value) return value;
    await delay(100);
  }
  throw new Error(description);
}

const target = await waitFor(async () => (await targets()).find(target => target.url.startsWith('http://127.0.0.1:5474/')), 'Open the local workspace first');
const workspace = await connect(target);
let originalBackground;
try {
  assert.equal(await evaluate(workspace, 'typeof window.OpenAIDESetup'), 'undefined');
  await evaluate(workspace, `window.__openaideSettingsTest = {location: location.pathname + location.search};
    history.pushState(null, '', '/settings?tab=common'); window.dispatchEvent(new PopStateEvent('popstate'))`);
  await waitFor(() => evaluate(workspace, `Boolean(document.querySelector('#settings-tab-connection'))`), 'Connection submenu is missing');
  await evaluate(workspace, `[...document.querySelectorAll('.settings-mobile-index.open button')].find(button => button.textContent.trim() === 'Connection')?.click();
    document.querySelector('#settings-tab-connection')?.click()`);
  await waitFor(() => evaluate(workspace, `Boolean(document.querySelector('input[aria-label="Continue while locked"]'))`), 'Native state did not reach the shared Settings submenu');
  assert.equal(await evaluate(workspace, 'location.pathname + location.search'), '/settings?tab=connection');
  assert.equal((await targets()).some(target => target.url === 'https://app.openaide.invalid/index.html'), false);
  originalBackground = await evaluate(workspace, `document.querySelector('input[aria-label="Continue while locked"]').checked`);
  await evaluate(workspace, `document.querySelector('input[aria-label="Continue while locked"]').click()`);
  await waitFor(() => evaluate(workspace, `document.querySelector('input[aria-label="Continue while locked"]').checked === ${!originalBackground}`), 'Background preference was not acknowledged by Android');
  await evaluate(workspace, `document.querySelector('input[aria-label="Continue while locked"]').click()`);
  await waitFor(() => evaluate(workspace, `document.querySelector('input[aria-label="Continue while locked"]').checked === ${originalBackground}`), 'Background preference was not restored');
  assert.equal(await evaluate(workspace, 'Boolean(window.__openaideSettingsTest)'), true);
  assert.equal(await evaluate(workspace, 'typeof window.OpenAIDESetup'), 'undefined');
  console.log('PASS: Connection is a shared Settings submenu; native preference changes stay in the same workspace document.');
} finally {
  await evaluate(workspace, `if (window.__openaideSettingsTest) {
    const toggle = document.querySelector('input[aria-label="Continue while locked"]');
    if (toggle && toggle.checked !== ${originalBackground ?? true}) toggle.click();
    history.replaceState(null, '', window.__openaideSettingsTest.location);
    delete window.__openaideSettingsTest;
    window.dispatchEvent(new PopStateEvent('popstate'));
  }`).catch(() => {});
  workspace.close();
}
