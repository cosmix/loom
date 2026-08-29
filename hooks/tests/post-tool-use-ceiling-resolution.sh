#!/usr/bin/env bash
# The context governor resolves its ceiling in a strict order: a stage's own
# `context_ceiling_tokens` frontmatter beats `[context] ceiling_tokens` in
# LOOM_WORK_DIR/config.toml, which beats the built-in default (150000). This
# is the regression test for the awk/grep pair doing that resolution
# (_loom_ctx_toml_get's [context]-table scan, _loom_ctx_resolve_ceiling's
# frontmatter grep) silently breaking after a format change: rather than
# asserting some fixed marker exists, each case drives a transcript whose
# resident usage sits just over 80% of the ONE ceiling that should win, and
# asserts the resulting warning names that exact number. If resolution falls
# back to the wrong tier, the arithmetic no longer lines up and the assertion
# fails.
#
# Each case runs in its OWN LOOM_WORK_DIR so the ceiling cache and warn
# marker (implementation detail; not asserted on directly here) can never
# leak between cases regardless of what key they are stored under.
set -euo pipefail
HOOK="$(dirname "$0")/../post-tool-use.sh"
TMP=$(mktemp -d)
trap 'rm -rf "$TMP"' EXIT

STAGE_ID="test-stage"
SESSION_ID="test-session"

# build_transcript <path> <resident-tokens>
# Writes a dummy first line (unconditionally dropped by the governor's
# byte-offset tail) followed by one assistant usage record summing to
# <resident-tokens>.
build_transcript() {
	local path="$1" resident="$2"
	{
		printf '%s\n' '{"type":"user","message":{"content":"dummy first line, dropped by design"}}'
		jq -nc --argjson n "$resident" \
			'{type:"assistant",message:{usage:{input_tokens:$n,cache_creation_input_tokens:0,cache_read_input_tokens:0}}}'
	} >"$path"
}

# run_case <workdir> <transcript-path>
# Invokes the hook for a plain Bash tool call against <transcript-path>.
# Sets CODE and STDERR_OUT.
run_case() {
	local workdir="$1" transcript="$2"
	local input
	input=$(jq -nc --arg tp "$transcript" '{tool_name:"Bash",tool_input:{command:"echo hi"},transcript_path:$tp}')
	set +e
	STDERR_OUT=$(printf '%s' "$input" |
		env -u LOOM_MAIN_AGENT_PID -u LOOM_WORKTREE_PATH \
			LOOM_STAGE_ID="$STAGE_ID" LOOM_SESSION_ID="$SESSION_ID" LOOM_WORK_DIR="$workdir" \
			bash "$HOOK" 2>&1 1>/dev/null)
	CODE=$?
	set -e
}

assert_warn() {
	local desc="$1" resident="$2" ceiling="$3"
	if [[ $CODE -ne 2 ]]; then
		echo "FAIL: $desc - expected exit 2, got $CODE (stderr: $STDERR_OUT)"
		exit 1
	fi
	if [[ "$STDERR_OUT" != *"${resident}/${ceiling}"* ]]; then
		echo "FAIL: $desc - expected stderr to name the resolved ceiling '${resident}/${ceiling}', got: $STDERR_OUT"
		exit 1
	fi
}

# --- Case 1: stage frontmatter wins over a conflicting config.toml value ---
WORK1="$TMP/work1"
mkdir -p "$WORK1/stages"
cat >"$WORK1/config.toml" <<'EOF'
[context]
ceiling_tokens = 9999
EOF
cat >"$WORK1/stages/${STAGE_ID}.md" <<'EOF'
---
id: test-stage
context_ceiling_tokens: 1000
---
body
EOF
TRANSCRIPT1="$TMP/transcript1.jsonl"
build_transcript "$TRANSCRIPT1" 850
run_case "$WORK1" "$TRANSCRIPT1"
assert_warn "(1) stage frontmatter beats config.toml" 850 1000

# --- Case 2: no stage value -> config.toml [context] ceiling_tokens wins ---
WORK2="$TMP/work2"
mkdir -p "$WORK2"
cat >"$WORK2/config.toml" <<'EOF'
[context]
ceiling_tokens = 2000
EOF
TRANSCRIPT2="$TMP/transcript2.jsonl"
build_transcript "$TRANSCRIPT2" 1700
run_case "$WORK2" "$TRANSCRIPT2"
assert_warn "(2) config.toml wins with no stage file at all" 1700 2000

# --- Case 3: neither present -> built-in default (150000) ---
# A stage file IS present here, but without the context_ceiling_tokens key -
# exercises the frontmatter-present-but-key-absent path through the grep,
# distinct from case 2's "no stage file at all".
WORK3="$TMP/work3"
mkdir -p "$WORK3/stages"
cat >"$WORK3/stages/${STAGE_ID}.md" <<'EOF'
---
id: test-stage
name: "No ceiling declared"
---
body
EOF
TRANSCRIPT3="$TMP/transcript3.jsonl"
build_transcript "$TRANSCRIPT3" 130000
run_case "$WORK3" "$TRANSCRIPT3"
assert_warn "(3) built-in default with neither stage nor config value" 130000 150000

echo "PASS"
