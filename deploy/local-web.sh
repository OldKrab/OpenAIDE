#!/usr/bin/env bash
set -euo pipefail

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
env_file="$repo_root/deploy/local-web.env"
env_override_names=(
  OPENAIDE_WEB_ROLE
  OPENAIDE_WEB_INSTANCE_LABEL
  OPENAIDE_WEB_TITLE
  OPENAIDE_WEB_HOST
  OPENAIDE_WEB_PORT
  OPENAIDE_WEB_VITE_PORT
  OPENAIDE_WEB_PROTOTYPE_PORT
  OPENAIDE_WEB_STATE_ROOT
  OPENAIDE_WEB_RUNTIME_ROOT
  OPENAIDE_WEB_STATIC_ROOT
  OPENAIDE_WEB_PROJECT_ROOTS
  OPENAIDE_WEB_PID_FILE
  OPENAIDE_WEB_LOG_FILE
  OPENAIDE_WEB_ALLOWED_HOSTS
  OPENAIDE_WEB_BUILD
  OPENAIDE_WEB_SKIP_BUILD
  OPENAIDE_WEB_DAEMON
  OPENAIDE_WEB_SYSTEMD_UNIT
  OPENAIDE_WEB_SMOKE
  OPENAIDE_WEB_SMOKE_TIMEOUT_MS
  OPENAIDE_WEB_SMOKE_SEND_TIMEOUT_MS
  OPENAIDE_WEB_SMOKE_PROMPT
  OPENAIDE_WEB_SMOKE_CLEANUP
  OPENAIDE_ACP_TRACE
  OPENAIDE_ACP_TRACE_DIR
)

for name in "${env_override_names[@]}"; do
  if [[ -v "$name" ]]; then
    declare "override_$name=${!name}"
  fi
done

if [[ -f "$env_file" ]]; then
  set -a
  # shellcheck disable=SC1090
  source "$env_file"
  set +a
fi

for name in "${env_override_names[@]}"; do
  override_name="override_$name"
  if [[ -v "$override_name" ]]; then
    declare "$name=${!override_name}"
  fi
done

if [[ -n "${OPENAIDE_WEB_ROLE:-}" ]]; then
  case "$OPENAIDE_WEB_ROLE" in
    *[!A-Za-z0-9_-]*)
      echo "Unsupported OPENAIDE_WEB_ROLE=$OPENAIDE_WEB_ROLE." >&2
      exit 2
      ;;
  esac
  role_env_file="$repo_root/deploy/local-web.$OPENAIDE_WEB_ROLE.env"
  if [[ -f "$role_env_file" ]]; then
    set -a
    # shellcheck disable=SC1090
    source "$role_env_file"
    set +a
  fi

  # Machine-specific role settings stay outside version control and override shared defaults.
  role_local_env_file="$repo_root/deploy/local-web.$OPENAIDE_WEB_ROLE.local.env"
  if [[ -f "$role_local_env_file" ]]; then
    set -a
    # shellcheck disable=SC1090
    source "$role_local_env_file"
    set +a
  fi
fi

command="${1:-refresh}"
host="${OPENAIDE_WEB_HOST:-127.0.0.1}"
port="${OPENAIDE_WEB_PORT:-5474}"
vite_port="${OPENAIDE_WEB_VITE_PORT:-5473}"
prototype_port="${OPENAIDE_WEB_PROTOTYPE_PORT:-}"
state_root="${OPENAIDE_WEB_STATE_ROOT:-$repo_root/.openaide-web-dev/state}"
runtime_root="${OPENAIDE_WEB_RUNTIME_ROOT:-$repo_root/.openaide-web-dev/runtime}"
static_root="${OPENAIDE_WEB_STATIC_ROOT:-$repo_root/.openaide-web-dev/static-$port}"
source_static_root="$repo_root/packages/frontend/dist"
project_roots="${OPENAIDE_WEB_PROJECT_ROOTS:-$repo_root}"
pid_file="${OPENAIDE_WEB_PID_FILE:-/tmp/openaide-local-web-$port.pid}"
log_file="${OPENAIDE_WEB_LOG_FILE:-/tmp/openaide-local-web-$port.log}"
static_root_file="$pid_file.static-root"
lock_file="${OPENAIDE_WEB_LOCK_FILE:-/tmp/openaide-local-web-$port.lock}"
daemon_mode="${OPENAIDE_WEB_DAEMON:-background}"
systemd_unit="${OPENAIDE_WEB_SYSTEMD_UNIT:-openaide-local-web-$port}"
# Keep JavaScript stack traces mapped to their TypeScript sources in every local role.
node_options="${NODE_OPTIONS:-}"
if [[ " $node_options " != *" --enable-source-maps "* ]]; then
  node_options="${node_options:+$node_options }--enable-source-maps"
