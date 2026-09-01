#!/bin/sh
set -eu

cd "$(dirname "$0")/.."

e2e=0
for argument in "$@"; do
    case "$argument" in
        --e2e) e2e=1 ;;
        -h|--help)
            echo "usage: scripts/test-all.sh [--e2e]"
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
python3 scripts/tests/test_package_stage.py
python3 scripts/tests/test_fedora_steamworks.py

if [ "$e2e" -eq 1 ]; then
    for suite in rpc schedule lifecycle apply apply_model restore we concurrent hotplug library; do
        cargo test --release -p skwd-e2e --test "$suite" -- --ignored --nocapture
    done
fi
