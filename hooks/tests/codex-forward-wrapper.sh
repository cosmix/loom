#!/usr/bin/env bash
set -euo pipefail

WRAPPER="$(dirname "$0")/../codex-forward.sh"
TMP=$(mktemp -d)
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

prompt=$'literal; operator\nsecond line with $HOME and `ticks`'
HOME="$HOME_DIR" PATH="$BIN_DIR:$PATH" CAPTURE="$CAPTURE" \
	bash "$WRAPPER" task "$prompt" --model gpt-5.6-terra --effort xhigh --write >"$STDOUT"

[[ -f "$CAPTURE" ]]
[[ $(wc -l <"$CAPTURE") -eq 8 ]]
rg -qF 'codex-companion.mjs' "$CAPTURE"
rg -qF 'literal;' "$CAPTURE"
[[ ! -e "$TMP/operator" ]]

rg -qF 'loom map --find-all' "$CAPTURE"
rg -qF 'loom knowledge context' "$CAPTURE"
rg -qF 'NEVER run git' "$CAPTURE"
rg -qF 'NEVER write anything under .work/' "$CAPTURE"
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

STDOUT_FAIL="$TMP/stdout-fail"
status=0
HOME="$HOME_DIR" PATH="$FAIL_BIN_DIR:$PATH" CAPTURE="$CAPTURE" \
	bash "$WRAPPER" task "$prompt" --model gpt-5.6-terra --effort xhigh --write >"$STDOUT_FAIL" ||
	status=$?

if [[ "$status" -ne 7 ]]; then
	printf '%s\n' "FAIL: wrapper exit status was $status, expected 7 (companion's status)"
	exit 1
fi

rg -qF -- '--- LOOM-CODEX-EVIDENCE ---' "$STDOUT_FAIL"
rg -qF 'exit: 7' "$STDOUT_FAIL"

printf '%s\n' 'PASS'
