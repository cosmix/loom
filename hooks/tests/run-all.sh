#!/usr/bin/env bash
set -euo pipefail
SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
PASS=0
FAIL=0
ERRORS=()

run_test() {
    local name="$1"
    local script="$2"
    if output=$(bash "$script" 2>&1); then
        echo "  PASS: $name"
        ((PASS++)) || true
    else
        echo "  FAIL: $name"
        echo "    Output: $output"
        ((FAIL++)) || true
        ERRORS+=("$name")
    fi
}

echo "Running hook tests..."
run_test "prefer-modern-tools: grep triggers warning" "$SCRIPT_DIR/prefer-modern-tools-grep.sh"
run_test "prefer-modern-tools: find triggers warning" "$SCRIPT_DIR/prefer-modern-tools-find.sh"
run_test "prefer-modern-tools: rg has no warning" "$SCRIPT_DIR/prefer-modern-tools-rg.sh"
run_test "prefer-modern-tools: quoted grep no warning" "$SCRIPT_DIR/prefer-modern-tools-quoted.sh"
run_test "post-tool-use: tool event written" "$SCRIPT_DIR/post-tool-use-tool-event.sh"
run_test "post-tool-use: empty output records output_bytes=0" "$SCRIPT_DIR/post-tool-use-empty-output.sh"
run_test "session-start: compact source emits re-anchor" "$SCRIPT_DIR/session-start-compact.sh"

echo ""
echo "Results: $PASS passed, $FAIL failed"

if [[ $FAIL -gt 0 ]]; then
    echo "Failed tests: ${ERRORS[*]}"
    exit 1
fi
