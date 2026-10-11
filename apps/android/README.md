# OpenAIDE Android

The Android app renders the shared Frontend in a WebView and works in one of two ways.
Paired with a **Remote computer**, it is a client of that computer's App Server
([ADR-0062](../../docs/adr/0062-remote-devices-are-app-server-clients.md)): the Frontend
ships in the APK, and execution, projects and authentication stay on the computer.
On **This phone**, it is a client of an App Server running in Termux, started and
reached the way an App Shell on a computer starts and reaches its own; the App Server,
ACP integration, projects and agent authentication stay in Termux. In both modes the
Frontend ships in the APK. Neither mode embeds a standalone execution environment.

## Connections and background work

Choose **This phone** or **Remote computer** once. Connection settings are
available under **Settings → Connection**, with a launcher shortcut
and notification action as recovery paths. There is no persistent native footer. The last connection is
remembered; changing it never migrates or resubmits tasks. Save unsent drafts before
switching. Local work retains its background protection when switching to remote.

Offline, full-screen setup guides Termux installation, one-time command access,
automatic compatibility checks, workspace installation and agent sign-in. It uses
OpenAIDE styling, large touch targets, inline progress/errors and system light/dark appearance,
not a list of Android debug dialogs. **Advanced** contains the setup check and
diagnostics. Android permission grants, initial Termux initialization and agent
sign-in still require user interaction. Local work needs one agent installed in
Termux: Codex or Claude. Dependency installation adds a pinned Termux-compatible
release of each agent that is missing; it does not replace an existing user
installation.

A remote computer needs no address, account or password. On the computer, open
**Settings → Devices → Show code**; on the phone, choose **Remote computer** and scan
the QR code with the camera or paste its text. Where the phone cannot scan, it shows
its own code instead, which the computer accepts under **Enter code**. A code works
once and for a few minutes. Pairing makes the computer trust this phone's key; the
phone keeps its private key encrypted with Android Keystore and remembers the computer's
key and name. Any client of that computer can remove the phone under **Settings → Devices**.

