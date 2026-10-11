const page = document.getElementById('page');
const notice = document.getElementById('notice');
const sheet = document.getElementById('sheet');
const infoButton = document.getElementById('info');
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
  qr: '<rect x="4" y="4" width="6" height="6" rx="1"/><rect x="14" y="4" width="6" height="6" rx="1"/><rect x="4" y="14" width="6" height="6" rx="1"/><path d="M14 14h2v2M20 14v.01M14 20h2M18 18v2h2"/>',
  keyboard: '<rect x="2.5" y="6" width="19" height="12" rx="2"/><path d="M6.5 10h.01M10 10h.01M13.5 10h.01M17 10h.01M7 14h10"/>',
  battery: '<rect x="3" y="7" width="16" height="10" rx="2"/><path d="M22 11v2M7 10v4"/>',
  bell: '<path d="M6 16V11a6 6 0 0 1 12 0v5l1.5 2h-15zM10 21h4"/>',
  wrench: '<path d="M14.5 6.5a4 4 0 0 0-5 5L4 17l3 3 5.5-5.5a4 4 0 0 0 5-5l-2.5 2.5-2-.5-.5-2z"/>',
  refresh: '<path d="M20 11a8 8 0 0 0-14.5-4M4 4v4h4M4 13a8 8 0 0 0 14.5 4M20 20v-4h-4"/>',
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
function row(glyph, title, action, value = '', detail = '') { return item('row', glyph, title, detail, action, value); }

