#!/usr/bin/env bash
set -euo pipefail

# Here comes the magic trick.
# we build `plugin` by re-using a prebuilt `libapi.rlib`. Here, we use the one created by `build_host.sh`,
# but in a real setup, we would distribute it for the user to download.
# Note that all the deps used by `api` in `target/debug/deps` *also* need to be distributed.
rustc +nightly \
    --edition 2024 \
    --crate-type=cdylib \
    --crate-name=plugin \
    --extern api="target/debug/libapi.rlib" \
    -L dependency=target/debug/deps \
    -C prefer-dynamic \
    -o target/debug/libplugin.so \
    plugin/src/lib.rs
