#!/usr/bin/env bash
# path_without <name>... - echo a directory that mirrors every executable on
# the current PATH as a symlink, minus the named binaries. Tests set
# PATH="$(path_without jq)" to simulate a machine without jq without
# touching the real installation. The caller removes the directory.
path_without() {
	local dir d f n skip x
	dir=$(mktemp -d "${TMPDIR:-/tmp}/loom-pathwithout.XXXXXX")
	local IFS=':'
	for d in $PATH; do
		[[ -d "$d" ]] || continue
		for f in "$d"/*; do
			[[ -x "$f" ]] || continue
			n=$(basename "$f")
			skip=0
			for x in "$@"; do [[ "$n" == "$x" ]] && skip=1; done
			[[ $skip -eq 1 ]] && continue
			[[ -e "$dir/$n" ]] || ln -s "$f" "$dir/$n"
		done
	done
	printf '%s\n' "$dir"
}
