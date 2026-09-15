set -eu
umask 077
state="$HOME/.local/share/openaide-android/state"
test -f "$state/start.sh"
test -f "$state/connection-password"
mkdir -p "$HOME/.termux/boot"
cat > "$HOME/.termux/boot/openaide" <<'BOOT'
#!/data/data/com.termux/files/usr/bin/bash
state="$HOME/.local/share/openaide-android/state"
export OPENAIDE_WEB_PASSWORD="$(cat "$state/connection-password")"
exec bash "$state/start.sh"
BOOT
chmod 700 "$HOME/.termux/boot/openaide"
printf 'Boot startup configured. Open Termux:Boot once to enable it.\n'
