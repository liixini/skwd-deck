#!/bin/sh
set -eu

cd "$(dirname "$0")/.."
cargo test --locked --release --workspace --test layer_guard
