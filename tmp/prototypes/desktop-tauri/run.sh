#!/usr/bin/env bash
set -euo pipefail

prototype_root="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
repo_root="$(cd "${prototype_root}/../../.." && pwd)"

cd "${repo_root}"
cargo build -p openaide-app-server

cd "${prototype_root}"
if [[ ! -d node_modules ]]; then
  npm install
fi

export OPENAIDE_APP_SERVER_BIN="${repo_root}/target/debug/openaide-app-server"
export OPENAIDE_DESKTOP_PROTOTYPE_ROOT="${prototype_root}"
exec npm run tauri -- dev
