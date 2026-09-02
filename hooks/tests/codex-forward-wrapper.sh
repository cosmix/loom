#!/usr/bin/env bash
set -euo pipefail

WRAPPER="$(dirname "$0")/../codex-forward.sh"
TMP=$(mktemp -d "${TMPDIR:-/tmp}/loom-hooktest.XXXXXX")
trap 'rm -rf "$TMP"' EXIT

HOME_DIR="$TMP/home"
BIN_DIR="$TMP/bin"
CAPTURE="$TMP/argv"
STDOUT="$TMP/stdout"
COMPANION_DIR="$HOME_DIR/.claude/plugins/cache/openai-codex/codex/1.0.6/scripts"
mkdir -p "$BIN_DIR" "$COMPANION_DIR"
printf '%s\n' '// fixture' >"$COMPANION_DIR/codex-companion.mjs"

printf '%s\n' '#!/usr/bin/env bash' 'printf '\''%q\n'\'' "$@" >"$CAPTURE"' >"$BIN_DIR/node"
chmod +x "$BIN_DIR/node"

# The companion path must be exercised whether or not the test itself runs
# inside a sandbox that already refuses a nested Seatbelt profile (on this
# machine hook tests run inside one) - so stub sandbox-exec to succeed in
# every stub dir that is meant to exercise the companion lane.
printf '%s\n' '#!/usr/bin/env bash' 'exit 0' >"$BIN_DIR/sandbox-exec"
chmod +x "$BIN_DIR/sandbox-exec"

prompt=$'literal; operator\nsecond line with $HOME and `ticks`'

# Redirect coverage: point CLAUDE_PLUGIN_DATA at a path whose parent is a
# regular file, so `mkdir -p` fails and the wrapper redirects to
# $HOME_DIR/.codex/plugin-data; pre-seed a job record there and confirm the
# trailer finds it at the redirected root.
BLOCKER="$TMP/blocker"
printf '%s\n' 'not a directory' >"$BLOCKER"
REDIRECTED_JOBS_DIR="$HOME_DIR/.codex/plugin-data/state/site-abc/jobs"
mkdir -p "$REDIRECTED_JOBS_DIR"
# Non-chronological creation order with explicit mtimes, so ordering in the
# trailer can only come from print_newest's -nt insertion, never from
# creation order; four records also exercises the cap of 3.
: >"$REDIRECTED_JOBS_DIR/task-new.json"
touch -t 202601030000 "$REDIRECTED_JOBS_DIR/task-new.json"
: >"$REDIRECTED_JOBS_DIR/task-old.json"
touch -t 202601010000 "$REDIRECTED_JOBS_DIR/task-old.json"
: >"$REDIRECTED_JOBS_DIR/task-newest.json"
touch -t 202601040000 "$REDIRECTED_JOBS_DIR/task-newest.json"
: >"$REDIRECTED_JOBS_DIR/task-mid.json"
touch -t 202601020000 "$REDIRECTED_JOBS_DIR/task-mid.json"

HOME="$HOME_DIR" PATH="$BIN_DIR:$PATH" CAPTURE="$CAPTURE" CLAUDE_PLUGIN_DATA="$BLOCKER/plugin" \
	bash "$WRAPPER" task "$prompt" --model gpt-5.6-terra --effort xhigh --write >"$STDOUT"

rg -qF 'mode: companion' "$STDOUT"

companion_block=$(rg -A 3 -F 'mode: companion' "$STDOUT" | tail -n +2)
companion_jobs=()
while IFS= read -r line; do
	[[ -n "$line" ]] && companion_jobs+=("$(basename "$line")")
done <<<"$companion_block"

