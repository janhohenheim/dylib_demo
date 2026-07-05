#!/usr/bin/env bash
set -euo pipefail

# Here comes the magic trick.
# we build `plugin` by re-using a prebuilt `libapi.rlib`. Here, we use the one created by `build_host.sh`,
# but in a real setup, we would distribute it for the user to download.
# Note that all the deps used by `api` in `target/debug/deps` *also* need to be distributed.

RUSTFLAGS="\
    -C prefer-dynamic \
" \
rustc +nightly \
    --edition 2024 \
    --crate-type=dylib \
    --crate-name=plugin \
    --extern api="target/debug/deps/libapi.rlib" \
    --extern api="target/debug/deps/libapi.rmeta" \
    --extern api="target/debug/deps/libapi.so" \
    -L target/debug/deps \
    -l dylib=api \
    -C prefer-dynamic \
    -o target/debug/libplugin.so \
    plugin/src/lib.rs