The paired connection is end-to-end encrypted between the two keys by
[iroh](https://www.iroh.computer). It uses a direct path when the networks allow one
and otherwise a public relay that sees only ciphertext. The WebView loads the bundled
Frontend from a gateway inside the app, on `127.0.0.1:5475` for a paired computer and
`127.0.0.1:5476` for this phone, so each workspace keeps its own stored drafts and
preferences. The gateway carries App Server requests to the App Server, one stream
per request to the computer, and answers only requests that hold the per-process
cookie given to the app's own WebView. A lost connection is
reopened on the next request, and the Frontend's resumable session continues from
where it stopped. The computer must be running OpenAIDE; the phone and the computer
must speak the same App Server Protocol major version.

Opening the local workspace runs the App Server in Termux in its handoff mode: it
attaches to the server already serving the state or launches one, and prints its
loopback address and per-process token. The app keeps that token in memory, never
gives it to the WebView, and adds it to each request the gateway carries. The app is
itself a client of the server and sends a heartbeat while its process lives. The
App Server stops as it does on a computer: after its last client has left. Closing
the view with background work enabled keeps the app's process, and so the server,
alive while agents work; once the app is gone, the server ends. There is no
supervisor and no start after reboot: opening OpenAIDE starts the server again.
A server that stopped while the workspace was open is started again by the next request.

The Android service reads metadata-only task counts from the App Server as that
same client, independently of WebView. Active local work holds a bounded, renewed wake lock; idle work releases it.
When the view is hidden and local work becomes idle, monitoring stops. A short
settling period protects a send immediately followed by screen lock. Temporary
status failures have a two-minute recovery grace, followed by a visible recovery
notification rather than an unbounded wake lock. End-of-work/attention notifications
are local-only; remote push notifications are not implemented.

Hidden WebView timers pause; returning resumes the existing transport and draft.
Idle foreground checks run less frequently, status reads coalesce, and ordinary
browser console output is not copied into Android diagnostics. Background safety
does not depend on rendering the chat or keeping its WebView awake.

The installer obtains the runtime URL and SHA-256 automatically from the canonical
`v<APK version>` release in `OldKrab/OpenAIDE`, checks the exact artifact URL and digest,
and verifies the archive before extraction. Users do not enter checksums or package lists.
The canonical release includes the signed APK and matching `openaide-termux-arm64.tar.gz`
asset together, enabling fresh-phone one-tap installation. Until that version is published,
the app reports an unavailable download without installing unverified content.
The runtime is the App Server binary and a `VERSION` file, and must be the app's own
version. After an app update, setup offers the matching runtime; installing it stops
a server left running from the old one and replaces the runtime directory. Tasks and
history live in the state directory beside it and are kept.

## Install

Use Android 8 or newer and Termux 0.118 or newer with Node.js, npm, Git and at least
one agent command that works in Termux: `codex` or `claude`. The managed Codex
integration pins Codex 0.153.3; use a compatible Termux build. Neither agent
publishes an Android build, so setup installs one that runs in Termux when it is
missing. Codex is a pinned community build. Claude Code ships only as a glibc binary:
setup takes the binary the app's Claude adapter was built against from the adapter's
own npm package, keeps it in `$PREFIX/opt/openaide-claude`, and installs a `claude`
command that starts it with Termux's glibc packages. Two things make it behave like
any other program there. The binary names Termux's glibc loader, written into unused
padding because tools that move the file's contents break it; started as an argument
of the loader instead, its built-in `grep` and `find` fail. And one `LD_PRELOAD` path
gives Claude the glibc build of `termux-exec` and the Android programs it starts the
ordinary one, so `#!/usr/bin/env` scripts run from its shell. The adapter is pointed
at that command instead of its bundled runtime. A phone that cannot start the binary
keeps working with Codex; the step that failed is in
`~/.local/share/openaide-android/install.log`. Authenticate the agent in Termux.
The runtime artifact currently targets ARM64 phones only.

1. Build the **Android APK and Termux runtime** GitHub Actions workflow.
2. Download and unzip the runtime and APK artifacts. Prefer `app-release.apk`
   from `openaide-android-user` for everyday use; `app-debug.apk` is for development.
3. Transfer `openaide-termux-arm64.tar.gz` into Termux and extract it:

   ```sh
   mkdir -p ~/.local/share/openaide-android
   tar -xzf openaide-termux-arm64.tar.gz -C ~/.local/share/openaide-android
   ```

4. Set `allow-external-apps=true` in `~/.termux/termux.properties`. This allows
   apps granted Termux's command permission to execute commands in Termux.
5. Open OpenAIDE, choose **This phone**, and follow the one-time setup.
   Later launches connect automatically. Use **Advanced → Check this phone**
   if Android no longer prompts.

The APK sends its bundled scripts through Intent stdin; it does not need access to
Termux's private directory. The App Server binds a loopback port of its own choosing
and accepts only its per-process token, which reaches the app in the start command's
result. No agent credential is copied into the APK. App backup is disabled.

A command started from another app does not get the `termux-exec` library that a
Termux terminal preloads, so the start script sets it: without it the server cannot
start an adapter through `npx`, and agents cannot run `#!/usr/bin/env` scripts.

Android may kill Termux processes; return to OpenAIDE to start the server again.
To stop it explicitly, close OpenAIDE or stop its process in Termux.

Logs: `~/.local/share/openaide-android/state/launcher.log`. A development build
installed beside the app with an application id suffix keeps its runtime and state in
its own folder, `openaide-android-<suffix>`, so the two never share a server. State is kept beside
the runtime directory so replacing runtime files does not delete task history.

## Build and validation

Use JDK 17, Gradle 8.13 and Android SDK 35. The APK ships the shared Frontend, so
build it first:

```sh
npm run build:typescript-deps && npm run build --workspace openaide-frontend
gradle -p apps/android testDebugUnitTest lintDebug assembleDebug
```

The APK is built for ARM64 only. The iroh library adds about 14 MB.

CI separately cross-compiles the App Server with the Android NDK, builds the
shared Frontend, runs existing Web Shell tests and packages the Termux runtime,
which is the App Server alone.
It runs on every pull request, pushes to `main` and `android/**`, and manual dispatch,
so shared Frontend and transport changes cannot bypass Android builds. Each run
uploads the debug APK, a compiled device-test APK, unit/lint reports and the ARM64
Termux runtime. Device tests still require a configured phone; CI compilation is
not a substitute for executing them via ADB.
With the persistent signing secrets configured, CI also produces
`openaide-android-user/app-release.apk`, a non-debuggable user build. The debug
artifact remains available for device tests. Store distribution still needs its
own signing/review process; the current persistent development certificate is
retained for in-place updates on existing test phones.
Signed user builds run only on pushes and manual dispatch, not pull requests.
The canonical Release workflow also requires a signed user APK and matching runtime
before publishing its immutable draft. The APK takes its version from root `package.json`;
its sideload version code is `major * 1000000 + minor * 1000 + patch`. Prereleases
share their base version code; Play Store distribution is not configured.

For installable updates, configure repository secrets
`OPENAIDE_ANDROID_DEBUG_KEYSTORE` (base64-encoded PKCS12, alias `openaide-debug`)
and `OPENAIDE_ANDROID_DEBUG_KEYSTORE_PASSWORD`. Keep the signing key private and
backed up. These secrets sign user builds. Debug and device-test APKs use a temporary
debug key; subsequent debug APKs may require uninstalling the previous app. Without
the secrets, CI does not publish a user APK. Switching from the initial
temporary key requires a one-time reinstall; Termux task history is unaffected,
and the phone must be paired with its computer again.

On a device, verify permission denial and grant, first startup, authenticated
task creation, streaming, tool approvals, attachments, Back navigation, rotation,
reconnection, and recovery after Termux is stopped. APK compilation alone does
not establish working Codex execution on Android.

Remote push notifications, support-export downloads and automatic runtime-update application
are not implemented. Shared task and settings behavior stays
in the existing Frontend and App Server.

Android Back navigates page history, then backgrounds the app. Returning wakes
the existing transport rather than reloading the page, preserving unsent drafts.
Stalled HTTP uploads retry the same sequenced frame, not a new user command.

Background mode uses a foreground service and a task-aware partial wake lock, with a visible
notification and **Turn off** action. It keeps the CPU awake, not the screen, and
uses additional battery. Enable unrestricted battery use for **both OpenAIDE and
Termux** in Android settings; Doze, vendor restrictions, force-stop, reboot or
memory pressure can still interrupt execution. This is not a guarantee against
Android killing processes. Turning off background mode releases protection; it
does not cancel tasks or stop the Termux server. The service uses the `specialUse`
type for user-enabled local agent execution, which requires review before store
distribution.

Use **Settings → Connection** (or long-press the launcher icon
and choose **App settings**) for setup, battery controls and
**Advanced → Connection diagnostics → Share**. Offline setup calls this **Share diagnostics**.
Logs include picker readability/delivery and WebView error
metadata, but exclude file names, URLs, message content and secrets.

The Android document picker grants access only to selected, readable `content:`
URIs. The WebView resource policy allows those exact documents while continuing
to block arbitrary providers, local files and external network resources.

Existing Codex sessions are grouped by their original working directory. A CLI
session started in Termux home appears under the home Project, even if commands
later operate inside a repository. Add that original folder as a Project to find
the session; changing command working directories does not move its history.

Termux API: https://github.com/termux/termux-app/wiki/RUN_COMMAND-Intent

## Shell boundary

Connection is a shell-owned submenu in shared Settings. It reuses the existing
Settings navigation, typography, rows, switches and forms; ordinary browsers and
other app shells keep their existing Settings UI. Selecting a workspace previews
its options; an explicit Connect action verifies and switches the connection.

The Android Web Shell advertises a narrow typed capability through a versioned
user-agent token. Native code transfers a message port only to the main frame of
the selected origin, and checks the visible Activity and exact Connection settings
route on every command. Commands are fixed actions, not arbitrary shell scripts.
Remote workspaces cannot run local setup or change local power preferences; the
user must first switch to this phone, which replaces the remote document and port.
Keys are never returned in settings snapshots. The workspace does not get
an `addJavascriptInterface` object. Pairing itself happens in the offline setup
Activity, because it must work before any workspace is reachable.

Offline onboarding and connection recovery retain a separate, non-exported Activity
because shared Settings requires a reachable workspace. Its JavaScript interface
is present **only** in the offline setup WebView, which serves three exact APK-owned
assets, rejects all other resources/navigation, disables file/provider access, and
applies a CSP that forbids network requests, frames and inline scripts. The camera
scanner is a separate native screen that returns only the code's text. See
[Android WebView bridge security guidance](https://developer.android.com/privacy-and-security/risks/insecure-webview-native-bridges).

## Device validation

System Back gives the shared UI first refusal: dismiss the top popup or drawer,
return from a Settings section to its index, then return to the workspace. Closing
Settings does not push another copy of the previous workspace into history. Android
13+ uses the platform Back callback; older versions retain the Activity callback.
The left-edge swipe accepts starts within 40 CSS pixels and leaves vertical scrolling
and zooming to the browser. Touch handling is set on the actual scrolling surfaces,
not only their parent shell. System-reserved edge gestures remain Android-owned.

With a debug APK, run `node apps/android/scripts/check-navigation-device.mjs` after
forwarding WebView DevTools to port 9222. It checks real Android Back presses, the
Settings hierarchy, repeated renderer touch swipes and the wide navigation layout.
Keep OpenAIDE unlocked and visible. `--renderer-only` avoids all system key input
and checks the renderer Back event instead; it is not a physical Back-button test.

`DeviceChecks` exercises native status reads from the local App Server, the gateway's
refusal of requests without the shell's cookie, the real Termux PendingIntent callback, selected-image result delivery,
Android wake-lock release and offline setup network isolation. Build the instrumentation APK and run:

```sh
adb shell am instrument -w -r -e class io.openaide.android.DeviceChecks \
  io.openaide.android.test/android.test.InstrumentationTestRunner
```

The checks require an initialized local installation; they do not submit paid agent
prompts. All generated artifacts remain ignored.

For power validation, test both active and idle states, lock/unlock, forced Doze,
launcher crashes, opening without a terminal UI, and restored connectivity. Restore
`dumpsys deviceidle unforce` and `dumpsys battery reset` after forced-idle tests.
A short successful test is not proof of overnight survival or a battery benchmark.

Run `node --test apps/android/scripts/setup-ui.test.mjs` for guided setup interactions,
pairing screens and error rendering, and the frontend `ConnectionSettings`
and `androidConnectionSettings` tests for default/Android composition. Verify the
rendered workspace and setup at narrow/wide sizes in light/dark themes on an unlocked
device before release. A passing DOM or instrumentation test is not visual approval.

With a debug APK and the local workspace open, forward the app's WebView DevTools
socket to port 9222 and run `node apps/android/scripts/check-settings-device.mjs`.
It opens the shared Connection submenu, changes and restores the native background
preference, and verifies the workspace document survives without opening a separate
Activity or receiving the offline setup JavaScript interface. The script restores
the previous workspace route and does not send agent messages.
