# shellcheck shell=bash
# shellcheck disable=SC2154 # Instance values are supplied by local-web.sh before sourcing.

validate_static_root() {
  local resolved_static_root
  local resolved_source_static_root
  resolved_static_root="$(canonical_path "$static_root")"
  resolved_source_static_root="$(canonical_path "$source_static_root")"

  if [[ "$resolved_static_root" == "$repo_root" || "$resolved_static_root" == "/" ]]; then
    echo "Refusing to use unsafe local web static root: $static_root" >&2
    exit 2
  fi

  if [[ "$resolved_static_root" == "$resolved_source_static_root" ]]; then
    echo "Refusing to use source frontend dist as a local web static root: $static_root" >&2
    echo "Set OPENAIDE_WEB_STATIC_ROOT to a role-owned directory such as .openaide-web-target/static." >&2
    exit 2
  fi

}
