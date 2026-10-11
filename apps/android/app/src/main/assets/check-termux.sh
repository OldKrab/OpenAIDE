set -eu
export PREFIX="${PREFIX:-/data/data/com.termux/files/usr}"
export PATH="$PREFIX/bin:$PATH"
runtime="$HOME/.local/share/openaide-android/runtime"
has() { "$@" >/dev/null 2>&1; }
flag() { if "$@" >/dev/null 2>&1; then printf 'true'; else printf 'false'; fi; }
codex_ready() { codex --version | grep -q "0\.153\.3"; }
claude_signed_in() { test -s "$HOME/.claude/.credentials.json" || test -n "${ANTHROPIC_API_KEY:-}"; }
# Either agent is enough. Codex must be the Android-compatible build; Claude must start.
agent=false; version=false; authenticated=false; signin='codex login'
if has timeout 20 claude --version; then
    agent=true; version=true; signin='claude'
    if has claude_signed_in; then authenticated=true; fi
fi
if has command -v codex; then
    agent=true
    if has codex_ready; then version=true; fi
    if [ "$authenticated" = false ]; then
        signin='codex login'
        if has timeout 15 codex login status; then authenticated=true; fi
    fi
fi
printf '{"node":%s,' "$(flag command -v node)"
printf '"git":%s,' "$(flag command -v git)"
printf '"npm":%s,' "$(flag command -v npm)"
printf '"nodeVersion":%s,' "$(flag node -e 'process.exit(Number(process.versions.node.split(".")[0]) >= 22 ? 0 : 1)')"
printf '"agent":%s,"agentVersion":%s,"authenticated":%s,"signin":"%s",' "$agent" "$version" "$authenticated" "$signin"
printf '"runtime":%s,' "$(flag test -x "$runtime/bin/openaide-app-server" -a "$(cat "$runtime/VERSION" 2>/dev/null)" = "${OPENAIDE_VERSION:-}")"
printf '"storage":%s,' "$(flag test -w "$HOME")"
printf '"arm64":%s,' "$(flag test "$(uname -m)" = aarch64)"
printf '"space":%s}\n' "$(flag test "$(df -Pk "$HOME" | awk 'END {print $4}')" -gt 524288)"
