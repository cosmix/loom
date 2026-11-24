#!/usr/bin/env bash
# Runs every hook test, LOOM_HOOK_TEST_JOBS at a time (default: CPU count,
# capped at 8). Each test runs in its own process with its output and exit
# status in a private results directory; the report is printed afterwards in
# the declared order, so it reads the same at any concurrency.
set -euo pipefail
SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
PASS=0
FAIL=0
SKIPS=0
ERRORS=()
NAMES=()
SCRIPTS=()
RUNNING=()

# LOOM_HOOK_TEST_BSD=1 runs every test with the BSD tool shims first on PATH
# (padded wc output, BSD-only stat and date options), as on macOS. The shim
# directory is built once here and shared read-only by every job.
source "$SCRIPT_DIR/_bsd_path.sh"
BSD_DIR=""
RESULTS_DIR=$(mktemp -d "${TMPDIR:-/tmp}/loom-hooktests.XXXXXX")
cleanup() {
	rm -rf "$RESULTS_DIR" ${BSD_DIR:+"$BSD_DIR"}
}
trap cleanup EXIT
trap 'exit 130' INT
trap 'exit 143' TERM
if [[ "${LOOM_HOOK_TEST_BSD:-}" == "1" ]]; then
	BSD_DIR=$(bsd_shim_dir)
	echo "BSD tool shims active"
fi

# default_jobs - CPU count (Linux getconf, macOS sysctl, else 1), capped at 8.
default_jobs() {
	local n
	n=$(getconf _NPROCESSORS_ONLN 2>/dev/null || sysctl -n hw.ncpu 2>/dev/null || echo 1)
	[[ "$n" =~ ^[1-9][0-9]*$ ]] || n=1
	if [[ "$n" -gt 8 ]]; then n=8; fi
	echo "$n"
}

JOBS="${LOOM_HOOK_TEST_JOBS:-}"
if [[ -z "$JOBS" ]]; then
	JOBS=$(default_jobs)
elif ! [[ "$JOBS" =~ ^[1-9][0-9]*$ ]]; then
	echo "run-all.sh: LOOM_HOOK_TEST_JOBS must be a positive integer, got '$JOBS'" >&2
	exit 2
fi

# run_test <name> <script> - register one test; it runs after the list is read.
run_test() {
	NAMES+=("$1")
	SCRIPTS+=("$2")
}

# run_job <index> - run one registered test and record <index>.out and, last
# and atomically, <index>.rc. A job that dies before writing .rc counts as FAIL.
run_job() {
	local i="$1" rc=0 path_args=()
	if [[ -n "$BSD_DIR" ]]; then
		path_args=(PATH="$BSD_DIR:$PATH")
	fi
	# A loom session exports these; inherited, they would override the stub
	# binaries and PATHs each test hands its hooks. LOOM_HOOK_PATH in
	# particular splices the real PATH back in over the shim directory. The
	# session identity variables would leak into any test that does not scrub
	# its own.
	env -u LOOM_HOOK_PATH -u LOOM_BIN -u LOOM_HOOK_CONTEXT \
		-u LOOM_SCRATCH_DIR -u LOOM_SESSION_TYPE -u LOOM_STAGE_ID -u LOOM_SESSION_ID \
		-u LOOM_WORK_DIR -u LOOM_WORKTREE_PATH -u LOOM_MAIN_AGENT_PID -u LOOM_CODEX_FORWARD \
		${path_args[@]+"${path_args[@]}"} \
		bash "${SCRIPTS[$i]}" >"$RESULTS_DIR/$i.out" 2>&1 </dev/null || rc=$?
	echo "$rc" >"$RESULTS_DIR/$i.rc.tmp"
	mv "$RESULTS_DIR/$i.rc.tmp" "$RESULTS_DIR/$i.rc"
}

