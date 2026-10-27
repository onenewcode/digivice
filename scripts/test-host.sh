#!/bin/sh
set -eu

repo=$(CDPATH= cd -- "$(dirname -- "$0")/.." && pwd)
toolchain=$(rustup show active-toolchain)
RUSTUP_TOOLCHAIN=${toolchain%% *}
export RUSTUP_TOOLCHAIN
host=$(rustc -vV | sed -n 's/^host: //p')
action=test
if [ "${1:-}" = "--clippy" ]; then
    action=clippy
    shift
fi

cd "$repo"
cargo "$action" -p digivice-core --target "$host" "$@"
