# Desktop Application Stack

## Decision

Use **Tauri 2 with intentional native desktop integration** for the first OpenAIDE desktop shell on macOS and Windows, subject to a focused validation spike. Keep the existing React Task, Chat, Composer, navigation, and settings surfaces shared.

The clarified requirement is not to ship a browser-shaped app with only a desktop wrapper. Meet it through native windows, application menus and commands, file dialogs, notifications, deep links, credential storage, process supervision, updates, and OS conventions. Do not create parallel AppKit and WinUI versions of shared product components merely to earn a “native” label.

Tauri fits the existing Rust App Server, shared web Frontend, and narrow App Shell capability model while avoiding a second bundled browser and Node.js runtime. A custom AppKit/Swift plus WinUI 3/C# host would use the same WKWebView and WebView2 engines as Tauri for the shared work surface, but would add two independent shell implementations. It is warranted only if OpenAIDE later commits to substantial visible platform-native UI outside the shared Frontend. Electron remains the fallback if Tauri cannot meet rendering, accessibility, process, or secure-storage requirements on both operating systems.

This recommendation is deliberately falsifiable. Electron has lower migration risk for the existing TypeScript host adapters, a consistent bundled Chromium engine, native Keychain/DPAPI integration through `safeStorage`, and built-in Crashpad reporting. Those advantages should win if the spike exposes material Tauri platform gaps.

## Clarified meaning of native

There are two different goals:

1. **Desktop-native behavior:** native window lifecycle, menus, commands, shortcuts, dialogs, notifications, drag-in from Finder/Explorer, deep links, secure storage, installation, updates, and accessibility integration around a shared work surface.
2. **Native-rendered product UI:** AppKit/WinUI implementations of navigation, Task, Chat, Composer, settings, and other visible surfaces.

The first goal is compatible with component reuse and is the recommendation. The second conflicts with “do not reimplement shared components.” A native host around a WKWebView/WebView2 does not turn the React DOM inside that control into AppKit or WinUI controls. It can feel like a well-integrated desktop app, but its core work surface remains web-rendered.

## Repository constraints

- `docs/adr/0022-backend-frontend-app-shell-architecture.md` defines Desktop as a thin App Shell. The App Server owns product state, workflow, persistence, Agent lifecycle, and decisions; the shared Frontend owns rendering and ephemeral presentation state.
- `docs/adr/0026-resumable-http-rpc-session.md` says initialized App Shell clients collectively own physical App Server lifetime. A window or launcher must not treat its child-process handle as sole ownership.
- `docs/adr/0012-acp-filesystem-through-host-bridge.md` and `docs/adr/0013-acp-terminals-through-host-bridge.md` require shell-owned filesystem and terminal bridges with explicit capability boundaries.
- `docs/adr/0003-acp-auth-secret-storage.md` requires shell-secure storage and prohibits secrets in App Server state, frontend state, logs, Chat, and support exports.
- `docs/adr/0027-task-attention-and-shell-local-notifications.md` leaves OS notification delivery and routing to the shell while keeping Task Attention decisions in the App Server.
- The current VS Code terminal host uses ordinary piped child processes, not a PTY. Its contract is create, output, wait, kill, and release.
- Existing App Server launch code already implements authenticated handoff, heartbeat recovery, generation replacement, graceful `runtime.shutdown`, and forced termination. Desktop should reuse those semantics rather than define a shell-specific lifecycle.

## Comparison