# reap_finished - drop pids that have exited from RUNNING.
reap_finished() {
	local live=() pid
	for pid in ${RUNNING[@]+"${RUNNING[@]}"}; do
		if kill -0 "$pid" 2>/dev/null; then
			live+=("$pid")
		else
			wait "$pid" 2>/dev/null || true
		fi
	done
	if [[ ${#live[@]} -gt 0 ]]; then RUNNING=("${live[@]}"); else RUNNING=(); fi
}

# run_all_jobs - start every registered test, at most $JOBS at once, and wait
# for the last. Polling (no `wait -n`) keeps this working on bash 3.2.
run_all_jobs() {
	local i
	for ((i = 0; i < ${#NAMES[@]}; i++)); do
		reap_finished
		while [[ ${#RUNNING[@]} -ge $JOBS ]]; do
			sleep 0.05
			reap_finished
		done
		run_job "$i" &
		RUNNING+=("$!")
	done
	while [[ ${#RUNNING[@]} -gt 0 ]]; do
		sleep 0.05
		reap_finished
	done
}

# report_job <index> - print one test's result block and update the counters.
report_job() {
	local i="$1" name rc="" output="" line
	name="${NAMES[$i]}"
	if [[ -f "$RESULTS_DIR/$i.rc" ]]; then
		rc=$(<"$RESULTS_DIR/$i.rc")
		output=$(<"$RESULTS_DIR/$i.out")
	else
		output="job ended without an exit status (killed?)"
	fi
	if [[ "$rc" == "0" ]]; then
		echo "  PASS: $name"
		((PASS++)) || true
		# A test that skipped a case still exits 0; its reason must stay visible.
		while IFS= read -r line; do
			case "$line" in
			skip:* | SKIP:*)
				echo "    $line"
				((SKIPS++)) || true
				;;
			esac
		done <<<"$output"
	else
		echo "  FAIL: $name"
		echo "    Output: $output"
		((FAIL++)) || true
		ERRORS+=("$name")
	fi
}

echo "Running hook tests..."
run_test "hook stdin: held-open pipes without GNU timeout" "$SCRIPT_DIR/bounded-stdin.sh"
run_test "direct Codex: retire descendants after leader exit" "$SCRIPT_DIR/codex-direct-lifecycle.sh"
run_test "prefer-modern-tools: grep triggers warning" "$SCRIPT_DIR/prefer-modern-tools-grep.sh"
run_test "prefer-modern-tools: find triggers warning" "$SCRIPT_DIR/prefer-modern-tools-find.sh"
run_test "prefer-modern-tools: rg has no warning" "$SCRIPT_DIR/prefer-modern-tools-rg.sh"
run_test "prefer-modern-tools: quoted grep no warning" "$SCRIPT_DIR/prefer-modern-tools-quoted.sh"
run_test "prefer-modern-tools: grep pipeline filter is exempt, real invocations still warn" "$SCRIPT_DIR/prefer-modern-tools-grep-pipeline-filter.sh"
run_test "prefer-modern-tools: cat with a single unredirected file operand warns" "$SCRIPT_DIR/prefer-modern-tools-cat.sh"
run_test "prefer-modern-tools: sed -i warns toward the Edit tool" "$SCRIPT_DIR/prefer-modern-tools-sed.sh"
run_test "prefer-modern-tools: ls with a path operand warns toward fd" "$SCRIPT_DIR/prefer-modern-tools-ls.sh"
run_test "prefer-modern-tools: head/tail warn only with a real file operand" "$SCRIPT_DIR/prefer-modern-tools-head-tail.sh"
run_test "prefer-modern-tools: each tool family warns at most once per session" "$SCRIPT_DIR/prefer-modern-tools-warn-once.sh"
run_test "post-tool-use: heartbeat only, no tool event" "$SCRIPT_DIR/post-tool-use-tool-event.sh"
run_test "progress-heartbeat: polling advances observation, not progress" "$SCRIPT_DIR/progress-heartbeat.sh"
run_test "post-tool-use does not persist tool output" "$SCRIPT_DIR/post-tool-use-empty-output.sh"
run_test "post-tool-use: forwarding Bash invokes the receipt writer only" "$SCRIPT_DIR/post-tool-use-forward-receipt.sh"
run_test "loom-control-complete: exact pinned command only" "$SCRIPT_DIR/loom-control-complete.sh"
run_test "loom-control-complete: tokenized pre-filter catches forged quoting, ignores prose" "$SCRIPT_DIR/loom-control-tokenized-prefilter.sh"
run_test "loom-control-subagents: one pinned spelling of start, message and cancel, main agent only" "$SCRIPT_DIR/loom-control-subagents.sh"
run_test "loom-control-complete: Knowledge sessions are pinned, heredoc prose is data" "$SCRIPT_DIR/loom-control-complete-knowledge.sh"
run_test "loom-control-complete: inert heredoc bodies ignored, shell-fed or ambiguous ones held" "$SCRIPT_DIR/loom-control-complete-heredoc.sh"
run_test "loom-control-complete: admitted completion marks the heartbeat until a main-agent progress write" "$SCRIPT_DIR/loom-control-complete-heartbeat.sh"
run_test "session-start: compact source emits re-anchor" "$SCRIPT_DIR/session-start-compact.sh"
run_test "session-end: records the SessionEnd reason from stdin" "$SCRIPT_DIR/session-end-reason.sh"
run_test "ask-user hooks: act only on AskUserQuestion, record the trigger" "$SCRIPT_DIR/ask-user-hooks.sh"
run_test "worktree-file-guard: empty and malformed input are blocked" "$SCRIPT_DIR/worktree-file-guard-truncated-input.sh"
run_test "worktree-file-guard: MultiEdit honours the worktree boundary" "$SCRIPT_DIR/worktree-file-guard-multiedit.sh"
run_test "worktree-file-guard: the session scratchpad is open, its surroundings stay blocked" "$SCRIPT_DIR/worktree-file-guard-scratchpad.sh"
run_test "worktree guards: an inherited LOOM_WORKTREE_PATH binds only its own session" "$SCRIPT_DIR/worktree-guards-inherited-env.sh"
run_test "spawn-guard: agent tier falls back to ~/.claude/agents" "$SCRIPT_DIR/spawn-guard-home-agent-tier.sh"
run_test "spawn-guard: loom-code-reviewer runs the configured model and effort" "$SCRIPT_DIR/spawn-guard-reviewer-model.sh"
run_test "spawn-guard: the spawn ledger records the effort" "$SCRIPT_DIR/spawn-guard-records-effort.sh"
run_test "spawn-guard: reviewer brief appended once, fails open" "$SCRIPT_DIR/spawn-guard-reviewer-brief.sh"
run_test "credential-guard: capability tokens blocked through the state-root symlink" "$SCRIPT_DIR/credential-guard-tokens.sh"
run_test "credential-guard: sandbox denyRead gates file tools, search roots above a denial run" "$SCRIPT_DIR/credential-guard-deny-read.sh"
run_test "credential-guard: effective session policy and aliased state root" "$SCRIPT_DIR/credential-guard-session-policy.sh"
run_test "credential-guard: a saved terminal history is closed to a loom session, open to the operator" "$SCRIPT_DIR/credential-guard-terminal-history.sh"
run_test "credential-guard: a .. after a nonexistent component is normalized, not treated as unresolvable" "$SCRIPT_DIR/credential-guard-dotdot.sh"
run_test "plans-path-guard: blocks ~/.claude/plans write" "$SCRIPT_DIR/plans-path-guard-blocks-claude-plans.sh"
run_test "plans-path-guard: blocks ~/.claude/projects/*/plans edit" "$SCRIPT_DIR/plans-path-guard-blocks-project-plans.sh"
run_test "plans-path-guard: allows doc/plans and other .claude paths" "$SCRIPT_DIR/plans-path-guard-allows-doc-plans.sh"
run_test "plans-path-guard: blocks MultiEdit to .claude plans, allows doc/plans" "$SCRIPT_DIR/plans-path-guard-multiedit.sh"
run_test "codex-forward-guard: agent_type gate pins both shim types" "$SCRIPT_DIR/codex-forward-guard-agent-type.sh"
run_test "poll-guard: semantic subagents-list counting and forward-receipt guidance" "$SCRIPT_DIR/poll-guard-subagents-list.sh"
run_test "poll-guard: owned subagent waits are bounded" "$SCRIPT_DIR/poll-guard-subagent-waits.sh"
run_test "poll-guard: git show/diff pathless rule treats -s/--no-patch as bounded" "$SCRIPT_DIR/poll-guard-git-pathless.sh"
run_test "subagent-gate-payload: agent_type/transcript_path decide before the process-tree fallback" "$SCRIPT_DIR/subagent-gate-payload.sh"
run_test "subagent-start: ledger rows retain parent session identity" "$SCRIPT_DIR/subagent-start-ledger.sh"
run_test "subagent review: documented payloads record a round through the real binary" "$SCRIPT_DIR/subagent-review-real-binary.sh"
run_test "subagent review: a job that builds the binary cannot skip the real-binary test" "$SCRIPT_DIR/subagent-review-real-binary-required.sh"
run_test "codex-forward-guard: blocks Edit by a forwarder" "$SCRIPT_DIR/codex-forward-guard-blocks-edit.sh"
run_test "codex-forward-guard: companion Bash allowed, other Bash blocked" "$SCRIPT_DIR/codex-forward-guard-bash-companion-only.sh"
run_test "codex-forward-guard: plain subagents, main sessions, no-path all untouched" "$SCRIPT_DIR/codex-forward-guard-ignores-others.sh"
run_test "codex-forward-guard: a native codex.backend refuses every forward and names loom subagents start" "$SCRIPT_DIR/codex-forward-guard-native-lane.sh"
run_test "codex-forward-guard: only the guard may place a minted invocation id in a command" "$SCRIPT_DIR/codex-forward-guard-quoted-invocation.sh"
run_test "codex-forward-guard: authorized forwards record requested model" "$SCRIPT_DIR/codex-forward-records-model.sh"
run_test "codex-forward-guard: tests isolate ambient live identity" "$SCRIPT_DIR/codex-forward-guard-live-identity.sh"
run_test "codex-forward: exact job lifecycle markers and argv" "$SCRIPT_DIR/codex-forward-wrapper.sh"
run_test "codex-forward: bounded direct process supervision" "$SCRIPT_DIR/codex-forward-direct.sh"
run_test "codex-forward-result: correlated direct lifecycle evidence" "$SCRIPT_DIR/codex-forward-result.sh"
run_test "codex apply_patch: file guards and no post-patch recording" "$SCRIPT_DIR/codex-apply-patch.sh"
run_test "codex-forward-guard: quoted and escaped prompts round-trip" "$SCRIPT_DIR/codex-forward-guard-quoting.sh"
run_test "codex-forward-guard: enforcement needs unforgeable stage evidence" "$SCRIPT_DIR/codex-forward-guard-stage-evidence.sh"
run_test "codex-forward: task-output channel confinement under a real sandbox runtime" "$SCRIPT_DIR/codex-forward-channel-confinement.sh"
run_test "codex-forward: the confinement probe skips without a runtime unless one is required" "$SCRIPT_DIR/codex-forward-channel-confinement-gate.sh"
run_test "blocking guards: empty, truncated and tool_name-less input are refused, advisory hooks allow" "$SCRIPT_DIR/blocking-guards-bad-payload.sh"
run_test "pre-compact: both handoff calls are time-bounded" "$SCRIPT_DIR/pre-compact-handoff-bounded.sh"
run_test "git-add-guard: quoted prose allowed, real args blocked" "$SCRIPT_DIR/git-add-guard-quoting.sh"
run_test "_common: token helpers scan argv values, not quoted prose" "$SCRIPT_DIR/common-token-helpers.sh"
run_test "commit-filter: quoted prose about git is allowed, real commits blocked" "$SCRIPT_DIR/commit-filter-quoted-payload.sh"
run_test "commit-filter: git commit blocked in a stage session, only as git's subcommand" "$SCRIPT_DIR/commit-filter-session-git-commit.sh"
run_test "worktree-isolation: quoted prose paths allowed, real traversal blocked" "$SCRIPT_DIR/worktree-isolation-quoted-payload.sh"
run_test "worktree guards consume shared policy and reject missing data" "$SCRIPT_DIR/worktree-shared-policy.sh"
run_test "containment: effective cwd tracking and physical path classes" "$SCRIPT_DIR/containment-cwd-resolve.sh"
run_test "_common: block ledger line, safety refusals and UTF-8 cut" "$SCRIPT_DIR/common-block-ledger.sh"
run_test "worktree-isolation: traversal resolves against the effective cwd" "$SCRIPT_DIR/worktree-isolation-traversal-resolve.sh"
run_test "worktree-isolation: git targets inside the worktree or scratch only" "$SCRIPT_DIR/worktree-isolation-git-target.sh"
run_test "worktree-isolation: main checkout paths are blocked in a stage worktree" "$SCRIPT_DIR/worktree-isolation-main-checkout.sh"
run_test "loom-control-complete: completion rewritten to the pinned command" "$SCRIPT_DIR/loom-control-complete-rewrite.sh"
run_test "worktree-file-guard: own harness tool-results output is readable" "$SCRIPT_DIR/worktree-file-guard-harness-output.sh"
run_test "commit-filter: git in a scratch repository is allowed for subagents" "$SCRIPT_DIR/commit-filter-scratch-repo.sh"
run_test "prefer-modern-tools: grep/find inside a quoted payload is not a command" "$SCRIPT_DIR/prefer-modern-tools-quoted-payload.sh"
run_test "prefer-modern-tools: missing rg/fd allows grep/find with a warning" "$SCRIPT_DIR/prefer-modern-tools-missing-rg-fd.sh"
run_test "require-jq: blocking guard fails closed without jq" "$SCRIPT_DIR/require-jq-blocking.sh"
run_test "require-jq: advisory hook warns non-blockingly without jq" "$SCRIPT_DIR/require-jq-advisory.sh"
run_test "stage-terminal-guard: blocks completed/verified, allows others" "$SCRIPT_DIR/stage-terminal-guard.sh"
run_test "stage-terminal-guard: blocks MultiEdit once the stage is terminal" "$SCRIPT_DIR/stage-terminal-guard-multiedit.sh"
run_test "commit-guard: a nested worktree resolves to its innermost stage" "$SCRIPT_DIR/commit-guard-nested-worktree.sh"
run_test "commit-guard: many dirty files never raise SIGPIPE through pipefail" "$SCRIPT_DIR/commit-guard-sigpipe-many-dirty-files.sh"
run_test "commit-guard: a contract session gets the freeze reminder, not the checklist" "$SCRIPT_DIR/commit-guard-contract-session.sh"
run_test "commit-guard: the memory reminder reads this stage's journal, shows for thin or absent, hides for 11+ lines, under padded wc" "$SCRIPT_DIR/commit-guard-memory-reminder.sh"
run_test "learning-validator: warns unless this stage's own journal holds an entry" "$SCRIPT_DIR/learning-validator-stage-journal.sh"
run_test "user-prompt-context: short prompt produces no output" "$SCRIPT_DIR/user-prompt-context-short-prompt.sh"
run_test "skill-trigger: forwarded Codex is silent, Claude unchanged" "$SCRIPT_DIR/skill-trigger-forward.sh"
run_test "user-prompt-context: no LOOM_WORK_DIR exits silently" "$SCRIPT_DIR/user-prompt-context-no-workdir.sh"
run_test "user-prompt-context: malformed JSON fails open" "$SCRIPT_DIR/user-prompt-context-malformed-json.sh"
run_test "user-prompt-context: output is a single JSON object" "$SCRIPT_DIR/user-prompt-context-single-json-line.sh"
run_test "user-prompt-context: oversized delegate output is suppressed" "$SCRIPT_DIR/user-prompt-context-oversized-output.sh"
run_test "retrieval wrapper: bounded fallback and failed output suppression" "$SCRIPT_DIR/retrieval-wrapper-bounds.sh"
run_test "post-tool-use: Write/Edit heartbeat without edit recording" "$SCRIPT_DIR/post-tool-use-write-edit-record.sh"
run_test "post-tool-use: exits 0 when loom is absent from PATH" "$SCRIPT_DIR/post-tool-use-no-loom-binary.sh"
run_test "post-tool-use: symlinked heartbeat skips only the heartbeat write" "$SCRIPT_DIR/post-tool-use-symlinked-heartbeat.sh"
run_test "post-tool-use: MultiEdit without edit recording" "$SCRIPT_DIR/post-tool-use-multiedit-record.sh"
run_test "post-tool-use: NotebookEdit without edit recording" "$SCRIPT_DIR/post-tool-use-notebookedit-record.sh"
run_test "post-tool-use: canonical context-ceiling pair is cached and selected by role" "$SCRIPT_DIR/post-tool-use-ceiling-resolution.sh"
run_test "post-tool-use: context ceiling hard block fires on every call at 100%" "$SCRIPT_DIR/post-tool-use-ceiling-hard-block.sh"
run_test "post-tool-use: context ceiling 80% warning fires once per session" "$SCRIPT_DIR/post-tool-use-ceiling-warn-once.sh"
run_test "post-tool-use: large Bash output gets one advisory, capped per session" "$SCRIPT_DIR/post-tool-use-large-output-notice.sh"
run_test "post-tool-use: main-session size advisory fires once on stdout, never for subagents or exempt paths" "$SCRIPT_DIR/post-tool-use-main-edit-advisory.sh"
run_test "post-tool-use: subagent ceiling ignores the stage's own context_ceiling_tokens" "$SCRIPT_DIR/post-tool-use-subagent-ceiling.sh"
run_test "subagent-stop: heartbeat refresh is serialized and cannot roll parent tokens back" "$SCRIPT_DIR/subagent-stop-heartbeat-lock.sh"
run_test "subagent-stop: lifecycle heartbeat tags subagent:true with no last_tool" "$SCRIPT_DIR/subagent-stop-heartbeat-subagent-flag.sh"
run_test "subagent-stop: only a loom-code-reviewer stop reaches review-harvest, silently" "$SCRIPT_DIR/subagent-stop-review-harvest.sh"
run_test "subagent-stop: a FIFO at stop-skips.jsonl or lifecycle.jsonl never blocks the hook" "$SCRIPT_DIR/subagent-stop-fifo-journal.sh"
run_test "heartbeat protocol: ownership, SessionStart lock, abandoned recovery, atomic JSON" "$SCRIPT_DIR/heartbeat-protocol.sh"
run_test "heartbeat reclaim: killed reclaimer recovers, needs only Bash and coreutils" "$SCRIPT_DIR/heartbeat-reclaimer-crash.sh"
run_test "heartbeat reclaim: creator that lost its directory never shares the lock" "$SCRIPT_DIR/heartbeat-aborted-creator.sh"
run_test "heartbeat protocol: subagent tool calls are tagged, main-agent calls are not" "$SCRIPT_DIR/heartbeat-subagent-flag.sh"
run_test "post-tool-use: resident-token arithmetic (last record wins, torn line survives)" "$SCRIPT_DIR/post-tool-use-resident-tokens.sh"
run_test "post-tool-use: heartbeat write survives a ceiling exit 2" "$SCRIPT_DIR/post-tool-use-ceiling-heartbeat-survives.sh"
run_test "post-tool-use: judge writes its own heartbeat, bypassing the ownership gate" "$SCRIPT_DIR/post-tool-use-judge-heartbeat.sh"
run_test "read-guard: offset/limit arithmetic injection is dead, well-formed reads unchanged" "$SCRIPT_DIR/read-guard-offset-injection.sh"
run_test "read-guard: INDEX.md is exempt from the tier-1 warning" "$SCRIPT_DIR/read-guard-index-exempt.sh"
run_test "read-guard: sibling whole-file reads are advised once and recorded in _shared.tsv" "$SCRIPT_DIR/read-guard-sibling-receipts.sh"
run_test "read-guard: out-of-stage ledgers are per session and per agent, private, and failure-silent" "$SCRIPT_DIR/read-guard-session-ledgers.sh"
run_test "loom_run_bounded: a bash watchdog bounds the command when neither gtimeout nor timeout exists" "$SCRIPT_DIR/loom-run-bounded.sh"
run_test "session-start: heartbeat write escapes a quoted transcript_path via jq" "$SCRIPT_DIR/session-start-heartbeat-escaping.sh"
run_test "knowledge-orient: emits the INDEX.md pointer on startup" "$SCRIPT_DIR/knowledge-orient-emits.sh"
run_test "knowledge-orient: silent without an index, inside a stage, on compact, past the git root" "$SCRIPT_DIR/knowledge-orient-silent.sh"
run_test "post-tool-use: commit reminder is tokenized - heredoc body ignored, real commits fire" "$SCRIPT_DIR/post-tool-use-commit-reminder-tokenized.sh"
run_test "loom-relay: kind derivation per loom command segment, no control kinds for subagents" "$SCRIPT_DIR/loom-relay-kinds.sh"
run_test "loom-relay: fast path, jq and LOOM_HOOK_PATH gates, helper call contract" "$SCRIPT_DIR/loom-relay-gates.sh"
run_test "loom-relay-guard: refuses a relayed loom command whose stdout would be hidden" "$SCRIPT_DIR/loom-relay-guard.sh"
run_test "loom-relay-guard: writes the call marker a relayed control ticket needs" "$SCRIPT_DIR/loom-relay-guard-call-marker.sh"
run_test "loom-relay-stop: one block per stranded control ticket" "$SCRIPT_DIR/loom-relay-stop.sh"

# Roster guard: every test script in this directory must be registered above.
for path in "$SCRIPT_DIR"/*.sh; do
	name=${path##*/}
	case "$name" in _* | run-all.sh) continue ;; esac
	case " ${SCRIPTS[*]} " in
	*" $SCRIPT_DIR/$name "*) ;;
	*)
		echo "run-all.sh: $name is not registered"
		exit 1
		;;
	esac
done

START=$SECONDS
run_all_jobs
for ((i = 0; i < ${#NAMES[@]}; i++)); do
	report_job "$i"
done

echo ""
echo "Results: $PASS passed, $FAIL failed, $SKIPS skipped cases"
echo "Wall time: $((SECONDS - START))s with $JOBS jobs"

if [[ $FAIL -gt 0 ]]; then
	echo "Failed tests: ${ERRORS[*]}"
	exit 1
fi
