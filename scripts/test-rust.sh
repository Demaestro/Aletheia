#!/usr/bin/env bash
set -euo pipefail

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$repo_root"

echo "Aletheia: running the canonical native Rust test path."
echo "SQLite/FTS5 uses bundled C code, so native Zig linking is the reliable WSL path."
./scripts/test-rust-native.sh