// Background information lives behind the header's info button, not on the page.
const info = {
  welcome: ['Where agents work', 'Projects and conversations stay on the device where your agents work. You can switch later in Settings.'],
  settings: ['Connection', 'Work on a remote computer keeps running when your phone is locked or OpenAIDE is closed. Work on this phone runs in Termux.', 'Switching does not stop or delete work on the other device.'],
  remote: ['How pairing works', 'Each device has its own key. Pairing tells your computer to trust this phone’s key, and the connection is encrypted end to end. There is no account or password.', 'A code works once and expires after a few minutes. Remove this phone any time in Settings → Devices on your computer.'],
  join: ['How pairing works', 'This code identifies your phone. Your computer asks you to confirm before it trusts it, and the connection is encrypted end to end.', 'The code is only valid while this screen is open.'],
  local: ['Working on this phone', 'OpenAIDE checks the Termux connection, compatible tools, agent sign-in, storage and automatic startup, and prepares what is missing.', 'Keep Termux installed. Closing its terminal window is fine; force-stopping Termux interrupts local work.'],
  background: ['Background work', 'Protection keeps the phone awake only while agents are working and is released when work is idle. Running agents still use battery and data, so plug in for long tasks.', 'Do not force-stop OpenAIDE or Termux during work. Some phones also need background activity allowed in their own battery settings.'],
};
function openSheet() {
  const [title, ...paragraphs] = info[screen];
  sheet.querySelector('h2').textContent = title;
  sheet.querySelector('.sheet-body').innerHTML = paragraphs.map(text => `<p>${text}</p>`).join('');
  sheet.hidden = false;
  sheet.querySelector('.sheet-close').focus();
}
function closeSheet() {
  if (sheet.hidden) return false;
  sheet.hidden = true;
  infoButton.focus();
  return true;
}
infoButton.addEventListener('click', openSheet);
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
  if (checks.codex && !checks.codexVersion) return { index: 2, title: 'Your agent needs an update', detail: 'Install the Android-compatible Codex 0.153.3 in Termux, then check again.', action: 'termux', label: 'Open Termux' };
  if (['node', 'nodeVersion', 'npm', 'git', 'codex', 'codexVersion', 'runtime', 'frontend', 'supervisor'].some(key => !checks[key])) return { index: 2, title: 'Prepare your workspace', detail: 'OpenAIDE installs the tools it needs in Termux. Your projects and history stay in place.', action: 'install', label: 'Install & continue' };
  if (!checks.authenticated) return { index: 3, title: 'Sign in to your agent', detail: 'Paste the copied sign-in command in Termux, finish signing in, then come back.', action: 'signin', label: 'Copy sign-in & open Termux' };
  return { index: 4, title: 'Your phone is ready', detail: '', action: 'local', label: state.remote ? 'Use this phone' : 'Open workspace' };
}
function render() {
  if (screen === 'welcome') {
    page.innerHTML = heading('Where will your<br>agents work?')
      + '<div class="group">'
      + choice('computer', 'Remote computer', 'Pair with a code', 'remote')
      + choice('phone', 'This phone', 'Runs in Termux', 'local')
      + '</div>';
  } else if (screen === 'settings') {
    const remote = state.remote;
    page.innerHTML = `<h1>${remote ? escape(state.computer || 'Remote computer') : 'This phone'}</h1><p class="status">Connected</p>`
      + '<div class="group">'
      + (remote ? row('phone', 'Switch to this phone', 'local_screen')
        : state.paired ? row('computer', `Switch to ${escape(state.computer || 'your computer')}`, 'paired') : '')
      + row('qr', remote || state.paired ? 'Pair another computer' : 'Pair with a computer', 'remote_screen')
      + (remote ? '' : row('battery', 'Background work', 'background_screen', state.background ? 'On' : 'Off'))
      + row('wrench', 'Advanced', 'advanced_screen')
      + '</div>';
  } else if (screen === 'local') {
    const step = localStep();
    page.innerHTML = `<div class="step" role="img" aria-label="Step ${step.index} of 4">${[1, 2, 3, 4].map(index => `<span class="${index <= step.index ? 'done' : ''}"></span>`).join('')}</div>`
      + heading(state.busy ? 'Getting things ready' : step.title, state.busy ? '' : step.detail)
      + (!state.busy ? button(step.label, step.action) : '')
      + (!state.busy && (step.index === 1 || step.action === 'termux') ? button('Check again', 'check', 'secondary') : '')
      + (!state.busy && !state.permission && state.termux ? button('Open Android app permissions', 'app_settings', 'link') : '')
      + (step.index === 4 ? button('Keep working with the screen locked', 'background_screen', 'secondary') : '')
      + (state.remote && step.index === 4 ? '<p class="caution">Switching closes the current view. Save unsent drafts first.</p>' : '');
  } else if (screen === 'remote') {
    const known = state.paired && !state.remote;
    page.innerHTML = heading('Pair with your computer', 'On your computer, open') + path('Settings', 'Devices', 'Show code')
      + (known ? `<div class="panel"><span class="copy"><h2>${escape(state.computer || 'Your computer')}</h2><small>Already paired</small></span>${button('Use', 'paired', 'mini')}</div>` : '')
      + '<div class="actions">'
      + (firstRun ? '' : '<p class="caution">Switching closes the current view. Save unsent drafts first.</p>')
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
    page.innerHTML = heading('Show this code', 'On your computer, open') + path('Settings', 'Devices', 'Enter code')
      + (join ? `<div class="qr" role="img" aria-label="Pairing code"><svg viewBox="-2 -2 ${join.qr.size + 4} ${join.qr.size + 4}" shape-rendering="crispEdges"><path d="${escape(join.qr.path)}"/></svg></div><p class="code">${escape(join.text)}</p><p class="waiting"><span class="pulse" aria-hidden="true"></span>Waiting for your computer</p>`
        : '<p class="waiting"><span class="pulse" aria-hidden="true"></span>Preparing this phone’s code…</p>');
  } else if (screen === 'background') {
    page.innerHTML = heading('Keep work going')
      + `<div class="group"><label class="row">${icon('power')}<span class="copy"><strong>Background work</strong></span><input id="background" type="checkbox" ${state.background ? 'checked' : ''}></label>`
      + row('battery', 'Battery access', 'battery', state.appBattery && state.termuxBattery ? 'Allowed' : 'Review')
      + row('bell', 'Notifications', 'app_settings', state.notifications ? 'Allowed' : 'Review')
      + '</div>'
      + (state.batterySaver ? '<p class="caution">Battery Saver is on. Android may pause local work.</p>' : '');
    document.getElementById('background').addEventListener('change', event => send('background', { enabled: event.target.checked }));
  } else {
    page.innerHTML = heading('Advanced')
      + '<div class="group">'
      + row('phone', 'Check this phone', 'local_screen')
      + row('refresh', 'Reconnect to Termux', 'repair')
      + row('power', 'Start after reboot', state.boot ? 'boot' : 'get_boot', state.boot ? '' : 'Get Termux:Boot')
      + row('wrench', 'Termux app settings', 'termux_settings')
      + row('share', 'Share diagnostics', 'diagnostics', '', 'No conversations or credentials')
      + '</div>';
  }
  infoButton.hidden = !info[screen];
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
    else send(action);
  });
}
