#!/usr/bin/env bash
set -euo pipefail

WRAPPER="$(dirname "$0")/../codex-forward.sh"
TMP=$(mktemp -d)
trap 'rm -rf "$TMP"' EXIT

HOME_DIR="$TMP/home"
BIN_DIR="$TMP/bin"
CAPTURE="$TMP/argv"
COMPANION_DIR="$HOME_DIR/.claude/plugins/cache/openai-codex/codex/1.0.6/scripts"
mkdir -p "$BIN_DIR" "$COMPANION_DIR"
printf '%s\n' '// fixture' >"$COMPANION_DIR/codex-companion.mjs"

printf '%s\n' '#!/usr/bin/env bash' 'printf '\''%q\n'\'' "$@" >"$CAPTURE"' >"$BIN_DIR/node"
chmod +x "$BIN_DIR/node"

prompt=$'literal; operator\nsecond line with $HOME and `ticks`'
HOME="$HOME_DIR" PATH="$BIN_DIR:$PATH" CAPTURE="$CAPTURE" \
	bash "$WRAPPER" task "$prompt" --model gpt-5.6-terra --effort xhigh --write

[[ -f "$CAPTURE" ]]
[[ $(wc -l <"$CAPTURE") -eq 8 ]]
rg -qF 'codex-companion.mjs' "$CAPTURE"
rg -qF 'literal;' "$CAPTURE"
[[ ! -e "$TMP/operator" ]]

if HOME="$HOME_DIR" PATH="$BIN_DIR:$PATH" CAPTURE="$CAPTURE" \
	bash "$WRAPPER" task hello --model unsupported --effort xhigh --write 2>/dev/null; then
	printf '%s\n' 'FAIL: unsupported model was accepted'
	exit 1
fi

printf '%s\n' 'PASS'