fi
state_root="$(node -e "console.log(require('node:path').resolve(process.argv[1]))" "$state_root")"
runtime_root="$(node -e "console.log(require('node:path').resolve(process.argv[1]))" "$runtime_root")"
static_root="$(node -e "console.log(require('node:path').resolve(process.argv[1]))" "$static_root")"

usage() {
  cat <<EOF
Usage: OPENAIDE_WEB_ALLOWED_HOSTS=<host>[,<host>...] $0 [refresh|start|stop|restart|status|logs|smoke]

Optional env:
  OPENAIDE_WEB_ROLE=<role> loads deploy/local-web.<role>.env and its ignored .local.env override
  OPENAIDE_WEB_PORT=$port
  OPENAIDE_WEB_VITE_PORT=$vite_port
  OPENAIDE_WEB_PROTOTYPE_PORT=${prototype_port:-<disabled>}
  OPENAIDE_WEB_STATE_ROOT=$state_root
  OPENAIDE_WEB_RUNTIME_ROOT=$runtime_root
  OPENAIDE_WEB_STATIC_ROOT=$static_root
  OPENAIDE_WEB_BUILD=1
  OPENAIDE_WEB_DAEMON=systemd
  OPENAIDE_WEB_SMOKE=1

You can also put these values in deploy/local-web.env.
EOF
}

require_allowed_hosts() {
  if [[ -z "${OPENAIDE_WEB_ALLOWED_HOSTS:-}" ]]; then
    usage >&2
    echo "OPENAIDE_WEB_ALLOWED_HOSTS is required." >&2
    exit 2
  fi
}

canonical_path() {
  node -e "console.log(require('node:path').resolve(process.argv[1]))" "$1"
}

# shellcheck source=deploy/local-web-liveness.sh
source "$repo_root/deploy/local-web-liveness.sh"
# shellcheck source=deploy/local-web-static.sh
source "$repo_root/deploy/local-web-static.sh"
# shellcheck source=deploy/local-web-systemd-restart.sh
source "$repo_root/deploy/local-web-systemd-restart.sh"

require_daemon_mode() {
  case "$daemon_mode" in
    background|systemd)
      ;;
    *)
      echo "Unsupported OPENAIDE_WEB_DAEMON=$daemon_mode. Use background or systemd." >&2
      exit 2
      ;;
  esac
}

require_systemd_user() {
  if ! systemctl --user show-environment >/dev/null 2>&1; then
    echo "OPENAIDE_WEB_DAEMON=systemd requires a working user systemd manager." >&2
    exit 2
  fi
}

with_lifecycle_lock() {
  mkdir -p "$(dirname "$lock_file")"
  exec 9>"$lock_file"
  flock 9
  "$@"
}

stop_server() {
  require_daemon_mode
  if [[ "$daemon_mode" == "systemd" ]]; then
    require_systemd_user
    systemctl --user stop "$systemd_unit.service" >/dev/null 2>&1 || true
    rm -f "$pid_file"
    rm -f "$static_root_file"
    return
  fi

  if [[ -f "$pid_file" ]]; then
    pid="$(cat "$pid_file")"
    if [[ -n "$pid" ]]; then
      kill "$pid" 2>/dev/null || true
    fi
  fi

  while IFS= read -r pid; do
    [[ -n "$pid" ]] && kill "$pid" 2>/dev/null || true
  done < <(OPENAIDE_WEB_PORT="$port" OPENAIDE_WEB_VITE_PORT="$vite_port" listener_pids)

  sleep 1

  while IFS= read -r pid; do
    [[ -n "$pid" ]] && kill -9 "$pid" 2>/dev/null || true
  done < <(OPENAIDE_WEB_PORT="$port" OPENAIDE_WEB_VITE_PORT="$vite_port" listener_pids)

  rm -f "$pid_file"
  rm -f "$static_root_file"
}

build_static_frontend() {
  validate_static_root
  (cd "$repo_root" && npm run build --workspace @openaide/app-server-client)
  (cd "$repo_root" && npm run build --workspace @openaide/app-shell-contracts)
  (cd "$repo_root" && npm run build --workspace openaide-frontend -- --outDir "$static_root" --emptyOutDir)
}

build_if_requested() {
  if [[ "${OPENAIDE_WEB_BUILD:-0}" == "1" ]]; then
    (cd "$repo_root" && npm run app-server:build)
    build_static_frontend
  fi

  if [[ ! -x "$repo_root/target/debug/openaide-app-server" ]]; then
    (cd "$repo_root" && npm run app-server:build)
  fi

  if [[ ! -f "$static_root/index.html" ]]; then
    build_static_frontend
  fi
}

