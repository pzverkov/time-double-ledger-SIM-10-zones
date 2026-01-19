#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"

echo "==> web: generating package-lock.json"
(
  cd "$ROOT_DIR/web"
  npm install
)

echo "==> go: generating go.sum"
(
  cd "$ROOT_DIR/go"
  go mod tidy
)

echo "==> rust: generating Cargo.lock"
(
  cd "$ROOT_DIR/rust/sim"
  cargo generate-lockfile
)

echo "Done. Commit package-lock.json, go.sum, and Cargo.lock for reproducible builds."
