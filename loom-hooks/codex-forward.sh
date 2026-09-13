#!/usr/bin/env bash
# codex-forward.sh - trusted argv boundary for the codex companion runtime

set -euo pipefail

if [[ $# -ne 7 || "$1" != "task" || "$3" != "--model" || "$5" != "--effort" || "$7" != "--write" ]]; then
	printf '%s\n' \
		'Usage: codex-forward.sh task <prompt> --model <model> --effort <effort> --write' >&2
	exit 2
fi
prompt=$2
model=$4
effort=$6
case "$model" in
gpt-6-astra | gpt-5.6-sol | gpt-5.6-terra | gpt-5.6-luna) ;;
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
cleanup() {
	[[ -z "$provider_log" ]] || rm -f -- "$provider_log"
	[[ -z "$command_log" ]] || rm -f -- "$command_log"
	[[ -z "$output_log" ]] || rm -f -- "$output_log"
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
	local exit_code="$1" backend_id="$2" record_path
	printf '%s\n' '--- LOOM-CODEX-EVIDENCE ---'
	printf 'exit: %s\n' "$exit_code"
	if [[ "$mode" == companion ]]; then
		printf 'mode: companion\n'
		printf 'job: %s\n' "${backend_id:-none}"
		record_path=
		if [[ -n "$backend_id" ]]; then
			record_path=$(resolve_exact_record "$state_root" "$backend_id" 2>/dev/null || true)
		fi
		printf 'record: %s\n' "${record_path:-not found}"
	else
		printf 'mode: direct (codex exec --sandbox danger-full-access; nested Seatbelt refused)\n'
		printf 'thread: %s\n' "${backend_id:-none observed}"
	fi
}
finish_without_end() {
	local exit_code="$1" backend_id="$2" diagnostic="$3"
	print_separator
	[[ -z "$diagnostic" ]] || printf '%s\n' "$diagnostic"
	print_bounded_file "$provider_log"
	print_deferred_notes
	print_evidence "$exit_code" "$backend_id"
	exit "$exit_code"
}
run_captured() {
	local captured_status=0
	: >"$command_log"
	"$@" >"$command_log" 2>>"$provider_log" || captured_status=$?
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
	printf '%s\n' '--- LOOM-CODEX-EVIDENCE ---' 'exit: 1' 'mode: companion' 'job: none' 'record: not found'
	exit 1
fi
mode=companion
deferred_notes=
if nested_seatbelt_refused; then
	mode=direct
	deferred_notes='note: the outer sandbox refuses a nested Seatbelt profile; running codex exec with --sandbox danger-full-access (the outer sandbox is the boundary)'
fi
state_root=${TMPDIR:-/tmp}/codex-companion
if [[ -n "${CLAUDE_PLUGIN_DATA:-}" ]]; then
	state_root=${CLAUDE_PLUGIN_DATA}/state
fi
if ! command -v jq >/dev/null 2>&1; then
	finish_without_end 1 '' 'codex-forward.sh requires jq to decode structured Codex output'
fi
read_direct_thread_id() {
	jq -rs 'map(select(type == "object" and .type == "thread.started" and (.thread_id | type == "string"))) | .[0].thread_id // empty' \
		"$provider_log" 2>/dev/null || true
}
run_direct() {
	local child_pid child_status=0 thread_id= final_status outcome
	codex exec --json --sandbox danger-full-access --skip-git-repo-check \
		--model "$model" -c "model_reasoning_effort=$effort" -- "$task" \
		</dev/null >"$provider_log" 2>"$command_log" &
	child_pid=$!
	while kill -0 "$child_pid" 2>/dev/null; do
		thread_id=$(read_direct_thread_id)
		if [[ -n "$thread_id" ]]; then
			break
		fi
		sleep 0.05 2>/dev/null || true
	done
	[[ -n "$thread_id" ]] || thread_id=$(read_direct_thread_id)
	if [[ -n "$thread_id" ]] && valid_backend_id "$thread_id"; then
		printf 'LOOM-FORWARD-START {"v":1,"backend":"direct","thread_id":"%s"}\n' "$thread_id"
	else
		thread_id=
	fi
	wait "$child_pid" || child_status=$?
	{ printf '\n'; command cat "$command_log"; } >>"$provider_log" 2>/dev/null || true
	if [[ -z "$thread_id" ]]; then
		final_status=$child_status
		[[ $final_status -ne 0 ]] || final_status=1
		finish_without_end "$final_status" '' 'codex exec ended without a valid thread.started event'
	fi
	if [[ $child_status -eq 0 ]]; then
		outcome=succeeded
	else
		outcome=failed
	fi
	printf 'LOOM-FORWARD-END {"v":1,"backend":"direct","thread_id":"%s","outcome":"%s","exit_code":%s}\n' \
		"$thread_id" "$outcome" "$child_status"
	print_separator
	print_bounded_file "$provider_log"
	print_deferred_notes
	print_evidence "$child_status" "$thread_id"
	exit "$child_status"
}
if [[ "$mode" == direct ]]; then
	run_direct
fi
if [[ -z "${HOME:-}" ]]; then
	finish_without_end 1 '' 'HOME is required to locate codex-companion.mjs'
fi
versions_dir=${HOME}/.claude/plugins/cache/openai-codex/codex
shopt -s nullglob
candidates=("$versions_dir"/*/scripts/codex-companion.mjs)
shopt -u nullglob
if [[ ${#candidates[@]} -eq 0 ]]; then
	finish_without_end 1 '' "codex-companion.mjs not found under $versions_dir"
fi

companion=${candidates[0]}
for candidate in "${candidates[@]:1}"; do
	if [[ "$candidate" > "$companion" ]]; then
		companion=$candidate
	fi
done
if [[ ! -f "$companion" || -L "$companion" ]]; then
	finish_without_end 1 '' "Refusing unsafe companion path: $companion"
fi

# Retain the existing plugin-data redirect, but defer its note.
if [[ -n "${CLAUDE_PLUGIN_DATA:-}" ]] && ! mkdir -p "${CLAUDE_PLUGIN_DATA}/state" 2>/dev/null; then
	CLAUDE_PLUGIN_DATA="${HOME}/.codex/plugin-data"
	export CLAUDE_PLUGIN_DATA
	state_root=${CLAUDE_PLUGIN_DATA}/state
	mkdir -p "$state_root" 2>/dev/null || true
	deferred_notes="note: plugin data root not writable; codex state redirected to $CLAUDE_PLUGIN_DATA"
fi
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
while [[ -z "$outcome" ]]; do
	wait_status=0
	run_captured node "$companion" status "$job_id" --wait --json || wait_status=$?
	if [[ $wait_status -ne 0 ]]; then
		exit_code=$wait_status
		wait_diagnostic='codex companion wait failed before authoritative completion'
		break
	fi
	status_phase=$(jq -er --arg id "$job_id" \
		'if type == "object" and .job.id == $id and (.job.status | type == "string") and (.job.phase | type == "string") then [.job.status, .job.phase] | @tsv else empty end' \
		"$command_log" 2>/dev/null || true)
	if [[ -z "$status_phase" ]]; then
		wait_diagnostic='codex companion returned a malformed status snapshot'
		break
	fi
	job_status=${status_phase%%$'\t'*}
	job_phase=${status_phase#*$'\t'}
	case "$job_status:$job_phase" in
	queued:* | running:*) ;;
	completed:done)
		outcome=succeeded
		exit_code=0
		;;
	failed:*) outcome=failed ;;
	cancelled:*) outcome=canceled ;;
	*)
		wait_diagnostic="codex companion returned unsupported terminal state $job_status/$job_phase"
		break
		;;
	esac
done
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
print_evidence "$exit_code" "$job_id"
exit "$exit_code"
