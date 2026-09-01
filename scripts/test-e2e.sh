#!/bin/sh
set -eu

cd "$(dirname "$0")/.."
for suite in rpc schedule lifecycle apply apply_model restore we concurrent hotplug library; do
    cargo test --locked --release -p skwd-e2e --test "$suite" -- --ignored --nocapture
done
