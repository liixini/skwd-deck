#!/bin/sh
set -eu

cd "$(dirname "$0")/.."
cargo test --locked --release --workspace \
    --test app_themes --test rpc --test schedule --test playlist --test lifecycle --test apply --test apply_model \
    --test restore --test we --test concurrent --test hotplug --test library --test playback \
    --test plasma --test theme \
    -- --ignored --nocapture
