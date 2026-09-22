#!/usr/bin/env bash
# codex-forward.sh - trusted argv boundary for the codex companion runtime

set -euo pipefail

usage_error() {
	printf '%s\n' \
		'Usage: codex-forward.sh task <prompt> --model <model> --effort <effort> --write --unit-id <unit> --invocation-id <invocation>' >&2
	exit 2
}
[[ $# -eq 11 ]] || usage_error
if [[ "$1" != "task" || "$3" != "--model" || "$5" != "--effort" ||
	"$7" != "--write" || "$8" != "--unit-id" || "${10}" != "--invocation-id" ]]; then
	usage_error
fi
prompt=$2
model=$4
effort=$6
unit_id=$9
invocation_id=${11}
if [[ ${#unit_id} -lt 1 || ${#unit_id} -gt 64 || ! "$unit_id" =~ ^[A-Za-z0-9][A-Za-z0-9._-]{0,63}$ ]]; then
	printf 'Invalid Codex unit id: %s\n' "$unit_id" >&2
	exit 2
fi
if [[ ! "$invocation_id" =~ ^inv-[0-9a-f]{32}$ ]]; then
	printf 'Invalid Codex invocation id: %s\n' "$invocation_id" >&2
	exit 2
fi
stage_id=${LOOM_STAGE_ID:-}
loom_session_id=${LOOM_SESSION_ID:-}
if [[ ! "$stage_id" =~ ^[A-Za-z0-9][A-Za-z0-9._-]*$ ||
	! "$loom_session_id" =~ ^[A-Za-z0-9][A-Za-z0-9._-]*$ ]]; then
	printf '%s\n' 'LOOM_STAGE_ID and LOOM_SESSION_ID are required safe identifiers' >&2
	exit 2
fi
export CODEX_COMPANION_SESSION_ID="loom.v1:${stage_id}:${loom_session_id}:${unit_id}:${invocation_id}"
case "$model" in
gpt-6-astra | gpt-6-sol | gpt-5.6-terra | gpt-6-luna) ;;
*)
	printf 'Unsupported forwarding model: %s\n' "$model" >&2
	exit 2
	;;
esac

case "$effort" in
low | medium | high | xhigh | max | ultra) ;;
*)
	printf 'Unsupported reasoning effort: %s\n' "$effort" >&2
	exit 2
	;;
esac

# The PreToolUse guard runs outside the stage sandbox and owns the requested
# model/effort ledger row; this in-sandbox wrapper cannot write it safely.

# This per-task preamble keeps the stage contract attached to every forward.
preamble=$(cat <<'CODEX_PREAMBLE'
=== LOOM CONTEXT (prepended automatically; your task follows the TASK marker) ===

You are implementing one slice of a loom-orchestrated stage inside a git worktree. That
worktree is your boundary. An orchestrator verifies and commits your work; you do neither.

NAVIGATE WITH THE SOURCE GRAPH INSTEAD OF PAGING FILES.
Loom keeps a tree-sitter index of this repository. Each command below answers in well under a
second, never writes inside your worktree, and works from any directory in the tree. Use them
first, and open a file only once one of them has told you which lines matter:

  loom map --find-all <symbol>      every definition of a name: path, line, kind
  loom map --outline <file>         the symbols in a file, with line ranges and signatures
  loom map --impact <symbol|path>   what reaches it, with path confidence
  loom knowledge context --query "<question>" --budget-tokens 1500
                                    ranked project knowledge plus matching source
  rg -n '<pattern>' <path>          literal text search
  sed -n '<first>,<last>p' <file>   the exact lines a lookup pointed you at
  loom knowledge context prints the sections it matched, quoted; open the file it names only when you need more of it.

Two things to expect from these commands, neither of which is a failure. They may print
`warning: could not refresh ...` or `warning: failed to refresh the context cache ...`, because
the cache they try to refresh lives outside your sandbox - the command still answers from the
published index, so do not retry it and do not report it as a block. And that index reflects your
branch point, so it will not show edits you or another agent made during this session; read a file
directly when you have already changed it.

DO NOT read CLAUDE.md. It instructs a different agent; nothing in it is addressed to you.
DO NOT read doc/loom/knowledge/ file by file. It is a ~200k-token corpus, and
`loom knowledge context --query` is how you query it. Ask it a question; do not read the library.

WRITE ONLY THE FILES YOUR TASK ASSIGNS YOU. Everything else in the tree is read-only to you.
NEVER write anything under .work/ or .loom/ - .loom/work/ (or the legacy .work/) is a symlink to
state shared with other running stages, and the rest of .loom/ is orchestrator-owned spool/cache
data.
NEVER run git: not add, not commit, not checkout, not stash, not restore.
DO NOT VERIFY. No full build, no test suite, no linter, no formatter, no type-checker, and never
a repeated or looping check. At most ONE narrowly-scoped check over the files you changed, run
once; skip it if you are unsure. The orchestrator compiles, tests, lints, and fixes.

FINISH BY REPORTING: files changed, assumptions you made, anything you could not resolve.

=== TASK ===
CODEX_PREAMBLE
)

task="${preamble}

${prompt}"

# macOS nested Seatbelt refusal selects the direct lane; the outer sandbox
# remains its boundary. PATH lookup is deliberate so tests can stub the probe.
nested_seatbelt_refused() {
	command -v sandbox-exec >/dev/null 2>&1 || return 1
	! sandbox-exec -p '(version 1)(allow default)' /usr/bin/true >/dev/null 2>&1
}

# Provider output stays private until the wrapper-owned prefix is complete.
umask 077
provider_log=
command_log=
output_log=
plugin_probe=
mode=companion
deferred_notes=
state_root=
cleanup() {
	[[ -z "$provider_log" ]] || rm -f -- "$provider_log"
	[[ -z "$command_log" ]] || rm -f -- "$command_log"
	[[ -z "$output_log" ]] || rm -f -- "$output_log"
	[[ -z "$plugin_probe" ]] || rm -f -- "$plugin_probe"
}
trap cleanup EXIT
make_private_temp() {
	local made
	made=$(mktemp "${TMPDIR:-/tmp}/loom-codex-forward.XXXXXX" 2>/dev/null) || return 1
	[[ -n "$made" ]] || return 1
	chmod 600 "$made" 2>/dev/null || return 1
	printf '%s\n' "$made"
}
print_separator() {
	printf '%s\n' '--- LOOM-FORWARD-OUTPUT ---'
}
print_bounded_file() {
	local file="$1"
	[[ -s "$file" ]] || return 0
	tail -c 65536 "$file" 2>/dev/null || true
	printf '\n'
}
print_deferred_notes() {
	[[ -z "$deferred_notes" ]] || printf '%s\n' "$deferred_notes"
}
resolve_exact_record() {
	local root="$1" id="$2" record_dir
	local matches=()
	shopt -s nullglob
	matches=("$root"/*/jobs/"${id}.json")
	shopt -u nullglob
	[[ ${#matches[@]} -eq 1 && -f "${matches[0]}" && ! -L "${matches[0]}" ]] || return 1
	record_dir=$(cd "$(dirname "${matches[0]}")" 2>/dev/null && pwd -P) || return 1
	printf '%s/%s\n' "$record_dir" "$(basename "${matches[0]}")"
}
print_evidence() {
	local exit_code="$1" backend_id="$2" evidence_state="$3" record_path
	printf '%s\n' '--- LOOM-CODEX-EVIDENCE ---'
	printf 'exit: %s\n' "$exit_code"
	if [[ "$mode" == companion ]]; then
		printf 'mode: companion\n'
		printf 'job: %s\n' "${backend_id:-none}"
		printf 'unit: %s\n' "$unit_id"
		printf 'invocation: %s\n' "$invocation_id"
		printf 'state: %s\n' "$evidence_state"
		record_path=
		if [[ -n "$backend_id" && -n "$state_root" ]]; then
			record_path=$(resolve_exact_record "$state_root" "$backend_id" 2>/dev/null || true)
		fi
		printf 'record: %s\n' "${record_path:-not found}"
	else
		printf 'mode: direct (codex exec --sandbox danger-full-access; nested Seatbelt refused)\n'
		printf 'thread: %s\n' "${backend_id:-none observed}"
		printf 'unit: %s\n' "$unit_id"
		printf 'invocation: %s\n' "$invocation_id"
		printf 'state: %s\n' "$evidence_state"
	fi
}
finish_without_end() {
	local exit_code="$1" backend_id="$2" diagnostic="$3"
	print_separator
	[[ -z "$diagnostic" ]] || printf '%s\n' "$diagnostic"
	print_bounded_file "$provider_log"
	print_deferred_notes
	print_evidence "$exit_code" "$backend_id" unknown
	exit "$exit_code"
}
run_captured() {
	local captured_status=0
	: >"$command_log"
	"$@" >"$command_log" 2>>"$provider_log" || captured_status=$?
	{ printf '\n'; command cat "$command_log"; } >>"$provider_log" 2>/dev/null || true
	return "$captured_status"
}
# Same output-capture contract as run_captured, bounded to timeout_secs
# without GNU timeout (this wrapper also runs on macOS): poll kill -0 on the
# background pid, then TERM and KILL a survivor before reaping it with wait.
run_captured_bounded() {
	local timeout_secs="$1" captured_status=0 pid waited=0 timed_out=0
	shift
	: >"$command_log"
	"$@" >"$command_log" 2>>"$provider_log" &
	pid=$!
	while [[ $waited -lt $timeout_secs ]] && kill -0 "$pid" 2>/dev/null; do
		sleep 1
		waited=$((waited + 1))
	done
	if kill -0 "$pid" 2>/dev/null; then
		timed_out=1
		kill -TERM "$pid" 2>/dev/null || true
		sleep 2
		kill -0 "$pid" 2>/dev/null && kill -KILL "$pid" 2>/dev/null || true
	fi
	wait "$pid" 2>/dev/null || captured_status=$?
	if [[ $timed_out -eq 1 ]]; then
		captured_status=124
	fi
	{ printf '\n'; command cat "$command_log"; } >>"$provider_log" 2>/dev/null || true
	return "$captured_status"
}
valid_backend_id() {
	local value="$1"
	[[ ${#value} -le 128 && "$value" =~ ^[A-Za-z0-9][A-Za-z0-9._-]{0,127}$ ]]
}
if ! provider_log=$(make_private_temp) || ! command_log=$(make_private_temp) ||
	! output_log=$(make_private_temp); then
	print_separator
	printf '%s\n' 'codex-forward.sh could not create its private provider-output file'
	print_evidence 1 '' unknown
	exit 1
fi
if nested_seatbelt_refused; then
	mode=direct
	deferred_notes='note: the outer sandbox refuses a nested Seatbelt profile; running codex exec with --sandbox danger-full-access (the outer sandbox is the boundary)'
fi
if ! command -v jq >/dev/null 2>&1; then
	finish_without_end 1 '' 'codex-forward.sh requires jq to decode structured Codex output'
fi
run_direct() {
	local supervisor direct_status=0 result_line result_json fields
	local state thread_id turn_id terminal_at result_status retained
	supervisor="$(dirname "$0")/_codex-direct.py"
	if [[ ! -f "$supervisor" || -L "$supervisor" ]] || ! command -v python3 >/dev/null 2>&1; then
		finish_without_end 1 '' 'codex direct supervisor or Python 3 is unavailable'
	fi
	python3 "$supervisor" --timeout-ms 540000 -- codex exec --json \
		--sandbox danger-full-access --skip-git-repo-check --model "$model" \
		-c "model_reasoning_effort=$effort" -- "$task" \
		>"$provider_log" 2>"$command_log" || direct_status=$?
	result_line=$(tail -n 1 "$provider_log" 2>/dev/null || true)
	result_json=${result_line#LOOM-CODEX-DIRECT-RESULT }
	if [[ "$result_json" == "$result_line" ]] ||
		! fields=$(printf '%s' "$result_json" | jq -er '
			select(type == "object" and
			 (keys | sort) == (["exit_code","ownership_retained","state","terminal_at","thread_id","turn_id","v"] | sort) and
			 .v == 1 and (.state | IN("succeeded","failed","cancelled","unknown")) and
			 (.thread_id == null or (.thread_id | type == "string")) and
			 (.turn_id == null or (.turn_id | type == "string")) and
			 (.terminal_at | type == "string") and (.exit_code | type == "number" and floor == .) and
			 (.ownership_retained | type == "boolean")) |
			[.state, (.thread_id // ""), (.turn_id // ""), .terminal_at,
			 (.exit_code | tostring), (.ownership_retained | tostring)] | @tsv' 2>/dev/null); then
		{ printf '\n'; command cat "$command_log"; } >>"$provider_log" 2>/dev/null || true
		finish_without_end 1 '' 'codex direct supervisor returned no valid result line'
	fi
	IFS=$'\t' read -r state thread_id turn_id terminal_at result_status retained <<<"$fields"
	if [[ "$result_status" != "$direct_status" || -z "$thread_id" || -z "$turn_id" ]] ||
		! valid_backend_id "$thread_id" || ! valid_backend_id "$turn_id"; then
		{ printf '\n'; command cat "$command_log"; } >>"$provider_log" 2>/dev/null || true
		finish_without_end 1 '' 'codex direct supervisor result was incomplete or inconsistent'
	fi
	case "$state:$result_status:$retained" in
	succeeded:0:false | failed:*:false | cancelled:124:false | unknown:125:true) ;;
	*)
		{ printf '\n'; command cat "$command_log"; } >>"$provider_log" 2>/dev/null || true
		finish_without_end 1 '' 'codex direct supervisor state was inconsistent'
		;;
	esac
	{ printf '\n'; command cat "$command_log"; } >>"$provider_log" 2>/dev/null || true
	printf 'LOOM-FORWARD-START {"v":1,"backend":"direct","thread_id":"%s"}\n' "$thread_id"
	printf 'LOOM-FORWARD-END {"v":1,"backend":"direct","thread_id":"%s","outcome":"%s","exit_code":%s}\n' \
		"$thread_id" "$state" "$direct_status"
	print_separator
	print_bounded_file "$provider_log"
	print_deferred_notes
	print_evidence "$direct_status" "$thread_id" "$state"
	exit "$direct_status"
}
if [[ "$mode" == direct ]]; then
	run_direct
fi
if [[ -z "${HOME:-}" || ! -d "$HOME" ]]; then
	finish_without_end 1 '' 'HOME is required to locate codex-companion.mjs'
fi
home_root=$(cd "$HOME" 2>/dev/null && pwd -P) || finish_without_end 1 '' 'HOME cannot be canonicalized'
versions_dir=${home_root}/.claude/plugins/cache/openai-codex/codex
companion=${versions_dir}/1.0.6/scripts/codex-companion.mjs
if [[ ! -f "$companion" || -L "$companion" ]]; then
	finish_without_end 1 '' "Supported codex companion 1.0.6 is missing or unsafe: $companion"
fi
companion_dir=$(cd "$(dirname "$companion")" 2>/dev/null && pwd -P) ||
	finish_without_end 1 '' "Supported codex companion 1.0.6 cannot be canonicalized: $companion"
companion=$companion_dir/codex-companion.mjs

CLAUDE_PLUGIN_DATA=${home_root}/.codex/plugin-data
export CLAUDE_PLUGIN_DATA
state_root=${CLAUDE_PLUGIN_DATA}/state
if ! mkdir -p "$state_root" 2>/dev/null ||
	! plugin_probe=$(mktemp "$state_root/.loom-write.XXXXXX" 2>/dev/null) || [[ -z "$plugin_probe" ]]; then
	finish_without_end 1 '' "canonical plugin data state root is not writable: $state_root"
fi
if ! rm -f -- "$plugin_probe" 2>/dev/null; then
	finish_without_end 1 '' "canonical plugin data state root failed its write probe: $state_root"
fi
plugin_probe=
launch_status=0
run_captured node "$companion" task "$task" --background --json --write \
	--model "$model" --effort "$effort" || launch_status=$?
if [[ $launch_status -ne 0 ]]; then
	finish_without_end "$launch_status" '' 'codex companion failed to launch a background job'
fi
job_id=$(jq -er 'if type == "object" and (.jobId | type == "string") then .jobId else empty end' \
	"$command_log" 2>/dev/null || true)
if [[ -z "$job_id" ]] || ! valid_backend_id "$job_id"; then
	finish_without_end 1 '' 'codex companion returned no valid jobId'
fi
printf 'LOOM-FORWARD-START {"v":1,"backend":"companion","job_id":"%s"}\n' "$job_id"
outcome=
exit_code=1
wait_diagnostic=
wait_status=0
run_captured node "$companion" status "$job_id" --wait --json --timeout-ms 540000 || wait_status=$?
if [[ $wait_status -ne 0 ]]; then
	exit_code=$wait_status
	wait_diagnostic='codex companion wait failed before authoritative completion'
else
	status_phase=$(jq -er --arg id "$job_id" \
		'if type == "object" and .job.id == $id and (.job.status | type == "string") and (.job.phase | type == "string") and (.waitTimedOut | type == "boolean") then [.job.status, .job.phase, (.waitTimedOut | tostring)] | @tsv else empty end' \
		"$command_log" 2>/dev/null || true)
	if [[ -z "$status_phase" ]]; then
		wait_diagnostic='codex companion returned a malformed status snapshot'
	else
		IFS=$'\t' read -r job_status job_phase wait_timed_out <<<"$status_phase"
		if [[ "$wait_timed_out" == true || "$job_status" == queued || "$job_status" == running ]]; then
			cancel_status=0
			run_captured_bounded 30 node "$companion" cancel "$job_id" --json || cancel_status=$?
			printf 'LOOM-FORWARD-END {"v":1,"backend":"companion","job_id":"%s","outcome":"timed_out","exit_code":124}\n' \
				"$job_id"
			print_separator
			printf 'codex companion job %s exceeded the 540000 ms unit deadline and was cancelled (cancel exit %s): the unit was too large - re-split the remainder against the partial tree into smaller interface-pinned units and re-forward\n' \
				"$job_id" "$cancel_status"
			print_bounded_file "$provider_log"
			print_deferred_notes
			print_evidence 124 "$job_id" timed_out
			exit 124
		fi
		case "$job_status:$job_phase" in
		completed:done) outcome=succeeded; exit_code=0 ;;
		failed:*) outcome=failed ;;
		cancelled:*) outcome=canceled ;;
		*) wait_diagnostic="codex companion returned unsupported terminal state $job_status/$job_phase" ;;
		esac
	fi
fi
if [[ -z "$outcome" ]]; then
	finish_without_end "$exit_code" "$job_id" "$wait_diagnostic"
fi
result_status=0
run_captured node "$companion" result "$job_id" --json || result_status=$?
if [[ $result_status -eq 0 ]]; then
	jq -r --arg id "$job_id" \
		'if type == "object" and .job.id == $id and .storedJob.id == $id then (.storedJob.rendered // .storedJob.errorMessage // .storedJob.result // empty) | if type == "string" then . else tojson end else empty end' \
		"$command_log" >"$output_log" 2>/dev/null || : >"$output_log"
fi
printf 'LOOM-FORWARD-END {"v":1,"backend":"companion","job_id":"%s","outcome":"%s","exit_code":%s}\n' \
	"$job_id" "$outcome" "$exit_code"
print_separator
if [[ -s "$output_log" ]]; then
	print_bounded_file "$output_log"
elif [[ $result_status -ne 0 ]]; then
	printf '%s\n' 'codex companion result retrieval failed after authoritative completion'
	print_bounded_file "$provider_log"
fi
print_deferred_notes
evidence_state=$outcome
[[ "$outcome" == canceled ]] && evidence_state=cancelled
print_evidence "$exit_code" "$job_id" "$evidence_state"
exit "$exit_code"
