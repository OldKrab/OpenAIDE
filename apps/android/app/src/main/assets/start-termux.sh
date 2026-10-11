# Starts the App Server the way an App Shell on a computer does: attach to the
# one already serving this state, or launch it, and print its connection line.
# The server stops by itself once its last client has left.
set -eu
umask 077
export PREFIX="${PREFIX:-/data/data/com.termux/files/usr}"
export PATH="$PREFIX/bin:$PATH"
export TMPDIR="$PREFIX/tmp"
root="$HOME/.local/share/openaide-android"
runtime="$root/runtime"
state="$root/state"
mkdir -p "$state"
log="$state/launcher.log"
note() { echo "openaide_android_start outcome=$1" >> "$log"; }
note started
test -x "$runtime/bin/openaide-app-server" || { note backend_missing; exit 1; }
test "$(cat "$runtime/VERSION" 2>/dev/null)" = "${OPENAIDE_VERSION:-}" || { note runtime_outdated; exit 1; }
if [ -d "$state/service" ]; then
    # Earlier versions kept a supervised Web Shell here. It would hold the old
    # App Server open, so it ends before this one starts.
    pkill -TERM -f "runsv $state/service" >/dev/null 2>&1 || true
    pkill -TERM -f "$runtime/bin/openaide-app-server" >/dev/null 2>&1 || true
    rm -rf "$state/service" "$state/connection-password" "$state/start.sh" "$state/supervisor.lock"
    rm -f "$HOME/.termux/boot/openaide"
    sleep 1
    note legacy_stopped
fi
# Agents installed in Termux are used as they are; the adapters cannot ship Android builds.
if command -v codex >/dev/null 2>&1; then export CODEX_PATH="$(command -v codex)"; fi
if command -v claude >/dev/null 2>&1; then export CLAUDE_CODE_EXECUTABLE="$(command -v claude)"; fi
export OPENAIDE_STORAGE_ROOT="$state"
export OPENAIDE_RUNTIME_ROOT="$state/runtime"
export OPENAIDE_PROJECT_ROOTS="$HOME"
export OPENAIDE_APP_SERVER_PROTOCOL=app-server-handoff
handoff=$(mktemp "$state/handoff.XXXXXX")
trap 'rm -f "$handoff"' EXIT
cd "$HOME"
nohup "$runtime/bin/openaide-app-server" < /dev/null > "$handoff" 2>> "$log" &
child=$!
waited=0
while ! grep -q '}' "$handoff" 2>/dev/null; do
    if ! kill -0 "$child" 2>/dev/null && ! grep -q '}' "$handoff" 2>/dev/null; then note handoff_exited; exit 1; fi
    if [ "$waited" -ge 300 ]; then note handoff_timeout; kill "$child" 2>/dev/null || true; exit 1; fi
    waited=$((waited + 1))
    sleep 0.2
done
note ready
head -n 1 "$handoff"
