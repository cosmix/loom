#!/usr/bin/env bash
# Hooks derive their asset root from their own location (<root>/hooks/loom).
# With a root other than $HOME/.claude: the codex wrapper is authorized only as
# the exact absolute path under that root, SKILL.md reads are exempt under that
# root's skills/ and loom-skill-catalog/ only, and agent tiers resolve from
# that root's agents/.
# The literal tilde is the spelling under test.
# shellcheck disable=SC2088
set -euo pipefail

unset LOOM_STAGE_ID LOOM_SESSION_ID LOOM_WORK_DIR LOOM_SESSION_TYPE LOOM_MAIN_AGENT_PID
d=$(mktemp -d "${TMPDIR:-/tmp}/custom-root.XXXXXX") && [[ -n "$d" ]]
d=$(cd "$d" && pwd -P)
trap 'rm -rf "$d"' EXIT
source "$(dirname "$0")/_stage_hooks.sh"

fail() {
	echo "FAIL: $*" >&2
	exit 1
}

HOME_DIR="$d/home"
ROOT="$d/custom root"
OTHER="$d/other"
mkdir -p "$HOME_DIR" "$ROOT" "$OTHER"
HOOKS=$(stage_hooks "$ROOT")
OTHER_HOOKS=$(stage_hooks "$OTHER")

# --- C1: wrapper matching is exact ------------------------------------------
check_wrapper() {
	local hooks="$1" word="$2" want="$3" plain="${4:-1}" got=0
	(HOME="$HOME_DIR" bash -c 'source "$1/_codex_forward.sh"; loom_is_forward_wrapper "$2" "$3"' _ "$hooks" "$word" "$plain") || got=$?
	[[ "$got" == "$want" ]] || fail "wrapper '$word' under $hooks: exit $got, want $want"
}
check_wrapper "$HOOKS" "$HOOKS/codex-forward.sh" 0
check_wrapper "$HOOKS" "$OTHER_HOOKS/codex-forward.sh" 1
check_wrapper "$HOOKS" "$HOOKS/codex-forward.sh/" 1
check_wrapper "$HOOKS" "$HOOKS/../loom/codex-forward.sh" 1
check_wrapper "$HOOKS" "~/.claude/hooks/loom/codex-forward.sh" 1
check_wrapper "$HOOKS" "$HOME_DIR/.claude/hooks/loom/codex-forward.sh" 1
# A hook tree that is not under hooks/loom has no absolute spelling to offer
# other than its own directory, which is still exact.
HOME_HOOKS=$(stage_hooks "$HOME_DIR/.claude")
check_wrapper "$HOME_HOOKS" "~/.claude/hooks/loom/codex-forward.sh" 0
check_wrapper "$HOME_HOOKS" "$HOME_DIR/.claude/hooks/loom/codex-forward.sh" 0
check_wrapper "$HOME_HOOKS" "$HOOKS/codex-forward.sh" 1
# Tilde spelling needs a fully unquoted word 0 (plain=0 rejects it).
check_wrapper "$HOME_HOOKS" "~/.claude/hooks/loom/codex-forward.sh" 1 0
check_wrapper "$HOME_HOOKS" "$HOME_DIR/.claude/hooks/loom/codex-forward.sh" 0 0

