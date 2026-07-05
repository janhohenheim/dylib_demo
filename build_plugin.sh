#!/usr/bin/env bash
set -euo pipefail

# Here comes the magic trick.
# we build `plugin` by re-using a prebuilt `libapi.rmeta`. Here, we use the one created by `build_host.sh`,
# but in a real setup, we would distribute it for the user to download.
# Note that all the deps used by `api` in `target/debug/deps` *also* need to be distributed to the plugin authors.
#
# Note: if we were using `-Z no-embed-metadata`, we would need to add the following line:
#  --extern api="target/debug/deps/libapi.rmeta"
RUSTFLAGS="\
-C prefer-dynamic \
-C rpath" \
rustc \
    --crate-name=plugin \
    --edition 2024 \
    --crate-type=dylib \
    --extern api="target/debug/deps/libapi.so" \
    -L dependency="target/debug/deps" \
    -o target/debug/libplugin.so \
    plugin/src/lib.rs
