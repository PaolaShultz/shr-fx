#!/usr/bin/env bash
set -euo pipefail
cd "$(dirname "$0")/.."
cargo build --locked --lib -j 2
fx_tmp=$(mktemp -d)
trap 'rm -rf "$fx_tmp"' EXIT
cc -std=c11 -Wall -Wextra -Werror -Iinclude tests/fixtures/cfx/v2/caller.c -Ltarget/debug -lshr_fx -lm -Wl,-rpath,"$PWD/target/debug" -o "$fx_tmp/caller"
"$fx_tmp/caller"
cc -std=c11 -Wall -Wextra -Werror -Iinclude tests/fixtures/cfx/v1/sample.c -Ltarget/debug -lshr_fx -lm -Wl,-rpath,"$PWD/target/debug" -o "$fx_tmp/v1"
"$fx_tmp/v1"

cc -std=c11 -Wall -Wextra -Werror -Iinclude tests/fixtures/cfx/delay-v2/sample.c -Ltarget/debug -lshr_fx -lm -Wl,-rpath,"$PWD/target/debug" -o "$fx_tmp/delay"
"$fx_tmp/delay"
