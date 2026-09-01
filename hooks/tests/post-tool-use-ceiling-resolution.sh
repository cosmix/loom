#!/usr/bin/env bash
# PostToolUse must treat `loom hook context-ceilings` as the ONE parser and
# resolver for both main and subagent ceilings. Shell selects one half of the
# canonical `<main>:<subagent>` result and caches the pair per Loom session;
# config.toml and stage YAML are opaque to this script.
set -euo pipefail

HOOK="$(dirname "$0")/../post-tool-use.sh"
TMP=$(mktemp -d "${TMPDIR:-/tmp}/loom-hooktest.XXXXXX")
trap 'rm -rf "$TMP"' EXIT

STAGE_ID="test-stage"
SESSION_ID="test-session"
FAKE_BIN="$TMP/bin"
FAKE_OUTPUT="$TMP/canonical-output"
FAKE_CALLS="$TMP/canonical-calls"
mkdir -p "$FAKE_BIN"

cat >"$FAKE_BIN/loom" <<'EOF'
#!/usr/bin/env bash
set -euo pipefail
[[ "$#" -eq 2 && "$1" == "hook" && "$2" == "context-ceilings" ]] || exit 64
printf '%s\n' call >>"$FAKE_LOOM_CALL_LOG"
[[ -f "$FAKE_LOOM_OUTPUT_FILE" ]] || exit 1
cat "$FAKE_LOOM_OUTPUT_FILE"
EOF
chmod +x "$FAKE_BIN/loom"

build_transcript() {
	local path="$1" resident="$2"
	{
		printf '%s\n' '{"type":"user","message":{"content":"dummy"}}'
		jq -nc --argjson n "$resident" \
			'{type:"assistant",message:{usage:{input_tokens:$n,cache_creation_input_tokens:0,cache_read_input_tokens:0}}}'
	} >"$path"
}

# run_case <workdir> <transcript> [agent-type]
run_case() {
	local workdir="$1" transcript="$2" agent_type="${3:-}" input
	if [[ -n "$agent_type" ]]; then
		input=$(jq -nc --arg tp "$transcript" --arg at "$agent_type" \
			'{tool_name:"Bash",tool_input:{command:"echo hi"},transcript_path:$tp,agent_type:$at}')
	else
		input=$(jq -nc --arg tp "$transcript" \
			'{tool_name:"Bash",tool_input:{command:"echo hi"},transcript_path:$tp}')
	fi
	set +e
	STDERR_OUT=$(printf '%s' "$input" |
		env -u LOOM_MAIN_AGENT_PID -u LOOM_WORKTREE_PATH \
			PATH="$FAKE_BIN:$PATH" \
			FAKE_LOOM_OUTPUT_FILE="$FAKE_OUTPUT" FAKE_LOOM_CALL_LOG="$FAKE_CALLS" \
			LOOM_STAGE_ID="$STAGE_ID" LOOM_SESSION_ID="$SESSION_ID" LOOM_WORK_DIR="$workdir" \
			bash "$HOOK" 2>&1 1>/dev/null)
	CODE=$?
	set -e
}

assert_warn() {
	local desc="$1" resident="$2" ceiling="$3"
	if [[ $CODE -ne 2 || "$STDERR_OUT" != *"${resident}/${ceiling}"* ]]; then
		echo "FAIL: $desc - expected warning ${resident}/${ceiling}, exit 2; got exit $CODE: $STDERR_OUT"
		exit 1
	fi
}

assert_hard_block() {
	local desc="$1" needle="$2"
	if [[ $CODE -ne 2 || "$STDERR_OUT" != *"$needle"* ]]; then
		echo "FAIL: $desc - expected hard block '$needle', exit 2; got exit $CODE: $STDERR_OUT"
		exit 1
	fi
}

assert_allowed() {
	local desc="$1"
	if [[ $CODE -ne 0 ]]; then
		echo "FAIL: $desc - expected exit 0, got exit $CODE: $STDERR_OUT"
		exit 1
	fi
}

call_count() {
	if [[ -f "$FAKE_CALLS" ]]; then
		wc -l <"$FAKE_CALLS" | tr -d ' '
	else
		printf '0\n'
	fi
}

# One canonical call supplies and caches both branches. Changing the fake's
# later output must not affect this session: main still sees 1000 and a
# teammate/subagent selects 500 from the same cached pair.
WORK_CACHE="$TMP/work-cache"
mkdir -p "$WORK_CACHE"
printf '%s\n' '1000:500' >"$FAKE_OUTPUT"
MAIN_TRANSCRIPT="$WORK_CACHE/main.jsonl"
build_transcript "$MAIN_TRANSCRIPT" 850
run_case "$WORK_CACHE" "$MAIN_TRANSCRIPT"
assert_warn "main selects the first canonical value" 850 1000

