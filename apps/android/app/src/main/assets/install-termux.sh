set -eu
umask 077
export PREFIX="${PREFIX:-/data/data/com.termux/files/usr}"
export PATH="$PREFIX/bin:$PATH"
pkg install -y nodejs-lts git curl ripgrep >/dev/null 2>&1
root="$HOME/.local/share/${OPENAIDE_DATA_NAME:-openaide-android}"
mkdir -p "$root"

# Claude Code ships only as a glibc binary and Termux is not glibc. Termux's glibc
# packages can run it once the binary names their loader. The binary is the one the
# Claude adapter of this app version was built against, taken from the adapter's
# package, and lives outside the app's data folder because the command is Termux-wide.
claude_home="$PREFIX/opt/openaide-claude"
# Names the step that stopped the Claude install, so a phone that cannot run it can be diagnosed.
claude_failed() { echo "openaide_android_install claude=unavailable step=$1" >> "$root/install.log"; return 1; }
install_claude() {
    test -n "${OPENAIDE_CLAUDE_ACP_VERSION:-}" || { claude_failed adapter_version; return 1; }
    sdk=$(npm view "@openaide/claude-agent-acp@$OPENAIDE_CLAUDE_ACP_VERSION" 'dependencies.@anthropic-ai/claude-agent-sdk' 2>/dev/null)
    case "$sdk" in ''|*[!0-9.]*) claude_failed sdk_version; return 1 ;; esac
    pkg install -y glibc-repo >/dev/null 2>&1 || { claude_failed glibc_repo; return 1; }
    pkg install -y glibc-runner termux-exec-glibc >/dev/null 2>&1 || { claude_failed glibc_packages; return 1; }
    loader="$PREFIX/glibc/lib/ld-linux-aarch64.so.1"
    test -x "$loader" -a -f "$PREFIX/glibc/lib/libtermux-exec.so" || { claude_failed glibc_files; return 1; }
    mkdir -p "$claude_home"
    claude_stage=$(mktemp -d "$claude_home/stage.XXXXXX") || { claude_failed stage; return 1; }
    # npm refuses a linux package on Android unless forced; only its one file is used.
    npm install --prefix "$claude_stage" --force --no-save --no-audit --no-fund \
        "@anthropic-ai/claude-agent-sdk-linux-arm64@$sdk" >/dev/null 2>&1 || { rm -rf "$claude_stage"; claude_failed download; return 1; }
    binary="$claude_home/claude-$sdk"
    mv "$claude_stage/node_modules/@anthropic-ai/claude-agent-sdk-linux-arm64/claude" "$binary" || { rm -rf "$claude_stage"; claude_failed binary_missing; return 1; }
    rm -rf "$claude_stage"
    chmod 700 "$binary"
    # The binary must run as itself, not as an argument of the loader: its built-in
    # search runs the binary again by its own path. Tools that rewrite the loader path
    # move the file's contents, which this binary does not survive, so the path is
    # written into unused padding of a loaded read-only segment instead.
    node - "$binary" "$loader" >/dev/null 2>&1 <<'ELF_LOADER_PATH' || { rm -f "$binary"; claude_failed loader_path; return 1; }
const fs = require('node:fs');
const [file, loader] = process.argv.slice(2);
const fd = fs.openSync(file, 'r+');
const read = (offset, length) => { const bytes = Buffer.alloc(length); fs.readSync(fd, bytes, 0, length, offset); return bytes; };
const head = read(0, 64);
if (head.readUInt32BE(0) !== 0x7f454c46 || head[4] !== 2 || head[5] !== 1) throw new Error('not a 64-bit little-endian ELF');
const tableOffset = Number(head.readBigUInt64LE(0x20)), entrySize = head.readUInt16LE(0x36), count = head.readUInt16LE(0x38);
const table = read(tableOffset, entrySize * count);
const headers = Array.from({ length: count }, (_, index) => {
  const at = index * entrySize;
  return { at: tableOffset + at, type: table.readUInt32LE(at), flags: table.readUInt32LE(at + 4), offset: Number(table.readBigUInt64LE(at + 8)),
    address: Number(table.readBigUInt64LE(at + 16)), fileSize: Number(table.readBigUInt64LE(at + 32)), memorySize: Number(table.readBigUInt64LE(at + 40)) };
});
const interpreter = headers.find(header => header.type === 3);
if (!interpreter) throw new Error('no loader path to replace');
const text = Buffer.from(loader + '\0');
const size = fs.fstatSync(fd).size;
// A segment is mapped in whole pages, so the bytes after its end up to the next
// 4 KiB boundary are in memory too; the loader reads its own path from there.
for (const segment of headers.filter(header => header.type === 1 && header.fileSize === header.memorySize && !(header.flags & 2))) {
  const start = segment.offset + segment.fileSize, end = Math.min(Math.ceil(start / 4096) * 4096, size);
  if (end - start < text.length) continue;
  if (headers.some(other => other !== segment && other.fileSize && other.offset < start + text.length && start < other.offset + other.fileSize)) continue;
  if (read(start, text.length).some(byte => byte !== 0)) continue;
  fs.writeSync(fd, text, 0, text.length, start);
  const entry = Buffer.alloc(40);
  entry.writeBigUInt64LE(BigInt(start), 0);
  entry.writeBigUInt64LE(BigInt(segment.address + segment.fileSize), 8);
  entry.writeBigUInt64LE(BigInt(segment.address + segment.fileSize), 16);
  entry.writeBigUInt64LE(BigInt(text.length), 24);
  entry.writeBigUInt64LE(BigInt(text.length), 32);
  fs.writeSync(fd, entry, 0, 40, interpreter.at + 8);
  fs.closeSync(fd);
  process.exit(0);
}
throw new Error('no room for the loader path');
ELF_LOADER_PATH
    # Termux makes "#!/usr/bin/env" scripts work with a preloaded library, and has one
    # for each libc. Claude is glibc and the shells and tools it starts are Android
    # programs, so both must find theirs under one setting: glibc replaces $LIB in the
    # path with "lib" or "lib64", and Android's linker reads the name as it is.
    rm -rf "$claude_home/preload"
    mkdir -p "$claude_home/preload/\$LIB" "$claude_home/preload/lib" "$claude_home/preload/lib64"
    ln -s "$PREFIX/glibc/lib/libtermux-exec.so" "$claude_home/preload/lib/exec.so"
    ln -s "$PREFIX/glibc/lib/libtermux-exec.so" "$claude_home/preload/lib64/exec.so"
    preload="unset LD_PRELOAD"
    for library in libtermux-exec-ld-preload.so libtermux-exec.so; do
        if [ -f "$PREFIX/lib/$library" ]; then
            ln -s "$PREFIX/lib/$library" "$claude_home/preload/\$LIB/exec.so"
            preload="export LD_PRELOAD='$claude_home/preload/\$LIB/exec.so'"
            break
        fi
    done
    cat > "$PREFIX/bin/claude" <<WRAPPER
#!$PREFIX/bin/sh
# Installed by OpenAIDE: Claude Code as published, started with Termux's glibc.
$preload
# An update would replace the binary with one that cannot start here.
export DISABLE_AUTOUPDATER=1
exec "$binary" "\$@"
WRAPPER
    chmod 700 "$PREFIX/bin/claude"
    # A phone whose kernel or glibc cannot start it keeps working with Codex.
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
