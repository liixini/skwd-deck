#!/bin/sh
set -eu

cd "$(dirname "$0")/.."
: "${SKWD_TEST_DRAWING_PACK:?set it from scripts/install_drawing_model.py}"
deferred=media_references_compact
sandbox=$(mktemp -d "${TMPDIR:-/var/tmp}/skwd-drawing-live.XXXXXX")
trap 'rm -rf "$sandbox"' EXIT
mkdir -m 700 "$sandbox/home" "$sandbox/runtime" "$sandbox/bin"

saved_ifs=$IFS
IFS=:
for directory in $PATH; do
    [ -d "$directory" ] || continue
    for tool in "$directory"/*; do
        name=${tool##*/}
        case $name in skwd-*) continue ;; esac
        if [ -x "$tool" ] && [ ! -L "$sandbox/bin/$name" ]; then
            ln -s "$tool" "$sandbox/bin/$name"
        fi
    done
done
IFS=$saved_ifs

export CARGO_HOME="${CARGO_HOME:-$HOME/.cargo}" RUSTUP_HOME="${RUSTUP_HOME:-$HOME/.rustup}"
export HOME="$sandbox/home" XDG_RUNTIME_DIR="$sandbox/runtime" PATH="$sandbox/bin"
export XDG_CONFIG_HOME="$HOME/.config" XDG_CACHE_HOME="$HOME/.cache"
export XDG_DATA_HOME="$HOME/.local/share" XDG_STATE_HOME="$HOME/.local/state"
export SKWD_WALL_V2_SOCK="$XDG_RUNTIME_DIR/wall.sock" SKWD_DRAWING_HOME="${SKWD_TEST_DRAWING_PACK%/*}"
unset WAYLAND_DISPLAY WAYLAND_SOCKET DISPLAY NIRI_SOCKET HYPRLAND_INSTANCE_SIGNATURE DBUS_SESSION_BUS_ADDRESS

cargo test --locked --release -p skwd-e2e --test drawing_live -- --ignored --skip "$deferred" --test-threads=1 --nocapture
cargo test --locked --release -p skwd-e2e --test drawing_live -- --exact "$deferred"
