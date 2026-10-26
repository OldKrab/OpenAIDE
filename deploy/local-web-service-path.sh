# shellcheck shell=bash
# The PATH a systemd-managed instance runs with. systemd-run starts a unit with the manager's
# environment, not the caller's, so the script hands the unit a PATH explicitly. Copying the
# caller's PATH broke Agent startup whenever a restart came from a non-login context (another
# unit, an Agent tool): `npx`, `codex`, nvm's node and `~/.cargo/bin` went missing. One
# source decides it instead, in this order:
#   1. OPENAIDE_WEB_SERVICE_PATH, usually from the role's ignored .local.env;
#   2. the user's login shell, which loads the profile files a terminal would;
#   3. the caller's PATH, with a warning that Agents may not start.
# The script's own tools (systemctl, curl) keep the caller's PATH; builds use the service PATH
# so a restart from a unit without ~/.cargo/bin still finds the toolchain the unit will use.

# Commands the instance or its Agents run; a missing one is reported by name, never by path.
service_path_commands=(node npm npx cargo codex claude opencode)

resolve_service_path() {
  if [[ -n "${service_path:-}" ]]; then
    return
  fi
  if [[ -n "${OPENAIDE_WEB_SERVICE_PATH:-}" ]]; then
    service_path="$OPENAIDE_WEB_SERVICE_PATH"
    service_path_source="explicit"
  elif service_path="$(login_shell_path)" && [[ -n "$service_path" ]]; then
    service_path_source="login_shell"
  else
    service_path="$PATH"
    service_path_source="caller"
    echo "Warning: the login shell reported no PATH; the service inherits this caller's PATH and Agents may not start. Set OPENAIDE_WEB_SERVICE_PATH in deploy/local-web.${OPENAIDE_WEB_ROLE:-<role>}.local.env." >&2
  fi
  report_service_path
}

login_shell_path() {
  local shell="${OPENAIDE_WEB_LOGIN_SHELL:-${SHELL:-/bin/bash}}"
  [[ -x "$shell" ]] || return 1
  # Interactive and login, so both profile and rc files run; rc output is skipped by the marker.
  # shellcheck disable=SC2016 # the login shell expands its own PATH
  timeout 10 "$shell" -lic 'printf "\n__openaide_path__%s\n" "$PATH"' </dev/null 2>/dev/null \
    | sed -n 's/^__openaide_path__//p' | tail -n 1
}

report_service_path() {
  local missing=()
  local name
  for name in "${service_path_commands[@]}"; do
    PATH="$service_path" command -v "$name" >/dev/null 2>&1 || missing+=("$name")
  done
  echo "Service PATH source: $service_path_source"
  if [[ ${#missing[@]} -gt 0 ]]; then
    echo "Service PATH lacks: ${missing[*]}"
  fi
}