| Concern | Tauri 2 | Electron | Decision impact |
|---|---|---|---|
| App Server launch and lifecycle | Packages target-specific external binaries and exposes spawn, stdin/stdout events, kill, and command scopes. Target-triple naming and per-platform sidecar assembly add build work. [Sidecars](https://v2.tauri.app/develop/sidecar/), [Shell](https://v2.tauri.app/plugin/shell/) | The main process can use Node `child_process`, closely matching the current TypeScript launcher. `utilityProcess` is for Node modules rather than the existing Rust executable. [Process model](https://www.electronjs.org/docs/latest/tutorial/process-model), [utilityProcess](https://www.electronjs.org/docs/latest/api/utility-process) | Both are feasible. Electron is the shorter port; Tauri better aligns long-term runtime ownership with Rust. Preserve App Server client-owned lifetime in either implementation. |
| ACP terminal and PTY | Shell commands provide pipes, not a true PTY. A PTY requires an additional Rust PTY/ConPTY component. | Node child processes also provide pipes. A PTY requires a native dependency such as `node-pty`, with ABI and packaging work. | Current ACP host behavior needs no PTY. Add one only if an Agent validation test proves TTY detection, resize, ANSI mode, or interactive input is required. Visible terminal authentication is a separate OS-terminal capability. |
| Files and dialogs | Official native file/folder dialogs; commands and filesystem access can be capability-scoped. [Dialog](https://v2.tauri.app/plugin/dialog/), [Shell permissions](https://v2.tauri.app/plugin/shell/) | Mature native dialogs and privileged main/preload filesystem access. [Dialog](https://www.electronjs.org/docs/latest/api/dialog) | Functional parity. Picker paths must stay behind narrow shell handlers and must not become generic frontend filesystem authority. |
| Credential storage | Official Stronghold stores secrets in an encrypted vault, but it is not automatically macOS Keychain or Windows Credential Manager and needs a key-bootstrap design. [Stronghold](https://v2.tauri.app/plugin/stronghold/) | `safeStorage` uses macOS Keychain and Windows DPAPI. The app still persists the encrypted blob. On Windows it does not protect against every app in the same user session. [safeStorage](https://www.electronjs.org/docs/latest/api/safe-storage) | Electron has the better ready-made fit. Tauri should use a narrow Rust adapter backed by native OS credential facilities rather than expose Stronghold directly to JavaScript. This is a spike blocker. |
| Windows, menus, notifications, deep links | Official APIs/plugins cover menus, notifications, deep links, single instance, and persisted window state. macOS deep links are build-time registrations; Windows notifications require an installed app. [Window menu](https://v2.tauri.app/learn/window-menu/), [Notifications](https://v2.tauri.app/plugin/notification/), [Deep links](https://v2.tauri.app/plugin/deep-linking/), [Window state](https://v2.tauri.app/plugin/window-state/), [Single instance](https://v2.tauri.app/plugin/single-instance/) | Mature built-ins for browser windows, menus, notifications, protocol handlers, and single-instance routing. macOS and Windows deep-link events differ. [BrowserWindow](https://www.electronjs.org/docs/latest/api/browser-window), [Menu](https://www.electronjs.org/docs/latest/api/menu), [Notification](https://www.electronjs.org/docs/latest/api/notification), [Deep links](https://www.electronjs.org/docs/latest/tutorial/launch-app-from-url-in-another-app) | Functional parity. Validate cold start, second instance, minimized-window focus, and notification/deep-link routing to stale or deleted Tasks. |
| Updates | Official updater supports static or dynamic feeds, signed update artifacts, and Windows pre-exit hooks. [Updater](https://v2.tauri.app/plugin/updater/) | `autoUpdater` supports macOS and Windows through platform-specific mechanisms such as Squirrel and MSIX. Signed macOS apps are required. [Updating](https://www.electronjs.org/docs/latest/tutorial/updates), [autoUpdater](https://www.electronjs.org/docs/latest/api/auto-updater/) | Tauri's explicit artifact signatures are attractive. Either shell must coordinate journal flush, active runs, App Server shutdown, and non-replay of ambiguous prompts. |
| Crash reporting and recovery | Core/webview process isolation exists, but the official stack reviewed has no Electron-equivalent first-party crash uploader. Panic capture and native crash reporting require application or third-party integration. [Process model](https://v2.tauri.app/concept/process-model/) | Built-in Crashpad-based `crashReporter` uploads native minidumps; renderer failures can be observed and windows recreated. [crashReporter](https://www.electronjs.org/docs/latest/api/crash-reporter) | Electron wins diagnostics. Framework-independent product recovery still belongs at the durable App Server journal and session-resync boundaries. |
| Rendering and accessibility | Uses system WebView2 on Windows and WKWebView on macOS. This reduces size but creates browser-engine and OS-version variance. [Process model](https://v2.tauri.app/concept/process-model/) | Ships Chromium and exposes its accessibility tree, giving a more consistent engine across operating systems. [Introduction](https://www.electronjs.org/docs/latest/), [Accessibility](https://www.electronjs.org/docs/latest/tutorial/accessibility) | Electron has lower rendering risk. Tauri must prove markdown, code, streaming, virtualization, IME, keyboard, screen reader, zoom, contrast, clipboard, and drag/drop behavior on real systems. |
| Footprint | Does not bundle the webview and documents size-focused release profiles. [App size](https://v2.tauri.app/concept/size/), [Process model](https://v2.tauri.app/concept/process-model/) | Bundles Chromium and Node.js. [Introduction](https://www.electronjs.org/docs/latest/) | Tauri clearly wins baseline installer and likely memory footprint. Measure signed release artifacts because the shared Rust App Server and Agent binaries reduce the relative difference. |
| Development and tests | Fits Cargo plus the existing Vite frontend. Current WebdriverIO support offers embedded cross-platform driving, including macOS. [WebDriver](https://v2.tauri.app/develop/tests/webdriver/) | Can reuse more TypeScript host code. Electron Forge is the recommended packaging path and Electron supports Playwright-style automated testing. [Packaging](https://www.electronjs.org/docs/latest/tutorial/tutorial-packaging), [Automated testing](https://www.electronjs.org/docs/latest/tutorial/automated-testing) | Electron is faster initially. Tauri avoids another production Node host but requires porting shell adapters to narrow Rust commands. Keep most Frontend tests browser-level and add packaged-app smoke tests. |
| Signing and distribution | Produces macOS app/DMG and Windows MSI/NSIS packages, with macOS signing/notarization and Windows signing support. Cross-compiling Windows has caveats; native runners are preferred. [Distribution](https://v2.tauri.app/distribute/), [macOS signing](https://v2.tauri.app/distribute/sign/macos/), [Windows signing](https://v2.tauri.app/distribute/sign/windows/), [Windows installers](https://v2.tauri.app/distribute/windows-installer/) | Forge packages, signs, and notarizes macOS and signs Windows installers. Mac App Store distribution requires Electron's MAS build and App Sandbox. [Distribution](https://www.electronjs.org/docs/latest/tutorial/distribution-overview), [Packaging and signing](https://www.electronjs.org/docs/latest/tutorial/tutorial-packaging), [Mac App Store](https://www.electronjs.org/docs/latest/tutorial/mac-app-store-submission-guide/) | Start with a notarized direct-download DMG and signed Windows installer. Store sandboxes complicate arbitrary workspaces, external Agents, terminals, and user-selected executables. |

## Native-host hybrid assessment

The proposed hybrid is:

- macOS: AppKit/Swift owns `NSApplication`, `NSWindow`, menus, commands, secure storage, process supervision, and a `WKWebView` containing the existing React work surface. AppKit windows distribute keyboard and mouse events to their views, and AppKit supplies native menus and drag/drop protocols. [NSWindow](https://developer.apple.com/documentation/appkit/nswindow), [Menus](https://developer.apple.com/documentation/appkit/menus), [Drag and drop](https://developer.apple.com/documentation/appkit/drag-and-drop), [WKWebView](https://developer.apple.com/documentation/webkit/wkwebview/)
- Windows: WinUI 3/C# owns the window, menus, commands, secure storage, process supervision, and a WebView2 containing the same React work surface. `AppWindow` maps one-to-one to a top-level HWND, while WebView2 explicitly supports embedding web code in part or all of a native app. [App windows](https://learn.microsoft.com/en-us/windows/apps/develop/ui/manage-app-windows), [Menus](https://learn.microsoft.com/en-us/windows/apps/develop/ui/controls/menus-and-context-menus), [WebView2](https://learn.microsoft.com/en-us/microsoft-edge/webview2/)

| Concern | AppKit + WinUI hybrid | Tauri 2 | Electron | React Native Windows/macOS |
|---|---|---|---|---|
| Focus, keyboard, accessibility | The host owns the outer focus chain and native commands, but focus must cross the native/webview boundary. WebView2 exposes focus traversal, accelerator interception, input forwarding, and an accessibility subtree under the parent HWND. WKWebView remains a web accessibility subtree inside AppKit. Native chrome does not repair semantic or IME problems inside React. [WebView2 API overview](https://learn.microsoft.com/en-us/microsoft-edge/webview2/concepts/overview-features-apis) | Uses the same WKWebView/WebView2 engines, so core work-surface behavior is effectively the same. Cross-platform APIs reduce shell code but do not remove real-device accessibility testing. | Bundled Chromium gives the most consistent work-surface engine and mature accelerator/accessibility behavior, at higher footprint. | Native views can give the strongest platform semantics only after porting React DOM components to React Native primitives. Putting the existing React app in a WebView returns to the same boundary problem. React Native macOS adds desktop-specific focus and key events but remains a separate component system. [macOS events](https://microsoft.github.io/react-native-macos/api/view-events) |
| Drag and drop | Native chrome can receive Finder/Explorer drops. The host must define ownership when a drag crosses into the web surface; WebView2 composition hosting explicitly requires forwarding drop events. Inside the work surface, browser drag/drop remains authoritative. [WebView2 drag/drop](https://learn.microsoft.com/en-us/microsoft-edge/webview2/concepts/overview-features-apis), [AppKit drag/drop](https://developer.apple.com/documentation/appkit/drag-and-drop) | Same system-webview constraints; Tauri can expose a narrow drop/picker command without two host implementations. | Chromium handles work-surface drag/drop consistently; native-to-renderer path still requires explicit validation and safe path handling. | Native drop behavior requires Windows and macOS components or modules; WebView embedding adds no benefit. |
| Native menus, commands, windowing | Best control and exact platform conventions. Also requires two command-routing, enablement, focus, restoration, and multi-window implementations. | Covers native windows and menus from one Rust shell. Use platform-specific hooks only where a demonstrated UX gap remains. | Mature cross-platform window/menu APIs, though Windows menus look Chromium-like rather than WinUI-native. | Possible through platform extensions, but every missing capability becomes a TurboModule/Fabric component. React Native Windows documents separate native module codegen and C++ implementation. [RN Windows native modules](https://microsoft.github.io/react-native-windows/docs/native-platform-modules/) |
| Process lifecycle | Two supervisors must reproduce handoff, heartbeat, client-owned lifetime, graceful shutdown, crash recovery, and sidecar signing. Swift `Process` and .NET `Process` are capable; the cost is duplicated policy-sensitive code. | One Rust supervisor can reuse App Server concepts and keep privileged ownership in the existing language. | Node host code can be adapted from the VS Code shell with the least initial work. | Requires native modules on both platforms; JavaScript must not become process-lifetime authority. |
| Secure storage | Strongest direct fit: macOS Keychain and Windows Credential Locker. Apple describes Keychain as encrypted storage for small secrets; Microsoft supports Credential Locker from WinUI and other desktop apps. [Apple Keychain](https://developer.apple.com/documentation/security/keychain-services), [Windows Credential Locker](https://learn.microsoft.com/en-us/windows/apps/develop/security/credential-locker) | A narrow Rust adapter can call the same native facilities. This needs validation but does not require two UI stacks. | `safeStorage` already uses Keychain/DPAPI, with the limitations recorded above. | Needs maintained platform modules and matching transaction semantics on both platforms. |
| Bridge and IPC | WKWebView supplies script message handlers; WebView2 supplies JSON/string web messages. Both are low-level transports, not an application protocol. The host must define schemas, reply correlation, cancellation, origin checks, payload limits, and reconnect behavior. [WKScriptMessage](https://developer.apple.com/documentation/webkit/wkscriptmessage), [WebView2 web messages](https://learn.microsoft.com/en-us/microsoft-edge/webview2/concepts/overview-features-apis) | Tauri already supplies typed command/event plumbing plus capability ACLs; product traffic should still use the App Server Protocol rather than Tauri-specific product APIs. | `contextBridge`/IPC supplies a mature privileged boundary but requires careful API minimization. | TurboModules are appropriate for native capabilities, but the embedded React DOM app would need an additional WebView bridge anyway. |
| Direct App Server transport | Prefer the existing authenticated finite-HTTP transport directly from the trusted bundled React work surface. Inject only endpoint, token, shell identity, and capabilities at bootstrap. Making Swift/C# proxy every App Server frame would duplicate sequencing, replay, cancellation, and recovery from ADR 0026. | Same recommendation. Tauri should launch/supervise the App Server and provide shell capabilities, not translate product messages. | Same recommendation unless a fixed Chromium custom protocol is proven materially safer. | Native React could use a native module transport, but existing React DOM can keep the current HTTP client only inside a WebView. |
| Crash recovery | Native hosts can separately recover the webview and App Server. WebView2 exposes process-failure, browser-exit, unresponsive-renderer events, and dump locations. Equivalent macOS recovery still needs application instrumentation. [WebView2 process recovery](https://learn.microsoft.com/en-us/microsoft-edge/webview2/concepts/process-related-events) | Same WebView2 recovery hooks are reachable through Rust/native extensions; App Server durability remains the real product recovery boundary. | Built-in Crashpad is the strongest ready-made option. | Requires separate native and JavaScript crash paths plus App Server recovery. |
| Testing | Requires shared browser tests plus Xcode/macOS UI tests, Windows native UI automation, and WebView-specific tests. Microsoft documents Playwright attachment to WebView2. The two hosts double packaged-shell test maintenance. [WebView2 Playwright](https://learn.microsoft.com/en-us/microsoft-edge/webview2/how-to/playwright) | One cross-platform WebdriverIO/Tauri suite plus native smoke tests. | One Electron/Playwright shell suite plus OS smoke tests. | Separate RN Windows and RN macOS integration suites; ecosystem modules must be verified per platform. |
| Updates and signing | Windows can use signed MSIX/App Installer auto-update; macOS direct distribution needs Developer ID signing, Hardened Runtime, nested executable signing, notarization, and a separately selected updater. [MSIX updates](https://learn.microsoft.com/en-us/windows/msix/app-installer/auto-update-and-repair--overview), [Apple notarization](https://developer.apple.com/documentation/security/notarizing-macos-software-before-distribution) | Integrated cross-platform updater and bundler reduce duplicated release machinery. | Forge and `autoUpdater` provide mature release machinery. | Falls back to each native platform's packaging/update story while also packaging RN runtimes and modules. |

### How much native UI is actually required?

To preserve shared components, the custom hybrid's native-owned code should stop at:

- application and window lifecycle;
- application menu, command routing, global shortcuts, and standard About/Preferences entry points;
- title bar/traffic-light or Windows system-backdrop integration where needed;
- file/folder dialogs, notifications, deep links, clipboard/drop handoff, and reveal/open actions;
- Keychain/Credential Locker;
- App Server and visible-terminal process supervision;
- updater, signing hooks, diagnostics, and support export.

Task navigation, project navigation, Task, Chat, Composer, permission/question UI, Agent Settings, and product error/recovery states should remain in the shared Frontend. In particular, the existing shared React `AppSurfaces`, `AppSidebarFrame`, and `Sidebar` already own the application surface and navigation composition. Replacing the sidebar/navigation with AppKit and WinUI controls would duplicate substantial shared UI, not merely add native chrome. It would also introduce a new synchronization seam for selection, routing, focus, badges, pending attention, responsive layout, and recovery state. Moving any of those surfaces into AppKit or WinUI creates two replicas of App Server-owned state and two visible component implementations.

This means the custom hybrid **meaningfully satisfies desktop-native behavior**, but it does **not** satisfy a demand for native-rendered core UI. If the native layer is kept thin enough to avoid reimplementation, its visible contribution is mostly window chrome and system affordances. Tauri already supplies most of that value with one host implementation. A bespoke Swift/C# pair is therefore costly chrome unless a prototype identifies specific platform UI that Tauri cannot deliver.

React Native does not resolve the contradiction. React Native Windows is rendered with native code, and React Native macOS is an out-of-tree Microsoft-maintained platform, but the existing React DOM components are not directly reusable as native views. Native functionality also requires per-platform modules/components. [React Native Windows](https://microsoft.github.io/react-native-windows/), [React Native macOS](https://microsoft.github.io/react-native-macos/docs/intro), [native platform overview](https://microsoft.github.io/react-native-windows/docs/native-platform/)

## Why Tauri

Tauri gives OpenAIDE one native Rust privileged shell layer around the existing Rust App Server and a narrowly scoped JavaScript frontend. Its capability ACLs reinforce the repository's shell-boundary rules, its updater has a clear signed-artifact model, and its system-webview architecture avoids shipping Chromium and Node with every install.

Electron's ability to reuse the existing VS Code TypeScript host code is real, but it is mostly an implementation-speed advantage. It would create a second privileged runtime layer whose process, filesystem, secret, and IPC APIs need careful preload isolation and long-term Node/Electron maintenance. [Electron's security checklist](https://www.electronjs.org/docs/latest/tutorial/security) makes that hardening an explicit application responsibility.

A Swift/AppKit and C#/WinUI split should not be the default. It produces the same webview-rendered work surface as Tauri, two implementations of every shell capability, two release pipelines, and mixed native/web focus and accessibility boundaries. Reconsider it only after a concrete Tauri prototype fails a named platform-native UX requirement that cannot be solved with a small native extension.

## Falsification risks

Choose Electron instead if any of these conditions holds after the spike:

1. The shared Frontend has material WKWebView/WebView2 differences that cannot be fixed without shell-specific UI forks.
2. VoiceOver, Narrator, IME, keyboard navigation, or high-contrast behavior is materially worse in the Tauri shell.
3. Native credential storage cannot meet ADR 0003 without an awkward master-password flow, secret exposure to JavaScript, or unreliable update migration.
4. App Server and Agent sidecars cannot be nested-signed, notarized, updated, and terminated reliably on both platforms.
5. Required PTY behavior is substantially harder or less reliable through the Rust shell than through a proven Electron/Node implementation.
6. Tauri crash diagnostics cannot provide enough actionable native and webview failure data for support.
7. Signed, installed measurements do not show a meaningful footprint or startup advantage over Electron.
8. A concrete native window, menu, command, title-bar, drag/drop, or accessibility requirement cannot be met by Tauri or a small platform extension without moving product UI into the shell.

## Validation spike

Build one disposable Tauri shell with no new product behavior:

1. Package and launch the existing `openaide-app-server` on macOS arm64, macOS x64, and Windows x64.
2. Exercise authenticated handoff, client initialization, heartbeat, window close/reopen, last-client grace shutdown, App Server crash, shell crash, and generation replacement.
3. Load the shared Frontend and test long streaming Chat, markdown/code, virtualization, clipboard, drag/drop, folder selection, IME, keyboard-only navigation, VoiceOver, Narrator, zoom, reduced motion, and high contrast.
4. Implement only narrow shell capabilities: folder picker, file bridge, current piped terminal contract, visible terminal auth, OS notification routing, deep-link/single-instance routing, and secure secret transaction.
5. Prove secret create/read/replace/forget and failed-auth rollback using macOS Keychain and a Windows native credential facility without returning saved values to the Frontend.
6. Build signed installers, notarize the macOS bundle including sidecars, install on clean machines, and test GUI-launch environment and executable discovery.
7. Exercise a signed update while idle and while a Task, permission, authentication, or terminal operation is active.
8. Measure signed installer size, cold start, first paint, idle and streaming memory, CPU, update time, and crash-recovery time.
9. Build one representative native integration on each platform: macOS application menu/command routing and Windows title-bar/menu/accelerator routing. Verify focus transfer, enablement from App Server snapshots, screen-reader ordering, and drop handoff at the webview boundary.

The decision passes only if those tests work without changing App Server ownership rules or introducing shell-specific product state. If rendering consistency, diagnostics, or host capability reliability fails, repeat the same thin spike in Electron. If Tauri alone fails a specific native-shell UX requirement while WKWebView/WebView2 passes, prototype only that requirement in AppKit and WinUI before accepting the cost of two custom hosts.

## Spike status — 2026-07-30

An ignored throwaway implementation now lives under
`tmp/prototypes/desktop-tauri/`. It reuses the shared Frontend, initializes the
App Server as `shellKind: "desktop"`, launches the existing App Server through
authenticated LocalHttp attach-or-launch, and adds native menu commands, folder
selection, notifications, external URL opening, and in-memory Desktop routing.
The prototype has one-command launchers for macOS/Linux shell environments and
Windows PowerShell.

Verified:

- the shared Frontend production bundle builds in the prototype;
- the prototype TypeScript passes strict checking;
- the Tauri Rust surface passes a Windows GNU target `cargo check`; the
  non-Windows compile-only check uses an explicit no-link resource-compiler shim,
  so this is not a Windows executable or installer result;
- the real `openaide-app-server` binary returns an authenticated, loopback-only
  LocalHttp handoff for the prototype state root;
- the prototype directory is ignored, leaving this research note as the only
  repository-visible artifact.

Not yet verified:

- an actual macOS or Windows window;
- native menu, picker, notification, focus, accessibility, and IME behavior on
  those systems;
- platform Keychain/Credential Manager integration;
- linked installers, signing/notarization, updater behavior, or crash recovery.

The current Linux environment cannot launch the window because its WebKit/GTK
development headers are absent and installing them requires an interactive
administrator password. No GUI result is claimed.

The spike also confirmed a repository seam that must be designed before
production implementation: App Server Protocol already supports Desktop, but
the transitional `app-shell-contracts` bootstrap still admits only Web and VS
Code, and the full sidebar workbench composition is selected by a Web-specific
check. The prototype isolates the type mismatch with a cast and uses the real
Desktop protocol identity; production must generalize the shell composition
contract instead of impersonating Web or retaining the cast.
