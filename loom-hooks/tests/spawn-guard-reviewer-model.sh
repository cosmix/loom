#!/usr/bin/env bash
# spawn-guard.sh runs loom-code-reviewer on the model and effort printed by
# `loom hook reviewer-model`, over any caller-supplied model; a failing or
# malformed reply falls back to the agent definition, other agent types are untouched.
set -euo pipefail

unset LOOM_STAGE_ID LOOM_SESSION_ID LOOM_WORK_DIR LOOM_SESSION_TYPE LOOM_MAIN_AGENT_PID
d=$(mktemp -d "${TMPDIR:-/tmp}/spawn-reviewer.XXXXXX") && [[ -n "$d" ]]
d=$(cd "$d" && pwd -P)
trap 'rm -rf "$d"' EXIT

HOOK="$(cd "$(dirname "$0")/.." && pwd)/spawn-guard.sh"
HOME_DIR="$d/home"
CWD="$d/cwd"
STUB="$d/loom-stub"

fail() {
	echo "FAIL: $*" >&2
	exit 1
}

mkdir -p "$HOME_DIR/.claude/agents" "$CWD/.claude/agents"
printf -- '---\nname: loom-code-reviewer\nmodel: haiku\n---\n' >"$CWD/.claude/agents/loom-code-reviewer.md"
printf -- '---\nname: other-agent\nmodel: haiku\n---\n' >"$CWD/.claude/agents/other-agent.md"

# set_stub <reviewer-model reply> <exit code>
set_stub() {
	cat >"$STUB" <<STUB_EOF
#!/usr/bin/env bash
if [[ "\$1 \$2" == "hook reviewer-model" ]]; then
	printf '%s\n' '$1'
	exit $2
fi
exit 1
STUB_EOF
	chmod +x "$STUB"
}

# spawn <agent-type> [model] - the full hook output for a spawn from $CWD.
spawn() {
	local payload
	payload=$(jq -nc --arg t "$1" --arg m "${2:-}" \
		'{tool_name:"Task",tool_input:({subagent_type:$t,description:"d",prompt:"p"} + (if $m != "" then {model:$m} else {} end))}')
	(cd "$CWD" && printf '%s' "$payload" | env -i PATH="$PATH" HOME="$HOME_DIR" LOOM_BIN="$STUB" bash "$HOOK" 2>/dev/null) || true
}
field() { jq -r "$1 // empty"; }

set_stub 'model=opus effort=high' 0
out=$(spawn loom-code-reviewer)
[[ $(field .hookSpecificOutput.updatedInput.model <<<"$out") == opus ]] || fail "omitted model not set to configured opus"
[[ $(field .hookSpecificOutput.updatedInput.effort <<<"$out") == high ]] || fail "effort not injected"

out=$(spawn loom-code-reviewer sonnet)
[[ $(field .hookSpecificOutput.updatedInput.model <<<"$out") == opus ]] || fail "explicit sonnet not replaced"
[[ $(field .hookSpecificOutput.updatedInput.effort <<<"$out") == high ]] || fail "effort missing on replaced model"
[[ $(field .hookSpecificOutput.additionalContext <<<"$out") == *"the requested 'sonnet' was replaced"* ]] || fail "replacement note missing"

out=$(spawn loom-code-reviewer opus)
[[ $(field .hookSpecificOutput.additionalContext <<<"$out") != *"was replaced"* ]] || fail "note emitted though model matched"

set_stub 'garbage here' 0
out=$(spawn loom-code-reviewer)
[[ $(field .hookSpecificOutput.updatedInput.model <<<"$out") == haiku ]] || fail "malformed output did not fall back to frontmatter"
[[ -z $(field .hookSpecificOutput.updatedInput.effort <<<"$out") ]] || fail "effort injected from malformed output"
[[ $(field .hookSpecificOutput.additionalContext <<<"$out") == *"loom hook reviewer-model failed; loom-code-reviewer runs its frontmatter model"* ]] || fail "malformed output warning missing"

set_stub 'model=opus effort=high' 1
out=$(spawn loom-code-reviewer)
[[ $(field .hookSpecificOutput.updatedInput.model <<<"$out") == haiku ]] || fail "non-zero exit did not fall back to frontmatter"
[[ $(field .hookSpecificOutput.additionalContext <<<"$out") == *"loom hook reviewer-model failed"*"models.code_reviewer_model"* ]] || fail "non-zero exit warning missing"

# stderr's first line reaches the warning with control characters stripped.
cat >"$STUB" <<'STUB_EOF'
#!/usr/bin/env bash
printf 'bad config\033[31m line\nsecond\n' >&2
exit 1
STUB_EOF
chmod +x "$STUB"
out=$(spawn loom-code-reviewer)
[[ $(field .hookSpecificOutput.additionalContext <<<"$out") == *"failed (bad config[31m line);"* ]] || fail "stderr not in warning"

set_stub 'model=opus effort=high' 0
out=$(spawn other-agent)
[[ $(field .hookSpecificOutput.updatedInput.model <<<"$out") == haiku ]] || fail "non-reviewer agent was rewritten"
[[ -z $(field .hookSpecificOutput.updatedInput.effort <<<"$out") ]] || fail "non-reviewer agent got effort"

echo "PASS"
