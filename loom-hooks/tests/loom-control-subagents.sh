#!/usr/bin/env bash
# loom-control-subagents.sh: the stage's main agent runs `loom subagents start`,
# `message` and `cancel` in exactly one spelling, rewritten to the trusted loom
# binary; every other spelling and every subagent caller is refused, and every
# other command passes silently.
set -euo pipefail

ROOT=$(cd "$(dirname "$0")/../.." && pwd)
HOOK="$ROOT/loom-hooks/loom-control-subagents.sh"
TMP=$(mktemp -d "${TMPDIR:-/tmp}/loom-hooktest.XXXXXX")
trap 'rm -rf "$TMP"' EXIT
WORKTREE="$TMP/repo/.worktrees/build-api"
mkdir -p "$TMP/bin" "$WORKTREE" "$TMP/home"

# The hook only compares paths; the binary is never run.
cat >"$TMP/bin/loom" <<'SH'
#!/usr/bin/env bash
exit 0
SH
chmod +x "$TMP/bin/loom"
STUB="$TMP/bin/loom"
HOOK_RC=0
HOOK_OUTPUT=""

run_payload() {
	set +e
	HOOK_OUTPUT=$(printf '%s' "$1" |
		env -u LOOM_BIN PATH="$TMP/bin:/usr/bin:/bin" HOME="$TMP/home" LOOM_CONTROL_TESTING=1 \
			LOOM_CONTROL_TEST_BIN="$STUB" \
			LOOM_STAGE_ID="${TEST_STAGE_ID-build-api}" LOOM_SESSION_ID="${TEST_SESSION_ID-session-123}" \
			LOOM_WORKTREE_PATH="${TEST_WORKTREE_OVERRIDE:-$WORKTREE}" bash "$HOOK" 2>&1)
	HOOK_RC=$?
	set -e
}

# run_pre <command> [agent_type] [tool_name]
run_pre() {
	local payload
	payload=$(jq -n --arg command "$1" --arg agent "${2:-}" --arg tool "${3:-Bash}" \
		'{tool_name:$tool,tool_input:{command:$command}} + (if $agent == "" then {} else {agent_type:$agent} end)')
	run_payload "$payload"
}

fail() {
	echo "FAIL: $CASE: $1" >&2
	exit 1
}

assert_rc() {
	[[ "$HOOK_RC" == "$1" ]] || fail "expected rc $1, got $HOOK_RC: $HOOK_OUTPUT"
}

# expect_allow <name> <command> <verb> <rest> - rc 0 and the rewrite on the stub.
expect_allow() {
	CASE=$1
	run_pre "$2"
	assert_rc 0
	local got
	got=$(jq -r .hookSpecificOutput.updatedInput.command <<<"$HOOK_OUTPUT")
	[[ "$got" == "$STUB subagents $3$4" ]] || fail "rewritten to '$got'"
	[[ "$(jq -r .hookSpecificOutput.permissionDecision <<<"$HOOK_OUTPUT")" == allow ]] || fail "no allow: $HOOK_OUTPUT"
}

# expect_block <name> <command> [agent_type]
expect_block() {
	CASE=$1
	run_pre "$2" "${3:-}"
	assert_rc 2
	[[ "$HOOK_OUTPUT" == *'LOOM_CONTROL_ERROR:'* ]] || fail "no refusal: $HOOK_OUTPUT"
}

# expect_silent <name> <command> [agent_type] [tool_name]
expect_silent() {
	CASE=$1
	run_pre "$2" "${3:-}" "${4:-Bash}"
	assert_rc 0
	[[ -z "$HOOK_OUTPUT" ]] || fail "expected no output: $HOOK_OUTPUT"
}

INV=inv-0123456789abcdef0123456789abcdef
START=" --provider codex --unit-id parser-1 --brief /w/doc/brief.md --model gpt-6.1-terra --effort xhigh"
MSG=" --worker codex:parser-1 --invocation $INV --delivery steer --message-id m-1"
CANCEL=" --worker codex:parser-1 --invocation $INV"
VALID="loom subagents start$START"

