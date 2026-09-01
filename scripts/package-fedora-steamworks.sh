#!/bin/sh
set -eu

root=$(CDPATH= cd -- "$(dirname -- "$0")/.." && pwd)
destination=${1:?usage: scripts/package-fedora-steamworks.sh DESTINATION}
binary_directory=${SKWD_DECK_BIN_DIR:-$root/target/release}

for tool in rpmbuild readelf; do
    if ! command -v "$tool" >/dev/null 2>&1; then
        echo "required Fedora packaging tool is missing: $tool" >&2
        exit 1
    fi
done
for asset in skwd-steam libsteam_api.so; do
    path="$binary_directory/$asset"
    if [ -L "$path" ] || [ ! -f "$path" ]; then
        echo "missing direct Steamworks release asset: $path" >&2
        exit 1
    fi
done
if ! readelf -d "$binary_directory/skwd-steam" | grep -Fq 'Library runpath: [$ORIGIN]'; then
    echo "skwd-steam must use the exact sibling-library RUNPATH" >&2
    exit 1
fi
if ! readelf -d "$binary_directory/skwd-steam" | grep -Fq 'Shared library: [libsteam_api.so]'; then
    echo "skwd-steam does not declare the Steamworks runtime" >&2
    exit 1
fi

case "$destination" in
    /*) ;;
    *) destination="$root/$destination" ;;
esac
if [ -L "$destination" ] || { [ -e "$destination" ] && [ ! -d "$destination" ]; }; then
    echo "RPM destination must be a directory: $destination" >&2
    exit 2
fi
mkdir -p "$destination"
workspace=$(mktemp -d "${TMPDIR:-/tmp}/skwd-fedora-steamworks.XXXXXX")
cleanup() {
    case "$workspace" in
        "${TMPDIR:-/tmp}"/skwd-fedora-steamworks.*) ;;
        *) echo "refusing to clean unexpected workspace: $workspace" >&2; return ;;
    esac
    [ -d "$workspace" ] && [ ! -L "$workspace" ] || return
    find "$workspace" -depth -type f -delete
    find "$workspace" -depth -type d -empty -delete
}
trap cleanup EXIT HUP INT TERM
mkdir -p "$workspace/BUILD" "$workspace/BUILDROOT" "$workspace/RPMS" \
    "$workspace/SOURCES" "$workspace/SPECS" "$workspace/SRPMS"
install -m755 "$binary_directory/skwd-steam" "$workspace/SOURCES/skwd-steam"
install -m755 "$binary_directory/libsteam_api.so" "$workspace/SOURCES/libsteam_api.so"
install -m644 "$root/packaging/fedora/skwd-deck-steamworks.spec" \
    "$workspace/SPECS/skwd-deck-steamworks.spec"
rpmbuild -bb --define "_topdir $workspace" "$workspace/SPECS/skwd-deck-steamworks.spec"
set -- "$workspace"/RPMS/x86_64/skwd-deck-steamworks-*.rpm
if [ "$#" -ne 1 ] || [ ! -f "$1" ]; then
    echo "RPM build did not produce exactly one companion package" >&2
    exit 1
fi
install -m644 "$1" "$destination/$(basename "$1")"
printf '%s\n' "$destination/$(basename "$1")"
