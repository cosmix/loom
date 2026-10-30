#!/usr/bin/env bash
set -euo pipefail
HOOK="$(cd "$(dirname "$0")/.." && pwd)/skill-trigger.sh"
TMP=$(mktemp -d "${TMPDIR:-/tmp}/loom-hooktest.XXXXXX")
trap 'rm -rf "$TMP"' EXIT
cp "$HOOK" "$TMP/skill-trigger.sh"
cp "$(dirname "$HOOK")/_bounded_input.py" "$TMP/_bounded_input.py"
HOOK="$TMP/skill-trigger.sh"
unset LOOM_CODEX_FORWARD LOOM_WORK_DIR LOOM_STAGE_ID LOOM_SESSION_ID
export HOME="$TMP/home" CODEX_HOME="$TMP/home/.codex"
export LOOM_BIN="$TMP/bin/loom"
mkdir -p "$TMP/bin" "$TMP/repo"
printf '#!/usr/bin/env bash\nexit 0\n' >"$LOOM_BIN"
chmod +x "$LOOM_BIN"
for root in "$HOME/.claude" "$CODEX_HOME"; do
	mkdir -p "$root/hooks/loom" "$root/skills/loom-fixture"
	printf '%s\n' '{"fixture":["loom-fixture"]}' >"$root/hooks/loom/skill-keywords.json"
	printf '%s\n' '# Fixture skill' >"$root/skills/loom-fixture/SKILL.md"
done
cd "$TMP/repo"
INPUT='{"prompt":"fixture","session_id":"forward-test"}'
NORMAL=$(printf '%s' "$INPUT" | python3 "$HOOK" --codex)
printf '%s' "$NORMAL" | jq -e \
	'.hookSpecificOutput.additionalContext | contains("SKILL MATCH:") and contains("SKILL.md")' >/dev/null
FORWARDED=$(printf '%s' "$INPUT" | LOOM_CODEX_FORWARD=1 python3 "$HOOK" --codex 2>&1)
[[ -z "$FORWARDED" ]]
CLAUDE=$(printf '%s' "$INPUT" | python3 "$HOOK")
CLAUDE_FORWARDED=$(printf '%s' "$INPUT" | LOOM_CODEX_FORWARD=1 python3 "$HOOK")
[[ -n "$CLAUDE" && "$CLAUDE" == "$CLAUDE_FORWARDED" ]]
printf '%s\n' PASS
