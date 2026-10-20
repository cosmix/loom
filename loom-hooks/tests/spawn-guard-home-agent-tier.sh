#!/usr/bin/env bash
# spawn-guard.sh resolves an agent's model tier from <cwd>/.claude/agents/<type>.md
# first, then falls back to ~/.claude/agents/<type>.md.
set -euo pipefail

unset LOOM_STAGE_ID LOOM_SESSION_ID LOOM_WORK_DIR LOOM_SESSION_TYPE LOOM_MAIN_AGENT_PID
d=$(mktemp -d "${TMPDIR:-/tmp}/spawn-tier.XXXXXX") && [[ -n "$d" ]]
d=$(cd "$d" && pwd -P)
trap 'rm -rf "$d"' EXIT

HOOK="$(cd "$(dirname "$0")/.." && pwd)/spawn-guard.sh"
HOME_DIR="$d/home"
CWD="$d/cwd"

fail() {
	echo "FAIL: $*" >&2
	exit 1
}

mkdir -p "$HOME_DIR/.claude/agents" "$CWD/.claude/agents"
printf -- '---\nname: probe-agent\nmodel: opus\n---\n' >"$HOME_DIR/.claude/agents/probe-agent.md"

# spawn_model - the model spawn-guard injects for a probe-agent spawn from $CWD.
spawn_model() {
	local payload
	payload=$(jq -nc '{tool_name:"Task",tool_input:{subagent_type:"probe-agent",description:"d",prompt:"p"}}')
	(cd "$CWD" && printf '%s' "$payload" | env -i PATH="$PATH" HOME="$HOME_DIR" bash "$HOOK" 2>/dev/null |
		jq -r '.hookSpecificOutput.updatedInput.model // empty') || true
}

[[ $(spawn_model) == opus ]] || fail "spawn-guard did not fall back to ~/.claude/agents"

printf -- '---\nname: probe-agent\nmodel: haiku\n---\n' >"$CWD/.claude/agents/probe-agent.md"
[[ $(spawn_model) == haiku ]] || fail "spawn-guard did not prefer the project agent definition"

echo "PASS"
