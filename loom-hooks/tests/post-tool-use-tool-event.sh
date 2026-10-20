#!/usr/bin/env bash
set -euo pipefail
HOOK="$(dirname "$0")/../post-tool-use.sh"
TMPDIR_TEST=$(mktemp -d "${TMPDIR:-/tmp}/loom-hooktest.XXXXXX")
trap 'rm -rf "$TMPDIR_TEST"' EXIT

export LOOM_STAGE_ID="test-stage"
export LOOM_SESSION_ID="test-session"
export LOOM_WORK_DIR="$TMPDIR_TEST"
export LOOM_CALLS="$TMPDIR_TEST/loom-calls"

FAKE_BIN="$TMPDIR_TEST/bin"
mkdir -p "$FAKE_BIN"
export PATH="$FAKE_BIN:$PATH"
export FAKE_LOOM="$FAKE_BIN/loom"

cat >"$FAKE_LOOM" <<'EOF'
#!/usr/bin/env bash
set -euo pipefail
cat >/dev/null
printf '%s\n' "$*" >>"$LOOM_CALLS"
EOF
chmod +x "$FAKE_LOOM"

TEXT_FILE="$TMPDIR_TEST/example.rs"
PNG_FILE="$TMPDIR_TEST/example.png"
touch "$TEXT_FILE" "$PNG_FILE"
INPUT=$(printf '{"tool_name":"Read","tool_input":{"file_path":"%s"},"tool_use_id":"read-1","transcript_path":"/repo/transcript.jsonl","tool_result":{"output":"private read output","is_error":false}}' "$TEXT_FILE")

bash "$HOOK" <<< "$INPUT"

# Check heartbeat was created
HEARTBEAT="$TMPDIR_TEST/heartbeat/test-stage.json"
if [[ ! -f "$HEARTBEAT" ]]; then
    echo "FAIL: heartbeat file not created"
    exit 1
fi

# Tool results are not persisted because a shell append cannot provide a
# race-free no-follow guarantee on the shared path.
EVENTS="$TMPDIR_TEST/tool-events.jsonl"
if [[ -e "$EVENTS" || -L "$EVENTS" ]]; then
    echo "FAIL: tool-events.jsonl must not be created"
    exit 1
fi

INELIGIBLE=$(printf '{"tool_name":"Read","tool_input":{"file_path":"%s"}}' "$PNG_FILE")
bash "$HOOK" <<< "$INELIGIBLE"

if [[ -e "$LOOM_CALLS" ]] && rg -q 'read[-]receipt' "$LOOM_CALLS"; then
    echo "FAIL: a Read invoked the receipt hook"
    exit 1
fi

echo "PASS"
