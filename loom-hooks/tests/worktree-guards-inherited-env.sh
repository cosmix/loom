#!/usr/bin/env bash
# commit-guard, worktree-file-guard and worktree-isolation are registered
# globally, and LOOM_WORKTREE_PATH leaks into any shell a loom session starts.
# Honouring it alone blocks an unrelated session. Ignoring it reopens an escape:
# a stage session that moved out of its worktree would leave containment. The
# session's own live agent, which an inherited value cannot name, tells them apart.
set -euo pipefail

ROOT=$(cd "$(dirname "$0")/../.." && pwd)
source "$ROOT/loom-hooks/tests/_fixture_base.sh"
TMP=$(mktemp -d "$(fixture_base)/worktree-inherited.XXXXXX")
sleep 30 &
BYSTANDER=$!
trap 'kill "$BYSTANDER" 2>/dev/null || true; rm -rf "$TMP"' EXIT
TMP=$(cd "$TMP" && pwd -P)

STAGE=stage-a
PROJECT="$TMP/project"
WORKTREE="$PROJECT/.worktrees/$STAGE"
UNRELATED="$TMP/unrelated"
mkdir -p "$WORKTREE" "$PROJECT/.loom/work/stages" "$UNRELATED"
printf '%s\n' '---' "id: $STAGE" 'status: executing' '---' '# Stage' \
	>"$PROJECT/.loom/work/stages/01-$STAGE.md"
: >"$UNRELATED/notes.txt"

# A dead pid for a session that exited.
bash -c 'exit 0' &
DEAD=$!
wait "$DEAD" || true

READ=$(jq -nc --arg file "$UNRELATED/notes.txt" '{tool_name:"Read",tool_input:{file_path:$file}}')
GIT=$(jq -nc '{tool_name:"Bash",tool_input:{command:"git -C /elsewhere status"}}')

# run <hook> <payload> [VAR=value...] from the unrelated directory; sets CODE, OUT.
run() {
	local hook="$1" payload="$2"
	shift 2
	CODE=0
	OUT=$(cd "$UNRELATED" && printf '%s' "$payload" |
		env -u LOOM_MAIN_AGENT_PID -u LOOM_MERGE_SESSION -u LOOM_SESSION_TYPE \
			GIT_CEILING_DIRECTORIES="$TMP" LOOM_WORKTREE_PATH="$WORKTREE" "$@" \
			bash "$ROOT/loom-hooks/$hook" 2>&1) || CODE=$?
}

# Inherited with no session, with a session that exited, and with a live process
# that is not this hook's agent: all three are unrelated sessions.
for agent in NONE="" LOOM_MAIN_AGENT_PID="$DEAD" LOOM_MAIN_AGENT_PID="$BYSTANDER"; do
	run worktree-file-guard.sh "$READ" "$agent"
	[[ $CODE -eq 0 ]] || { echo "FAIL: worktree-file-guard blocked an unrelated session ($agent): $OUT"; exit 1; }
	run worktree-isolation.sh "$GIT" "$agent"
	[[ $CODE -eq 0 ]] || { echo "FAIL: worktree-isolation blocked an unrelated session ($agent): $OUT"; exit 1; }
	run commit-guard.sh '{}' "$agent"
	[[ "$OUT" != *"COMPLETION CHECKLIST"* ]] || { echo "FAIL: commit-guard nagged an unrelated session ($agent): $OUT"; exit 1; }
done

# The stage session itself, moved out of its worktree: this test's shell stands in
# for the agent, and is the hook's ancestor as the agent is. It stays contained.
run worktree-file-guard.sh "$READ" LOOM_MAIN_AGENT_PID=$$
[[ $CODE -eq 2 ]] || { echo "FAIL: a stage session escaped worktree-file-guard by leaving its worktree (exit $CODE)"; exit 1; }
run worktree-isolation.sh "$GIT" LOOM_MAIN_AGENT_PID=$$
[[ $CODE -eq 2 ]] || { echo "FAIL: a stage session escaped worktree-isolation by leaving its worktree (exit $CODE)"; exit 1; }
run commit-guard.sh '{}' LOOM_MAIN_AGENT_PID=$$
[[ "$OUT" == *"COMPLETION CHECKLIST for stage '$STAGE'"* ]] || { echo "FAIL: commit-guard lost a stage session that left its worktree: $OUT"; exit 1; }

# A session located in a worktree stays contained whatever its environment says.
CODE=0
OUT=$(cd "$WORKTREE" && printf '%s' "$READ" |
	env -u LOOM_MAIN_AGENT_PID -u LOOM_WORKTREE_PATH bash "$ROOT/loom-hooks/worktree-file-guard.sh" 2>&1) || CODE=$?
[[ $CODE -eq 2 ]] || { echo "FAIL: location alone no longer contains a session (exit $CODE)"; exit 1; }

echo PASS
