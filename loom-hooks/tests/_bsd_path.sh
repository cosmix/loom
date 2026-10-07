#!/usr/bin/env bash
# bsd_shim_dir - copy the BSD tool shims (wc, stat, date) into a fresh
# directory and print it. Tests put it first on PATH so the hooks meet the
# padded wc output and the BSD stat/date option sets that macOS has. The
# caller removes the directory.
_BSD_SHIM_SRC="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)/bsd-shims"

bsd_shim_dir() {
	local dir name
	dir=$(mktemp -d "${TMPDIR:-/tmp}/loom-bsdshims.XXXXXX")
	for name in wc stat date; do
		install -m 0755 "$_BSD_SHIM_SRC/$name" "$dir/$name"
	done
	printf '%s\n' "$dir"
}
