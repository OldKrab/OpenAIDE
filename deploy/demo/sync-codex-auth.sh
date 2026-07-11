#!/usr/bin/env bash
set -euo pipefail

volume="${1:-openaide-demo-codex}"
source_dir="${CODEX_HOME:-$HOME/.codex}"

if [[ ! -d "$source_dir" ]]; then
  echo "Codex auth source does not exist: $source_dir" >&2
  exit 1
fi

docker volume create "$volume" >/dev/null

docker run --rm \
  -v "$volume:/dest" \
  -v "$source_dir:/source:ro" \
  alpine:3.20 \
  sh -eu -c '
    rm -rf /dest/*
    mkdir -p /dest/accounts
    for file in auth.json config.toml installation_id; do
      if [ -f "/source/$file" ]; then
        cp "/source/$file" "/dest/$file"
      fi
    done
    if [ -d /source/accounts ]; then
      find /source/accounts -maxdepth 1 -type f \( -name "*.auth.json" -o -name "registry.json" \) \
        -exec cp {} /dest/accounts/ \;
    fi
    chown -R 10001:10001 /dest
    chmod -R go-rwx /dest
  '

echo "Synced minimal Codex auth into Docker volume: $volume"
