#!/usr/bin/env bash
# spawn-guard.sh appends the `loom hook reviewer-brief` brief to a
# loom-code-reviewer spawn of a live stage session exactly once, after the
# worker brief. Other agent types never call the hook; a bad envelope, a failing
# stub or a stub slower than the bound changes nothing and the spawn stays allowed.
set -euo pipefail

d=$(mktemp -d "${TMPDIR:-/tmp}/spawn-reviewer-brief.XXXXXX") && [[ -n "$d" ]]
d=$(cd "$d" && pwd -P)
trap 'rm -rf "$d"' EXIT

HOOK="$(cd "$(dirname "$0")/.." && pwd)/spawn-guard.sh"
HOME_DIR="$d/home"
CWD="$d/cwd"
WORK="$d/work"
STUB="$d/loom-stub"
CALLS="$d/calls"
STDIN_LOG="$d/stub-stdin"
REPLY="$d/reviewer-reply"
WORKER_REPLY="$d/worker-reply"
NONCE=fedcba9876543210fedcba9876543210
WORKER_NONCE=0123456789abcdef0123456789abcdef
MARKER="<!-- loom-reviewer-brief nonce=$NONCE -->"
BRIEF=$(printf '%s\n## Test-integrity events\n\n### TI-edit-src/a_tests.rs (accepted)\n\nbody' "$MARKER")
WORKER_BRIEF=$(printf '## Scoped\n\n<!-- loom-worker-brief nonce=%s -->\n\nfact' "$WORKER_NONCE")

fail() {
	echo "FAIL: $*" >&2
	exit 1
}

mkdir -p "$HOME_DIR" "$CWD" "$WORK" "$d/tmp"

# The stub logs its argv and consumes stdin to STDIN_LOG; reviewer-brief replies
# with the contents of $REVIEWER_REPLY after an optional sleep, then exits
# $REVIEWER_EXIT (default 0); reviewer-model answers so every spawn is rewritten
# and its output always carries an explicit allow.
cat >"$STUB" <<'STUB_EOF'
#!/usr/bin/env bash
printf '%s\n' "$*" >>"$CALLS"
case "$1 $2" in
"hook worker-brief")
	cat >/dev/null
	[[ ! -f "$WORKER_REPLY" ]] || cat "$WORKER_REPLY"
	;;
"hook reviewer-brief")
	cat >"$STDIN_LOG"
	[[ -z "${REVIEWER_SLEEP:-}" ]] || sleep "$REVIEWER_SLEEP"
	[[ ! -f "$REVIEWER_REPLY" ]] || cat "$REVIEWER_REPLY"
	exit "${REVIEWER_EXIT:-0}"
	;;
"hook reviewer-model") printf 'model=opus effort=high\n' ;;
esac
exit 0
STUB_EOF
chmod +x "$STUB"

envelope() { jq -nc --arg n "$1" --arg b "$2" '{nonce: $n, brief: $b}'; }
set_reply() { printf '%s\n' "$1" >"$REPLY"; }
set_worker() { envelope "$WORKER_NONCE" "$WORKER_BRIEF" >"$WORKER_REPLY"; }
reset() { rm -f "$REPLY" "$WORKER_REPLY" "$CALLS" "$STDIN_LOG"; }

# spawn <gate|nogate> <tool_input json> - the hook output. Only the "gate" run
# carries the stage environment; env -i drops anything inherited from the
# session that runs this test. The hook runs as a descendant of this script, so
# LOOM_MAIN_AGENT_PID=$$ is a live ancestor.
spawn() {
	local payload gate_env=()
	payload=$(jq -nc --argjson t "$2" '{tool_name: "Task", tool_input: $t}')
	if [[ "$1" == gate ]]; then
		gate_env=(LOOM_STAGE_ID=test-stage LOOM_MAIN_AGENT_PID=$$ LOOM_WORK_DIR="$WORK" LOOM_SESSION_ID=test-session)
	fi
	(cd "$CWD" && printf '%s' "$payload" | env -i PATH="$PATH" HOME="$HOME_DIR" TMPDIR="$d/tmp" LOOM_BIN="$STUB" \
		CALLS="$CALLS" STDIN_LOG="$STDIN_LOG" REVIEWER_REPLY="$REPLY" WORKER_REPLY="$WORKER_REPLY" \
		REVIEWER_SLEEP="${REVIEWER_SLEEP:-}" REVIEWER_EXIT="${REVIEWER_EXIT:-0}" \
		${gate_env[@]+"${gate_env[@]}"} bash "$HOOK" 2>/dev/null) || true
}

tool_input() { jq -nc --arg t "$1" '{subagent_type: $t, description: "d", prompt: "Review the diff."}'; }

# prompt_of <hook output> - the rewritten prompt byte for byte (a sentinel keeps
# $(...) from stripping trailing newlines).
prompt_of() {
	local p
	p=$(printf '%s' "$1" | jq -j '.hookSpecificOutput.updatedInput.prompt // empty'
		printf x)
	printf '%s' "${p%x}"
}

allowed() { [[ $(printf '%s' "$1" | jq -r '.hookSpecificOutput.permissionDecision // empty') == allow ]]; }
count_marker() { printf '%s\n' "$1" | grep -c "^<!-- loom-$2-brief nonce=" || true; }
called_brief() { [[ -f "$CALLS" ]] && grep -q 'hook reviewer-brief' "$CALLS"; }

