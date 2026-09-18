#!/usr/bin/env bash
set -euo pipefail

unset LOOM_STAGE_ID LOOM_SESSION_ID LOOM_WORK_DIR LOOM_SESSION_TYPE LOOM_MAIN_AGENT_PID
GUARD="$(cd "$(dirname "$0")/.." && pwd)/codex-forward-guard.sh"
d=$(mktemp -d "${TMPDIR:-/tmp}/cfw-records.XXXXXX") && [[ -n "$d" ]]
trap 'rm -rf "$d"' EXIT

HOME_DIR="$d/home"
WORKSPACE="$d/workspace"
WORK_DIR="$d/work"
COMPANION_DIR="$HOME_DIR/.claude/plugins/cache/openai-codex/codex/1.0.6/scripts"
COMPANION="$COMPANION_DIR/codex-companion.mjs"
LEDGER="$WORK_DIR/subagents/my-stage/codex.jsonl"
mkdir -p "$COMPANION_DIR" "$WORKSPACE/.git" "$WORK_DIR/subagents/my-stage"
printf '%s\n' '// pinned fixture' >"$COMPANION"

write_start() {
	local root="$1"
	mkdir -p "$root/subagents/my-stage"
	jq -nc '{agent_id:"forwarder-7",agent_type:"loom-codex-forwarder",stage_id:"my-stage",
		loom_session_id:"session-abc",parent_session_id:"parent-uuid",
		ts:"2000-01-01T00:00:00.000Z"}' >"$root/subagents/my-stage/starts.jsonl"
}
write_start "$WORK_DIR"

payload() {
	jq -nc --arg command "$1" --arg tool_use_id "$2" --arg cwd "$WORKSPACE" \
		'{tool_name:"Bash",tool_input:{command:$command,timeout:600000},agent_type:"loom-codex-forwarder",agent_id:"forwarder-7",session_id:"parent-uuid",tool_use_id:$tool_use_id,cwd:$cwd}'
}

authorize() {
	local command="$1" tool_use_id="$2" output="$3"
	printf '%s' "$(payload "$command" "$tool_use_id")" | HOME="$HOME_DIR" \
		LOOM_WORK_DIR="$WORK_DIR" LOOM_STAGE_ID=my-stage LOOM_SESSION_ID=session-abc \
		bash "$GUARD" >"$output"
}

BASE='~/.claude/hooks/loom/codex-forward.sh task hello --model gpt-5.6-terra --effort xhigh --write'
WITH_UNIT="$BASE --unit-id stable-unit"
OUT1="$d/first.json"
OUT2="$d/second.json"
authorize "$WITH_UNIT" tool-first "$OUT1"
authorize "$WITH_UNIT" tool-second "$OUT2"

[[ -f "$LEDGER" && $(wc -l <"$LEDGER") -eq 2 ]]
[[ $(wc -l <"$OUT1") -eq 1 && $(wc -l <"$OUT2") -eq 1 ]]
invocation_one=$(jq -er '.hookSpecificOutput.updatedInput.command | capture("--invocation-id (?<id>inv-[0-9a-f]{32})$").id' "$OUT1")
invocation_two=$(jq -er '.hookSpecificOutput.updatedInput.command | capture("--invocation-id (?<id>inv-[0-9a-f]{32})$").id' "$OUT2")
[[ "$invocation_one" != "$invocation_two" ]]

expected_one="$WITH_UNIT --invocation-id $invocation_one"
jq -e --arg command "$expected_one" '
	.hookSpecificOutput == {
		hookEventName:"PreToolUse",
		permissionDecision:"allow",
		updatedInput:{command:$command,timeout:600000}
	}' "$OUT1" >/dev/null

workspace_root=$(cd "$WORKSPACE" && pwd -P)
companion_path=$(cd "$COMPANION_DIR" && pwd -P)/codex-companion.mjs
state_root=$(cd "$HOME_DIR" && pwd -P)/.codex/plugin-data/state
first_row=$(sed -n '1p' "$LEDGER")
[[ "$first_row" != *$'\n'* ]]
printf '%s' "$first_row" | jq -e \
	--arg invocation "$invocation_one" --arg workspace "$workspace_root" \
	--arg companion "$companion_path" --arg state "$state_root" '
	(keys | sort) == (["companion_path","companion_version","effort","forwarder_agent_id","invocation_id","model","parent_session_id","session_id","stage_id","state_root","tool_use_id","ts","unit_id","v","workspace_root"] | sort) and
	.v == 2 and (.ts | test("^[0-9]{4}-[0-9]{2}-[0-9]{2}T[0-9]{2}:[0-9]{2}:[0-9]{2}\\.[0-9]{3}Z$")) and
	.stage_id == "my-stage" and .session_id == "session-abc" and
	.parent_session_id == "parent-uuid" and .forwarder_agent_id == "forwarder-7" and
	.tool_use_id == "tool-first" and .unit_id == "stable-unit" and
	.invocation_id == $invocation and .model == "gpt-5.6-terra" and .effort == "xhigh" and
	.workspace_root == $workspace and .companion_version == "1.0.6" and
	.companion_path == $companion and .state_root == $state' >/dev/null

