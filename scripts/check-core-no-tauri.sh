#!/usr/bin/env bash

set -euo pipefail

if cargo tree -p thoughttree-core --prefix none | grep -Eq '(^|[[:space:]])tauri(-| v)'; then
  echo "thoughttree-core must not depend on tauri"
  exit 1
fi
