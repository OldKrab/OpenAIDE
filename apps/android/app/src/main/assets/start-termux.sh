set -eu
: "${OPENAIDE_WEB_PASSWORD:?OpenAIDE must provide a connection password}"
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
export OPENAIDE_ANDROID_RUNTIME="$runtime"
if command -v runsv >/dev/null 2>&1; then
    service="$state/service"
    mkdir -p "$service"
    cat > "$service/run" <<'RUN'
#!/data/data/com.termux/files/usr/bin/sh
cd "$OPENAIDE_ANDROID_RUNTIME"
date +%s > "$OPENAIDE_WEB_STATE_ROOT/service/started"
exec node apps/web/src/dev-server.mjs
RUN
    cat > "$service/finish" <<'FINISH'
#!/data/data/com.termux/files/usr/bin/sh
service="$OPENAIDE_WEB_STATE_ROOT/service"
count=$(cat "$service/failures" 2>/dev/null || echo 0)
started=$(cat "$service/started" 2>/dev/null || echo 0)
if [ $(( $(date +%s) - started )) -gt 60 ]; then count=0; fi
count=$((count + 1))
printf '%s' "$count" > "$service/failures"
if [ "$count" -ge 5 ]; then
    touch "$service/down"
    sv -w 1 down "$service" >/dev/null 2>&1 || true
fi
sleep 10
FINISH
    chmod 700 "$service/run" "$service/finish"
    exec 9> "$state/supervisor.lock"
    if ! flock -n 9; then
        rm -f "$service/down" "$service/failures"
        sv up "$service"
        exit 0
    fi
    rm -f "$service/down" "$service/failures"
    printf '%s' "$OPENAIDE_WEB_PASSWORD" > "$state/connection-password"
    exec runsv "$service"
fi
echo 'openaide_android_start outcome=supervisor_missing'
exec node apps/web/src/dev-server.mjs
