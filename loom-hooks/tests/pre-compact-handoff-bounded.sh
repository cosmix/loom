#!/usr/bin/env bash
# pre-compact.sh bounds both of its `loom handoff` calls (first attempt: block;
# second attempt: allow) with a 30 s limit, so a hung handoff cannot hold up
# compaction. Shims named timeout and gtimeout log their arguments
# (-k 1 <seconds> command...) and then run the command.
set -euo pipefail
HOOK="$(cd "$(dirname "$0")/.." && pwd)/pre-compact.sh"
TMP=$(mktemp -d "${TMPDIR:-/tmp}/loom-precompact-bounded.XXXXXX")
trap 'rm -rf "$TMP"' EXIT

fail() {
	echo "FAIL: $1"
	exit 1
}

mkdir -p "$TMP/shims" "$TMP/work"
for shim in timeout gtimeout; do
	printf '%s\n' '#!/usr/bin/env bash' 'printf "%s\n" "$*" >>"$SHIM_LOG"' 'shift 3' 'exec "$@"' >"$TMP/shims/$shim"
	chmod +x "$TMP/shims/$shim"
done
printf '%s\n' '#!/usr/bin/env bash' 'exit 0' >"$TMP/loom-stub"
chmod +x "$TMP/loom-stub"
export SHIM_LOG="$TMP/shim.log"

run_hook() {
	printf '{}' | LOOM_HOOK_PATH="$TMP/shims:$PATH" LOOM_BIN="$TMP/loom-stub" LOOM_WORK_DIR="$TMP/work" \
		LOOM_STAGE_ID=test-stage LOOM_SESSION_ID=session-test-abc bash "$HOOK" >/dev/null 2>&1 || true
}

run_hook
[[ -f "$TMP/work/compaction-pending/session-test-abc" ]] || fail "the first attempt did not create the pending flag"
run_hook
[[ ! -e "$TMP/work/compaction-pending/session-test-abc" ]] || fail "the second attempt did not remove the pending flag"

BOUNDED=$(grep ' handoff ' "$SHIM_LOG" || true)
[[ $(printf '%s\n' "$BOUNDED" | wc -l | tr -d ' ') == 2 ]] ||
	fail "expected two bounded handoff calls, got: $(cat "$SHIM_LOG")"
while IFS= read -r line; do
	[[ "$line" == "-k 1 30 $TMP/loom-stub handoff "* ]] || fail "handoff was not bounded by 30 s: $line"
done <<<"$BOUNDED"
echo PASS
