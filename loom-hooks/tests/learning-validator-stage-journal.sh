#!/usr/bin/env bash
# learning-validator.sh warns "No memories recorded for this stage" unless
# memory/<stage-id>.md holds a `###` entry. A missing journal, a header-only
# journal and another stage's journal all warn.
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
HOOK="$SCRIPT_DIR/../learning-validator.sh"

TMP=$(mktemp -d "${TMPDIR:-/tmp}/loom-hooktest.XXXXXX")
trap 'rm -rf "$TMP"' EXIT

WARNING="No memories recorded"

# run_hook - stderr only, with every inherited LOOM_* variable scrubbed.
run_hook() {
	printf '{}' |
		env -u LOOM_MAIN_AGENT_PID -u LOOM_HOOK_DEBUG -u LOOM_BIN -u LOOM_HOOK_CONTEXT \
			-u LOOM_SCRATCH_DIR -u LOOM_SESSION_TYPE -u LOOM_MERGE_SESSION \
			LOOM_STAGE_ID="s1" LOOM_SESSION_ID="sess" LOOM_WORK_DIR="$TMP/work" \
			bash "$HOOK" 2>&1 1>/dev/null
}

# fresh_memory_dir - an empty memory directory for the next case.
fresh_memory_dir() {
	rm -rf "$TMP/work"
	mkdir -p "$TMP/work/memory"
}

ENTRY=$'### 📝 Note [2026-10-18 10:00:00] id=abc\nfound something'

expect_warning() {
	local label="$1" stderr
	stderr=$(run_hook)
	if [[ "$stderr" != *"$WARNING"* ]]; then
		echo "FAIL: $label: expected the warning, got: $stderr"
		exit 1
	fi
}

fresh_memory_dir
printf '# Memory Journal: s1\n\n%s\n' "$ENTRY" >"$TMP/work/memory/s1.md"
stderr=$(run_hook)
if [[ "$stderr" == *"$WARNING"* ]]; then
	echo "FAIL: a journal with an entry must not warn, got: $stderr"
	exit 1
fi

fresh_memory_dir
expect_warning "no journal for the stage"

fresh_memory_dir
printf '# Memory Journal: s1\n' >"$TMP/work/memory/s1.md"
expect_warning "a header-only journal"

fresh_memory_dir
printf '# Memory Journal: other\n\n%s\n' "$ENTRY" >"$TMP/work/memory/other.md"
expect_warning "only another stage's journal"

echo "PASS"
