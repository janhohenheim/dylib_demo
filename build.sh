#!/usr/bin/env bash

RUSTFLAGS="-C prefer-dynamic" cargo build

COMMON_RLIB=$(ls target/debug/deps/libcommon-*.rlib | head -n1)

rustc --edition 2024 \
  --crate-type=dylib \
  --crate-name=plugin \
  --extern common="$COMMON_RLIB" \
  -C prefer-dynamic \
  -o target/debug/libplugin.so \
  plugin/src/lib.rs
