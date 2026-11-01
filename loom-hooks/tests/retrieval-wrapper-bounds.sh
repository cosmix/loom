#!/usr/bin/env bash
# The prompt and edit-recording wrappers must use the portable watchdog when
# timeout and gtimeout are absent, and never forward partial failed output.
set -euo pipefail
unset LOOM_HOOK_PATH LOOM_BIN LOOM_CODEX_FORWARD

HOOKS_DIR="$(cd "$(dirname "$0")/.." && pwd)"
source "$HOOKS_DIR/tests/_path_without.sh"
TMP=$(mktemp -d "${TMPDIR:-/tmp}/loom-retrieval-bounds.XXXXXX")
NO_TIMEOUT_DIR=$(path_without gtimeout timeout)
trap 'rm -rf -- "$TMP" "$NO_TIMEOUT_DIR"' EXIT
mkdir -p "$TMP/bin" "$TMP/work"
PATH="$TMP/bin:$NO_TIMEOUT_DIR"
if command -v gtimeout >/dev/null 2>&1 || command -v timeout >/dev/null 2>&1; then
	echo "FAIL: timeout binary remained in the fallback PATH"
	exit 1
fi

cat >"$TMP/bin/loom" <<'SH'
#!/usr/bin/env bash
case "$DELEGATE_MODE" in
stall) exec sleep 30 ;;
partial) printf '{"partial":true}\n'; exit 1 ;;
success) printf '{"hookSpecificOutput":{"additionalContext":"ready"}}\n' ;;
esac
SH
chmod +x "$TMP/bin/loom"

INPUT='{"session_id":"s1","prompt":"explain the current context"}'
start=$SECONDS
output=$(printf '%s' "$INPUT" | DELEGATE_MODE=stall LOOM_WORK_DIR="$TMP/work" \
	bash "$HOOKS_DIR/user-prompt-context.sh")
if ((SECONDS - start > 8)) || [[ -n "$output" ]]; then
	echo "FAIL: stalled retrieval exceeded its bound or published output"
	exit 1
fi
output=$(printf '%s' "$INPUT" | DELEGATE_MODE=partial LOOM_WORK_DIR="$TMP/work" \
	bash "$HOOKS_DIR/user-prompt-context.sh")
[[ -z "$output" ]] || { echo "FAIL: failed retrieval published partial output"; exit 1; }
output=$(printf '%s' "$INPUT" | DELEGATE_MODE=success LOOM_WORK_DIR="$TMP/work" \
	bash "$HOOKS_DIR/user-prompt-context.sh")
[[ "$output" == '{"hookSpecificOutput":{"additionalContext":"ready"}}' ]] || {
	echo "FAIL: successful retrieval output changed"
	exit 1
}

PATCH_INPUT=$(jq -nc '{tool_name:"apply_patch",tool_input:{command:"*** Begin Patch\n*** Add File: src/new.rs\n+new\n*** End Patch"}}')
start=$SECONDS
printf '%s' "$PATCH_INPUT" | DELEGATE_MODE=stall LOOM_STAGE_ID=stage-1 \
	bash "$HOOKS_DIR/codex-apply-patch.sh" post
if ((SECONDS - start > 6)); then
	echo "FAIL: stalled edit recording exceeded its bound"
	exit 1
fi

echo "PASS"