server_running() {
  if [[ "$daemon_mode" == "systemd" ]]; then
    systemctl --user is-active --quiet "$systemd_unit.service"
    return
  fi
  if [[ -f "$pid_file" ]] && kill -0 "$(cat "$pid_file")" 2>/dev/null; then
    return 0
  fi
  owned_web_listener_running
}

running_static_root() {
  if server_running && [[ -f "$static_root_file" ]]; then
    local recorded_static_root
    recorded_static_root="$(head -n 1 "$static_root_file")"
    if [[ -n "$recorded_static_root" ]]; then
      canonical_path "$recorded_static_root"
      return
    fi
  fi
  echo "$static_root"
}

run_smoke_check() {
  node "$repo_root/deploy/local-web-smoke.mjs" "http://$host:$port"
}

smoke_if_requested() {
  if [[ "${OPENAIDE_WEB_SMOKE:-0}" == "1" ]]; then
    run_smoke_check
  fi
}

start_server() {
  require_daemon_mode
  if [[ "$daemon_mode" == "systemd" ]]; then
    start_systemd_server
    return
  fi

  require_allowed_hosts
  validate_static_root
  if [[ "${OPENAIDE_WEB_SKIP_BUILD:-0}" != "1" ]]; then
    build_if_requested
  fi
  mkdir -p "$state_root" "$runtime_root" "$(dirname "$log_file")"
  rm -f "$pid_file"

  local trace_env_args=()
  if [[ -v OPENAIDE_ACP_TRACE ]]; then
    trace_env_args+=(OPENAIDE_ACP_TRACE="$OPENAIDE_ACP_TRACE")
  fi
  if [[ -n "${OPENAIDE_ACP_TRACE_DIR:-}" ]]; then
    trace_env_args+=(OPENAIDE_ACP_TRACE_DIR="$OPENAIDE_ACP_TRACE_DIR")
  fi

  setsid env \
    NODE_OPTIONS="$node_options" \
    OPENAIDE_WEB_INSTANCE_LABEL="${OPENAIDE_WEB_INSTANCE_LABEL:-}" \
    OPENAIDE_WEB_TITLE="${OPENAIDE_WEB_TITLE:-}" \
    OPENAIDE_WEB_HOST="$host" \
    OPENAIDE_WEB_PORT="$port" \
    OPENAIDE_WEB_VITE_PORT="$vite_port" \
    OPENAIDE_WEB_PROTOTYPE_PORT="$prototype_port" \
    OPENAIDE_WEB_ALLOWED_HOSTS="$OPENAIDE_WEB_ALLOWED_HOSTS" \
    OPENAIDE_WEB_STATE_ROOT="$state_root" \
    OPENAIDE_WEB_RUNTIME_ROOT="$runtime_root" \
    OPENAIDE_PROJECT_ROOTS="$project_roots" \
    OPENAIDE_WEB_PROJECT_ROOTS="$project_roots" \
    OPENAIDE_WEB_STATIC_ROOT="$static_root" \
    "${trace_env_args[@]}" \
    npm run web:dev 9>&- > "$log_file" 2>&1 < /dev/null &

  echo "$!" > "$pid_file"
  echo "$static_root" > "$static_root_file"
  sleep 3

  if ! kill -0 "$(cat "$pid_file")" 2>/dev/null && ! owned_web_listener_running; then
    rm -f "$static_root_file"
    cat "$log_file" >&2
    exit 1
  fi

  echo "OpenAIDE local web started on http://$host:$port"
  echo "PID file: $pid_file"
  echo "Log file: $log_file"
  echo "State root: $state_root"
  echo "Static root: $static_root"
  smoke_if_requested
}

