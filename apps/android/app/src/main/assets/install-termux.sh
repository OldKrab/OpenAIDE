set -eu
umask 077
export PREFIX="${PREFIX:-/data/data/com.termux/files/usr}"
export PATH="$PREFIX/bin:$PATH"
pkg install -y nodejs-lts git curl ripgrep >/dev/null 2>&1
root="$HOME/.local/share/openaide-android"
mkdir -p "$root"

# Claude Code ships only as a glibc binary and Termux is not glibc. Termux's own
# glibc runner makes that binary start here. The binary is the one the Claude
# adapter of this app version was built against, taken from the adapter's package.
# Names the step that stopped the Claude install, so a phone that cannot run it can be diagnosed.
claude_failed() { echo "openaide_android_install claude=unavailable step=$1" >> "$root/install.log"; return 1; }
install_claude() {
    test -n "${OPENAIDE_CLAUDE_ACP_VERSION:-}" || { claude_failed adapter_version; return 1; }
    sdk=$(npm view "@openaide/claude-agent-acp@$OPENAIDE_CLAUDE_ACP_VERSION" 'dependencies.@anthropic-ai/claude-agent-sdk' 2>/dev/null)
    case "$sdk" in ''|*[!0-9.]*) claude_failed sdk_version; return 1 ;; esac
    pkg install -y glibc-repo >/dev/null 2>&1 || { claude_failed glibc_repo; return 1; }
    pkg install -y glibc-runner patchelf-glibc >/dev/null 2>&1 || { claude_failed glibc_runner; return 1; }
    claude_stage=$(mktemp -d "$root/claude.XXXXXX") || { claude_failed stage; return 1; }
    # npm refuses a linux package on Android unless forced; only its one file is used.
    npm install --prefix "$claude_stage" --force --no-save --no-audit --no-fund \
        "@anthropic-ai/claude-agent-sdk-linux-arm64@$sdk" >/dev/null 2>&1 || { rm -rf "$claude_stage"; claude_failed download; return 1; }
    binary="$root/agents/claude/claude-$sdk"
    mkdir -p "$root/agents/claude"
    mv "$claude_stage/node_modules/@anthropic-ai/claude-agent-sdk-linux-arm64/claude" "$binary" || { rm -rf "$claude_stage"; claude_failed binary_missing; return 1; }
    rm -rf "$claude_stage"
    chmod 700 "$binary"
    # Termux preloads a library built for Android's libc; a glibc program must start without it.
    (unset LD_PRELOAD; grun --set "$binary") >/dev/null 2>&1 || { rm -f "$binary"; claude_failed interpreter; return 1; }
    # The binary looks for resolvers in a file Termux does not have.
    printf '%s\n' "require('node:dns').setServers(['1.1.1.1', '8.8.8.8']);" > "$root/agents/claude/dns.js"
    cat > "$PREFIX/bin/claude" <<WRAPPER
#!$PREFIX/bin/sh
# Installed by OpenAIDE: starts Claude Code through Termux's glibc runner.
unset LD_PRELOAD
export DISABLE_AUTOUPDATER=1
export BUN_OPTIONS="--preload=$root/agents/claude/dns.js\${BUN_OPTIONS:+ \$BUN_OPTIONS}"
exec "$binary" "\$@"
WRAPPER
    chmod 700 "$PREFIX/bin/claude"
    # A phone whose kernel or glibc runner cannot start it keeps working with Codex.
    if ! timeout 60 claude --version >/dev/null 2>&1; then
        rm -f "$PREFIX/bin/claude" "$binary"
        claude_failed start
        return 1
    fi
    echo "openaide_android_install claude=installed sdk=$sdk" >> "$root/install.log"
}

# Neither agent publishes an Android build. An agent the user already installed is left as it is.
if ! command -v codex >/dev/null; then
    npm install -g @mmmbuto/codex-cli-termux@0.153.3 >/dev/null 2>&1 || true
fi
if ! command -v claude >/dev/null; then
    install_claude || true
fi
command -v codex >/dev/null || command -v claude >/dev/null
if [ -n "${OPENAIDE_RUNTIME_URL:-}" ]; then
    case "$OPENAIDE_RUNTIME_URL" in https://*) ;; *) exit 1 ;; esac
    test "${#OPENAIDE_RUNTIME_SHA256}" = 64
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
