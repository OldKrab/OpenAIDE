const page = document.getElementById('page');
const notice = document.getElementById('notice');
let state = {};
let screen;
let history = [];
let codeDraft = '';
const icons = {
  phone: '<rect x="6" y="2.5" width="12" height="19" rx="3"/><path d="M10.5 18.5h3"/>',
  computer: '<rect x="3" y="4" width="18" height="12" rx="2"/><path d="M8 20h8M12 16v4"/>',
  scan: '<path d="M4 8V6a2 2 0 0 1 2-2h2M16 4h2a2 2 0 0 1 2 2v2M20 16v2a2 2 0 0 1-2 2h-2M8 20H6a2 2 0 0 1-2-2v-2M4 12h16"/>',
  qr: '<rect x="4" y="4" width="6" height="6" rx="1"/><rect x="14" y="4" width="6" height="6" rx="1"/><rect x="4" y="14" width="6" height="6" rx="1"/><path d="M14 14h2v2M20 14v.01M14 20h2M18 18v2h2"/>',
  plus: '<path d="M12 5v14M5 12h14"/>',
  battery: '<rect x="3" y="7" width="16" height="10" rx="2"/><path d="M22 11v2M7 10v4"/>',
  wrench: '<path d="M14.5 6.5a4 4 0 0 0-5 5L4 17l3 3 5.5-5.5a4 4 0 0 0 5-5l-2.5 2.5-2-.5-.5-2z"/>',
  back: '<path d="M15 5l-7 7 7 7"/>',
  chevron: '<path d="M9 5l7 7-7 7"/>',
};
function icon(name, extra = '') { return `<svg class="i ${extra}" viewBox="0 0 24 24" aria-hidden="true">${icons[name]}</svg>`; }
function send(action, fields = {}) { window.OpenAIDESetup.send(JSON.stringify({ action, ...fields })); }
function button(label, action, style = 'primary', glyph = '') { return `<button class="${style}" data-action="${action}">${glyph ? icon(glyph) : ''}${label}</button>`; }
function heading(kicker, title, description) { return `${kicker ? `<p class="eyebrow">${kicker}</p>` : ''}<h1>${title}</h1><p class="intro">${description}</p>`; }
function item(kind, glyph, title, detail, target, value) {
  const attribute = kind === 'choice' ? `data-screen="${target}"` : `data-action="${target}"`;
  const trailing = value ? `<span class="value">${value}</span>` : icon('chevron', 'chev');
  return `<button class="${kind}" ${attribute}><span class="tile">${icon(glyph)}</span><span class="copy"><strong>${title}</strong><small>${detail}</small></span>${trailing}</button>`;
}
function choice(glyph, title, detail, destination) { return item('choice', glyph, title, detail, destination); }
function row(glyph, title, detail, action, value = '') { return item('row', glyph, title, detail, action, value); }
function escape(text) { return String(text).replace(/[&<>"']/g, character => `&#${character.charCodeAt(0)};`); }
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
  if (first) screen = next.initial || 'settings';
  render();
  if (first) enter(screen);
};
function rememberCode() {
  const field = document.getElementById('code');
  if (field) codeDraft = field.value;
}
function localStep() {
  if (!state.termux) return { index: 1, title: 'Install Termux', detail: 'Termux provides a private place for your tools and projects. Install it from its official releases, then open it once and return here.', action: 'get_termux', label: 'Get Termux' };
  if (!state.permission) return { index: 1, title: 'Connect to Termux', detail: 'Allow OpenAIDE to start your tools and use files in Termux. Android will ask for permission. You only do this once.', action: 'grant', label: 'Allow connection' };
  if (!state.checks) return { index: 1, title: 'Allow the connection in Termux', detail: 'Termux also needs a one-time setting. Copy the setup command, paste it in Termux and press Enter. Then come back; we’ll check automatically.', action: 'access', label: 'Copy setup & open Termux' };
  const checks = state.checks;
  if (!checks.arm64) return { index: 2, title: 'Use a remote computer', detail: 'Local work requires an ARM64 Android phone. You can still use a remote OpenAIDE workspace on this device.', action: 'remote_screen', label: 'Connect to a computer' };
  if (!checks.storage || !checks.space) return { index: 2, title: 'Your phone needs more room', detail: 'Free at least 512 MB in Termux’s storage, then check again. Your existing projects will not be removed.', action: 'check', label: 'Check again' };
  if (checks.codex && !checks.codexVersion) return { index: 2, title: 'Your agent needs an update', detail: 'The installed Codex is not compatible with this workspace. Use the Android-compatible Codex 0.153.3 in Termux, then check again. We won’t replace your existing agent automatically.', action: 'termux', label: 'Open Termux' };
  if (['node', 'nodeVersion', 'npm', 'git', 'codex', 'codexVersion', 'runtime', 'frontend', 'supervisor'].some(key => !checks[key])) return { index: 2, title: 'Prepare your workspace', detail: 'We’ll install the tools OpenAIDE needs in Termux. Your projects and history stay in place. This download can use mobile data.', action: 'install', label: 'Install & continue' };
  if (!checks.authenticated) return { index: 3, title: 'Sign in to your agent', detail: 'Copy the sign-in command, paste it in Termux and follow the sign-in steps. Return here when you’re done; we’ll check automatically.', action: 'signin', label: 'Copy sign-in & open Termux' };
  return { index: 4, title: 'Your phone is ready', detail: 'OpenAIDE starts your workspace automatically. You won’t need to open Termux each time.', action: 'local', label: state.remote ? 'Use this phone' : 'Open workspace' };
}
function render() {
  if (screen === 'welcome') {
    page.innerHTML = heading('', 'Where will your<br>agents work?', 'Choose once. You can change it later in Settings.')
      + '<div class="group">'
      + choice('computer', 'Remote computer', 'Pair with OpenAIDE on your computer. No Termux needed.', 'remote')
      + choice('phone', 'This phone', 'Work locally with Termux.', 'local')
      + '</div>'
      + '<p class="note">Projects and conversations stay on the device where your agents work.</p>';
  } else if (screen === 'settings') {
    const remote = state.remote;
    const name = remote ? escape(state.computer || 'Remote computer') : 'This phone';
    page.innerHTML = heading('Settings', 'Connection', 'Where your agents work. Your open conversation stays put while you’re here.')
      + `<div class="current"><span class="tile">${icon(remote ? 'computer' : 'phone')}</span><span class="meta"><span class="label">Connected</span><h2>${name}</h2><small>${remote ? 'Work runs on your computer, even when your phone is locked.' : 'Tools and projects run in Termux on this phone.'}</small></span></div>`
      + '<p class="section">Switch</p><div class="group">'
      + (remote ? row('phone', 'Use this phone', 'Run new work in Termux.', 'local_screen') : row('computer', state.paired ? 'Use ' + escape(state.computer || 'your computer') : 'Use a remote computer', state.paired ? 'Paired and trusted.' : 'Pair with OpenAIDE on a computer.', state.paired ? 'paired' : 'remote_screen'))
      + row('qr', state.remote || state.paired ? 'Pair another computer' : 'Pair with a code', 'Scan or paste a code from a computer.', 'remote_screen')
      + '</div>'
      + '<p class="section">More</p><div class="group">'
      + (remote ? '' : row('battery', 'Background work', 'Keep work going while the screen is locked.', 'background_screen'))
      + row('wrench', 'Advanced', 'Recovery tools and diagnostics.', 'advanced_screen')
      + '</div>';
  } else if (screen === 'local') {
    const step = localStep();
    page.innerHTML = heading(`This phone · Step ${step.index} of 4`, state.busy ? 'Getting things ready' : step.title, state.busy ? 'We’re checking and preparing what’s needed. Your existing work stays safe.' : step.detail)
      + `<div class="step" aria-hidden="true">${[1, 2, 3, 4].map(index => `<span class="${index <= step.index ? 'done' : ''}"></span>`).join('')}</div>`
      + (!state.busy ? button(step.label, step.action) : '')
      + (!state.busy && (step.index === 1 || step.action === 'termux') ? button('I’ve done this · Check again', 'check', 'secondary') : '')
      + (!state.busy && !state.permission && state.termux ? button('Open Android app permissions', 'app_settings', 'link') : '')
      + (step.index === 4 ? '<p class="note">Keep Termux installed. Closing its terminal window is fine; force-stopping Termux interrupts local work.</p>' + button('Keep working with the screen locked', 'background_screen', 'secondary') : '')
      + (state.remote && step.index === 4 ? '<p class="note">Switching closes the current view. Save unsent drafts first. Existing tasks stay on their original computer.</p>' : '')
      + '<details><summary>What does OpenAIDE check?</summary><p>Connection permission, compatible tools, agent sign-in, storage and automatic startup. These checks run for you.</p></details>';
  } else if (screen === 'remote') {
    const known = state.paired && !state.remote;
    page.innerHTML = heading('', 'Pair with your computer', 'On your computer, open OpenAIDE, then Settings → Devices → Show code.')
      + (known ? `<div class="panel"><p class="tag">ALREADY PAIRED</p><h2>${escape(state.computer || 'Your computer')}</h2><p>This phone is still trusted there.</p>${button('Use this computer', 'paired')}</div>` : '')
      + button('Scan code', 'scan', known ? 'secondary' : 'primary', 'scan')
      + '<p class="divider">or paste it</p>'
      + '<form id="pair-form"><label for="code">Pairing code</label><textarea id="code" name="code" rows="3" placeholder="OAI1…" autocapitalize="characters" autocomplete="off" spellcheck="false" required></textarea>'
      + '<button class="secondary" type="submit">Pair with code</button></form>'
      + button('Show this phone’s code instead', 'join_screen', 'link')
      + '<p class="note">No account or password. The code works once and expires in a few minutes. Save any unsent draft before switching.</p>'
      + '<details><summary>How is this secure?</summary><p>Each device has its own key. Pairing tells your computer to trust this phone’s key; the connection is encrypted end to end. Remove this phone any time in Settings → Devices on your computer.</p></details>';
    document.getElementById('code').value = codeDraft;
    document.getElementById('pair-form').addEventListener('submit', event => { event.preventDefault(); rememberCode(); send('pair', { code: codeDraft }); });
  } else if (screen === 'join') {
    const join = state.join;
    page.innerHTML = heading('', 'Show this code', 'On your computer, open OpenAIDE, then Settings → Devices → Enter code, and scan or type it.')
      + (join ? `<div class="qr" role="img" aria-label="Pairing code"><svg viewBox="-2 -2 ${join.qr.size + 4} ${join.qr.size + 4}" shape-rendering="crispEdges"><path d="${escape(join.qr.path)}"/></svg></div><p class="code">${escape(join.text)}</p><p class="waiting"><span class="spinner" aria-hidden="true" style="margin:0"></span>Waiting for your computer. Keep this screen open.</p>`
        : '<p class="waiting">Preparing this phone’s code…</p>');
  } else if (screen === 'background') {
    page.innerHTML = heading('This phone', 'Keep work going', 'Protect active work when you leave OpenAIDE or lock your phone. Protection is released when work is idle.')
      + `<div class="group"><label class="row"><span class="tile">${icon('battery')}</span><span class="copy"><strong>Background work</strong><small>Only keeps the phone awake while needed.</small></span><input id="background" type="checkbox" ${state.background ? 'checked' : ''}></label>`
      + row('battery', 'Battery access', 'Allow unrestricted use for OpenAIDE and Termux.', 'battery', state.appBattery && state.termuxBattery ? 'Allowed' : 'Review')
      + row('qr', 'Notifications', 'Know when local work finishes or needs you.', 'app_settings', state.notifications ? 'Allowed' : 'Review')
      + '</div>'
      + (state.batterySaver ? '<p class="note">Battery Saver is on. Android may delay or interrupt local work.</p>' : '')
      + '<p class="note">Running agents still use battery and data, so plug in for long tasks. Don’t force-stop OpenAIDE or Termux during work; some phones also need background activity allowed in their own battery settings.</p>';
    document.getElementById('background').addEventListener('change', event => send('background', { enabled: event.target.checked }));
  } else {
    page.innerHTML = heading('Connection', 'Advanced', 'Recovery tools for when something isn’t working. You don’t need these for everyday use.')
      + '<div class="group">'
      + row('phone', 'Check this phone', 'Find and fix missing local setup.', 'local_screen')
      + row('wrench', 'Reconnect to Termux', 'Restore the saved local connection. No history is deleted.', 'repair')
      + row('phone', 'Start after reboot', 'Optional. Requires the Termux:Boot companion.', 'boot')
      + row('phone', 'Termux app settings', 'Review Android permissions and battery restrictions.', 'termux_settings')
      + row('qr', 'Share diagnostics', 'Connection events only. No conversations or credentials.', 'diagnostics')
      + '</div>'
      + (!state.boot ? button('Get Termux:Boot', 'get_boot', 'link') : '')
      + '<p class="note">OpenAIDE for Android</p>';
  }
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