if [[ ${#companion_jobs[@]} -ne 3 ]]; then
	printf '%s\n' "FAIL: expected 3 companion job lines, got ${#companion_jobs[@]}: ${companion_jobs[*]}"
	exit 1
fi
if [[ "${companion_jobs[0]}" != 'task-newest.json' || "${companion_jobs[1]}" != 'task-new.json' ||
	"${companion_jobs[2]}" != 'task-mid.json' ]]; then
	printf '%s\n' "FAIL: companion job order was: ${companion_jobs[*]}"
	exit 1
fi
if rg -qF 'task-old.json' "$STDOUT"; then
	printf '%s\n' 'FAIL: task-old.json (oldest, beyond the cap of 3) leaked into stdout'
	exit 1
fi

HOME="$HOME_DIR" PATH="$BIN_DIR:$PATH" CAPTURE="$CAPTURE" CLAUDE_PLUGIN_DATA= \
	bash "$WRAPPER" task "$prompt" --model gpt-5.6-terra --effort xhigh --write >"$STDOUT"

[[ -f "$CAPTURE" ]]
[[ $(wc -l <"$CAPTURE") -eq 8 ]]
rg -qF 'codex-companion.mjs' "$CAPTURE"
rg -qF 'literal;' "$CAPTURE"
[[ ! -e "$TMP/operator" ]]

rg -qF 'loom map --find-all' "$CAPTURE"
rg -qF 'loom knowledge context' "$CAPTURE"
rg -qF 'NEVER run git' "$CAPTURE"
rg -qF 'NEVER write anything under .work/ or .loom/' "$CAPTURE"
rg -qF 'never writes inside your worktree' "$CAPTURE"
rg -qF 'warning: could not refresh' "$CAPTURE"

task_line=$(rg -F '=== TASK ===' "$CAPTURE")
after_marker=${task_line#*'=== TASK ==='}
if [[ "$after_marker" != *'literal;'* ]]; then
	printf '%s\n' 'FAIL: === TASK === marker did not precede the original prompt'
	exit 1
fi

rg -qF -- '--- LOOM-CODEX-EVIDENCE ---' "$STDOUT"
rg -qF 'exit: 0' "$STDOUT"

if HOME="$HOME_DIR" PATH="$BIN_DIR:$PATH" CAPTURE="$CAPTURE" \
	bash "$WRAPPER" task hello --model unsupported --effort xhigh --write 2>/dev/null; then
	printf '%s\n' 'FAIL: unsupported model was accepted'
	exit 1
fi

# A companion that fails must not have its failure swallowed: the wrapper's own exit
# status must equal the companion's, and the evidence trailer must still be printed.
FAIL_BIN_DIR="$TMP/bin-fail"
mkdir -p "$FAIL_BIN_DIR"
printf '%s\n' '#!/usr/bin/env bash' 'printf '\''%q\n'\'' "$@" >"$CAPTURE"' 'exit 7' >"$FAIL_BIN_DIR/node"
chmod +x "$FAIL_BIN_DIR/node"
printf '%s\n' '#!/usr/bin/env bash' 'exit 0' >"$FAIL_BIN_DIR/sandbox-exec"
chmod +x "$FAIL_BIN_DIR/sandbox-exec"

STDOUT_FAIL="$TMP/stdout-fail"
status=0
# CLAUDE_PLUGIN_DATA is explicitly cleared (not merely left unset in this
# script) so this run observes the true "no override" default even when the
# ambient shell already exports it - otherwise an unwritable ambient path
# would trigger the same redirect-to-$HOME_DIR/.codex/plugin-data logic as
# the deliberate redirect case above and pick up its leftover job files.
HOME="$HOME_DIR" PATH="$FAIL_BIN_DIR:$PATH" CAPTURE="$CAPTURE" CLAUDE_PLUGIN_DATA= \
	bash "$WRAPPER" task "$prompt" --model gpt-5.6-terra --effort xhigh --write >"$STDOUT_FAIL" ||
	status=$?

if [[ "$status" -ne 7 ]]; then
	printf '%s\n' "FAIL: wrapper exit status was $status, expected 7 (companion's status)"
	exit 1
fi

rg -qF -- '--- LOOM-CODEX-EVIDENCE ---' "$STDOUT_FAIL"
rg -qF 'exit: 7' "$STDOUT_FAIL"
rg -qF 'jobs: none found' "$STDOUT_FAIL"

# Direct-exec mode: when the outer sandbox refuses a nested Seatbelt profile,
# the wrapper must call `codex exec` itself and never reach the companion.
DIRECT_BIN_DIR="$TMP/bin-direct"
mkdir -p "$DIRECT_BIN_DIR"

printf '%s\n' '#!/usr/bin/env bash' 'exit 71' >"$DIRECT_BIN_DIR/sandbox-exec"
chmod +x "$DIRECT_BIN_DIR/sandbox-exec"

CAPTURE_DIRECT="$TMP/argv-direct"
printf '%s\n' '#!/usr/bin/env bash' 'printf '\''%q\n'\'' "$@" >"$CAPTURE_DIRECT"' \
	'printf '\''final message\n'\''' >"$DIRECT_BIN_DIR/codex"
chmod +x "$DIRECT_BIN_DIR/codex"

printf '%s\n' '#!/usr/bin/env bash' "touch \"$TMP/node-called\"" >"$DIRECT_BIN_DIR/node"
chmod +x "$DIRECT_BIN_DIR/node"

# CODEX_HOME points somewhere other than $HOME_DIR/.codex, so the override
# itself (not just the HOME-derived default) is what the test exercises.
# Three rollouts, created out of mtime order, so the newest-one-wins result
# can only come from print_newest's -nt insertion.
CODEX_HOME_DIR="$HOME_DIR/codex-home"
SESSIONS_DIR="$CODEX_HOME_DIR/sessions/2026/09/02"
mkdir -p "$SESSIONS_DIR"
ROLLOUT_C="$SESSIONS_DIR/rollout-2026-09-02T00-00-00-c.jsonl"
printf '%s\n' '{}' >"$ROLLOUT_C"
touch -t 202601010000 "$ROLLOUT_C"
ROLLOUT_B="$SESSIONS_DIR/rollout-2026-09-02T00-00-00-b.jsonl"
printf '%s\n' '{}' >"$ROLLOUT_B"
touch -t 202601030000 "$ROLLOUT_B"
ROLLOUT_A="$SESSIONS_DIR/rollout-2026-09-02T00-00-00-a.jsonl"
printf '%s\n' '{}' >"$ROLLOUT_A"
touch -t 202601020000 "$ROLLOUT_A"

STDOUT_DIRECT="$TMP/stdout-direct"
HOME="$HOME_DIR" PATH="$DIRECT_BIN_DIR:$PATH" CAPTURE_DIRECT="$CAPTURE_DIRECT" TMP="$TMP" \
	CODEX_HOME="$CODEX_HOME_DIR" \
	bash "$WRAPPER" task "$prompt" --model gpt-5.6-terra --effort xhigh --write >"$STDOUT_DIRECT"

[[ -f "$CAPTURE_DIRECT" ]]
first_line=$(head -n 1 "$CAPTURE_DIRECT")
if [[ "$first_line" != 'exec' ]]; then
	printf '%s\n' "FAIL: direct-mode codex invocation did not start with exec (got: $first_line)"
	exit 1
fi

rg -qF -- '--sandbox' "$CAPTURE_DIRECT"
rg -qF 'danger-full-access' "$CAPTURE_DIRECT"
rg -qF -- '--skip-git-repo-check' "$CAPTURE_DIRECT"
rg -qF -- '--model' "$CAPTURE_DIRECT"
rg -qF 'gpt-5.6-terra' "$CAPTURE_DIRECT"
rg -qF 'model_reasoning_effort=' "$CAPTURE_DIRECT"
rg -qF 'xhigh' "$CAPTURE_DIRECT"
rg -qF '=== TASK ===' "$CAPTURE_DIRECT"
rg -qF 'literal;' "$CAPTURE_DIRECT"
rg -qF 'loom map --find-all' "$CAPTURE_DIRECT"

[[ ! -e "$TMP/operator" ]]
[[ ! -e "$TMP/node-called" ]]

rg -qF 'final message' "$STDOUT_DIRECT"
rg -qF -- '--- LOOM-CODEX-EVIDENCE ---' "$STDOUT_DIRECT"
rg -qF 'exit: 0' "$STDOUT_DIRECT"
rg -qF 'mode: direct' "$STDOUT_DIRECT"

session_count=$(rg -cF 'session: ' "$STDOUT_DIRECT")
if [[ "$session_count" -ne 1 ]]; then
	printf '%s\n' "FAIL: expected exactly one session: line, got $session_count"
	exit 1
fi
rg -qF "session: $ROLLOUT_B" "$STDOUT_DIRECT"
if rg -qF "$ROLLOUT_A" "$STDOUT_DIRECT" || rg -qF "$ROLLOUT_C" "$STDOUT_DIRECT"; then
	printf '%s\n' 'FAIL: an older rollout path leaked into stdout'
	exit 1
fi
[[ ! -e "$HOME_DIR/.codex/sessions" ]]

# Direct-exec failure: a failing codex must not be swallowed either.
DIRECT_FAIL_BIN_DIR="$TMP/bin-direct-fail"
mkdir -p "$DIRECT_FAIL_BIN_DIR"
printf '%s\n' '#!/usr/bin/env bash' 'exit 71' >"$DIRECT_FAIL_BIN_DIR/sandbox-exec"
chmod +x "$DIRECT_FAIL_BIN_DIR/sandbox-exec"
printf '%s\n' '#!/usr/bin/env bash' 'exit 9' >"$DIRECT_FAIL_BIN_DIR/codex"
chmod +x "$DIRECT_FAIL_BIN_DIR/codex"

STDOUT_DIRECT_FAIL="$TMP/stdout-direct-fail"
status=0
HOME="$HOME_DIR" PATH="$DIRECT_FAIL_BIN_DIR:$PATH" CODEX_HOME="$TMP/no-such-codex-home" \
	bash "$WRAPPER" task "$prompt" --model gpt-5.6-terra --effort xhigh --write >"$STDOUT_DIRECT_FAIL" ||
	status=$?

if [[ "$status" -ne 9 ]]; then
	printf '%s\n' "FAIL: direct-mode wrapper exit status was $status, expected 9 (codex's status)"
	exit 1
fi

rg -qF -- '--- LOOM-CODEX-EVIDENCE ---' "$STDOUT_DIRECT_FAIL"
rg -qF 'exit: 9' "$STDOUT_DIRECT_FAIL"
rg -qF 'session: none found' "$STDOUT_DIRECT_FAIL"

printf '%s\n' 'PASS'
