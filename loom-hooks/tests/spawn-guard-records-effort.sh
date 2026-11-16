#!/usr/bin/env bash
# spawn-guard.sh records the effort a spawn runs at in spawns.jsonl: the
# caller's explicit `effort`, else the agent definition's frontmatter default,
# else an empty string.
set -euo pipefail

unset LOOM_STAGE_ID LOOM_SESSION_ID LOOM_WORK_DIR LOOM_SESSION_TYPE LOOM_MAIN_AGENT_PID
d=$(mktemp -d "${TMPDIR:-/tmp}/spawn-effort.XXXXXX") && [[ -n "$d" ]]
d=$(cd "$d" && pwd -P)
trap 'rm -rf "$d"' EXIT

HOOK="$(cd "$(dirname "$0")/.." && pwd)/spawn-guard.sh"
HOME_DIR="$d/home"
CWD="$d/cwd"
WORK="$d/work"
LEDGER="$WORK/subagents/test-stage/spawns.jsonl"

fail() {
	echo "FAIL: $*" >&2
	exit 1
}

mkdir -p "$HOME_DIR" "$CWD/.claude/agents" "$WORK"
printf -- '---\nname: with-effort\nmodel: opus\neffort: medium\n---\n' >"$CWD/.claude/agents/with-effort.md"
printf -- '---\nname: no-effort\nmodel: sonnet\n---\n' >"$CWD/.claude/agents/no-effort.md"

# spawn <agent-type> [effort] - run the hook inside a stage and print the
# `effort` of the ledger row it appended.
spawn() {
	local payload
	payload=$(jq -nc --arg t "$1" --arg e "${2:-}" \
		'{tool_name:"Task",tool_input:({subagent_type:$t,description:"d",prompt:"p"} + (if $e != "" then {effort:$e} else {} end))}')
	rm -f "$LEDGER"
	(cd "$CWD" && printf '%s' "$payload" | env -i PATH="$PATH" HOME="$HOME_DIR" TMPDIR="$d" \
		LOOM_STAGE_ID=test-stage LOOM_MAIN_AGENT_PID=$$ LOOM_WORK_DIR="$WORK" LOOM_SESSION_ID=test-session \
		bash "$HOOK" >/dev/null 2>&1) || true
	[[ -f "$LEDGER" ]] || fail "no ledger row written for $1"
	jq -r '.effort' "$LEDGER"
}

[[ $(spawn with-effort high) == high ]] || fail "explicit effort not recorded"
[[ $(spawn with-effort) == medium ]] || fail "frontmatter effort default not recorded"
[[ -z $(spawn no-effort) ]] || fail "effort recorded though none is known"

echo "PASS"
