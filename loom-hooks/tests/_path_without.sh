#!/usr/bin/env bash
# path_without <name>... - echo a directory that mirrors every executable on
# the current PATH as a symlink, minus the named binaries. Tests set
# PATH="$(path_without jq)" to simulate a machine without jq without
# touching the real installation. The caller removes the directory.
#
# One `ln` per PATH directory, in PATH order: ln refuses a name that already
# exists, so the first directory to provide a name wins, as in a PATH lookup.
# Linking per file forked thousands of processes and took ~40 s per call.
path_without() {
	local dir d x
	dir=$(mktemp -d "${TMPDIR:-/tmp}/loom-pathwithout.XXXXXX")
	local IFS=':'
	for d in $PATH; do
		[[ -d "$d" ]] || continue
		ln -s "$d"/* "$dir"/ 2>/dev/null || true
	done
	for x in "$@"; do rm -f "$dir/$x"; done
	printf '%s\n' "$dir"
}
