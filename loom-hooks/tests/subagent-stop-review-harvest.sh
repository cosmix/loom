#!/usr/bin/env bash
# SubagentStop hands a loom-code-reviewer's stop event, and only that exact
# agent type's, to `loom hook review-harvest` (DESIGN D12). The delegate's
# output and failure never change the hook's own output or exit code.
set -euo pipefail

unset LOOM_STAGE_ID LOOM_SESSION_ID LOOM_WORK_DIR LOOM_SESSION_TYPE LOOM_HOOK_PATH \
	LOOM_HOOK_DEBUG LOOM_HOOK_CONTEXT
HOOK="$(cd "$(dirname "$0")/.." && pwd)/subagent-stop.sh"
TMP=""
TMP=$(mktemp -d "${TMPDIR:-/tmp}/subagent-stop-harvest.XXXXXX") && [[ -n "$TMP" ]] || {
	echo "FAIL: could not create scratch directory"
	exit 1
}
trap '[[ -n "${TMP:-}" ]] && rm -rf -- "$TMP"' EXIT
# The hook refuses paths that cross a symlink, so use the physical path.
TMP=$(cd "$TMP" && pwd -P)

WORKDIR="$TMP/work"
STAGE_ID="test-stage"
SESSION_ID="test-session"
PARENT_SESSION_ID="parent-session"
PROJECT="$TMP/claude-project"
PARENT_TRANSCRIPT="$PROJECT/${PARENT_SESSION_ID}.jsonl"
CALLS="$TMP/harvest-calls.jsonl"
mkdir -p "$WORKDIR/stages" "$WORKDIR/subagents/$STAGE_ID" "$TMP/bin" \
	"$PROJECT/$PARENT_SESSION_ID/subagents"
printf '%s\n' '---' "id: $STAGE_ID" "session: $SESSION_ID" '---' \
	>"$WORKDIR/stages/01-${STAGE_ID}.md"
printf '%s\n' '{"type":"parent","message":"waiting"}' >"$PARENT_TRANSCRIPT"

# The stub records one JSON line per call, then talks on both streams and
# exits with HARVEST_EXIT, so the test sees whether the hook swallows them.
cat >"$TMP/bin/loom" <<'STUB'
#!/usr/bin/env bash
stdin=$(cat)
jq -nc --arg argv "$*" --arg stdin "$stdin" --arg context "${LOOM_HOOK_CONTEXT:-}" \
	--arg work_dir "${LOOM_WORK_DIR:-}" \
	'{argv:$argv,stdin:$stdin,context:$context,work_dir:$work_dir}' >>"$HARVEST_CALLS"
echo "stub stdout"
echo "stub stderr" >&2
exit "${HARVEST_EXIT:-0}"
STUB
chmod +x "$TMP/bin/loom"

# BSD `wc -c` left-pads its count; the hooks must cope on macOS.
mkdir -p "$TMP/shim"
cat >"$TMP/shim/wc" <<'SHIM'
#!/usr/bin/env bash
out=$(/usr/bin/env -u PATH PATH=/usr/bin:/bin wc "$@") || exit $?
if [[ "${1:-}" == "-c" ]]; then printf '%8s\n' "${out//[[:space:]]/}"; else printf '%s\n' "$out"; fi
SHIM
chmod +x "$TMP/shim/wc"

transcript_of() {
	printf '%s' "$PROJECT/$PARENT_SESSION_ID/subagents/agent-$1.jsonl"
}

call_count() {
	if [[ -f "$CALLS" ]]; then
		wc -l <"$CALLS" | tr -d '[:space:]'
	else
		printf '0'
	fi
}

# run_stop <agent-id> <agent-type> <delegate-exit>: one SubagentStop event,
# with the SubagentStart row the hook requires. Sets HOOK_STATUS.
run_stop() {
	local agent="$1" type="$2" delegate_exit="$3" transcript=""
	transcript=$(transcript_of "$agent")
	printf '%s\n' '{"type":"assistant","message":{"content":[{"type":"text","text":"done"}]}}' \
		>"$transcript"
	jq -nc --arg agent_id "$agent" --arg agent_type "$type" --arg stage_id "$STAGE_ID" \
		--arg parent_session_id "$PARENT_SESSION_ID" --arg loom_session_id "$SESSION_ID" \
		'{agent_id:$agent_id,agent_type:$agent_type,stage_id:$stage_id,
		  parent_session_id:$parent_session_id,loom_session_id:$loom_session_id,
		  ts:"2000-01-01T00:00:00.000Z"}' >>"$WORKDIR/subagents/$STAGE_ID/starts.jsonl"
	HOOK_STATUS=0
	jq -nc --arg agent_id "$agent" --arg agent_type "$type" \
		--arg session_id "$PARENT_SESSION_ID" --arg transcript_path "$PARENT_TRANSCRIPT" \
		--arg agent_transcript_path "$transcript" \
		'{session_id:$session_id,hook_event_name:"SubagentStop",agent_id:$agent_id,
		  agent_type:$agent_type,transcript_path:$transcript_path,
		  agent_transcript_path:$agent_transcript_path}' |
		env PATH="$TMP/shim:$PATH" LOOM_WORK_DIR="$WORKDIR" LOOM_STAGE_ID="$STAGE_ID" LOOM_SESSION_ID="$SESSION_ID" \
			LOOM_BIN="$TMP/bin/loom" HARVEST_CALLS="$CALLS" HARVEST_EXIT="$delegate_exit" \
			bash "$HOOK" >"$TMP/stdout" 2>"$TMP/stderr" || HOOK_STATUS=$?
}

