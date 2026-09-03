#!/bin/sh
set -eu

cd "$(dirname "$0")/.."
cargo test --locked --release --workspace \
    --test rpc --test schedule --test lifecycle --test apply --test apply_model \
    --test restore --test we --test concurrent --test hotplug --test library \
    -- --ignored --nocapture
