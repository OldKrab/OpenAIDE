# OpenAIDE Android

The Android app starts the existing OpenAIDE Web Shell in Termux through the
`com.termux.RUN_COMMAND` service and renders the shared Frontend in a WebView.
The Rust App Server, ACP integration, projects and Codex authentication stay in
Termux. This is an initial Android shell, not a standalone execution environment.

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
5. Open OpenAIDE, tap **Connect to Termux**, and grant the command permission.
   If Android no longer prompts, use **App permissions** to grant it manually.

The APK sends its bundled startup script through Intent stdin; it does not need
access to Termux's private directory. A random per-install password stays in the
APK's private preferences and is passed to the Termux process at startup. The
Web Shell binds only to `127.0.0.1:5474`, using existing HTTP Basic authentication.
No agent credential is copied into the APK. App backup is disabled.

Reconnect reuses the running server when its credentials match. A server using
different credentials must be stopped before connecting. Clearing app data or
reinstalling the APK resets its password. Android may kill Termux processes;
return to Connection and reconnect to restart after a failure. Closing the
Android view leaves the Termux Web Shell running, following existing Web Shell
lifetime semantics. To stop it explicitly, stop its process in Termux.

Logs: `~/.local/share/openaide-android/state/launcher.log`. State is kept beside
the runtime directory so replacing runtime files does not delete task history.

## Build and validation

Use JDK 17, Gradle 8.13 and Android SDK 35:

```sh
gradle -p apps/android lintDebug assembleDebug
```

CI separately cross-compiles the App Server with the Android NDK, builds the
shared Frontend, runs existing Web Shell tests and packages the Termux runtime.
The APK is debug signed for testing, not a store release.

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

Native notifications, support-export downloads and automatic runtime updates
are not implemented in this first shell. Shared task and settings behavior stays
in the existing Frontend and App Server.

Android Back navigates page history, then returns to the connection screen. This
keeps native connection controls out of the shared task interface. The connection
screen includes **Share connection diagnostics** for native picker outcomes and
WebView error metadata; it excludes file names, URLs, message content and secrets.

Existing Codex sessions are grouped by their original working directory. A CLI
session started in Termux home appears under the home Project, even if commands
later operate inside a repository. Add that original folder as a Project to find
the session; changing command working directories does not move its history.

Termux API: https://github.com/termux/termux-app/wiki/RUN_COMMAND-Intent