start_systemd_server() {
  require_allowed_hosts
  require_systemd_user
  validate_static_root
  if [[ "${OPENAIDE_WEB_SKIP_BUILD:-0}" != "1" ]]; then
    build_if_requested
  fi
  mkdir -p "$state_root" "$runtime_root" "$(dirname "$log_file")"
  : > "$log_file"

  systemctl --user stop "$systemd_unit.service" >/dev/null 2>&1 || true

  local npm_bin
  npm_bin="$(command -v npm)"
  local systemd_trace_env_args=()
  if [[ -v OPENAIDE_ACP_TRACE ]]; then
    systemd_trace_env_args+=(--setenv "OPENAIDE_ACP_TRACE=$OPENAIDE_ACP_TRACE")
  fi
  if [[ -n "${OPENAIDE_ACP_TRACE_DIR:-}" ]]; then
    systemd_trace_env_args+=(--setenv "OPENAIDE_ACP_TRACE_DIR=$OPENAIDE_ACP_TRACE_DIR")
  fi
  systemd-run --user \
    --unit "$systemd_unit" \
    --description "OpenAIDE local web on $host:$port" \
    --working-directory "$repo_root" \
    --property Restart=always \
    --property RestartSec=2 \
    --property "StandardOutput=append:$log_file" \
    --property "StandardError=append:$log_file" \
    --setenv "HOME=$HOME" \
    --setenv "PATH=$PATH" \
    --setenv "NODE_OPTIONS=$node_options" \
    --setenv "OPENAIDE_WEB_INSTANCE_LABEL=${OPENAIDE_WEB_INSTANCE_LABEL:-}" \
    --setenv "OPENAIDE_WEB_TITLE=${OPENAIDE_WEB_TITLE:-}" \
    --setenv "OPENAIDE_WEB_HOST=$host" \
    --setenv "OPENAIDE_WEB_PORT=$port" \
    --setenv "OPENAIDE_WEB_VITE_PORT=$vite_port" \
    --setenv "OPENAIDE_WEB_PROTOTYPE_PORT=$prototype_port" \
    --setenv "OPENAIDE_WEB_ALLOWED_HOSTS=$OPENAIDE_WEB_ALLOWED_HOSTS" \
    --setenv "OPENAIDE_WEB_STATE_ROOT=$state_root" \
    --setenv "OPENAIDE_WEB_RUNTIME_ROOT=$runtime_root" \
    --setenv "OPENAIDE_PROJECT_ROOTS=$project_roots" \
    --setenv "OPENAIDE_WEB_PROJECT_ROOTS=$project_roots" \
    --setenv "OPENAIDE_WEB_STATIC_ROOT=$static_root" \
    "${systemd_trace_env_args[@]}" \
    "$npm_bin" run web:dev >/dev/null
  echo "$static_root" > "$static_root_file"
  sleep 3

  if ! systemctl --user is-active --quiet "$systemd_unit.service"; then
    rm -f "$static_root_file"
    systemctl --user status "$systemd_unit.service" --no-pager >&2 || true
    cat "$log_file" >&2 || true
    exit 1
  fi

  systemctl --user show "$systemd_unit.service" --property MainPID --value > "$pid_file"
  echo "OpenAIDE local web started on http://$host:$port"
  echo "systemd unit: $systemd_unit.service"
  echo "PID file: $pid_file"
  echo "Log file: $log_file"
  echo "State root: $state_root"
  echo "Static root: $static_root"
  smoke_if_requested
}

restart_server() {
  require_allowed_hosts
  if [[ "$daemon_mode" == "systemd" ]] \
    && [[ "${OPENAIDE_WEB_RESTART_HELPER:-0}" != "1" ]] \
    && current_process_runs_in_systemd_unit; then
    delegate_systemd_restart
    return
  fi
  build_if_requested
  stop_server
  OPENAIDE_WEB_SKIP_BUILD=1 start_server
}

refresh_server() {
  require_allowed_hosts
  if ! server_running; then
    echo "OpenAIDE local web is not running; starting it."
    start_server
    return
  fi

  static_root="$(running_static_root)"
  build_static_frontend
  echo "OpenAIDE local web refreshed on http://$host:$port"
  echo "Static root: $static_root"
  echo "App Server was not restarted."
  echo "Use '$0 restart' only after backend/App Server changes."
}

status_server() {
  require_daemon_mode
  local status=0
  echo "url: http://$host:$port"
  if [[ "$daemon_mode" == "systemd" ]]; then
    require_systemd_user
    systemctl --user status "$systemd_unit.service" --no-pager || true
    if systemctl --user is-active --quiet "$systemd_unit.service"; then
      echo "service: running pid $(systemctl --user show "$systemd_unit.service" --property MainPID --value)"
    else
      echo "service: not running"
      status=1
    fi
  elif [[ -f "$pid_file" ]] && kill -0 "$(cat "$pid_file")" 2>/dev/null; then
    echo "wrapper: running pid $(cat "$pid_file")"
  else
    echo "wrapper: not running"
  fi
  echo "static root: $(running_static_root)"
  ss -ltnp | grep -E ":($port|$vite_port)\\b" || true
  return "$status"
}

case "$command" in
  refresh)
    with_lifecycle_lock refresh_server
    ;;
  start)
    with_lifecycle_lock start_server
    ;;
  stop)
    with_lifecycle_lock stop_server
    ;;
  restart)
    with_lifecycle_lock restart_server
    ;;
  status)
    status_server
    ;;
  logs)
    if [[ "$daemon_mode" == "systemd" ]]; then
      journalctl --user -u "$systemd_unit.service" -n "${OPENAIDE_WEB_LOG_LINES:-120}" --no-pager
      exit
    fi
    tail -n "${OPENAIDE_WEB_LOG_LINES:-120}" "$log_file"
    ;;
  smoke)
    run_smoke_check
    ;;
  -h|--help|help)
    usage
    ;;
  *)
    usage >&2
    exit 2
    ;;
esac