# parse_word0_ok <command> <hooks> - parse the command, then judge word 0.
parse_word0_ok() {
	HOME="$HOME_DIR" bash -c 'source "$1/_codex_forward.sh"; parse_shell_words "$2" || exit 1
		loom_is_forward_wrapper "${PARSED_WORDS[0]}" "$PARSED_WORD0_PLAIN"' _ "$2" "$1" >/dev/null 2>&1
}
want_word0() {
	local want="$1" cmd="$2" got=0
	parse_word0_ok "$cmd" "$HOME_HOOKS" || got=$?
	[[ "$got" == "$want" ]] || fail "word0 command '$cmd': exit $got, want $want"
}
ARGS=' task x --model gpt-5.6-terra --effort high --write'
T='~/.claude/hooks/loom/codex-forward.sh'
ABS="$HOME_DIR/.claude/hooks/loom/codex-forward.sh"
want_word0 0 "$T$ARGS"
want_word0 1 "'$T'$ARGS"
want_word0 1 "\"$T\"$ARGS"
want_word0 1 "\\~/.claude/hooks/loom/codex-forward.sh$ARGS"
want_word0 1 "~/.claude/hooks/loom/codex-forward.sh'' $ARGS"
# shellcheck disable=SC2016
want_word0 1 '$HOME/.claude/hooks/loom/codex-forward.sh'"$ARGS"
want_word0 1 "'\$HOME/.claude/hooks/loom/codex-forward.sh'$ARGS"
want_word0 1 "\\\$HOME/.claude/hooks/loom/codex-forward.sh$ARGS"
want_word0 0 "$ABS$ARGS"
want_word0 0 "'$ABS'$ARGS"
want_word0 0 "\"$ABS\"$ARGS"

# --- C2: SKILL.md exemption --------------------------------------------------
WORKTREE="$d/repo/.worktrees/build-api"
mkdir -p "$WORKTREE" "$ROOT/skills/foo" "$ROOT/loom-skill-catalog/bar" "$ROOT/skills/a/b" "$ROOT/sibling/foo" "$OTHER/skills/foo"
for f in "$ROOT/skills/foo/SKILL.md" "$ROOT/loom-skill-catalog/bar/SKILL.md" "$ROOT/skills/a/b/SKILL.md" \
	"$ROOT/sibling/foo/SKILL.md" "$OTHER/skills/foo/SKILL.md"; do printf 'x\n' >"$f"; done
read_rc() {
	local payload rc=0
	payload=$(jq -nc --arg p "$1" '{tool_name:"Read",tool_input:{file_path:$p}}')
	(cd "$WORKTREE" && printf '%s' "$payload" | env -i PATH="$PATH" HOME="$HOME_DIR" bash "$HOOKS/worktree-file-guard.sh" >/dev/null 2>&1) || rc=$?
	printf '%s' "$rc"
}
[[ $(read_rc "$ROOT/skills/foo/SKILL.md") == 0 ]] || fail "own-root skill read blocked"
[[ $(read_rc "$ROOT/loom-skill-catalog/bar/SKILL.md") == 0 ]] || fail "own-root catalog read blocked"
[[ $(read_rc "$ROOT/skills/a/b/SKILL.md") == 2 ]] || fail "multi-component skill name allowed"
[[ $(read_rc "$ROOT/sibling/foo/SKILL.md") == 2 ]] || fail "sibling path allowed"
[[ $(read_rc "$OTHER/skills/foo/SKILL.md") == 2 ]] || fail "other root skill allowed"

# --- C3: agent tier from the own root ----------------------------------------
mkdir -p "$ROOT/agents" "$HOME_DIR/.claude/agents" "$d/cwd"
printf -- '---\nname: probe-agent\nmodel: haiku\n---\n' >"$ROOT/agents/probe-agent.md"
printf -- '---\nname: probe-agent\nmodel: opus\n---\n' >"$HOME_DIR/.claude/agents/probe-agent.md"
spawn_model() {
	local hooks="$1" payload
	payload=$(jq -nc '{tool_name:"Task",tool_input:{subagent_type:"probe-agent",description:"d",prompt:"p"}}')
	(cd "$d/cwd" && printf '%s' "$payload" | env -i PATH="$PATH" HOME="$HOME_DIR" bash "$hooks/spawn-guard.sh" 2>/dev/null |
		jq -r '.hookSpecificOutput.updatedInput.model // empty') || true
}
[[ $(spawn_model "$HOOKS") == haiku ]] || fail "spawn-guard did not prefer the own-root agent definition"
[[ $(spawn_model "$HOME_HOOKS") == opus ]] || fail "spawn-guard did not fall back to ~/.claude/agents"

printf '%s\n' PASS
