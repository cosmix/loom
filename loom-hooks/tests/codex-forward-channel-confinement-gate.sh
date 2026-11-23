#!/usr/bin/env bash
# The confinement probe must skip, not fail, when its runtime is absent, and
# must fail when LOOM_HOOK_TEST_REQUIRE_SRT=1 says the runtime is required.
set -euo pipefail
TESTS=$(cd "$(dirname "$0")" && pwd)
PROBE="$TESTS/codex-forward-channel-confinement.sh"
if [[ "$(uname -s)" != Linux ]]; then
	echo "SKIP: the confinement probe gate is exercised on Linux only"
	exit 0
fi

OUT=$(env -u LOOM_HOOK_TEST_REQUIRE_SRT LOOM_TEST_SRT_BIN=/nonexistent/srt bash "$PROBE" 2>&1) || {
	echo "FAIL: a runner without srt must skip: $OUT"
	exit 1
}
[[ "$OUT" == SKIP:* ]] || {
	echo "FAIL: expected a SKIP line, got: $OUT"
	exit 1
}

CODE=0
OUT=$(LOOM_HOOK_TEST_REQUIRE_SRT=1 LOOM_TEST_SRT_BIN=/nonexistent/srt bash "$PROBE" 2>&1) || CODE=$?
[[ $CODE -eq 1 && "$OUT" == FAIL:* ]] || {
	echo "FAIL: a required srt that is missing passed (exit $CODE): $OUT"
	exit 1
}
echo PASS
