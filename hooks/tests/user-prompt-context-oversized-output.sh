#!/usr/bin/env bash
# The 8 KiB payload ceiling is enforced on BOTH sides. The Rust delegate caps
# itself, but this script runs against whatever `loom` is on PATH, so an
# oversized reply from a stale or third-party binary must be dropped here
# rather than injected into the session. A reply under the ceiling still
# passes through verbatim.
set -euo pipefail
HOOK="$(dirname "$0")/../user-prompt-context.sh"
TMP=$(mktemp -d)
trap 'rm -rf "$TMP"' EXIT
mkdir -p "$TMP/big" "$TMP/small" "$TMP/work"

# A fake `loom` whose additionalContext is $1 bytes of filler.
write_fake_loom() {
	cat >"$1/loom" <<SH
#!/usr/bin/env bash
[[ "\$1" == "hook" && "\$2" == "user-prompt" ]] || exit 1
cat >/dev/null
FILL=\$(head -c $2 /dev/zero | tr '\0' 'x')
jq -nc --arg ctx "\$FILL" '{hookSpecificOutput:{hookEventName:"UserPromptSubmit",additionalContext:\$ctx}}'
SH
	chmod +x "$1/loom"
}

# 9 KiB of context - past the 8192-byte ceiling however the object is framed.
write_fake_loom "$TMP/big" 9216
# 4 KiB of context - the framing adds ~100 bytes, so the object stays under.
write_fake_loom "$TMP/small" 4096

INPUT='{"session_id":"s1","prompt":"explain how the retrieval pipeline picks which knowledge sections to quote"}'

run_hook() {
	printf '%s' "$INPUT" |
		env -u LOOM_MAIN_AGENT_PID -u LOOM_WORKTREE_PATH -u LOOM_SESSION_ID \
			PATH="$1:/usr/bin:/bin" LOOM_WORK_DIR="$TMP/work" LOOM_STAGE_ID="test-stage" \
			bash "$HOOK"
}

OVERSIZED=$(run_hook "$TMP/big")
if [[ -n "$OVERSIZED" ]]; then
	echo "FAIL: a delegate reply past the 8 KiB ceiling must be suppressed by the shell hook, but ${#OVERSIZED} bytes were printed"
	exit 1
fi

UNDERSIZED=$(run_hook "$TMP/small")
UNDERSIZED_BYTES=$(LC_ALL=C printf '%s' "$UNDERSIZED" | wc -c)
if [[ "$UNDERSIZED_BYTES" -le 4096 || "$UNDERSIZED_BYTES" -gt 8192 ]]; then
	echo "FAIL: expected an under-the-ceiling reply to pass through intact, got $UNDERSIZED_BYTES bytes"
	exit 1
fi

echo "PASS: the shell wrapper drops oversized delegate output and passes under-the-ceiling output through"