printf '%s\n' '9999:9999' >"$FAKE_OUTPUT"
build_transcript "$MAIN_TRANSCRIPT" 1000
run_case "$WORK_CACHE" "$MAIN_TRANSCRIPT"
assert_hard_block "main reuses the cached pair" "1000 >= 1000"

SUBAGENT_TRANSCRIPT="$WORK_CACHE/agent-worker.jsonl"
build_transcript "$SUBAGENT_TRANSCRIPT" 500
run_case "$WORK_CACHE" "$SUBAGENT_TRANSCRIPT" "loom-software-engineer"
assert_hard_block "subagent selects the cached second value" "SUBAGENT CEILING REACHED"
if [[ "$(call_count)" != "1" ]]; then
	echo "FAIL: expected one canonical resolver call for a cached session, got $(call_count)"
	exit 1
fi

# Zero is a valid canonical u32 sentinel: Rust could not verify this stage, so
# main enforcement is disabled rather than applying a ceiling from the wrong
# stage. It is distinct from malformed helper output, which falls back below.
WORK_DISABLED="$TMP/work-disabled"
mkdir -p "$WORK_DISABLED"
printf '%s\n' '0:500' >"$FAKE_OUTPUT"
TRANSCRIPT_DISABLED="$WORK_DISABLED/transcript.jsonl"
build_transcript "$TRANSCRIPT_DISABLED" 200000
run_case "$WORK_DISABLED" "$TRANSCRIPT_DISABLED"
assert_allowed "canonical main zero disables unverified-stage enforcement"

# Malformed TOML scalars remain defaults because the fake canonical resolver
# says they are defaults. Their digit runs must never be interpreted by shell.
for malformed in '-1' '"500"' '500oops'; do
	WORK_BAD="$TMP/work-bad-${malformed//[^A-Za-z0-9]/_}"
	mkdir -p "$WORK_BAD"
	printf '[context]\nceiling_tokens = %s\n' "$malformed" >"$WORK_BAD/config.toml"
	printf '%s\n' '150000:120000' >"$FAKE_OUTPUT"
	TRANSCRIPT_BAD="$WORK_BAD/transcript.jsonl"
	build_transcript "$TRANSCRIPT_BAD" 130000
	run_case "$WORK_BAD" "$TRANSCRIPT_BAD"
	assert_warn "malformed config '$malformed' uses canonical default" 130000 150000
done

# This is the former parser exploit: table-looking text inside a TOML string
# names a tiny ceiling. The hook must not inspect it; only canonical output is
# live, and prose resembling shell syntax must remain inert too.
WORK_MALICIOUS="$TMP/work-malicious"
mkdir -p "$WORK_MALICIOUS"
SENTINEL="$TMP/config-was-executed"
cat >"$WORK_MALICIOUS/config.toml" <<EOF
[plan]
note = """
[context]
ceiling_tokens = 1
\$(touch "$SENTINEL")
"""
EOF
printf '%s\n' '150000:120000' >"$FAKE_OUTPUT"
TRANSCRIPT_MALICIOUS="$WORK_MALICIOUS/transcript.jsonl"
build_transcript "$TRANSCRIPT_MALICIOUS" 130000
run_case "$WORK_MALICIOUS" "$TRANSCRIPT_MALICIOUS"
assert_warn "malicious config text is never parsed by shell" 130000 150000
if [[ -e "$SENTINEL" ]]; then
	echo "FAIL: config.toml content was executed by the shell hook"
	exit 1
fi

# A broken helper must fail safely to the hand-kept defaults rather than
# accepting a partial pair or disabling the governor. 700000 sits above 80%
# of the 800000 fallback (LOOM_DEFAULT_CONTEXT_CEILING_TOKENS), the same band
# this case was written to exercise.
WORK_BAD_OUTPUT="$TMP/work-bad-output"
mkdir -p "$WORK_BAD_OUTPUT"
printf '%s\n' '1000:not-a-u32' >"$FAKE_OUTPUT"
TRANSCRIPT_BAD_OUTPUT="$WORK_BAD_OUTPUT/transcript.jsonl"
build_transcript "$TRANSCRIPT_BAD_OUTPUT" 700000
run_case "$WORK_BAD_OUTPUT" "$TRANSCRIPT_BAD_OUTPUT"
assert_warn "malformed canonical output falls back safely" 700000 800000

echo "PASS"
