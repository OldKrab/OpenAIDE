current_process_runs_in_systemd_unit() {
  local cgroup_file="${OPENAIDE_WEB_CURRENT_CGROUP_FILE:-/proc/$$/cgroup}"
  [[ -r "$cgroup_file" ]] && grep -Fq "/$systemd_unit.service" "$cgroup_file"
}

delegate_systemd_restart() {
  require_systemd_user
  local helper_unit="${systemd_unit}-restart-$(date +%s%N)"
  local helper_env_args=(
    --setenv "HOME=$HOME"
    --setenv "PATH=$PATH"
    --setenv "OPENAIDE_WEB_RESTART_HELPER=1"
    --setenv "OPENAIDE_WEB_ROLE=${OPENAIDE_WEB_ROLE:-}"
    --setenv "OPENAIDE_WEB_INSTANCE_LABEL=${OPENAIDE_WEB_INSTANCE_LABEL:-}"
    --setenv "OPENAIDE_WEB_TITLE=${OPENAIDE_WEB_TITLE:-}"
    --setenv "OPENAIDE_WEB_HOST=$host"
    --setenv "OPENAIDE_WEB_PORT=$port"
    --setenv "OPENAIDE_WEB_VITE_PORT=$vite_port"
    --setenv "OPENAIDE_WEB_ALLOWED_HOSTS=$OPENAIDE_WEB_ALLOWED_HOSTS"
    --setenv "OPENAIDE_WEB_STATE_ROOT=$state_root"
    --setenv "OPENAIDE_WEB_RUNTIME_ROOT=$runtime_root"
    --setenv "OPENAIDE_WEB_STATIC_ROOT=$static_root"
    --setenv "OPENAIDE_WEB_PROTOTYPE_ROOT=$prototype_root"
    --setenv "OPENAIDE_WEB_PROJECT_ROOTS=$project_roots"
    --setenv "OPENAIDE_WEB_PID_FILE=$pid_file"
    --setenv "OPENAIDE_WEB_LOG_FILE=$log_file"
    --setenv "OPENAIDE_WEB_BUILD=${OPENAIDE_WEB_BUILD:-0}"
    --setenv "OPENAIDE_WEB_SKIP_BUILD=${OPENAIDE_WEB_SKIP_BUILD:-0}"
    --setenv "OPENAIDE_WEB_DAEMON=$daemon_mode"
    --setenv "OPENAIDE_WEB_SYSTEMD_UNIT=$systemd_unit"
  )
  if [[ -v OPENAIDE_ACP_TRACE ]]; then
    helper_env_args+=(--setenv "OPENAIDE_ACP_TRACE=$OPENAIDE_ACP_TRACE")
  fi
  if [[ -n "${OPENAIDE_ACP_TRACE_DIR:-}" ]]; then
    helper_env_args+=(--setenv "OPENAIDE_ACP_TRACE_DIR=$OPENAIDE_ACP_TRACE_DIR")
  fi

  # This helper is outside Target's cgroup, so it survives stopping the service it replaces.
  systemd-run --user \
    --unit "$helper_unit" \
    --collect \
    --description "Restart $systemd_unit.service outside its own cgroup" \
    --working-directory "$repo_root" \
    "${helper_env_args[@]}" \
    bash "$repo_root/deploy/local-web.sh" restart >/dev/null
  echo "Restart delegated outside $systemd_unit.service via $helper_unit.service."
}
