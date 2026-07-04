#!/usr/bin/env bash
set -euo pipefail

# Having multiple dynamic libraries using the standard library statically
# causes issues with static / global state used by std.
# So we must pass `-C prefer-dynamic` to use the pre-installed standard library dylib.
RUSTFLAGS="\
    -C prefer-dynamic \
" \
    cargo +nightly build -p host -p api
