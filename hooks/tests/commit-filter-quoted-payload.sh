#!/usr/bin/env bash
# commit-filter-quoted-payload.sh - commit-filter.sh must scan a TOKENIZED
# command (loom_tokenize_command via the loom_tokens_* helpers in
# _common.sh), not regex-match the raw command string, so prose sitting
# inside ONE quoted argument - a codex-forward task brief, a heredoc test
# payload - is never mistaken for a real git/loom invocation, while a
# genuine `git commit`, `git add -A`, an eval-wrapped git command, an
# `env -u`-unset of the subagent gate, or an attribution trailer in a real
# commit message body still blocks exactly as before.
#
# Two confirmed false positives motivated this file:
#   (a) commit-filter.sh's subagent git-operation check - a HARD CONSTRAINTS
#       bullet inside a codex task brief telling the subagent NOT to use git
#       ("no git add, no git commit") blocked the forward, even though no
#       git binary was ever invoked - the whole brief is one quoted argv
#       token, never a command position.
#   (b) commit-filter.sh's anti-evasion guard - a heredoc BODY that
#       mentioned the gate variable name as pure test data, after `env -u`,
#       was blocked as "unsetting LOOM_MAIN_AGENT_PID" even though the body
#       is heredoc content, not shell (strip_embedded_content drops it
#       entirely before tokenizing).
set -euo pipefail

ROOT=$(cd "$(dirname "$0")/../.." && pwd)
HOOK="$ROOT/hooks/commit-filter.sh"

TMP=$(mktemp -d)
trap 'rm -rf "$TMP"' EXIT

# run_hook - Invoke commit-filter.sh with the given JSON payload on stdin,
# scrubbing LOOM_WORK_DIR/LOOM_STAGE_ID/LOOM_MAIN_AGENT_PID first (this suite
# may itself run inside a live loom stage - see subagent-gate-payload.sh's
# header for why an inherited LOOM_MAIN_AGENT_PID would otherwise silently
# change which branch a "no live loom session" case exercises), then
# layering on any caller-supplied env assignments from "$@" (e.g.
# LOOM_MAIN_AGENT_PID=$$ to fake a live loom-session ancestor for the
# subagent-only cases).
run_hook() {
	local input="$1"
	shift
	(cd "$TMP" && printf '%s' "$input" | env -u LOOM_WORK_DIR -u LOOM_STAGE_ID -u LOOM_MAIN_AGENT_PID "$@" bash "$HOOK" 2>/dev/null)
}

expect_exit() {
	local desc="$1" want="$2" input="$3"
	shift 3
	set +e
	run_hook "$input" "$@"
	local code=$?
	set -e
	if [[ $code -ne $want ]]; then
		echo "FAIL: $desc - expected exit $want, got exit $code"
		exit 1
	fi
}

# plain_payload - a Bash command with no agent_type/transcript_path, i.e.
# what the main agent's own Bash calls look like.
plain_payload() {
	jq -nc --arg c "$1" '{tool_name:"Bash",tool_input:{command:$c}}'
}

# subagent_payload - a Bash command whose payload identifies it as a
# Task-spawned subagent. Combined with a live LOOM_MAIN_AGENT_PID ancestor
# (passed via run_hook's "$@"), this makes loom_is_subagent return true.
subagent_payload() {
	jq -nc --arg c "$1" \
		'{tool_name:"Bash",tool_input:{command:$c},agent_type:"loom-software-engineer",transcript_path:"/h/.claude/projects/p/subagents/agent-x.jsonl"}'
}

# The gate variable's name, built by concatenation so this test file's own
# source never contains the literal contiguous string - the same care
# git-add-guard-quoting.sh and subagent-gate-payload.sh take with sensitive
# trigger text.
GATE_VAR_NAME="LOOM_MAIN""_AGENT_PID"

# =============================================================================
# ALLOW (exit 0) - the reported false positives
# =============================================================================

# (a) THE REGRESSION: the full codex-forward invocation, detected as a
# subagent, whose entire task brief is ONE single-quoted argument that
# happens to instruct the subagent not to touch git at all.
CODEX_FORWARD_CMD=$'~/.claude/hooks/loom/codex-forward.sh task \'Fix the bug.\nHARD CONSTRAINTS:\n- Do NOT run git at all (no git add, no git commit).\n\' --model gpt-5.6-terra --effort xhigh --write'
expect_exit "(a) codex-forward brief telling a subagent not to use git is allowed" \
	0 "$(subagent_payload "$CODEX_FORWARD_CMD")" LOOM_MAIN_AGENT_PID=$$

# (b) Prose about git commit inside echo, run as a detected subagent.
expect_exit "(b) 'echo do not run git commit' is allowed for a subagent" \
	0 "$(subagent_payload "echo 'do not run git commit'")" LOOM_MAIN_AGENT_PID=$$

