import { execFileSync } from 'node:child_process';
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


const target = await waitFor(async () => (await targets()).find(target => target.url.startsWith('http://127.0.0.1:5474/')), 'Workspace unavailable');
const workspace = await connect(target);
const originalRoute = await evaluate(workspace, 'location.pathname + location.search');
const rendererOnly = process.argv.includes('--renderer-only');
function visibleApp() {
  if (rendererOnly) return;
  const focus = execFileSync('adb', ['shell','dumpsys','window'], {encoding:'utf8'}).split('\n').find(line=>line.includes('mCurrentFocus='));
  assert.ok(focus?.includes('io.openaide.android/'), 'Leave OpenAIDE visible for device checks');
}
async function back() {
  if (rendererOnly) {
    await evaluate(workspace, `window.dispatchEvent(new Event('openaide:back', {cancelable:true}))`);
    return;
  }
  visibleApp();
  execFileSync('adb', ['shell','input','keyevent','KEYCODE_BACK']);
}
async function drawerIs(open) {
  return evaluate(workspace, `Boolean(document.querySelector('button[aria-label="${open ? 'Close' : 'Open'} task navigation"]'))`);
}
async function swipe(startX, startY, endX, endY) {
  visibleApp();
  await workspace.call('Input.dispatchTouchEvent', {type:'touchStart', touchPoints:[{x:startX,y:startY,id:1}]});
  for (let step=1; step<=8; step++) {
    await workspace.call('Input.dispatchTouchEvent', {type:'touchMove', touchPoints:[{x:startX+(endX-startX)*step/8,y:startY+(endY-startY)*step/8,id:1}]});
    await delay(20);
  }
  await workspace.call('Input.dispatchTouchEvent', {type:'touchEnd', touchPoints:[]});
}
try {
  visibleApp();
  await evaluate(workspace, `history.pushState(null, '', '/new-task'); window.dispatchEvent(new PopStateEvent('popstate'))`);
  await evaluate(workspace, `document.querySelector('[aria-label="Close task navigation"]')?.click()`);
  await waitFor(() => drawerIs(false), 'Mobile workbench unavailable');
  await evaluate(workspace, `document.querySelector('[aria-label="Open task navigation"]').click()`);
  await waitFor(() => drawerIs(true), 'Drawer did not open');
  await back();
  await waitFor(() => drawerIs(false), 'Back did not dismiss the drawer');
  assert.equal(await evaluate(workspace, 'location.pathname'), '/new-task');
  console.log('PASS: ' + (rendererOnly ? 'Renderer Back' : 'Android Back') + ' closes the drawer without navigating away.');

  await evaluate(workspace, `document.querySelector('[aria-label="Open task navigation"]').click()`);
  await waitFor(() => drawerIs(true), 'Drawer unavailable');
  await evaluate(workspace, `document.querySelector('.sidebar .settings-button').click()`);
  await waitFor(() => evaluate(workspace, `Boolean(document.querySelector('#settings-tab-connection'))`), 'Settings unavailable');
  await evaluate(workspace, `document.querySelector('#settings-tab-connection').click()`);
  await waitFor(() => evaluate(workspace, `!document.querySelector('.settings-content.mobile-index-open')`), 'Submenu did not open');
  await back();
  await waitFor(() => evaluate(workspace, `Boolean(document.querySelector('.settings-content.mobile-index-open'))`), 'Back skipped the Settings index');
  await back();
  await waitFor(() => drawerIs(false), 'Back did not return to the workspace');
  assert.equal(await evaluate(workspace, 'location.pathname'), '/new-task');
  console.log('PASS: ' + (rendererOnly ? 'Renderer Back' : 'Android Back') + ' goes from Settings section to index to workspace.');

  for (const [startX,startY] of [[6,220],[20,360],[36,450],[6,500],[36,280]]) {
    await swipe(startX,startY,210,startY+7);
    await waitFor(() => drawerIs(true), 'Edge swipe did not open navigation');
    await back();
    await waitFor(() => drawerIs(false), 'Drawer did not close after swipe');
  }
  await swipe(20,400,24,230);
  assert.equal(await drawerIs(false), true);
  console.log('PASS: Five edge swipes open the menu; vertical gestures do not.');

  await workspace.call('Emulation.setDeviceMetricsOverride', {width:1100,height:800,deviceScaleFactor:1,mobile:false});
  await waitFor(() => evaluate(workspace, `getComputedStyle(document.querySelector('.mobile-workbench-bar')).display === 'none'`), 'Wide layout did not restore desktop navigation');
  console.log('PASS: Wide layout retains persistent navigation.');
} finally {
  await evaluate(workspace, `document.querySelector('[aria-label="Close task navigation"]')?.click()`).catch(()=>{});
  await workspace.call('Emulation.clearDeviceMetricsOverride').catch(()=>{});
  await evaluate(workspace, `history.replaceState(null, '', ${JSON.stringify(originalRoute)}); window.dispatchEvent(new PopStateEvent('popstate'))`).catch(()=>{});
  workspace.close();
}