assert_quiet_success() {
	if [[ "$HOOK_STATUS" != "0" || -s "$TMP/stdout" || -s "$TMP/stderr" ]]; then
		echo "FAIL: $1: hook exited $HOOK_STATUS with output:"
		cat "$TMP/stdout" "$TMP/stderr"
		exit 1
	fi
}

run_stop "reviewer-1" "loom-code-reviewer" 0
assert_quiet_success "reviewer stop"
if [[ "$(call_count)" != "1" ]]; then
	echo "FAIL: a loom-code-reviewer stop should invoke the delegate once, got $(call_count)"
	exit 1
fi
if ! jq -e --arg stage "$STAGE_ID" --arg session "$PARENT_SESSION_ID" \
	--arg transcript "$(transcript_of reviewer-1)" --arg work_dir "$WORKDIR" '
	.argv == "hook review-harvest" and .context == "1" and .work_dir == $work_dir and
	(.stdin | fromjson) == {stage_id:$stage,session_id:$session,agent_id:"reviewer-1",
	  transcript_path:$transcript}' "$CALLS" >/dev/null; then
	echo "FAIL: the delegate was not invoked with the reviewer's stop event"
	cat "$CALLS"
	exit 1
fi

run_stop "worker-1" "loom-software-engineer" 0
assert_quiet_success "worker stop"
run_stop "near-miss-1" "loom-code-reviewer-2" 0
assert_quiet_success "near-miss stop"
if [[ "$(call_count)" != "1" ]]; then
	echo "FAIL: only the exact loom-code-reviewer type may invoke the delegate"
	cat "$CALLS"
	exit 1
fi

run_stop "reviewer-2" "loom-code-reviewer" 3
assert_quiet_success "failing delegate"
if [[ "$(call_count)" != "2" ]]; then
	echo "FAIL: the second reviewer stop should have reached the delegate"
	exit 1
fi

# A reviewer stop with no SubagentStart row is skipped, and says so on disk as
# one stop-skips.jsonl row.
SKIP_LOG="$WORKDIR/subagents/$STAGE_ID/stop-skips.jsonl"
rm -f "$WORKDIR/subagents/$STAGE_ID/starts.jsonl" "$SKIP_LOG"
transcript=$(transcript_of "reviewer-orphan")
printf '%s\n' '{"type":"assistant"}' >"$transcript"
jq -nc --arg t "$transcript" --arg p "$PARENT_TRANSCRIPT" --arg s "$PARENT_SESSION_ID" \
	'{session_id:$s,hook_event_name:"SubagentStop",agent_id:"reviewer-orphan",
	  agent_type:"loom-code-reviewer",transcript_path:$p,agent_transcript_path:$t}' |
	env LOOM_WORK_DIR="$WORKDIR" LOOM_STAGE_ID="$STAGE_ID" LOOM_SESSION_ID="$SESSION_ID" \
		LOOM_BIN="$TMP/bin/loom" HARVEST_CALLS="$CALLS" bash "$HOOK" >"$TMP/stdout" 2>"$TMP/stderr"
if [[ -s "$TMP/stdout" || -s "$TMP/stderr" ]] || ! jq -es '
	length == 1 and (.[0] | .agent_id == "reviewer-orphan" and
	.agent_type == "loom-code-reviewer" and .reason == "no_unambiguous_start_row" and
	(.ts | test("^[0-9]{4}-[0-9]{2}-[0-9]{2}T[0-9:.]+Z$")))' "$SKIP_LOG" >/dev/null; then
	echo "FAIL: a skipped reviewer stop should append one row to stop-skips.jsonl, quietly"
	cat "$SKIP_LOG" 2>/dev/null || true
	exit 1
fi

