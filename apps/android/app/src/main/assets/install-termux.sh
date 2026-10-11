set -eu
umask 077
export PREFIX="${PREFIX:-/data/data/com.termux/files/usr}"
export PATH="$PREFIX/bin:$PATH"
pkg install -y nodejs-lts git curl >/dev/null 2>&1
# An agent the user already installed, Codex or Claude, is left as it is.
if ! command -v codex >/dev/null && ! command -v claude >/dev/null; then
    npm install -g @mmmbuto/codex-cli-termux@0.153.3 >/dev/null 2>&1
fi
if [ -n "${OPENAIDE_RUNTIME_URL:-}" ]; then
    case "$OPENAIDE_RUNTIME_URL" in https://*) ;; *) exit 1 ;; esac
    test "${#OPENAIDE_RUNTIME_SHA256}" = 64
    root="$HOME/.local/share/openaide-android"
    mkdir -p "$root"
    stage=$(mktemp -d "$root/install.XXXXXX")
    trap 'rm -rf "$stage"' EXIT
    curl --fail --location --proto '=https' --proto-redir '=https' --max-time 120 --max-filesize 536870912 \
        --output "$stage/runtime.tar.gz" "$OPENAIDE_RUNTIME_URL" >/dev/null 2>&1
    printf '%s  %s\n' "$OPENAIDE_RUNTIME_SHA256" "$stage/runtime.tar.gz" | sha256sum -c - >/dev/null
    tar -tzf "$stage/runtime.tar.gz" > "$stage/entries"
    if grep -E '(^/|(^|/)\.\.(/|$))' "$stage/entries" >/dev/null; then exit 1; fi
    if grep -vE '^runtime(/|$)' "$stage/entries" >/dev/null; then exit 1; fi
    if tar -tvzf "$stage/runtime.tar.gz" | grep -vE '^[-d]' >/dev/null; then exit 1; fi
    tar -xzf "$stage/runtime.tar.gz" -C "$stage" --no-same-owner
    test -x "$stage/runtime/bin/openaide-app-server"
    test -s "$stage/runtime/VERSION"
    if [ -d "$root/runtime" ]; then
        # The runtime and the app are one version. Nothing may still run from the
        # old files, so a server left from before the update ends here; its tasks
        # and history stay in the state directory.
        pkill -TERM -f "runsv $root/state/service" >/dev/null 2>&1 || true
        pkill -TERM -f "$root/runtime/bin/openaide-app-server" >/dev/null 2>&1 || true
        waited=0
        while pgrep -f "$root/runtime/bin/openaide-app-server" >/dev/null 2>&1 && [ "$waited" -lt 50 ]; do
            waited=$((waited + 1))
            sleep 0.2
        done
        rm -rf "$root/runtime.previous" "$root/runtime.pending"
        mv "$root/runtime" "$root/runtime.previous"
        mv "$stage/runtime" "$root/runtime"
        rm -rf "$root/runtime.previous"
    else
        mv "$stage/runtime" "$root/runtime"
    fi
fi
printf 'Installation complete. Run setup checks again.\n'
