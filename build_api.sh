#!/usr/bin/env bash
set -euo pipefail

RUSTFLAGS="\
    -C prefer-dynamic \
" \
cargo +nightly rustc \
    -p api \
    -Z no-embed-metadata \
    -- \
    -C prefer-dynamic
