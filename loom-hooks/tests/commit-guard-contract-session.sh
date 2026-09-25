#!/usr/bin/env bash
# commit-guard.sh must give a contract session (LOOM_SESSION_TYPE=contract) the
# contract reminder - freeze, do not commit, do not complete - instead of the
# commit-and-complete checklist, and still exit 0. A stage session in the same
# worktree keeps the checklist. A contract session that stops with a refused
# freeze on record (in its scratch directory) parks its stage with
# `loom stage waiting`; one with no refusal on record parks nothing.
set -euo pipefail

ROOT=$(cd "$(dirname "$0")/../.." && pwd)
HOOK="$ROOT/loom-hooks/commit-guard.sh"
TMP=$(mktemp -d "${TMPDIR:-/tmp}/loom-hooktest.XXXXXX")
trap 'rm -rf "$TMP"' EXIT

STAGE_ID="build-api"
PROJECT_ROOT="$TMP/repo"
WORKTREE="$PROJECT_ROOT/.worktrees/$STAGE_ID"
STAGES_DIR="$PROJECT_ROOT/.loom/work/stages"
SCRATCH="$TMP/scratch/session-1"
REFUSAL="$SCRATCH/contract-freeze-refused.txt"
CALLS="$TMP/loom.calls"
mkdir -p "$WORKTREE" "$STAGES_DIR" "$SCRATCH"
cat >"$STAGES_DIR/01-$STAGE_ID.md" <<EOF
---
id: $STAGE_ID
status: executing
---

# Stage
EOF

# A stub loom that records how it was called.
cat >"$TMP/loom" <<'STUB'
#!/usr/bin/env bash
printf '%s %s\n' "${LOOM_HOOK_CONTEXT:-}" "$*" >>"$(dirname "$0")/loom.calls"
STUB
chmod +x "$TMP/loom"

# GIT_CEILING_DIRECTORIES stops `git status` from finding whatever repository
# encloses $TMP, keeping the run TMPDIR-independent.
run_hook() {
	local session_type="$1"
	(cd "$WORKTREE" && printf '{}' |
		env -u LOOM_MERGE_SESSION LOOM_SESSION_TYPE="$session_type" LOOM_STAGE_ID="$STAGE_ID" \
			LOOM_SCRATCH_DIR="$SCRATCH" LOOM_BIN="$TMP/loom" \
			GIT_CEILING_DIRECTORIES="$TMP" bash "$HOOK" 2>&1)
}

if ! OUTPUT=$(run_hook contract); then
	echo "FAIL: the hook must exit 0 for a contract session, got: $OUTPUT"
	exit 1
fi
if [[ -e "$CALLS" ]]; then
	echo "FAIL: a writer with no refused freeze must not park its stage, loom got: $(cat "$CALLS")"
	exit 1
fi
for expected in \
	"LOOM CONTRACT REMINDER" \
	"loom stage contracts freeze $STAGE_ID" \
	"do not commit" \
	"do not complete the stage"; do
	if [[ "$OUTPUT" != *"$expected"* ]]; then
		echo "FAIL: contract reminder is missing '$expected', got: $OUTPUT"
		exit 1
	fi
done
if [[ "$OUTPUT" == *"COMPLETION CHECKLIST"* ]]; then
	echo "FAIL: a contract session must not get the commit-and-complete checklist, got: $OUTPUT"
	exit 1
fi

# A refusal behind a symlink is never taken for one.
ln -s "$TMP/elsewhere" "$REFUSAL"
run_hook contract >/dev/null
if [[ -e "$CALLS" ]]; then
	echo "FAIL: a symlinked refusal must not park the stage, loom got: $(cat "$CALLS")"
	exit 1
fi
rm "$REFUSAL"

printf '%s\n' "src/lib.rs is neither a contract file nor matched by a harness glob" >"$REFUSAL"
if ! OUTPUT=$(run_hook contract); then
	echo "FAIL: the hook must exit 0 for a refused contract session, got: $OUTPUT"
	exit 1
fi
if [[ "$(cat "$CALLS")" != "1 stage waiting $STAGE_ID" ]]; then
	echo "FAIL: a writer stopping on a refused freeze must run 'loom stage waiting' as a hook, got: $(cat "$CALLS")"
	exit 1
fi
for expected in \
	"the latest freeze was refused" \
	"waiting-for-input" \
	"loom stage contracts freeze $STAGE_ID" \
	"loom stage resume $STAGE_ID"; do
	if [[ "$OUTPUT" != *"$expected"* ]]; then
		echo "FAIL: refused contract reminder is missing '$expected', got: $OUTPUT"
		exit 1
	fi
done
rm "$CALLS"

OUTPUT=$(run_hook stage)
if [[ "$OUTPUT" != *"COMPLETION CHECKLIST for stage '$STAGE_ID'"* ]]; then
	echo "FAIL: a stage session must keep the completion checklist, got: $OUTPUT"
	exit 1
fi
if [[ "$OUTPUT" == *"CONTRACT REMINDER"* ]]; then
	echo "FAIL: a stage session must not get the contract reminder, got: $OUTPUT"
	exit 1
fi
if [[ -e "$CALLS" ]]; then
	echo "FAIL: a stage session must never park its stage on a contract refusal, loom got: $(cat "$CALLS")"
	exit 1
fi

echo "PASS"
