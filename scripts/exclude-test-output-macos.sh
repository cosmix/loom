#!/bin/sh
# Explicit, opt-in exclusions for generated output. Never run from test fixtures.
set -eu

if [ "$(uname -s)" != Darwin ]; then
    printf '%s\n' 'This script requires macOS.' >&2
    exit 1
fi

repository=$(CDPATH= cd -- "$(dirname -- "$0")/.." && pwd)
target=${1:-"$repository/loom/target"}
test_root=${2:-"/var/tmp/loom-tests-$(id -u)"}

if [ "$#" -gt 2 ]; then
    printf '%s\n' "Usage: $0 [cargo-target-directory [test-temporary-parent]]" >&2
    exit 2
fi

mkdir -p "$target"
if [ ! -d "$test_root" ]; then
    (umask 077 && mkdir -p "$test_root")
fi
touch "$target/.metadata_never_index"
/usr/bin/tmutil addexclusion "$target" "$test_root"
printf 'Time Machine exclusions requested for:\n  %s\n  %s\n' "$target" "$test_root"
printf 'Spotlight opt-out marker created at %s/.metadata_never_index\n' "$target"
