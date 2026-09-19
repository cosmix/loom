#!/usr/bin/env bash
# When the preferred replacement itself is not installed, prefer-modern-tools.sh
# must let the legacy command through with a "tool not installed" warning
# instead of its usual STOP-and-redo guidance.
set -euo pipefail
# See prefer-modern-tools-grep.sh for why the live stage vars must be unset.
# LOOM_HOOK_PATH must go too, specifically for this file: _read_discipline.sh
# (sourced by the hook) does `PATH="${LOOM_HOOK_PATH:-$PATH}"`, so a live
# stage's LOOM_HOOK_PATH would splice the real PATH - rg and fd included -
# back in over the rg-less/fd-less PATH this test builds below.
unset LOOM_WORK_DIR LOOM_SESSION_ID LOOM_STAGE_ID LOOM_SESSION_TYPE LOOM_HOOK_PATH
HOOK="$(dirname "$0")/../prefer-modern-tools.sh"
source "$(dirname "$0")/_path_without.sh"

TMP=$(mktemp -d "${TMPDIR:-/tmp}/loom-hooktest.XXXXXX")
# Fresh TMPDIR for the hook's own "tools" ledger: outside a loom stage it
# falls back to ${TMPDIR:-/tmp}/loom-tools/<session>.tsv keyed only by
# session id (absent here, so "unknown") - shared with every other test in
# the same run unless isolated, which would silently suppress a warning
# expected here as "already warned" by an earlier test.
HOOK_TMPDIR=$(mktemp -d "${TMPDIR:-/tmp}/pmt-missing.XXXXXX")
trap 'rm -rf "$TMP" "$HOOK_TMPDIR" "${NORG_PATH:-}" "${NOFD_PATH:-}"' EXIT

# (a) rg missing: grep is allowed through with a "ripgrep is not installed"
# warning, not the usual STOP guidance.
NORG_PATH=$(path_without rg)
INPUT_A='{"tool_name":"Bash","tool_input":{"command":"grep -rn foo src/"}}'
set +e
OUTPUT_A=$(printf '%s' "$INPUT_A" | PATH="$NORG_PATH" TMPDIR="$HOOK_TMPDIR" bash "$HOOK")
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
OUTPUT_B=$(printf '%s' "$INPUT_B" | PATH="$NOFD_PATH" TMPDIR="$HOOK_TMPDIR" bash "$HOOK")
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
