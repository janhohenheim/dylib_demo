#!/usr/bin/env bash
set -euo pipefail

# Having multiple dynamic libraries using the standard library statically
# causes issues with static / global state used by std.
# So we must pass `-C prefer-dynamic -C rpath` to use the pre-installed standard library dylib.
#
# Note: we *could* use `-Z no-embed-metadata` to jank the `.rmeta` out of the `.so`s and into their own files.
# that would allow removing them when distributing the final game to users, since you only need that information at build time.
RUSTFLAGS="\
-C prefer-dynamic \
-C rpath" \
cargo build -p host
