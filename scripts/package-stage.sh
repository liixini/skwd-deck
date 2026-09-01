#!/bin/sh
set -eu

root=$(CDPATH= cd -- "$(dirname -- "$0")/.." && pwd)
destination=${1:?usage: scripts/package-stage.sh DESTINATION}
case "$destination" in
    /)
        echo "refusing to stage directly into /" >&2
        exit 2
        ;;
    /*) ;;
    *) destination="$root/$destination" ;;
esac

if [ -L "$destination" ] || { [ -e "$destination" ] && [ ! -d "$destination" ]; }; then
    echo "package destination must be a directory: $destination" >&2
    exit 2
fi
if [ -d "$destination" ] && [ -n "$(find "$destination" -mindepth 1 -print -quit)" ]; then
    echo "package destination is not empty: $destination" >&2
    exit 2
fi

binary_directory=${SKWD_DECK_BIN_DIR:-$root/target/release}
case "$binary_directory" in
    /*) ;;
    *) binary_directory="$root/$binary_directory" ;;
esac

if [ -z "${SKWD_DECK_BIN_DIR:-}" ]; then
    cargo build --manifest-path "$root/Cargo.toml" --release --workspace
fi

for binary in skwd-walld skwd-wall-scan skwd-wall-effects skwd-steam skwd-helm; do
    path="$binary_directory/$binary"
    if [ ! -f "$path" ] || [ ! -x "$path" ]; then
        echo "missing executable Deck release binary: $path" >&2
        exit 1
    fi
done

for path in \
    "$root/LICENSE" \
    "$root/LICENSES/Apache-2.0.txt" \
    "$root/LICENSES/MIT.txt" \
    "$root/LICENSES/ffmpeg-sys-the-third-WTFPL.txt"
do
    if [ ! -f "$path" ]; then
        echo "missing Deck license material: $path" >&2
        exit 1
    fi
done

umask 022
license_directory="$destination/usr/share/licenses/skwd-deck"
template_directory="$destination/usr/share/skwd-wall-v2/data/matugen/templates"
mkdir -p "$destination/usr/bin" "$destination/usr/lib/systemd/user"
mkdir -p "$license_directory" "$template_directory"

for binary in skwd-walld skwd-wall-scan skwd-wall-effects skwd-steam skwd-helm; do
    install -m755 "$binary_directory/$binary" "$destination/usr/bin/$binary"
done

if [ -f "$binary_directory/libsteam_api.so" ]; then
    install -m755 "$binary_directory/libsteam_api.so" "$destination/usr/bin/libsteam_api.so"
fi

install -m644 "$root/data/skwd-walld.service" \
    "$destination/usr/lib/systemd/user/skwd-walld.service"
install -m644 "$root/LICENSE" "$license_directory/LICENSE"
install -m644 "$root/LICENSES/Apache-2.0.txt" "$license_directory/Apache-2.0.txt"
install -m644 "$root/LICENSES/MIT.txt" "$license_directory/MIT.txt"
install -m644 "$root/LICENSES/ffmpeg-sys-the-third-WTFPL.txt" \
    "$license_directory/ffmpeg-sys-the-third-LICENSE"
for template in "$root"/data/matugen/templates/*; do
    install -m644 "$template" "$template_directory/"
done
