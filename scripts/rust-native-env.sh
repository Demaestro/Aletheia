#!/usr/bin/env bash
set -euo pipefail

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$repo_root"

source "$HOME/.cargo/env"

export CC="$repo_root/scripts/zig-cc.sh"
export RUSTFLAGS="-C linker=$repo_root/scripts/zig-cc.sh ${RUSTFLAGS:-}"

exec "$@"
