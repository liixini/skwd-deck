#!/bin/sh
set -eu

cd "$(dirname "$0")/.."

e2e=0
theme_vm=0
for argument in "$@"; do
    case "$argument" in
        --e2e) e2e=1 ;;
        --theme-vm) theme_vm=1 ;;
        -h|--help)
            echo "usage: scripts/test-all.sh [--e2e] [--theme-vm]"
            exit 0
            ;;
        *)
            echo "unknown argument: $argument" >&2
            exit 2
            ;;
    esac
done

cargo fmt --check
cargo clippy --workspace --all-targets -- -D warnings
cargo build --release --workspace
cargo test --release --workspace
cargo test --release -p skwd-wall-core --test layer_guard
cargo test --release -p skwd-walld --test layer_guard
PYTHONDONTWRITEBYTECODE=1 python3 -m unittest discover -s scripts/tests -p test_*.py

if [ "$e2e" -eq 1 ]; then
    for suite in app_themes rpc schedule playlist lifecycle apply apply_model restore we concurrent hotplug library playback plasma theme; do
        cargo test --release -p skwd-e2e --test "$suite" -- --ignored --nocapture
    done
fi

if [ "$theme_vm" -eq 1 ]; then
    python3 scripts/theme-provider-vm.py all
fi
