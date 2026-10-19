#!/usr/bin/env bash
# stage_hooks <root> - copy the hook scripts under <root>/hooks/loom (a Claude
# root laid out as installed) and print that directory. Hooks derive their
# root from their own location, so tests that exercise the codex wrapper path
# must run the staged copy, not the source checkout.
stage_hooks() {
	local root="$1" src
	src="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
	mkdir -p "$root/hooks/loom"
	cp -R "$src"/. "$root/hooks/loom/"
	rm -rf "$root/hooks/loom/tests" "$root/hooks/loom/__pycache__"
	printf '%s' "$root/hooks/loom"
}
