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
hooks_dir="$repo_root/hooks"

if [ ! -d "$hooks_dir" ]; then
	printf 'check-hook-syntax: no hooks directory at %s\n' "$hooks_dir" >&2
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
done < <(find "$hooks_dir" -type f -name '*.sh' | sort)

if [ "$failed" -ne 0 ]; then
	printf '\ncheck-hook-syntax: %d of %d shell hooks failed to parse\n' "$failed" "$checked" >&2
	exit 1
fi

printf 'check-hook-syntax: %d shell hooks parse cleanly\n' "$checked"
