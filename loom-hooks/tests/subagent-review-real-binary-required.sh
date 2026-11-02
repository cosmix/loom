#!/usr/bin/env bash
# A job that builds the binary must not pass subagent-review-real-binary.sh by
# skipping it. The copy resolves its repository root under the scratch directory,
# where no binary exists.
set -euo pipefail
TESTS=$(cd "$(dirname "$0")" && pwd)
TMP=$(mktemp -d "${TMPDIR:-/tmp}/review-binary-required.XXXXXX")
trap 'rm -rf "$TMP"' EXIT
mkdir -p "$TMP/loom-hooks/tests"
cp "$TESTS/subagent-review-real-binary.sh" "$TMP/loom-hooks/tests/"
COPY="$TMP/loom-hooks/tests/subagent-review-real-binary.sh"

OUT=$(env -u LOOM_HOOK_TEST_REQUIRE_BINARY bash "$COPY") || { echo "FAIL: a job with no build must skip: $OUT"; exit 1; }
[[ "$OUT" == skip:* ]] || { echo "FAIL: expected a skip line, got: $OUT"; exit 1; }

CODE=0
OUT=$(LOOM_HOOK_TEST_REQUIRE_BINARY=1 bash "$COPY") || CODE=$?
[[ $CODE -eq 1 && "$OUT" == FAIL:* ]] || {
	echo "FAIL: a required binary that is missing passed (exit $CODE): $OUT"
	exit 1
}
echo PASS