expect_allow allow_start_bare_loom_is_rewritten "$VALID" start "$START"
expect_allow allow_start_absolute_trusted_binary "$STUB subagents start$START" start "$START"
expect_allow allow_trailing_2_gt_1 "$VALID 2>&1" start "$START 2>&1"
expect_allow allow_message_text_single_quoted "loom subagents message$MSG --text 'keep going; use \$HOME | x'" \
	message "$MSG --text 'keep going; use \$HOME | x'"
expect_allow allow_message_text_double_quoted "loom subagents message$MSG --text \"add the tests, then stop\"" \
	message "$MSG --text \"add the tests, then stop\""
expect_allow allow_message_file "loom subagents message$MSG --file /w/doc/note.md" message "$MSG --file /w/doc/note.md"
expect_allow allow_cancel_reason_quoted "loom subagents cancel$CANCEL --reason 'wrong brief'" \
	cancel "$CANCEL --reason 'wrong brief'"
REORDERED=" --effort high --brief '/w/doc/my brief.md' --model gpt-6.1-sol --unit-id u2 --provider codex"
expect_allow allow_flags_in_any_order "loom subagents start$REORDERED" start "$REORDERED"
expect_allow allow_single_quoted_substitution_is_inert "loom subagents cancel$CANCEL --reason '\$(id)'" \
	cancel "$CANCEL --reason '\$(id)'"

ln -s "$STUB" "$TMP/bin/loom-link"
LONG_TEXT=$(head -c 9000 /dev/zero | tr '\0' a)
expect_block block_pipe "$VALID | cat"
expect_block block_and_and "$VALID && true"
expect_block block_semicolon "$VALID; true"
expect_block block_redirect_to_file "$VALID > out.txt"
expect_block block_background "$VALID &"
expect_block block_newline "$VALID"$'\n'"true"
expect_block block_command_substitution "loom subagents cancel$CANCEL --reason \$(id)"
expect_block block_backtick_outside_quotes "loom subagents cancel$CANCEL --reason \`id\`"
expect_block block_unquoted_variable "loom subagents cancel$CANCEL --reason \$HOME"
expect_block block_double_quoted_expansion "loom subagents cancel$CANCEL --reason \"\$(id)\""
expect_block block_name_prefix "FOO=1 $VALID"
expect_block block_env_wrapper "env $VALID"
expect_block block_shell_c_wrapper "bash -c '$VALID'"
expect_block block_env_unset_wrapper "env -u LOOM_SESSION_ID loom subagents cancel$CANCEL --reason x"
[[ "$HOOK_OUTPUT" == *'an env wrapper'* ]] || fail "refusal does not name the env wrapper: $HOOK_OUTPUT"
expect_block block_name_prefix_reason "FOO=1 $VALID"
[[ "$HOOK_OUTPUT" == *'NAME=value prefix'* ]] || fail "refusal does not name the prefix: $HOOK_OUTPUT"
# Every wrapper and keyword loom_tokens_command_word_index sees through, and so
# loom-relay.sh relays as subagents:control, is detected and refused.
for wrapped in "time $VALID" "command $VALID" "exec $VALID" "nohup $VALID" "timeout 5 $VALID" \
	"timeout -s KILL 5 $VALID" "nice $VALID" "nice -n 5 $VALID" "setsid $VALID" "stdbuf -o L $VALID" \
	"sudo $VALID" "sudo -E $VALID" "xargs $VALID" "/usr/bin/time $VALID" "{ $VALID; }" \
	"if $VALID; then true; fi" "! $VALID" "FOO=1 time $VALID"; do
	expect_block block_wrapper_or_keyword "$wrapped"
