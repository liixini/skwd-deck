#!/bin/sh
set -eu

cd "$(dirname "$0")/.."
cargo test --locked --release -p skwd-wall-core --test layer_guard
cargo test --locked --release -p skwd-walld --test layer_guard