# 1. A valid envelope on a loom-code-reviewer spawn: the prompt ends with the
# brief, which holds its marker line exactly once.
reset
set_reply "$(envelope "$NONCE" "$BRIEF")"
out=$(spawn gate "$(tool_input loom-code-reviewer)")
p=$(prompt_of "$out")
allowed "$out" || fail "1: spawn not allowed with a valid reviewer brief"
[[ "$p" == *$'\n\n'"$BRIEF" ]] || fail "1: prompt does not end with the reviewer brief"
[[ $(count_marker "$p" reviewer) == 1 ]] || fail "1: reviewer marker not exactly once"

# 2. Worker brief and reviewer brief together: worker first, reviewer last, once each.
set_worker
out=$(spawn gate "$(tool_input loom-code-reviewer)")
p=$(prompt_of "$out")
[[ "$p" == *$'\n\n'"$BRIEF" ]] || fail "2: reviewer brief is not last"
[[ $(count_marker "$p" worker) == 1 && $(count_marker "$p" reviewer) == 1 ]] || fail "2: a brief is not present exactly once"
head_part=${p%%"$MARKER"*}
[[ "$head_part" == *"loom-worker-brief"* ]] || fail "2: worker brief does not come before the reviewer brief"

# 3. Other agent types never call the hook and get no reviewer marker.
for t in loom-software-engineer Explore; do
	reset
	set_reply "$(envelope "$NONCE" "$BRIEF")"
	out=$(spawn gate "$(tool_input "$t")")
	! called_brief || fail "3: $t spawn called hook reviewer-brief"
	[[ $(count_marker "$(prompt_of "$out")" reviewer) == 0 ]] || fail "3: $t prompt carries a reviewer marker"
done

# 4. A non-string prompt cannot take the append: no call, the spawn is allowed.
reset
set_reply "$(envelope "$NONCE" "$BRIEF")"
out=$(spawn gate '{"subagent_type":"loom-code-reviewer","description":"d","prompt":5}')
! called_brief || fail "4: non-string prompt still called hook reviewer-brief"
allowed "$out" || fail "4: spawn not allowed with a non-string prompt"

# 5. Without the gate environment there is no call and no append.
reset
set_reply "$(envelope "$NONCE" "$BRIEF")"
out=$(spawn nogate "$(tool_input loom-code-reviewer)")
! called_brief || fail "5: spawn outside a stage session called hook reviewer-brief"
[[ $(count_marker "$(prompt_of "$out")" reviewer) == 0 ]] || fail "5: reviewer marker appended outside a stage session"

# 6. Bad envelopes change nothing: the prompt equals the one built with no brief.
reset
baseline=$(prompt_of "$(spawn gate "$(tool_input loom-code-reviewer)")")
[[ -n "$baseline" ]] || fail "6: baseline prompt empty"
other_nonce=0000000000000000000000000000ffff
cases=(
	"marker nonce differs from nonce|$(envelope "$other_nonce" "$BRIEF")"
	"extra key|$(jq -nc --arg n "$NONCE" --arg b "$BRIEF" '{nonce: $n, brief: $b, extra: 1}')"
	"two lines|$(envelope "$NONCE" "$BRIEF")"$'\n'"$(envelope "$NONCE" "$BRIEF")"
	"nonce not 32 hex|$(envelope abc123 "<!-- loom-reviewer-brief nonce=abc123 -->")"
	"empty brief|$(envelope "$NONCE" "")"
	"not JSON|garbage"
	"marker not first line|$(envelope "$NONCE" $'## Title\n'"$MARKER")"
	"second loom marker line|$(envelope "$NONCE" "$BRIEF"$'\n<!-- loom-other nonce=1 -->')"
	"marker twice|$(envelope "$NONCE" "$BRIEF"$'\n'"$MARKER")"
)
for c in "${cases[@]}"; do
	set_reply "${c#*|}"
	out=$(spawn gate "$(tool_input loom-code-reviewer)")
	allowed "$out" || fail "6: ${c%%|*}: spawn not allowed"
	[[ $(prompt_of "$out") == "$baseline" ]] || fail "6: ${c%%|*}: prompt changed"
done

# 7. A stub that exits 1 after a valid reply changes nothing.
set_reply "$(envelope "$NONCE" "$BRIEF")"
out=$(REVIEWER_EXIT=1 spawn gate "$(tool_input loom-code-reviewer)")
allowed "$out" || fail "7: spawn not allowed when the stub fails"
[[ $(prompt_of "$out") == "$baseline" ]] || fail "7: prompt changed when the stub fails"

# 8. A stub slower than the 5 s bound changes nothing and returns near the bound.
start=$SECONDS
out=$(REVIEWER_SLEEP=7 spawn gate "$(tool_input loom-code-reviewer)")
elapsed=$((SECONDS - start))
(( elapsed <= 6 )) || fail "8: slow stub held the guard for ${elapsed}s"
allowed "$out" || fail "8: spawn not allowed when the stub is slow"
[[ $(prompt_of "$out") == "$baseline" ]] || fail "8: prompt changed when the stub is slow"

# 9. The guard gives the command an empty stdin (the file exists and is empty).
reset
set_reply "$(envelope "$NONCE" "$BRIEF")"
spawn gate "$(tool_input loom-code-reviewer)" >/dev/null
[[ -e "$STDIN_LOG" ]] || fail "9: stub never recorded its stdin"
[[ ! -s "$STDIN_LOG" ]] || fail "9: the stub received data on stdin"

echo "PASS"
