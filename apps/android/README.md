# OpenAIDE Android

The Android app can connect to a remote HTTPS OpenAIDE Web Shell or start the existing Web Shell in Termux through the
`com.termux.RUN_COMMAND` service and renders the shared Frontend in a WebView.
The Rust App Server, ACP integration, projects and Codex authentication stay in
Termux in local mode. Remote mode keeps execution, projects and authentication on the selected server.
Neither mode embeds a standalone execution environment.

## Connections and background work

Choose **This phone** or **Remote computer** once. Connection settings are
available under **Settings → Connection**, with a launcher shortcut
and notification action as recovery paths. There is no persistent native footer. The last connection is
remembered; changing it never migrates or resubmits tasks. Save unsent drafts before
switching. Local work retains its background protection when switching to remote.

Offline, full-screen setup guides Termux installation, one-time command access,
automatic compatibility checks, workspace installation and agent sign-in. It uses
OpenAIDE styling, large touch targets, inline progress/errors and system light/dark appearance,
not a list of Android debug dialogs. **Advanced** contains pairing recovery,
optional reboot startup and diagnostics. Android permission grants,
initial Termux initialization and agent sign-in still require user interaction.
Dependency installation uses the pinned Android-compatible Codex package only when
Codex is missing; it does not replace an existing user installation.

Remote setup requires an HTTPS origin, username and password. It accepts an address
directly or from a QR image; scanning does not authorize or connect automatically.
Remote credentials use Android Keystore encryption. WebView resources and credentials
remain scoped to the selected origin; invalid TLS certificates are not bypassed.
The server must have the mobile status endpoint from this revision, enabled Web
authentication, and its public hostname in `OPENAIDE_WEB_ALLOWED_HOSTS`. An HTTPS
reverse proxy must forward the original Host and `X-Forwarded-Proto: https`.
Tailscale Serve is one private-network option. Do not expose an unauthenticated port.

Local startup reuses a healthy server. With `termux-services` installed, an app-owned
`runsv` supervisor remains a Termux background task, restarts a crashed Web launcher,
and stops after five rapid failures. Reopening OpenAIDE offers another attempt.
It does not supervise unrelated Termux sessions or replay agent commands.
Optional Termux:Boot configuration starts the runtime after reboot without an idle
wake lock; install the compatible plugin and open it once first.

The Android service reads authenticated, metadata-only task counts independently of
WebView. Active local work holds a bounded, renewed wake lock; idle work releases it.
When the view is hidden and local work becomes idle, monitoring stops. A short
settling period protects a send immediately followed by screen lock. Temporary
status failures have a two-minute recovery grace, followed by a visible recovery
notification rather than an unbounded wake lock. End-of-work/attention notifications
are local-only; remote push notifications are not implemented.

Hidden WebView timers pause; returning resumes the existing transport and draft.
Idle foreground checks run less frequently, status reads coalesce, and ordinary
browser console output is not copied into Android diagnostics. Background safety
does not depend on rendering the chat or keeping its WebView awake.

The installer obtains the runtime URL and SHA-256 automatically from the fixed
`android-v0.4.0` release in `OldKrab/OpenAIDE`, checks the exact artifact URL and digest,
and verifies the archive before extraction. Users do not enter checksums or package lists.
**Release prerequisite:** that public Android release and its `openaide-termux-arm64.tar.gz`
asset must be published before fresh-phone one-tap installation works. Until published,
the app reports an unavailable download without installing unverified content.
An existing runtime is never overwritten: updates are verified and placed in
`~/.local/share/openaide-android/runtime.pending`. Applying staged updates remains
manual; do not replace a live runtime or delete state to repair a connection.

## Install

Use Android 8 or newer and Termux 0.118 or newer with Node.js, npm, Git and a
working Android-compatible `codex` command. The current managed ACP integration
pins Codex 0.153.3; use a compatible Termux build. Authenticate Codex in Termux.
The runtime artifact currently targets ARM64 phones only.

1. Build the **Android APK and Termux runtime** GitHub Actions workflow.
2. Download and unzip both artifacts. Install `app-debug.apk` on the phone.
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

