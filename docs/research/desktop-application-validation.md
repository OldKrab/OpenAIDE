# Desktop Application Validation

Research date: 2026-08-09
Current-main basis: `aeb337de01704b8f795c0ef5106bb633b0f8ca60`

## Question and verdict gate

Can Tauri 2 become the permanent OpenAIDE Desktop shell without weakening OpenAIDE's shared-product, App Server ownership, security, recovery, accessibility, or platform-integration requirements?

The existing stack comparison and prototype record remains [`desktop-application-stack.md` on `origin/shushakov/desktop-tauri-prototype`](https://github.com/OldKrab/OpenAIDE/blob/d270f799443311c3fb3af180126e42ec69009fbd/docs/research/desktop-application-stack.md). This note does not repeat or replace that comparison. It narrows the next step into the user-approved validation contract against current `main`.

**Tauri is provisional.** Run a disposable but production-shaped validation shell first. Tauri becomes the permanent `apps/desktop` choice only after **every hard gate** in this note passes on real supported macOS and Windows systems. Security, correctness, recovery, accessibility, and packaging failures cannot be offset by better footprint or startup measurements. One narrow native extension for a platform capability such as credential access or WebView failure observation is acceptable; duplicated shell hosts, shell-owned product workflow, exposed secrets, or unresolved hard failures are not.

If Tauri fails a hard gate, do not assume Electron fixes it. Build the smallest Electron comparison that exercises the same failed boundary and choose Electron only if that proof passes without weakening the accepted architecture. Electron exposes first-party `unresponsive` and `render-process-gone` events, so it is a credible recovery comparator, not an automatic winner ([Electron `webContents`](https://www.electronjs.org/docs/latest/api/web-contents)).

## Accepted product envelope

- Validate the current shared Project Navigation, Task, Chat, Composer, Settings, and recovery UI; do not redesign product behavior during the spike. Desktop is a native App Shell around the shared product surface, consistent with the product intent in [`PRODUCT.md`](../../PRODUCT.md), the visual direction in [`DESIGN.md`](../../DESIGN.md), and the ownership boundary in [ADR 0022](../adr/0022-backend-frontend-app-shell-architecture.md).
- Initial support is **macOS 15 or newer on Apple silicon** and **Windows 11 x64**. Windows ARM64, Intel Macs, and Windows 10 wait for real-machine evidence and CI. Microsoft ended ordinary Windows 10 Home/Pro support on 2025-10-14 ([Microsoft lifecycle](https://learn.microsoft.com/en-us/lifecycle/products/windows-10-home-and-pro)).
- V1 has one application process and one main window. Relaunch, Dock/taskbar activation, and notification clicks focus and route that window. The accepted window, distribution, and local-diagnostics behavior is recorded in [ADR 0032](../adr/0032-desktop-v1-platform-and-window-lifecycle.md).
- Desktop, Desktop Development, and the VS Code Extension use distinct state roots and distinct credential namespaces. They may work on the same selected Project files, but their OpenAIDE Tasks, Chat projection, settings, credentials, endpoint records, and diagnostics cannot collide.

Tauri uses the operating system's WebView2 on Windows and WKWebView on macOS rather than shipping one browser engine, which is why real-machine rendering and accessibility evidence is a hard gate ([Tauri process model](https://v2.tauri.app/concept/process-model/)).

## Reusable seams on current `main`

Current `main` already contains most of the product and transport seams the validation shell should reuse or deepen rather than reproduce:

| Seam | Current source of truth | Validation use and remaining work |
| --- | --- | --- |
| Product ownership | [ADR 0022](../adr/0022-backend-frontend-app-shell-architecture.md) and [`packages/frontend/AGENTS.md`](../../packages/frontend/AGENTS.md) | App Server remains the owner of durable product state, workflow, persistence, ordering, Agent lifecycle, and recovery. The native shell owns only application lifetime and narrow native capabilities. |
| Shared render root | [`startFrontend.tsx`](../../packages/frontend/src/startFrontend.tsx) and [`frontendShell.ts`](../../packages/frontend/src/services/frontendShell.ts) | `startFrontend(shell)` already installs one typed shell adapter. `FrontendShell` already has navigation, recovery, native project/file acquisition, and an optional shell-owned logical `backendConnection`. Desktop should add a composition root here rather than fork product components. |
| Project-first Desktop UI | [`AppSurfaces.tsx`](../../packages/frontend/src/components/AppSurfaces.tsx), [`AppSidebarFrame.tsx`](../../packages/frontend/src/components/AppSidebarFrame.tsx), and [ADR 0022](../adr/0022-backend-frontend-app-shell-architecture.md) | Reuse the Web/Desktop project-first workbench and shared sidebar. Current code still identifies the full workbench with a Web-specific check, so production hardening must generalize that composition instead of making Desktop impersonate Web. |
| Desktop protocol identity | [`client.rs`](../../openaide-rs/app-server-protocol/src/client.rs) and generated [`protocol.ts`](../../packages/app-server-client/src/generated/protocol.ts) | App Server Protocol already admits `desktop`. The transitional [`bootstrap.ts`](../../packages/app-shell-contracts/src/webview/bootstrap.ts) admits only `web` and `vscodeExtension`; validation must remove that mismatch without inventing a Desktop-only product protocol. |
| Authenticated attach-or-launch | [`appServerHandoff.ts`](../../packages/app-server-client/src/appServerHandoff.ts), [`process.ts`](../../apps/vscode-extension/src/runtime/process.ts), and [`app_server_handoff.rs`](../../openaide-rs/app-server/src/app_server_handoff.rs) | Reuse the loopback endpoint/token validation and App Server handoff semantics. Tauri sidecars are target-specific external binaries, so packaged discovery must be tested for each target rather than inferred from a development path ([Tauri sidecars](https://v2.tauri.app/develop/sidecar/)). |
| One logical session across renderer recreation | [`appServerSessionBridge.ts`](../../packages/app-server-client/src/appServerSessionBridge.ts), [`appServerHostClient.ts`](../../apps/vscode-extension/src/runtime/appServerHostClient.ts), and [`reliableBackendConnection.ts`](../../packages/app-server-client/src/reliableBackendConnection.ts) | The bridge already lets a view consume a shell-owned `AppServerSession`, while the reliable connection already handles heartbeats, endpoint-generation replacement, fresh baselines, and non-replay of ambiguous requests. Extract or reproduce this ownership in the native shell; do not make the WebView the lifetime client. |
| Secret transaction UI seam | [`agentSecretTransaction.ts`](../../packages/frontend/src/services/agentSecretTransaction.ts) and [ADR 0003](../adr/0003-acp-auth-secret-storage.md) | The Frontend already speaks apply/commit/rollback without receiving saved values. Desktop must supply the native generation-backed implementation described below. |
| Task Attention | [ADR 0027](../adr/0027-task-attention-and-shell-local-notifications.md) and the existing VS Code manager in [`taskNotificationManager.ts`](../../apps/vscode-extension/src/notifications/taskNotificationManager.ts) | Reuse App Server-owned Attention Events and implement only Desktop-local permission, focus, receipts, replacement, delivery, and click routing. |
| Development launch precedent | [`.vscode/launch.json`](../../.vscode/launch.json), [`.vscode/tasks.json`](../../.vscode/tasks.json), and [`developmentLaunch.test.ts`](../../apps/vscode-extension/src/runtime/developmentLaunch.test.ts) | The Extension Host already uses a pre-launch build and an isolated development storage root. Add an equivalent Desktop launch entry; do not reuse the Extension's state or credentials. |
| Metadata-only diagnostics | [`diagnostics.ts`](../../packages/app-server-client/src/diagnostics.ts), [`rotating_file.rs`](../../openaide-rs/app-server/src/logging/rotating_file.rs), and the allowlisted Support Export in [`bundle.ts`](../../apps/vscode-extension/src/diagnostics/bundle.ts) | Extend the same redaction and lifecycle-correlation model through the native shell rather than creating a content-bearing crash log. |

## Desktop Development launch

Add one VS Code launch configuration named **`OpenAIDE: Desktop (Development)`**. Selecting it and pressing F5 must build the required App Server, shared Frontend, and Tauri source artifacts and then open the Desktop window. It must not require the user to start a dev server, App Server, or second command manually. This is a developer convenience launch, not a substitute for future platform CI.

The launch uses an ignored development state root and a development-only credential service/target prefix. The namespaces must be distinct from both installed Desktop and the VS Code Extension. Tests must assert the separation in launch/config resolution, following the current Extension development-root test. No requirement for backend hot replacement or draft preservation during rebuild is added here; F5's approved purpose is build-and-start from sources.

## Native credential transaction

Desktop credentials are owned by a Rust capability backed by macOS Keychain and Windows Credential Manager. Apple documents Keychain as encrypted storage for small secrets, and the Win32 credential APIs create, read, modify, and delete entries in the current user's credential set ([Apple Keychain Services](https://developer.apple.com/documentation/security/keychain-services), [Microsoft Credentials Management](https://learn.microsoft.com/en-us/windows/win32/secauthn/credentials-management), [`wincred.h`](https://learn.microsoft.com/en-us/windows/win32/api/wincred/)). Tauri Stronghold is not the Desktop credential store. The WebView cannot invoke a generic credential API and never receives a persisted secret value.

Because one authentication method may contain several secret fields while both platform APIs operate on individual items, Desktop uses immutable secret generations:

1. Serialize operations for one credential identity.
2. Write every proposed value under a transaction-scoped, immutable generation without changing the active generation.
3. Supply only that staged generation to the Agent authentication attempt.
4. On failure or cancellation, delete the staged generation and leave the previous generation authoritative.
5. On success, App Server atomically commits the non-secret active-generation reference together with the corresponding auth configuration; only then may the shell garbage-collect the former generation.
6. After a crash, the last committed reference remains authoritative and unreferenced staged generations are safe startup garbage.

The gate covers create, read/inject, replace, forget, multi-value update, failed authentication, cancellation, crash between each phase, orphan cleanup, and concurrent-request serialization on both platforms. It also proves that prompts, WebView state, App Server state, logs, diagnostics, and Support Export never contain the secret. This is the Desktop realization of [ADR 0003](../adr/0003-acp-auth-secret-storage.md).

## Shell-owned lifetime and session bridge

The native shell—not the window and not the WebView—owns one initialized Desktop lifetime client and the physical endpoint recovery feed. The App Server is still an on-demand process collectively owned by initialized App Shell clients; no launcher or child-process handle becomes product-lifetime authority ([ADR 0026](../adr/0026-resumable-http-rpc-session.md), [ADR 0022 lifecycle](../adr/0022-backend-frontend-app-shell-architecture.md)).

The validation implementation must:

- attach to or launch the state-root App Server through authenticated loopback handoff;
- initialize one stable Desktop `clientInstanceId` for the native application process;
- keep that logical session alive while the native shell is resident even if the window or WebView is destroyed;
- bridge renderer requests, subscriptions, reverse requests, status, and fresh recovery baselines through the existing `AppServerSession` contract;
- publish physical endpoint-generation replacement to the reliable session after App Server failure;
- never replay an RPC whose dispatch result is ambiguous, especially `task/send`, prompts, permissions, authentication, or other work-producing mutations;
- detach through the normal last-client/graceful-shutdown path only when the application really quits.

## Startup, recovery, close, and restore

- **Startup:** show a real recoverable native window within 1 second and make the shared UI interactive within 2 seconds. App Server and Agent preparation continues asynchronously with renderable progress or recovery states.
- **Single instance:** one application process owns one main window. Second launch, Dock/taskbar activation, and notification routing focus and route the existing window.
- **App Server recovery:** prove attach, clean shutdown, crash, endpoint-generation replacement, restart, fresh baselines, and lazy Task hydration. Durable Task and Chat state recovers; accepted prompts and mutations are never replayed automatically.
- **WebView recovery:** detect a real WebView crash or sustained hang on each supported platform and return the application to a usable state. Whole-application restart is acceptable. An unsent Frontend-only Composer draft may be lost; durable Task/Chat must recover. Tauri's current public `WebviewEvent` surface exposes drag/drop rather than Electron-like crash events, so the validation may need a narrow native failure hook plus a renderer/native heartbeat ([Tauri `WebviewEvent`](https://docs.rs/tauri/latest/tauri/enum.WebviewEvent.html)). Inability to detect and recover reliably fails Tauri.
- **macOS close:** closing the last window keeps the application, lifetime client, and active work resident. Explicit Quit warns when work is active and uses the coherent interruption/shutdown path.
- **Windows close:** idle last-window close exits. With active work it offers **Keep running in background**, **Quit and stop**, and **Cancel**. Background mode retains the lifetime client and shows a notification-area icon with **Open OpenAIDE**, **Quit**, and a non-interactive active-work count; it contains no Task controls. Explicit Quit warns when work is active.
- **Restore:** normal relaunch restores display-clamped bounds, maximized/fullscreen state, theme, and a durable Task or Settings route. It does not restore dialogs, permissions, secret forms, or unsent Composer state.

## Native commands and the Windows title bar

macOS uses standard native application menus and platform shortcuts for New Task, Add/Open Project Folder, Settings, editing, zoom, window actions, About, and Quit. Development-only Reload and developer tools do not ship in release builds. Commands route through typed shell/Frontend intents; the native menu does not own product workflow. Tauri has native desktop menus and predefined editing roles suitable for this boundary ([Tauri window menus](https://v2.tauri.app/learn/window-menu/)).

The approved Windows direction is a **compact app-command entry integrated into the window title bar**, visually aligned with the top of the sidebar. Do not add a permanent macOS-style `File / Edit / View` strip. Frequent New Task, Add Project, and Settings actions remain discoverable in the shared UI.

This direction is not yet approved as implemented UI. A production-aligned prototype must prove the real sidebar/title-bar composition with native caption buttons, dragging, the system window menu, maximize/restore and snap layouts, keyboard access, focus order, high contrast, text scaling, common display scale factors, RTL-safe insets, and narrow window widths. Microsoft requires interactive title-bar regions to be separated from drag regions and requires layouts to account for system-reserved caption-button insets and scaling ([Windows title-bar implementation](https://learn.microsoft.com/en-us/windows/apps/develop/title-bar), [Windows title-bar design](https://learn.microsoft.com/en-us/windows/apps/design/controls/title-bar)). If this proof fails, place the same command entry in the sidebar header; failure of the integrated placement alone does not change command semantics.

## Notifications

Desktop consumes App Server-owned Task Attention Events. It suppresses an OS notification only when the affected Task was focused at the event occurrence time. Otherwise it applies startup-baseline suppression and local delivery receipts, replaces the previous notification for that Task, and routes a click into the existing main window and Task. The notification contains only the Task title and a short product-authored reason—never prompt, Chat, Tool, permission, question, credential, path, URL, or arbitrary error content. These ownership and content rules are accepted in [ADR 0027](../adr/0027-task-attention-and-shell-local-notifications.md).

Validate permission denial, enabled/disabled state, app focused on another Task, app backgrounded, window closed, second Attention Event replacement, stale/deleted Task routing, cold app activation, and duplicate suppression. Windows notification tests use the installed package because Tauri documents Windows notification delivery as installed-app behavior ([Tauri notifications](https://v2.tauri.app/plugin/notification/)).

## WebView2 and installer decisions

Windows uses automatically updated **Evergreen WebView2**, with the small bootstrapper included for missing or damaged installations. Microsoft recommends Evergreen for most apps, notes it is preinstalled on Windows 11, and still recommends distributing a runtime installer for edge cases ([Microsoft Evergreen guidance](https://learn.microsoft.com/en-us/microsoft-edge/webview2/concepts/evergreen-vs-fixed-version)). Tauri supports embedded/downloaded bootstrappers, offline installers, and fixed runtimes; a fixed runtime adds roughly 180 MB and shifts browser servicing responsibility to the app ([Tauri Windows installer](https://v2.tauri.app/distribute/windows-installer/)). Do not bundle a fixed runtime or offline installer in v1.

The release formats are a direct-download notarized DMG on macOS and a signed per-user NSIS installer on Windows. MSI, Microsoft Store, Mac App Store, and a fully offline Windows installer are deferred. Tauri supports DMG/direct distribution and NSIS setup executables ([Tauri distribution](https://v2.tauri.app/distribute/), [Tauri Windows installer](https://v2.tauri.app/distribute/windows-installer/)).

## Hard validation matrix

Every row is pass/fail. Run the matrix on a real macOS 15+ Apple-silicon machine and a real Windows 11 x64 machine unless the row names only one platform. Automated checks may supplement this evidence, but future CI does not replace real installed-app interaction.

| Boundary | Required proof |
| --- | --- |
| F5 development launch | From the repository, select `OpenAIDE: Desktop (Development)`, press F5 once, observe required source builds, and reach the real Desktop window without a manual prerequisite command. Prove Desktop Development state and credential namespaces cannot collide with installed Desktop or VS Code. |
| Shared product surface | Run the current project-first navigation, New Task, Task, Chat, Composer, Settings, permission/question, Agent setup/auth, and recovery surfaces without a Desktop fork or shell-owned workflow decision. |
| Rendering and long work | Exercise realistic long Markdown/code, large and virtualized history, animated tool disclosure, a 30-minute streaming turn, history prepend, image/file interactions, selection/copy/paste, external links, folder/file pickers, Finder/Explorer drag/drop, zoom, theme changes, and narrow/wide windows. No unbounded memory growth, input degradation, content loss, or shell-specific product UI fork. |
| Accessibility and input | Complete core workflows keyboard-only; verify visible focus, semantic busy/disabled state, reduced motion, high contrast, scalable text, VoiceOver on macOS, Narrator on Windows, common IMEs, dead keys, composition/candidate windows, clipboard, drag/drop, and 100%/125%/150%/200% Windows scaling where supported by the validation hardware. Meet the repository's WCAG 2.2 AA intent in [`PRODUCT.md`](../../PRODUCT.md). |
| Credential security | Pass the complete immutable-generation transaction matrix above using real Keychain and Credential Manager entries. Prove saved values never enter WebView, product state, logs, Chat, diagnostics, or Support Export. |
| App Server packaging | Package the target-specific `openaide-app-server`, launch from Finder/Launchpad and Start/taskbar rather than a shell, find the bundled executable without relying on a developer `PATH`, authenticate handoff, and clean up or recover endpoint records correctly. |
| Lifetime and close | Prove the native shell remains the one logical lifetime client across window/WebView recreation, second launch, macOS last-window close, Windows active/idle close choices, background tray restoration, explicit Quit, reconnect grace, and clean last-client shutdown. |
| App Server failure | Kill or crash the App Server during idle, streaming, pending permission/question, authentication, and terminal activity. Observe classified failure, generation replacement, fresh authoritative baselines, coherent durable recovery, and no automatic replay of ambiguous work. |
| WebView failure | Crash the renderer and simulate a sustained hang. Detect it, record actionable metadata, restart the whole app if necessary, restore a usable durable route, and never replay accepted work. Failure to demonstrate this on either platform rejects Tauri. |
| Window integration | Prove single-instance focus/routing, bounds and route restoration, native macOS menus/shortcuts, and the Windows integrated-title-bar prototype. On Windows verify drag, system menu, caption controls, snap layouts, keyboard, focus, scaling, high contrast, RTL insets, activation, and narrow widths; use the sidebar-header fallback if integrated placement fails. |
| Notifications | Pass focus-at-occurrence suppression, startup baseline, replacement, permission/enablement, background/closed-window delivery, click routing, stale Task handling, and metadata-only content. Prove installed Windows delivery. |
| Packaging and install | Build and install a local `.app`/DMG and per-user NSIS artifact, including App Server discovery and uninstall/reinstall behavior. Validate Evergreen bootstrapper recovery for a missing/damaged WebView2 runtime without adopting Fixed Version. |
| Diagnostics | For every shell/WebView/App Server boundary operation, correlate structured start and terminal events with operation name, safe IDs, outcome, duration, attempt/retry count, and error class. Forced native panic, WebView hang/crash, App Server crash, launch failure, and shutdown failure must leave actionable local evidence with no prohibited content. |
| Performance | Meet every hard target in the next section on both validation machines and retain the raw measurement method and machine facts with the result. |

## Performance gates

- A real recoverable native window is visible within **1 second** of launch.
- The shared UI is interactive within **2 seconds**, while App Server and Agents may continue preparing asynchronously.
- Local input produces visible response within **100 ms**.
- Median idle CPU remains below **1%** on each validation machine.
- A **30-minute** streaming run has no unbounded memory growth and no degraded input behavior.

Also record cold-start distribution, first recoverable paint, interactive time, idle and streaming memory, idle and streaming CPU, local installer size, bundled App Server size, and App Server/WebView recovery duration. Those measurements remain decision evidence even where the threshold is not independently hard-coded.

## Local diagnostics

V1 performs no automatic crash or telemetry upload. The native shell records bounded, metadata-only local lifecycle logs, Rust panic outcomes, renderer-heartbeat failures, App Server handoff/generation/shutdown transitions, close decisions, recovery stages, and stable safe correlation IDs. It must not record prompts, Chat, tool/terminal content, secrets, tokens, credentials, environment values, local paths, URLs, or arbitrary free-form error messages. Troubleshooting data leaves the machine only through the explicit allowlisted and redacted Support Export boundary. If a forced real crash cannot be classified and followed across first/last events for every lifecycle stage, Tauri fails validation. This follows [ADR 0032](../adr/0032-desktop-v1-platform-and-window-lifecycle.md) and the repository-wide diagnostics rules in [`AGENTS.md`](../../AGENTS.md).

## Validation signing versus release signing

The validation milestone must build, install, launch, and exercise local macOS app/DMG and Windows NSIS artifacts with the bundled App Server. It does **not** require production Developer ID credentials, Apple notarization, Authenticode, or a signed updater. Those require release credentials and future release CI; they remain mandatory release gates, not prerequisites for the F5 workflow or this local framework verdict. Tauri's official distribution guidance confirms that direct macOS distribution ultimately requires signing and notarization and documents platform signing credentials ([Tauri distribution](https://v2.tauri.app/distribute/), [Tauri macOS signing](https://v2.tauri.app/distribute/sign/macos/)).

The validation report must still record the package layout and every nested executable that release signing will cover. It may not claim release readiness from an unsigned or ad-hoc local package.

## Explicitly out of scope

- Creating permanent `apps/desktop` before Tauri passes every hard gate.
- Redesigning Task, Project, Chat, Composer, Settings, navigation, or App Server workflow.
- A native AppKit/WinUI rewrite of shared product surfaces, duplicated macOS/Windows shell hosts, or a Desktop-specific product protocol.
- Windows 10, Windows ARM64, Intel Macs, Linux, mobile, multiple main windows, MSI, app stores, and fully offline installers.
- Production signing/notarization, Authenticode, automatic updates, signed-update interruption policy, and release CI implementation. They remain future release work.
- Automatic crash upload, analytics, or telemetry collection.
- A persistent idle Windows tray icon or Task controls in the tray.
- Renderer-only recovery, guaranteed recovery of an unsent Composer draft after a crash, or automatic continuation/replay of interrupted Agent work.
- A durable background App Server daemon; lifetime remains attached to initialized App Shell clients.
- A PTY unless a real Agent test proves the current piped terminal contract is insufficient.
- Deep-link product behavior beyond proving that the single-instance routing seam can later focus and route the existing window.

## Electron fallback proof

If one Tauri hard gate fails, isolate the failing boundary and implement the smallest Electron shell slice that runs the same shared Frontend and App Server contract. Repeat the same test data, machine, packaging mode, diagnostics/redaction rules, and acceptance criterion. Do not broaden the comparison into a second full application.

Choose Electron only when that focused proof passes and Tauri cannot pass with at most one narrow native extension. Prefer Electron if Tauri would require duplicated platform shell hosts, WebView-accessible secrets, shell-owned product state, weakened no-replay semantics, or acceptance of an unresolved accessibility, recovery, security, or packaging defect. If Electron also fails, the result is an unresolved product requirement, not permission to lower the gate.
