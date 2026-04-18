#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT_DIR"

echo "[Aletheia] Rust format"
cargo fmt --all --check

echo "[Aletheia] Rust workspace check"
cargo check --workspace

echo "[Aletheia] Rust tests"
cargo test --workspace

echo "[Aletheia] TypeScript typecheck"
npm run typecheck

echo "[Aletheia] Frontend production build"
npm run build

echo "[Aletheia] npm audit"
run_npm_audit() {
  for attempt in 1 2 3; do
    if npm audit --audit-level=moderate --omit=optional --registry=https://registry.npmjs.com --fetch-timeout=120000; then
      return 0
    fi
    echo "[Aletheia] npm audit attempt ${attempt} failed; retrying after registry cooldown." >&2
    sleep $((attempt * 5))
  done
  return 1
}

if ! run_npm_audit; then
  echo "[Aletheia] npm audit could not complete or found issues. Review before release." >&2
  exit 1
fi

echo "[Aletheia] cargo audit"
if command -v cargo-audit >/dev/null 2>&1; then
  cargo audit
else
  echo "[Aletheia] cargo-audit is not installed. Install with: cargo install cargo-audit" >&2
  exit 1
fi

echo "[Aletheia] Production verification completed"
