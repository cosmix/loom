#!/usr/bin/env bash
# check-hook-syntax.sh - parse every shell hook, fail on the first syntax error.
#
# Exists because two hooks shipped syntactically invalid and nothing caught it:
# `spawn-guard.sh` and `codex-forward.sh` each carried a heredoc inside a command
# substitution (`X=$(cat <<'EOF' ... EOF)`). That construct is NOT protected by its
# own `<<'EOF'` quoting - bash's `$( )` lexer re-scans the body for quote
# characters, so a lone apostrophe in the text (`session's`, `a file's symbols`)
# opens a single-quoted region that runs for dozens of lines and destroys quote
# parity far from the real cause. `spawn-guard.sh` reported its failure at line
# 342 for a defect on line 208, and blocked every Agent spawn while it did.
#
# bash parses scripts incrementally, so a script with a syntax error LATER in the
# file still runs correctly up to an early `exit`. `codex-forward.sh` looked
# healthy in every manual test for exactly that reason. Only `bash -n` over the
# whole file finds these.

set -euo pipefail

repo_root=$(cd "$(dirname "$0")/.." && pwd)
# hooks/ and scripts/ both carry hand-written shell; a syntax error in either
# one is the same failure mode this script exists to catch.
script_dirs=("$repo_root/hooks" "$repo_root/scripts")

found_a_dir=0
for dir in "${script_dirs[@]}"; do
	if [ -d "$dir" ]; then
		found_a_dir=1
	fi
done
if [ "$found_a_dir" -eq 0 ]; then
	printf 'check-hook-syntax: none of the expected script directories exist (%s)\n' "${script_dirs[*]}" >&2
	exit 1
fi

failed=0
checked=0

while IFS= read -r script; do
	# hooks/skill-trigger.sh is Python with a .sh extension; bash -n on it is a
	# false positive. Select on the shebang, never on the extension.
	if head -n 1 "$script" | grep -q 'python'; then
		continue
	fi

	checked=$((checked + 1))
	if ! bash -n "$script" 2>/dev/null; then
		printf 'SYNTAX ERROR: %s\n' "${script#"$repo_root"/}" >&2
		bash -n "$script" 2>&1 | sed 's/^/    /' >&2
		failed=$((failed + 1))
	fi
done < <(find "${script_dirs[@]}" -type f -name '*.sh' 2>/dev/null | sort)

if [ "$failed" -ne 0 ]; then
	printf '\ncheck-hook-syntax: %d of %d shell scripts failed to parse\n' "$failed" "$checked" >&2
	exit 1
fi

printf 'check-hook-syntax: %d shell scripts parse cleanly\n' "$checked"
