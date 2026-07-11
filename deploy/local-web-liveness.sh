# shellcheck shell=bash
# shellcheck disable=SC2154 # Instance values are supplied by local-web.sh before sourcing.

listener_pids_for_port() {
  local listener_port="$1"
  ss -ltnp 2>/dev/null \
    | awk -v endpoint=":$listener_port" '
      $4 ~ endpoint "$" {
        line = $0
        while (match(line, /pid=[0-9]+/)) {
          print substr(line, RSTART + 4, RLENGTH - 4)
          line = substr(line, RSTART + RLENGTH)
        }
      }
    ' \
    | sort -u
}

listener_pids() {
  {
    listener_pids_for_port "$port"
    listener_pids_for_port "$vite_port"
  } | sort -u
}

process_owns_web_instance() {
  # Port occupancy alone is not ownership; the launcher gives the serving process these exact roots.
  local pid="$1"
  local expected_static_root="$static_root"
  if [[ -f "$static_root_file" ]]; then
    local recorded_static_root
    recorded_static_root="$(head -n 1 "$static_root_file")"
    if [[ -n "$recorded_static_root" ]]; then
      expected_static_root="$(canonical_path "$recorded_static_root")"
    fi
  fi

  [[ -r "/proc/$pid/environ" ]] \
    && grep -zFqx -- "OPENAIDE_WEB_STATE_ROOT=$state_root" "/proc/$pid/environ" \
    && grep -zFqx -- "OPENAIDE_WEB_RUNTIME_ROOT=$runtime_root" "/proc/$pid/environ" \
    && grep -zFqx -- "OPENAIDE_WEB_STATIC_ROOT=$expected_static_root" "/proc/$pid/environ"
}

owned_web_listener_running() {
  local pid
  while IFS= read -r pid; do
    if [[ -n "$pid" ]] && kill -0 "$pid" 2>/dev/null && process_owns_web_instance "$pid"; then
      return 0
    fi
  done < <(listener_pids_for_port "$port")

  return 1
}
