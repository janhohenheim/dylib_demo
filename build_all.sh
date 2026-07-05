#!/usr/bin/env bash
set -euo pipefail

cargo clean

./build_host.sh
./build_plugin.sh

./target/debug/host