The APK sends its bundled startup script through Intent stdin; it does not need
access to Termux's private directory. A random per-install password stays in the
APK's private preferences and is passed to the Termux process at startup. The
Web Shell binds only to `127.0.0.1:5474`, using existing HTTP Basic authentication.
No agent credential is copied into the APK. App backup is disabled.

Reconnect reuses the running server when its credentials match. An authentication
failure automatically attempts to recover the saved local pairing and verifies it
before storing it. In **Settings → Connection**, use **Advanced → Reconnect Termux → Reconnect**
for manual recovery. Offline setup calls this **Advanced → Reconnect to Termux**.
Older runtimes without a saved pairing need manual recovery. Android may kill Termux processes;
return to OpenAIDE to reconnect after a failure. Closing the
Android view leaves the Termux Web Shell running, following existing Web Shell
lifetime semantics. To stop it explicitly, stop its process in Termux.

Logs: `~/.local/share/openaide-android/state/launcher.log`. State is kept beside
the runtime directory so replacing runtime files does not delete task history.

## Build and validation

Use JDK 17, Gradle 8.13 and Android SDK 35:

```sh
gradle -p apps/android testDebugUnitTest lintDebug assembleDebug
```

CI separately cross-compiles the App Server with the Android NDK, builds the
shared Frontend, runs existing Web Shell tests and packages the Termux runtime.
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

For installable updates, configure repository secrets
`OPENAIDE_ANDROID_DEBUG_KEYSTORE` (base64-encoded PKCS12, alias `openaide-debug`)
and `OPENAIDE_ANDROID_DEBUG_KEYSTORE_PASSWORD`. Keep the signing key private and
backed up. Without these secrets, CI uses a temporary debug key and subsequent
APKs may require uninstalling the previous app. Switching from the initial
temporary key requires a one-time reinstall; Termux task history is unaffected,
but the app's connection password resets.

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
Passwords are never returned in settings snapshots. The workspace does not get
an `addJavascriptInterface` object.

Offline onboarding and connection recovery retain a separate, non-exported Activity
because shared Settings requires a reachable workspace. Its JavaScript interface
is present **only** in the offline setup WebView, which serves three exact APK-owned
assets, rejects all other resources/navigation, disables file/provider access, and
applies a CSP that forbids network requests, frames and inline scripts. See
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

`DeviceChecks` exercises authenticated native status reads, wrong-password rejection,
the real Termux PendingIntent callback, selected-image result delivery, QR image decoding,
Android wake-lock release and offline setup network isolation. Build the instrumentation APK and run:

```sh
adb shell am instrument -w -r -e class io.openaide.android.DeviceChecks \
  io.openaide.android.test/android.test.InstrumentationTestRunner
```

The checks require an initialized local installation; they do not submit paid agent
prompts. `scripts/build-on-termux.sh` and `scripts/build-device-tests.sh` also support
direct device-side debug builds with the Android SDK jars, Termux AAPT2, D8, ZXing
and the existing signing key. All generated artifacts remain ignored.
Set `OPENAIDE_ANDROID_BUILD_TYPE=release` for the device-side script to produce
`app-release.apk` without WebView debugging or debuggable application access.

For power validation, test both active and idle states, lock/unlock, forced Doze,
launcher crashes, opening without a terminal UI, and restored connectivity. Restore
`dumpsys deviceidle unforce` and `dumpsys battery reset` after forced-idle tests.
A short successful test is not proof of overnight survival or a battery benchmark.

Run `node --test apps/android/scripts/setup-ui.test.mjs` for guided setup interactions,
remote form preservation and error rendering, and the frontend `ConnectionSettings`
and `androidConnectionSettings` tests for default/Android composition. Verify the
rendered workspace and setup at narrow/wide sizes in light/dark themes on an unlocked
device before release. A passing DOM or instrumentation test is not visual approval.

With a debug APK and the local workspace open, forward the app's WebView DevTools
socket to port 9222 and run `node apps/android/scripts/check-settings-device.mjs`.
It opens the shared Connection submenu, changes and restores the native background
preference, and verifies the workspace document survives without opening a separate
Activity or receiving the offline setup JavaScript interface. The script restores
the previous workspace route and does not send agent messages.
