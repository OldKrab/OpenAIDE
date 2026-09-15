set -eu
umask 077
export PREFIX="${PREFIX:-/data/data/com.termux/files/usr}"
export PATH="$PREFIX/bin:$PATH"
pkg install -y nodejs-lts git curl termux-services >/dev/null 2>&1
if ! command -v codex >/dev/null; then
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
    test -f "$stage/runtime/apps/web/src/dev-server.mjs"
    if [ -d "$root/runtime" ]; then
        test ! -e "$root/runtime.pending"
        mv "$stage/runtime" "$root/runtime.pending"
        printf 'Update verified and staged. Apply after all local tasks are idle.\n'
        exit 0
    fi
    mv "$stage/runtime" "$root/runtime"
fi
printf 'Installation complete. Run setup checks again.\n'
