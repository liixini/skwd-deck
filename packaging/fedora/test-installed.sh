#!/bin/sh
set -eu

rpm -q skwd-deck-steamworks >/dev/null
test "$(readlink /usr/bin/skwd-steam)" = ../libexec/skwd-deck/skwd-steam
test -x /usr/libexec/skwd-deck/skwd-steam
test -x /usr/libexec/skwd-deck/libsteam_api.so
ldd /usr/libexec/skwd-deck/skwd-steam | \
    grep -Fq 'libsteam_api.so => /usr/libexec/skwd-deck/libsteam_api.so'
output=$(mktemp)
if /usr/bin/skwd-steam 431960 >"$output" 2>&1; then
    echo "Steam-absent helper invocation unexpectedly succeeded" >&2
    exit 1
fi
grep -Fq 'Steam is not running' "$output"
! grep -Fq 'error while loading shared libraries' "$output"
rm -f "$output"
