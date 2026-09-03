#!/usr/bin/env bash
# When the preferred replacement itself is not installed, prefer-modern-tools.sh
# must let the legacy command through with a "tool not installed" warning
# instead of its usual STOP-and-redo guidance.
set -euo pipefail
HOOK="$(dirname "$0")/../prefer-modern-tools.sh"
source "$(dirname "$0")/_path_without.sh"

TMP=$(mktemp -d "${TMPDIR:-/tmp}/loom-hooktest.XXXXXX")
trap 'rm -rf "$TMP" "${NORG_PATH:-}" "${NOFD_PATH:-}"' EXIT

# (a) rg missing: grep is allowed through with a "ripgrep is not installed"
# warning, not the usual STOP guidance.
NORG_PATH=$(path_without rg)
INPUT_A='{"tool_name":"Bash","tool_input":{"command":"grep -rn foo src/"}}'
set +e
OUTPUT_A=$(printf '%s' "$INPUT_A" | PATH="$NORG_PATH" bash "$HOOK")
CODE_A=$?
set -e

if [[ $CODE_A -ne 0 ]]; then
	echo "FAIL(a): expected exit 0 when rg is missing, got $CODE_A"
	exit 1
fi
if [[ "$OUTPUT_A" != *"ripgrep is not installed"* ]]; then
	echo "FAIL(a): expected stdout to mention 'ripgrep is not installed', got: $OUTPUT_A"
	exit 1
fi
if [[ "$OUTPUT_A" == *"STOP"* ]]; then
	echo "FAIL(a): expected no STOP guidance when rg is missing, got: $OUTPUT_A"
	exit 1
fi

# (b) fd missing: find is allowed through with a "fd is not installed"
# warning, not the usual STOP guidance.
NOFD_PATH=$(path_without fd)
INPUT_B="{\"tool_name\":\"Bash\",\"tool_input\":{\"command\":\"find . -name '*.rs'\"}}"
set +e
OUTPUT_B=$(printf '%s' "$INPUT_B" | PATH="$NOFD_PATH" bash "$HOOK")
CODE_B=$?
set -e

if [[ $CODE_B -ne 0 ]]; then
	echo "FAIL(b): expected exit 0 when fd is missing, got $CODE_B"
	exit 1
fi
if [[ "$OUTPUT_B" != *"fd is not installed"* ]]; then
	echo "FAIL(b): expected stdout to mention 'fd is not installed', got: $OUTPUT_B"
	exit 1
fi
if [[ "$OUTPUT_B" == *"STOP"* ]]; then
	echo "FAIL(b): expected no STOP guidance when fd is missing, got: $OUTPUT_B"
	exit 1
fi

echo "PASS"