done
expect_block block_wrapper_reason "time $VALID"
[[ "$HOOK_OUTPUT" == *'wrapper or shell keyword (time)'* ]] || fail "refusal does not name the wrapper: $HOOK_OUTPUT"
expect_block block_xargs_after_pipe "true | xargs $VALID"
expect_block block_unit_is_not_a_clap_flag "loom subagents start${START/--unit-id/--unit}"
expect_block block_relative_loom "./loom subagents start$START"
expect_block block_symlinked_binary "$TMP/bin/loom-link subagents start$START"
expect_block block_duplicate_flag "$VALID --effort high"
expect_block block_unknown_flag "$VALID --force yes"
expect_block block_stage_flag_on_cancel "loom subagents cancel --stage build-api --worker codex:parser-1 --reason x"
expect_block block_provider_not_codex "loom subagents start${START/codex/claude}"
expect_block block_bad_unit_id "loom subagents start${START/parser-1/-bad}"
expect_block block_relative_brief "loom subagents start${START/\/w\/doc\/brief.md/doc/brief.md}"
expect_block block_dotdot_brief "loom subagents start${START/\/w\/doc\/brief.md//w/../etc/brief.md}"
expect_block block_text_and_file_together "loom subagents message$MSG --text hi --file /w/n.md"
expect_block block_neither_text_nor_file "loom subagents message$MSG"
expect_block block_bad_invocation_id "loom subagents cancel --worker codex:parser-1 --invocation inv-XYZ --reason x"
expect_block block_missing_flag "loom subagents start --provider codex --unit-id u1 --brief /w/b.md --model m"
expect_block block_trailing_garbage_after_2_gt_1 "$VALID 2>&1 x"
expect_block block_oversized_command "loom subagents message$MSG --text '$LONG_TEXT'"
expect_block block_quoted_verb "loom subagents 'start'$START"
expect_block block_tab_separator "loom subagents start	--provider codex"

expect_block block_subagent_caller "$VALID" general-purpose
[[ "$HOOK_OUTPUT" == *"main agent"* ]] || fail "refusal does not name the main agent: $HOOK_OUTPUT"

expect_silent silent_quoted_mention 'echo "loom subagents start --provider codex"'
expect_silent silent_quoted_mention 'git commit -m "loom subagents cancel x"'
for verb in status result release list harvest watch supervise; do
	expect_silent silent_other_subagents_commands "loom subagents $verb --worker codex:u1"
done
expect_silent silent_wrapped_non_worker_command "time loom status"
expect_silent silent_wrapped_other_subagents_command "time loom subagents status --worker codex:u1"
expect_silent silent_non_bash_tool "$VALID" "" Read
CASE=silent_outside_a_worktree
TEST_WORKTREE_OVERRIDE="$TMP/repo" run_pre "$VALID"
assert_rc 0
[[ -z "$HOOK_OUTPUT" ]] || fail "expected no output: $HOOK_OUTPUT"
CASE=silent_without_stage_identity
TEST_STAGE_ID="" run_pre "$VALID"
assert_rc 0
[[ -z "$HOOK_OUTPUT" ]] || fail "expected no output: $HOOK_OUTPUT"
expect_silent silent_subagent_unrelated_command 'cargo test --lib' general-purpose

# The test-binary seam is honoured only next to a tests/ directory: an installed
# copy never trusts LOOM_CONTROL_TEST_BIN.
CASE=production_copy_ignores_test_binary
installed="$TMP/installed"
mkdir -p "$installed"
cp "$ROOT"/loom-hooks/_*.sh "$installed/"
cp "$HOOK" "$installed/loom-control-subagents.sh"
set +e
out=$(jq -n --arg command "$VALID" '{tool_name:"Bash",tool_input:{command:$command}}' |
	env -u LOOM_BIN PATH="/usr/bin:/bin" HOME="$TMP/empty-home" LOOM_CONTROL_TESTING=1 LOOM_CONTROL_TEST_BIN="$STUB" \
		LOOM_STAGE_ID=build-api LOOM_SESSION_ID=session-123 LOOM_WORKTREE_PATH="$WORKTREE" \
		bash "$installed/loom-control-subagents.sh" 2>&1)
rc=$?
set -e
# A host with a loom at a fixed install location pins that one instead.
[[ "$out" != *"$STUB"* ]] || fail "the test binary escaped the harness: $out"
[[ "$rc" == 0 || ("$rc" == 2 && "$out" == *'no trusted loom binary'*) ]] || fail "rc $rc: $out"

echo PASS
