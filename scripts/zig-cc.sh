#!/usr/bin/env bash
set -euo pipefail

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
export ZIG_LOCAL_CACHE_DIR="${ZIG_LOCAL_CACHE_DIR:-$repo_root/.zig-cache/local}"
export ZIG_GLOBAL_CACHE_DIR="${ZIG_GLOBAL_CACHE_DIR:-$repo_root/.zig-cache/global}"
export TMPDIR="${TMPDIR:-$repo_root/.zig-cache/tmp}"

mkdir -p "$ZIG_LOCAL_CACHE_DIR" "$ZIG_GLOBAL_CACHE_DIR" "$TMPDIR"

args=()
for arg in "$@"; do
  case "$arg" in
    --target=x86_64-unknown-linux-gnu)
      args+=("-target" "x86_64-linux-gnu")
      ;;
    --target=x86_64-unknown-linux-musl)
      args+=("-target" "x86_64-linux-musl")
      ;;
    *)
      args+=("$arg")
      ;;
  esac
done

exec "$HOME/.local/bin/zig" cc "${args[@]}"
