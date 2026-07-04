#!/usr/bin/env bash
set -euo pipefail

./build_host.sh
./build_plugin.sh

./target/debug/host
