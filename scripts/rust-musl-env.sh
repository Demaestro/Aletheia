#!/usr/bin/env bash
set -euo pipefail

source "$HOME/.cargo/env"

export CARGO_BUILD_TARGET="${ALETHEIA_MUSL_TARGET:-x86_64-unknown-linux-musl}"
export RUSTFLAGS="-C linker=$HOME/.rustup/toolchains/stable-x86_64-unknown-linux-gnu/lib/rustlib/x86_64-unknown-linux-gnu/bin/rust-lld -C linker-flavor=ld.lld ${RUSTFLAGS:-}"
export CC="$PWD/scripts/zig-cc.sh"
export CARGO_TARGET_X86_64_UNKNOWN_LINUX_GNU_LINKER="$PWD/scripts/zig-cc.sh"
export CARGO_TARGET_X86_64_UNKNOWN_LINUX_MUSL_LINKER="$HOME/.rustup/toolchains/stable-x86_64-unknown-linux-gnu/lib/rustlib/x86_64-unknown-linux-gnu/bin/rust-lld"

exec "$@"