# Legacy callers receive a stable logical unit derived from the exact agent id.
OUT3="$d/legacy.json"
authorize "$BASE" tool-legacy "$OUT3"
legacy_invocation=$(jq -er '.hookSpecificOutput.updatedInput.command | capture("--invocation-id (?<id>inv-[0-9a-f]{32})$").id' "$OUT3")
jq -e --arg command "$BASE --unit-id fwd-forwarder-7 --invocation-id $legacy_invocation" \
	'.hookSpecificOutput.updatedInput.command == $command' "$OUT3" >/dev/null
jq -e 'select(.tool_use_id == "tool-legacy") | .unit_id == "fwd-forwarder-7"' "$LEDGER" >/dev/null

# Caller-owned invocation identity is never accepted or recorded.
before=$(wc -l <"$LEDGER")
FORGED="$WITH_UNIT --invocation-id inv-00000000000000000000000000000000"
status=0
printf '%s' "$(payload "$FORGED" tool-forged)" | HOME="$HOME_DIR" \
	LOOM_WORK_DIR="$WORK_DIR" LOOM_STAGE_ID=my-stage LOOM_SESSION_ID=session-abc \
	bash "$GUARD" >"$d/forged.stdout" 2>"$d/forged.stderr" || status=$?
[[ $status -eq 2 && ! -s "$d/forged.stdout" && $(wc -l <"$LEDGER") -eq $before ]]
rg -qF 'caller-supplied --invocation-id is forbidden' "$d/forged.stderr"

# Unit ids share the Rust receipt rule: the first byte must be alphanumeric.
for unsafe_unit in -leading-dash .leading-dot; do
	before=$(wc -l <"$LEDGER")
	status=0
	printf '%s' "$(payload "$BASE --unit-id $unsafe_unit" "tool-$unsafe_unit")" | HOME="$HOME_DIR" \
		LOOM_WORK_DIR="$WORK_DIR" LOOM_STAGE_ID=my-stage LOOM_SESSION_ID=session-abc \
		bash "$GUARD" >"$d/unsafe-unit.stdout" 2>"$d/unsafe-unit.stderr" || status=$?
	[[ $status -eq 2 && ! -s "$d/unsafe-unit.stdout" && $(wc -l <"$LEDGER") -eq $before ]]
done

# An exact forward with stage evidence but no resolvable stage is rejected
# before companion launch. LOOM_SESSION_ID alone is that evidence; with none at
# all the guard has no forwarding policy to apply and allows the call.
status=0
printf '%s' "$(payload "$WITH_UNIT" tool-outside-stage)" | HOME="$HOME_DIR" \
	LOOM_SESSION_ID=session-abc \
	bash "$GUARD" >"$d/outside-stage.stdout" 2>"$d/outside-stage.stderr" || status=$?
[[ $status -eq 2 && ! -s "$d/outside-stage.stdout" ]]
rg -qF 'codex forwarding is allowed only inside an active loom stage (safe LOOM_STAGE_ID, LOOM_SESSION_ID, and LOOM_WORK_DIR are required)' \
	"$d/outside-stage.stderr"

# The guard pins 1.0.6 and fails closed when only another version exists.
UNSUPPORTED_HOME="$d/unsupported-home"
mkdir -p "$UNSUPPORTED_HOME/.claude/plugins/cache/openai-codex/codex/1.0.7/scripts" \
	"$d/unsupported-work/subagents/my-stage"
write_start "$d/unsupported-work"
printf '%s\n' '// unsupported' >"$UNSUPPORTED_HOME/.claude/plugins/cache/openai-codex/codex/1.0.7/scripts/codex-companion.mjs"
status=0
printf '%s' "$(payload "$WITH_UNIT" tool-unsupported)" | HOME="$UNSUPPORTED_HOME" \
	LOOM_WORK_DIR="$d/unsupported-work" LOOM_STAGE_ID=my-stage LOOM_SESSION_ID=session-abc \
	bash "$GUARD" >"$d/unsupported.stdout" 2>"$d/unsupported.stderr" || status=$?
[[ $status -eq 2 && ! -s "$d/unsupported.stdout" ]]
rg -qF 'supported codex companion 1.0.6 is missing or unsafe' "$d/unsupported.stderr"

# Failure to append the authorization is a block, never a silent allow.
BLOCKED_WORK="$d/blocked-work"
mkdir -p "$BLOCKED_WORK"
printf '%s\n' blocker >"$BLOCKED_WORK/subagents"
status=0
printf '%s' "$(payload "$WITH_UNIT" tool-unwritable)" | HOME="$HOME_DIR" \
	LOOM_WORK_DIR="$BLOCKED_WORK" LOOM_STAGE_ID=my-stage LOOM_SESSION_ID=session-abc \
	bash "$GUARD" >"$d/unwritable.stdout" 2>/dev/null || status=$?
[[ $status -eq 2 && ! -s "$d/unwritable.stdout" ]]

printf '%s\n' PASS
