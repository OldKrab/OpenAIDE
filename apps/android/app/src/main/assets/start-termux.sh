set -eu
umask 077
export PREFIX="${PREFIX:-/data/data/com.termux/files/usr}"
export PATH="$PREFIX/bin:$PATH"
export TMPDIR="$PREFIX/tmp"
runtime="$HOME/.local/share/openaide-android/runtime"
state="$HOME/.local/share/openaide-android/state"
mkdir -p "$state"
exec >> "$state/launcher.log" 2>&1
echo 'openaide_android_start outcome=started'
test -f "$runtime/apps/web/src/dev-server.mjs" || {
    echo 'openaide_android_start outcome=runtime_missing'
    exit 1
}
test -x "$runtime/bin/openaide-app-server" || {
    echo 'openaide_android_start outcome=backend_missing'
    exit 1
}
export CODEX_PATH="$(command -v codex)"
test -n "$CODEX_PATH" || {
    echo 'openaide_android_start outcome=codex_missing'
    exit 1
}
export OPENAIDE_WEB_HOST=127.0.0.1
export OPENAIDE_WEB_PORT=5474
export OPENAIDE_WEB_ALLOWED_HOSTS=127.0.0.1
export OPENAIDE_WEB_USERNAME=android
export OPENAIDE_WEB_AUTH_REALM=OpenAIDE
export OPENAIDE_WEB_STATE_ROOT="$state"
export OPENAIDE_WEB_RUNTIME_ROOT="$state/runtime"
export OPENAIDE_WEB_STATIC_ROOT="$runtime/packages/frontend/dist"
export OPENAIDE_APP_SERVER_PATH="$runtime/bin/openaide-app-server"
export OPENAIDE_WEB_PROJECT_ROOTS="$HOME"
cd "$runtime"
exec node apps/web/src/dev-server.mjs