# (c) THE OTHER REGRESSION: a heredoc BODY mentioning the gate variable name
# after `env -u`, as pure test data - not a real env -u invocation. No
# subagent context needed; the anti-evasion guard runs on every Bash call.
HEREDOC_CMD="cat <<'EOF'
some test data mentioning env -u ${GATE_VAR_NAME} here, not an actual invocation
EOF
"
expect_exit "(c) heredoc body mentioning the gate var after env -u is allowed" \
	0 "$(plain_payload "$HEREDOC_CMD")"

# =============================================================================
# BLOCK (exit 2) - must not regress
# =============================================================================

# (d) A real git commit by a detected subagent.
expect_exit "(d) real 'git commit' by a subagent is blocked" \
	2 "$(subagent_payload 'git commit -m "wip"')" LOOM_MAIN_AGENT_PID=$$

# (e) A real git add -A by a detected subagent.
expect_exit "(e) real 'git add -A' by a subagent is blocked" \
	2 "$(subagent_payload 'git add -A')" LOOM_MAIN_AGENT_PID=$$

# (f) A real loom stage complete by a detected subagent.
expect_exit "(f) real 'loom stage complete' by a subagent is blocked" \
	2 "$(subagent_payload 'loom stage complete my-stage')" LOOM_MAIN_AGENT_PID=$$

# (g) A commit carrying a real Co-Authored-By trailer naming Claude/Anthropic
# (attribution check must still fire on the real message body). No subagent
# context needed - this check applies to any Bash call.
expect_exit "(g) Co-Authored-By trailer naming Claude is blocked" \
	2 "$(plain_payload 'git commit -m "fix: thing" -m "Co-Authored-By: Claude <noreply@anthropic.com>"')"

# (h) A real eval wrapping git.
expect_exit "(h) eval wrapping a real git commit is blocked" \
	2 "$(plain_payload 'eval "git commit -m wip"')"

# (i) A real env -u unsetting the gate variable ahead of a real git commit.
expect_exit "(i) env -u unsetting the gate variable is blocked" \
	2 "$(plain_payload "env -u ${GATE_VAR_NAME} git commit -m wip")"

# =============================================================================
# A1 regression: the gate variable must be the OPERAND of unset/env -u, not
# merely a bare argv word anywhere in the command.
# =============================================================================

# (j) THE A1 REGRESSION: merely SEARCHING for the gate variable's name is a
# bare argv word, not an operand of `unset`/`env -u` - must be allowed.
expect_exit "(j) 'rg -n <gatevar> file' (bare mention, no unset/env -u) is allowed" \
	0 "$(plain_payload "rg -n ${GATE_VAR_NAME} hooks/_common.sh")"

# (k) A standalone `unset <gatevar>` (no git involved at all) must still be
# blocked - the anti-evasion guard applies to ALL Bash calls, not just ones
# that also invoke git/loom.
expect_exit "(k) standalone 'unset <gatevar>' is blocked" \
	2 "$(plain_payload "unset ${GATE_VAR_NAME}")"

# =============================================================================
# A2 regression: `loom stage complete` requires "stage" and "complete" as
# ADJACENT argv words in the SAME invoking segment, not two independent
# segments each satisfying one half.
# =============================================================================

# (l) THE A2 REGRESSION: two independent `loom` invocations, one mentioning
# "stage" and a different one mentioning "complete" - neither one is a real
# `loom stage complete`, so this must be allowed for a detected subagent.
expect_exit "(l) 'loom stage list && loom log complete' is allowed" \
	0 "$(subagent_payload "loom stage list && loom log complete")" LOOM_MAIN_AGENT_PID=$$

# =============================================================================
# A5: raw-regex FALLBACK branch coverage - nothing previously exercised the
# path loom_tokenize_command falls back to on an unterminated quote.
# =============================================================================

# (m) An UNTERMINATED QUOTE makes loom_tokenize_command return 1, forcing the
# pre-tokenizing regex fallback - a real subagent git-commit invocation
# inside that malformed command must still be blocked.
expect_exit "(m) unterminated quote wrapping a real subagent git commit is blocked" \
	2 "$(subagent_payload 'git commit -m "wip')" LOOM_MAIN_AGENT_PID=$$

# (n) Prose mentioning "loom stage complete" inside ONE quoted argument (a
# task-brief style reminder, not an invocation) must be allowed for a
# detected subagent - is_stage_complete_command must not fire on words that
# only sit inside a single whitespace-bearing quoted token.
expect_exit "(n) prose mentioning 'loom stage complete' in a quoted arg is allowed" \
	0 "$(subagent_payload "echo 'remember: only the main agent runs loom stage complete'")" LOOM_MAIN_AGENT_PID=$$

echo "PASS"