# A worker (non-reviewer) stop with no start row is skipped without a row.
rm -f "$SKIP_LOG"
transcript=$(transcript_of "worker-orphan")
printf '%s\n' '{"type":"assistant"}' >"$transcript"
jq -nc --arg t "$transcript" --arg p "$PARENT_TRANSCRIPT" --arg s "$PARENT_SESSION_ID" \
	'{session_id:$s,hook_event_name:"SubagentStop",agent_id:"worker-orphan",
	  agent_type:"loom-software-engineer",transcript_path:$p,agent_transcript_path:$t}' |
	env LOOM_WORK_DIR="$WORKDIR" LOOM_STAGE_ID="$STAGE_ID" LOOM_SESSION_ID="$SESSION_ID" \
		LOOM_BIN="$TMP/bin/loom" HARVEST_CALLS="$CALLS" bash "$HOOK" >"$TMP/stdout" 2>"$TMP/stderr"
if [[ -s "$TMP/stdout" || -s "$TMP/stderr" || -e "$SKIP_LOG" ]]; then
	echo "FAIL: a skipped worker stop must write no stop-skips.jsonl row"
	cat "$SKIP_LOG" 2>/dev/null || true
	exit 1
fi

# Under the BSD tool shims (padded wc, no date -d) the epoch fallback accepts
# fractional seconds and offsets, and a start row still resolves through the
# inline padded-wc strip.
HOOKS_DIR="$(cd "$(dirname "$HOOK")" && pwd)"
source "$HOOKS_DIR/tests/_bsd_path.sh"
source "$HOOKS_DIR/tests/_path_without.sh"
bsd_lifecycle_checks() {
	local want="1767225600" got="" ledger="$WORKDIR/subagents/$STAGE_ID/starts.jsonl"
	[[ -z "$1" ]] || PATH="$1:$PATH"
	source "$HOOKS_DIR/_lifecycle.sh"
	for ts in 2026-01-01T00:00:00.123Z 2026-01-01T02:00:00+02:00; do
		got=$(loom_lifecycle_epoch "$ts") || got=""
		[[ "$got" == "$want" ]] || { echo "epoch of $ts: want $want, got '$got'"; exit 1; }
	done
	jq -nc --arg agent_id "bsd-agent" --arg agent_type "loom-software-engineer" \
		--arg stage_id "$STAGE_ID" --arg parent_session_id "$PARENT_SESSION_ID" \
		--arg loom_session_id "$SESSION_ID" \
		'{agent_id:$agent_id,agent_type:$agent_type,stage_id:$stage_id,
		  parent_session_id:$parent_session_id,loom_session_id:$loom_session_id,
		  ts:"2000-01-01T00:00:00.000Z"}' >"$ledger"
	[[ "$(wc -c <"$ledger")" != "$(wc -c <"$ledger" | tr -d '[:space:]')" ]] ||
		{ echo "the BSD wc shim is not first on PATH"; exit 1; }
	loom_lifecycle_resolve_start "$WORKDIR" "$STAGE_ID" "$PARENT_SESSION_ID" "$SESSION_ID" \
		bsd-agent loom-software-engineer 2026-01-01T00:00:00.000Z test ||
		{ echo "no start row resolved under a padded wc"; exit 1; }
}
# run-all.sh in BSD mode has put the shims on PATH already; do not stack a second set.
BSD_DIR=""
[[ "${LOOM_HOOK_TEST_BSD:-}" == "1" ]] || BSD_DIR=$(bsd_shim_dir)
BSD_OUT=$(bsd_lifecycle_checks "$BSD_DIR" 2>&1) || {
	echo "FAIL: BSD-mode lifecycle helpers: $BSD_OUT"
	[[ -z "$BSD_DIR" ]] || rm -rf "$BSD_DIR"
	exit 1
}
[[ -z "$BSD_DIR" ]] || rm -rf "$BSD_DIR"

# Without sha256sum the digest helper falls back to shasum -a 256.
if command -v shasum >/dev/null 2>&1; then
	NO_SHA_DIR=$(path_without sha256sum)
	SHA_OUT=$(PATH="$NO_SHA_DIR" bash -c 'source "$1" && printf abc | loom_lifecycle_sha256' _ \
		"$HOOKS_DIR/_lifecycle.sh" 2>&1) || SHA_OUT="exit $?: $SHA_OUT"
	rm -rf "$NO_SHA_DIR"
	if [[ "$SHA_OUT" != ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad* ]]; then
		echo "FAIL: loom_lifecycle_sha256 without sha256sum printed: $SHA_OUT"
		exit 1
	fi
else
	echo "skip: shasum is not installed, the sha256 fallback is untested"
fi

echo "PASS"
