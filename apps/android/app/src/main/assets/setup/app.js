const page = document.getElementById('page');
const notice = document.getElementById('notice');
const sheet = document.getElementById('sheet');
let state = {};
let screen;
let history = [];
let codeDraft = '';
let pasteOpen = false;
// Switching warnings only matter once a workspace is in use, not during first setup.
let firstRun = false;
const icons = {
  phone: '<rect x="6" y="2.5" width="12" height="19" rx="3"/><path d="M10.5 18.5h3"/>',
  computer: '<rect x="3" y="4" width="18" height="12" rx="2"/><path d="M8 20h8M12 16v4"/>',
  server: '<rect x="3" y="4" width="18" height="7" rx="2"/><rect x="3" y="13" width="18" height="7" rx="2"/><path d="M7 7.5h.01M7 16.5h.01"/>',
  qr: '<rect x="4" y="4" width="6" height="6" rx="1"/><rect x="14" y="4" width="6" height="6" rx="1"/><rect x="4" y="14" width="6" height="6" rx="1"/><path d="M14 14h2v2M20 14v.01M14 20h2M18 18v2h2"/>',
  keyboard: '<rect x="2.5" y="6" width="19" height="12" rx="2"/><path d="M6.5 10h.01M10 10h.01M13.5 10h.01M17 10h.01M7 14h10"/>',
  battery: '<rect x="3" y="7" width="16" height="10" rx="2"/><path d="M22 11v2M7 10v4"/>',
  bell: '<path d="M6 16V11a6 6 0 0 1 12 0v5l1.5 2h-15zM10 21h4"/>',
  wrench: '<path d="M14.5 6.5a4 4 0 0 0-5 5L4 17l3 3 5.5-5.5a4 4 0 0 0 5-5l-2.5 2.5-2-.5-.5-2z"/>',
  power: '<path d="M12 3v8M7 6.5a7 7 0 1 0 10 0"/>',
  share: '<path d="M12 15V4M8 8l4-4 4 4M5 13v5a2 2 0 0 0 2 2h10a2 2 0 0 0 2-2v-5"/>',
  chevron: '<path d="M9 5l7 7-7 7"/>',
};
function icon(name, extra = '') { return `<svg class="i ${extra}" viewBox="0 0 24 24" aria-hidden="true">${icons[name]}</svg>`; }
function send(action, fields = {}) { window.OpenAIDESetup.send(JSON.stringify({ action, ...fields })); }
function escape(text) { return String(text).replace(/[&<>"']/g, character => `&#${character.charCodeAt(0)};`); }
function button(label, action, style = 'primary') { return `<button class="${style}" data-action="${action}">${label}</button>`; }
function heading(title, description = '') { return `<h1>${title}</h1>${description ? `<p class="intro">${description}</p>` : ''}`; }
// Where to click on the computer, shown as a path instead of a sentence.
function path(...steps) {
  return `<p class="path" aria-label="On your computer: ${steps.join(', then ')}">${steps.map(step => `<span>${step}</span>`).join(icon('chevron', 'sep'))}</p>`;
}
function item(kind, glyph, title, detail, target, value) {
  const attribute = kind === 'choice' ? `data-screen="${target}"` : `data-action="${target}"`;
  const trailing = (value ? `<span class="value">${value}</span>` : '') + icon('chevron', 'chev');
  return `<button class="${kind}" ${attribute}>${icon(glyph)}<span class="copy"><strong>${title}</strong>${detail ? `<small>${detail}</small>` : ''}</span>${trailing}</button>`;
}
function choice(glyph, title, detail, destination) { return item('choice', glyph, title, detail, destination); }
// A choice that starts a native flow, such as adding an SSH computer, instead of opening a screen.
function choiceAction(glyph, title, detail, action) { return item('row choice', glyph, title, detail, action); }
// A test build may leave out the App Server for SSH computers; the option stays and says so.
function sshDetail() { return state.sshAvailable ? 'A Linux computer you sign in to' : 'Not included in this build'; }
function row(glyph, title, action, value = '', detail = '') { return item('row', glyph, title, detail, action, value); }
// The place in use: named like the others, but not something to tap.
function place(glyph, title, detail) { return `<div class="row current">${icon(glyph)}<span class="copy"><strong>${title}</strong><small>${detail}</small></span><span class="value">In use</span></div>`; }
function computerRow(computer) {
  return `<button class="row" data-action="use_computer" data-id="${escape(computer.id)}">${icon('computer')}<span class="copy"><strong>${escape(computer.name || 'Remote computer')}</strong><small>Remote computer</small></span>${icon('chevron', 'chev')}</button>`;
}
function sshRow(computer) {
  if (computer.current) return place('server', escape(computer.name), `SSH · ${escape(computer.destination)}`);
  return `<button class="row" data-action="use_ssh" data-id="${escape(computer.id)}">${icon('server')}<span class="copy"><strong>${escape(computer.name)}</strong><small>SSH · ${escape(computer.destination)}</small></span>${icon('chevron', 'chev')}</button>`;
}

// Background information is prose behind a question placed where it comes up, not text on the page.
// Shared by every screen that explains a remote connection; names the public service it may use (ADR 0062).
const network = 'The connection uses iroh over QUIC. It goes directly between the devices when their networks allow it (NAT hole punching). Otherwise it goes through public relays run by the iroh project, which see network addresses and public keys, never content.';
const info = {
  welcome: ['What’s the difference?', 'Where agents work', 'On a remote computer, work keeps running when your phone is locked or OpenAIDE is closed. On this phone, agents run locally in Termux.', 'Projects and conversations stay on the device where the agents work. You can switch later in Settings.'],
  remote: ['How pairing works', 'How pairing works', 'Each device has its own key. Pairing tells your computer to trust this phone’s key, and the connection is encrypted end to end. There is no account or password.', 'A code works once and expires after a few minutes. You can remove this phone any time in Settings → Remote on your computer.', network],
  join: ['How pairing works', 'How pairing works', 'This code identifies your phone. Your computer asks you to confirm before it trusts it, and the connection is encrypted end to end.', 'The code is only valid while this screen is open.', network],
  settings: ['How the connection works', 'How the connection works', 'Your phone and computer each have their own key, and the connection is encrypted end to end with those keys. There is no account or password.', network],
  local: ['What gets checked?', 'Working on this phone', 'OpenAIDE checks the Termux connection, the tools it needs, a signed-in agent (Codex or Claude) and storage, and prepares what is missing.', 'Keep Termux installed. Closing its terminal window is fine; force-stopping Termux interrupts local work.'],
  background: ['How does this affect battery?', 'Background work', 'Protection keeps the phone awake only while agents are working and is released when work is idle. Running agents still use battery and data, so plug in for long tasks.', 'Do not force-stop OpenAIDE or Termux during work. Some phones also need background activity allowed in their own battery settings.'],
  notifications: ['How does this affect battery?', 'Task notifications', 'While OpenAIDE is hidden, it keeps one quiet connection to your computer, which answers when a task finishes or needs you. The phone is not kept awake.', 'Notifications stop if OpenAIDE is force-stopped. Some phones also need background activity allowed in their own battery settings.'],
};
// The background screen explains what it controls for the workspace in use.
function infoKey() { return screen === 'background' && state.remote ? 'notifications' : screen; }
function hint() { return info[infoKey()] ? `<button class="hint" id="about" type="button">${info[infoKey()][0]}</button>` : ''; }
function openSheet() {
  const [, title, ...paragraphs] = info[infoKey()];
  sheet.querySelector('h2').textContent = title;
  sheet.querySelector('.sheet-body').innerHTML = paragraphs.map(text => `<p>${text}</p>`).join('');
  sheet.hidden = false;
  sheet.querySelector('.sheet-close').focus();
}
function closeSheet() {
  if (sheet.hidden) return false;
  sheet.hidden = true;
  document.getElementById('about')?.focus();
  return true;
}
sheet.addEventListener('click', event => { if (event.target === sheet || event.target.closest('.sheet-close')) closeSheet(); });
document.addEventListener('keydown', event => { if (event.key === 'Escape') closeSheet(); });

// This phone's code is disclosed only while its screen is open.
function enter(next) {
  if (next === 'local') send('check');
  if (next === 'join') send('join');
}
function leave(previous) { if (previous === 'join') send('join_stop'); }
function go(next) {
  rememberCode();
  leave(screen);
  history.push(screen);
  screen = next;
  render();
  page.focus();
  window.scrollTo(0, 0);
  enter(next);
}
window.back = () => {
  if (closeSheet()) return;
  if (state.busy) return;
  rememberCode();
  leave(screen);
  if (!history.length) { send('close'); return; }
  screen = history.pop();
  render();
  page.focus();
  enter(screen);
};
document.getElementById('back').addEventListener('click', window.back);
window.receive = (next) => {
  rememberCode();
  const first = !screen;
  state = next;
  if (first) { screen = next.initial || 'settings'; firstRun = screen === 'welcome'; }
  render();
  if (first) enter(screen);
};
function rememberCode() {
  const field = document.getElementById('code');
  if (field) codeDraft = field.value;
}
function localStep() {
  if (!state.termux) return { index: 1, title: 'Install Termux', detail: 'Termux gives your tools and projects a private place on this phone. Install it, open it once, then return here.', action: 'get_termux', label: 'Get Termux' };
  if (!state.permission) return { index: 1, title: 'Connect to Termux', detail: 'Android will ask once to let OpenAIDE start your tools in Termux.', action: 'grant', label: 'Allow connection' };
  if (!state.checks) return { index: 1, title: 'Allow the connection in Termux', detail: 'Paste the copied setup command in Termux and press Enter, then come back.', action: 'access', label: 'Copy setup & open Termux' };
  const checks = state.checks;
  if (!checks.arm64) return { index: 2, title: 'Use a remote computer', detail: 'Local work needs an ARM64 phone. A remote computer works on this device.', action: 'remote_screen', label: 'Connect to a computer' };
  if (!checks.storage || !checks.space) return { index: 2, title: 'Your phone needs more room', detail: 'Free at least 512 MB in Termux’s storage. Your projects are not removed.', action: 'check', label: 'Check again' };
  if (checks.agent && !checks.agentVersion) return { index: 2, title: 'Your agent needs an update', detail: 'Install the Android-compatible Codex 0.153.3 in Termux, then check again.', action: 'termux', label: 'Open Termux' };
  // A test build may leave out the App Server for this phone; installing would fail later.
  if (!checks.runtime && !state.phoneAvailable) return { index: 2, title: 'This build can’t set up this phone', detail: 'This build of OpenAIDE does not include the App Server for this phone. Install a release build, or connect to a computer instead.', action: 'remote_screen', label: 'Connect to a computer' };
  if (['node', 'nodeVersion', 'npm', 'git', 'agent', 'agentVersion', 'runtime'].some(key => !checks[key])) return { index: 2, title: 'Prepare your workspace', detail: 'OpenAIDE installs the tools it needs in Termux. Your projects and history stay in place.', action: 'install', label: 'Install & continue' };
  if (!checks.authenticated) return { index: 3, title: 'Sign in to your agent', detail: 'Paste the copied sign-in command in Termux, finish signing in, then come back.', action: 'signin', label: 'Copy sign-in & open Termux' };
  return { index: 4, title: 'Your phone is ready', detail: '', action: 'local', label: state.remote ? 'Use this phone' : 'Open workspace' };
}
function render() {
  if (screen === 'welcome') {
    page.innerHTML = heading('Where will your<br>agents work?')
      + '<div class="group">'
      + choice('computer', 'Remote computer', 'Pair with a code', 'remote')
      + choiceAction('server', 'SSH computer', sshDetail(), 'ssh_add')
      + choice('phone', 'This phone', 'Runs in Termux', 'local')
      + '</div>' + hint();
  } else if (screen === 'settings') {
    const remote = state.remote;
    const computers = state.computers || [];
    const ssh = state.sshComputers || [];
    // One list of places with the one in use marked; adding a computer is a separate act below it.
    page.innerHTML = `<h1>${remote ? escape(state.computer || 'Remote computer') : 'This phone'}</h1>${state.connected ? '<p class="status">Connected</p>' : '<p class="status off">Not connected</p>'}`
      + '<h2 class="label">Where agents work</h2><div class="group">'
      + (remote ? row('phone', 'This phone', 'local_screen', '', 'Runs in Termux') : place('phone', 'This phone', 'Runs in Termux'))
      + computers.map(computer => computer.current ? place('computer', escape(computer.name || 'Remote computer'), 'Remote computer')
        : computerRow(computer)).join('')
      + ssh.map(sshRow).join('')
      + '</div><div class="group">'
      + row('qr', computers.length ? 'Add another computer' : 'Add a computer', 'remote_screen', '', 'Pair with a code')
      + row('server', 'Add an SSH computer', 'ssh_add', '', sshDetail())
      + '</div><div class="group">'
      // A paired computer's Tasks only notify here; work on this phone or an SSH Computer
      // is also held by this phone in the background.
      + (remote && !ssh.some(computer => computer.current)
        ? row('bell', 'Task notifications', 'background_screen', state.background ? 'On' : 'Off')
        : row('battery', 'Background work', 'background_screen', state.background ? 'On' : 'Off'))
      + row('wrench', 'Advanced', 'advanced_screen')
      + '</div>' + (computers.length ? hint() : '');
  } else if (screen === 'local') {
    const step = localStep();
    page.innerHTML = `<div class="step" role="img" aria-label="Step ${step.index} of 4">${[1, 2, 3, 4].map(index => `<span class="${index <= step.index ? 'done' : ''}"></span>`).join('')}</div>`
      + heading(state.busy ? 'Getting things ready' : step.title, state.busy ? '' : step.detail)
      + (!state.busy ? button(step.label, step.action) : '')
      + (!state.busy && (step.index === 1 || step.action === 'termux') ? button('Check again', 'check', 'secondary') : '')
      + (!state.busy && !state.permission && state.termux ? button('Open Android app permissions', 'app_settings', 'link') : '')
      + (step.index === 4 ? button('Keep working with the screen locked', 'background_screen', 'secondary') : '')
      + (state.remote && step.index === 4 ? '<p class="caution">Switching closes this view. Unsent drafts are lost.</p>' : '')
      + hint();
  } else if (screen === 'remote') {
    // Computers already paired can be used again without a new code.
    const known = (state.computers || []).filter(computer => !computer.current);
    page.innerHTML = heading('Pair with your computer', 'On your computer, open') + path('Settings', 'Remote', 'Show code') + hint()
      + known.map(computer => `<div class="panel"><span class="copy"><h2>${escape(computer.name || 'Your computer')}</h2><small>Already paired</small></span><button class="mini" data-action="use_computer" data-id="${escape(computer.id)}">Use</button></div>`).join('')
      + '<div class="actions">'
      + (firstRun ? '' : '<p class="caution">Switching closes this view. Unsent drafts are lost.</p>')
      + `<form id="pair-form" ${pasteOpen ? '' : 'hidden'}><textarea id="code" name="code" rows="3" aria-label="Pairing code" placeholder="OAI1…" autocapitalize="characters" autocomplete="off" spellcheck="false" required></textarea><button class="secondary" type="submit">Pair</button></form>`
      + button('Scan code', 'scan')
      + `<div class="alternatives"><button class="link" id="paste-toggle" aria-expanded="${pasteOpen}" aria-controls="pair-form">Enter code</button><button class="link" data-action="join_screen">Show my code</button></div>`
      + '</div>';
    document.getElementById('code').value = codeDraft;
    document.getElementById('pair-form').addEventListener('submit', event => { event.preventDefault(); rememberCode(); send('pair', { code: codeDraft }); });
    document.getElementById('paste-toggle').addEventListener('click', () => {
      rememberCode();
      pasteOpen = !pasteOpen;
      render();
      if (pasteOpen) document.getElementById('code').focus();
    });
  } else if (screen === 'join') {
    const join = state.join;
    page.innerHTML = heading('Show this code', 'On your computer, open') + path('Settings', 'Remote', 'Enter code')
      + (join ? `<div class="qr" role="img" aria-label="Pairing code"><svg viewBox="-2 -2 ${join.qr.size + 4} ${join.qr.size + 4}" shape-rendering="crispEdges"><path d="${escape(join.qr.path)}"/></svg></div><p class="code">${escape(join.text)}</p><p class="waiting"><span class="pulse" aria-hidden="true"></span>Waiting for your computer</p>${hint()}`
        : '<p class="waiting"><span class="pulse" aria-hidden="true"></span>Preparing this phone’s code…</p>');
  } else if (screen === 'background') {
    // One setting: it keeps local work running, or a paired computer's Task notifications arriving.
    const remote = state.remote;
    page.innerHTML = heading(remote ? 'Stay notified' : 'Keep work going')
      + `<div class="group"><label class="row">${icon('power')}<span class="copy"><strong>${remote ? 'Task notifications' : 'Background work'}</strong></span><input id="background" type="checkbox" ${state.background ? 'checked' : ''}></label>`
      + row('battery', 'Battery access', 'battery', state.appBattery && (remote || state.termuxBattery) ? 'Allowed' : 'Review')
      + row('bell', 'Notifications', 'app_settings', state.notifications ? 'Allowed' : 'Review')
      + '</div>'
      + (state.batterySaver ? `<p class="caution">Battery Saver is on. Android may pause ${remote ? 'notifications' : 'local work'}.</p>` : '')
      + hint();
    document.getElementById('background').addEventListener('change', event => send('background', { enabled: event.target.checked }));
  } else {
    page.innerHTML = heading('Advanced')
      + '<div class="group">'
      + row('phone', 'Check this phone', 'local_screen')
      + row('wrench', 'Termux app settings', 'termux_settings')
      + row('share', 'Share diagnostics', 'diagnostics', '', 'No conversations or credentials')
      + '</div>';
  }
  document.getElementById('about')?.addEventListener('click', openSheet);
  page.setAttribute('aria-busy', String(Boolean(state.busy)));
  notice.hidden = !state.notice;
  notice.textContent = state.notice || '';
  if (state.busy) { const spinner = document.createElement('span'); spinner.className = 'spinner'; spinner.setAttribute('aria-hidden', 'true'); notice.prepend(spinner); }
  for (const control of page.querySelectorAll('button, input, textarea')) control.disabled = Boolean(state.busy);
  document.getElementById('back').disabled = Boolean(state.busy);
  for (const control of page.querySelectorAll('[data-screen]')) control.addEventListener('click', () => go(control.dataset.screen));
  for (const control of page.querySelectorAll('[data-action]')) control.addEventListener('click', () => {
    const action = control.dataset.action;
    if (action.endsWith('_screen')) go(action.slice(0, -7));
    else send(action, control.dataset.id ? { id: control.dataset.id } : {});
  });
}
