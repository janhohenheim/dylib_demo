#!/usr/bin/env bash

cargo +nightly rustc -p host -- \
  -C prefer-dynamic \
  -C link-args=-Wl,-rpath,'$ORIGIN/'

API_RLIB=$(ls target/debug/deps/libapi.rlib | head -n1)

 rustc +nightly \
  --edition 2024 \
  --crate-type=dylib \
  --crate-name=plugin \
  --extern api="$API_RLIB" \
  -C prefer-dynamic \
  -o target/debug/libplugin.so \
  plugin/src/lib.rs

./target/debug/host
