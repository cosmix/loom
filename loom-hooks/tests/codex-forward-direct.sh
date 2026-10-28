#!/usr/bin/env bash
set -euo pipefail

unset LOOM_STAGE_ID LOOM_SESSION_ID LOOM_WORK_DIR LOOM_SESSION_TYPE LOOM_MAIN_AGENT_PID
d=$(mktemp -d "${TMPDIR:-/tmp}/codex-direct.XXXXXX") && [[ -n "$d" ]]
trap 'rm -rf "$d"' EXIT

HOOKS_DIR=$(cd "$(dirname "$0")/.." && pwd)
WRAPPER="$HOOKS_DIR/codex-forward.sh"
SUPERVISOR="$HOOKS_DIR/_codex-direct.py"
BIN_DIR="$d/bin"
INVOCATION=inv-0123456789abcdef0123456789abcdef
mkdir -p "$BIN_DIR"

cat >"$BIN_DIR/sandbox-exec" <<'STUB'
#!/usr/bin/env bash
exit 71
STUB
cat >"$BIN_DIR/codex" <<'STUB'
#!/usr/bin/env python3
import json
import os
import signal
import sys

# Exec schema: Codex 0.162.1 / 092d3acd6bec3e3a14bdc7e7a2810ab628ab759d.
# The START descriptor (fd 3) belongs to the supervisor; codex must not inherit it.
try:
    os.fstat(3)
except OSError:
    pass
else:
    sys.stderr.write("fixture: fd 3 leaked into codex\n")
    sys.exit(97)
with open(os.environ["FAKE_CODEX_PID"], "w", encoding="ascii") as output:
    output.write(str(os.getpid()))
with open(os.environ["FAKE_CODEX_ARGV"], "w", encoding="utf-8") as output:
    json.dump(sys.argv[1:], output)

def emit(value):
    print(json.dumps(value, separators=(",", ":")), flush=True)

emit({"type": "thread.started", "thread_id": "thread-exact"})
if os.environ.get("DIRECT_SCENARIO") == "gated":
    import time
    for _ in range(600):
        if os.path.exists(os.environ["RELEASE"]):
            break
        time.sleep(0.05)
emit({"type": "turn.started"})
if os.environ.get("DIRECT_SCENARIO") == "oversized-item":
    emit({"type": "item.completed", "item": {"id": "item_0", "type": "command_execution",
          "aggregated_output": "x" * 70_000, "status": "completed"}})
if os.environ.get("DIRECT_SCENARIO") == "oversized-other":
    emit({"type": "turn.note", "padding": "x" * 70_000})
if os.environ.get("DIRECT_SCENARIO") == "failed":
    emit({"type": "turn.failed", "error": {"message": "fixture failure"}})
    sys.exit(1)
if os.environ.get("DIRECT_SCENARIO") == "timeout":
    signal.signal(signal.SIGTERM, signal.SIG_IGN)
    while True:
        signal.pause()
emit({"type": "item.completed", "item": {"type": "agent_message", "text": "direct final"}})
emit({"type": "turn.completed", "usage": {"input_tokens": 10, "cached_input_tokens": 2, "output_tokens": 3}})
STUB
chmod +x "$BIN_DIR/codex" "$BIN_DIR/sandbox-exec"

assert_gone() {
	local pid
	pid=$(command cat "$1")
	! kill -0 "$pid" 2>/dev/null
}

# The nested-Seatbelt detection seam selects one foreground supervisor call.
OUT="$d/wrapper.out"; ERR="$d/wrapper.err"
HOME="$d/home" PATH="$BIN_DIR:$PATH" FAKE_CODEX_PID="$d/wrapper.pid" \
	FAKE_CODEX_ARGV="$d/wrapper.argv" LOOM_STAGE_ID=stage-one LOOM_SESSION_ID=session-one \
	bash "$WRAPPER" task 'direct prompt' --model gpt-6.1-terra --effort xhigh --write \
	--unit-id unit-a --invocation-id "$INVOCATION" >"$OUT" 2>"$ERR"
[[ ! -s "$ERR" ]]
sed -n '1p' "$OUT" | jq -Rer '
	ltrimstr("LOOM-FORWARD-START ") | fromjson |
	. == {v:1,backend:"direct",thread_id:"thread-exact"}' >/dev/null
sed -n '2p' "$OUT" | jq -Rer '
	ltrimstr("LOOM-FORWARD-END ") | fromjson |
	. == {v:1,backend:"direct",thread_id:"thread-exact",outcome:"succeeded",exit_code:0}' >/dev/null
rg -qF 'LOOM-CODEX-DIRECT-RESULT {"v":1,"state":"succeeded","thread_id":"thread-exact"' "$OUT"
rg -qFx 'unit: unit-a' "$OUT"
rg -qFx "invocation: $INVOCATION" "$OUT"
rg -qFx 'state: succeeded' "$OUT"
jq -e '
	.[0:3] == ["exec","--json","--sandbox"] and
	index("danger-full-access") != null and index("--skip-git-repo-check") != null' \
	"$d/wrapper.argv" >/dev/null
assert_gone "$d/wrapper.pid"

# A real exec failure also reaches the wrapper without a fabricated turn ID.
OUT="$d/failed.out"; ERR="$d/failed.err"; status=0
HOME="$d/home" PATH="$BIN_DIR:$PATH" DIRECT_SCENARIO=failed \
	FAKE_CODEX_PID="$d/failed.pid" FAKE_CODEX_ARGV="$d/failed.argv" \
	LOOM_STAGE_ID=stage-one LOOM_SESSION_ID=session-one \
	bash "$WRAPPER" task 'direct failure' --model gpt-6.1-terra --effort xhigh --write \
	--unit-id unit-a --invocation-id "$INVOCATION" >"$OUT" 2>"$ERR" || status=$?
[[ $status -eq 1 && ! -s "$ERR" ]]
rg '^LOOM-CODEX-DIRECT-RESULT ' "$OUT" | jq -Rer '
	ltrimstr("LOOM-CODEX-DIRECT-RESULT ") | fromjson |
	.state == "failed" and .turn_id == null and .exit_code == 1 and
	.ownership_retained == false' >/dev/null
assert_gone "$d/failed.pid"

# A timed-out child is killed immediately and reaped within the cleanup deadline.
# The timeout leaves a loaded runner time to start the fake Codex; the grace is
# only a bound, since _retire returns once the group is reaped.
OUT="$d/timeout.out"; ERR="$d/timeout.err"; status=0
PATH="$BIN_DIR:$PATH" DIRECT_SCENARIO=timeout FAKE_CODEX_PID="$d/timeout.pid" \
	FAKE_CODEX_ARGV="$d/timeout.argv" python3 "$SUPERVISOR" --timeout-ms 1000 --grace-ms 5000 -- \
	codex exec --json -- test >"$OUT" 2>"$ERR" || status=$?
[[ $status -eq 124 && ! -s "$ERR" ]]
tail -n 1 "$OUT" | jq -Rer '
	ltrimstr("LOOM-CODEX-DIRECT-RESULT ") | fromjson |
	.state == "cancelled" and .thread_id == "thread-exact" and .turn_id == null and
	.exit_code == 124 and .ownership_retained == false' >/dev/null
assert_gone "$d/timeout.pid"

# Signal failure is fail-closed even when individual tracked-PID cleanup succeeds.
PATH="$BIN_DIR:$PATH" DIRECT_SCENARIO=timeout FAKE_CODEX_PID="$d/signal.pid" \
	FAKE_CODEX_ARGV="$d/signal.argv" SUPERVISOR="$SUPERVISOR" python3 - <<'PY' >"$d/signal.out"
import importlib.util
import os
import sys

spec = importlib.util.spec_from_file_location("codex_direct", os.environ["SUPERVISOR"])
module = importlib.util.module_from_spec(spec)
sys.modules[spec.name] = module
sys.path.insert(0, os.path.dirname(os.environ["SUPERVISOR"]))
spec.loader.exec_module(module)
module._signal_group = lambda _pgid, _signal: False
result = module.supervise(["codex", "exec", "--json", "--", "test"], 1000, 5000)
assert result.state == "unknown"
assert result.ownership_retained is True
assert result.exit_code == 125
PY
assert_gone "$d/signal.pid"

# A tracked survivor can never be upgraded to successful terminal evidence.
PATH="$BIN_DIR:$PATH" FAKE_CODEX_PID="$d/survivor.pid" \
	FAKE_CODEX_ARGV="$d/survivor.argv" SUPERVISOR="$SUPERVISOR" python3 - <<'PY' >"$d/survivor.out"
import importlib.util
import os
import sys

spec = importlib.util.spec_from_file_location("codex_direct", os.environ["SUPERVISOR"])
module = importlib.util.module_from_spec(spec)
sys.modules[spec.name] = module
sys.path.insert(0, os.path.dirname(os.environ["SUPERVISOR"]))
spec.loader.exec_module(module)
module.Descendants.living = lambda _self: [999999]
result = module.supervise(["codex", "exec", "--json", "--", "test"], 500, 20)
assert result.state == "unknown"
assert result.ownership_retained is True
assert result.exit_code == 125
PY
assert_gone "$d/survivor.pid"

run_wrapper() {
	local scenario="$1" out="$2" wrapper="${3:-$WRAPPER}"
	HOME="$d/home" PATH="$BIN_DIR:$PATH" DIRECT_SCENARIO="$scenario" RELEASE="$d/release" \
		FAKE_CODEX_PID="$d/$scenario.pid" FAKE_CODEX_ARGV="$d/$scenario.argv" \
		LOOM_STAGE_ID=stage-one LOOM_SESSION_ID=session-one \
		bash "$wrapper" task "direct $scenario" --model gpt-6.1-terra --effort xhigh --write \
		--unit-id unit-a --invocation-id "$INVOCATION" >"$out" 2>"$d/$scenario.err"
}

# START reaches stdout while codex is still running (a backgrounded forward's
# output file holds it); END follows only once the thread finishes.
OUT="$d/gated.out"; rm -f "$d/release"
run_wrapper gated "$OUT" &
gated_pid=$!
for _ in $(seq 1 200); do
	[[ -s "$OUT" ]] && break
	sleep 0.05
done
[[ $(wc -l <"$OUT") -eq 1 ]]
sed -n '1p' "$OUT" | jq -Rer '
	ltrimstr("LOOM-FORWARD-START ") | fromjson | . == {v:1,backend:"direct",thread_id:"thread-exact"}' >/dev/null
touch "$d/release"
wait "$gated_pid"
sed -n '2p' "$OUT" | rg -qF 'LOOM-FORWARD-END {"v":1,"backend":"direct","thread_id":"thread-exact","outcome":"succeeded"'
[[ $(rg -c '^LOOM-FORWARD-START ' "$OUT") -eq 1 ]]

# An item event above the per-event bound (whole command output) is skipped;
# any other oversized line still leaves the turn without a verdict.
OUT="$d/oversized-item.out"
run_wrapper oversized-item "$OUT"
rg -qFx 'state: succeeded' "$OUT"
rg -q '^note: skipped a [0-9]+-byte codex item\.completed event over the 65536-byte line cap$' "$OUT"
OUT="$d/oversized-other.out"; status=0
run_wrapper oversized-other "$OUT" || status=$?
[[ $status -eq 125 ]]
sed -n '2p' "$OUT" | rg -qF '"outcome":"unknown","exit_code":125}'
rg -qF '"state":"unknown","thread_id":"thread-exact"' "$OUT"

# Require rg to report no match (exit 1); a read error (exit 2) fails too.
assert_absent() {
	local status=0
	rg -q "$@" || status=$?
	[[ $status -eq 1 ]] || {
		printf '%s\n' "FAIL: rg $* exited $status, expected 1 (no match)"
		exit 1
	}
}

# Fake supervisors pin the wrapper's handling of result lines the real one
# emits only under failure. Args: name, START thread ("" writes none), result
# state, result thread, exit status, ownership_retained, announced_thread_id.
fake_supervisor() {
	local dir="$d/fake-$1"
	mkdir -p "$dir"
	cp "$WRAPPER" "$dir/codex-forward.sh"
	cat >"$dir/_codex-direct.py" <<PY
import json, os, sys
if "$2":
    os.write(3, b'LOOM-FORWARD-START {"v":1,"backend":"direct","thread_id":"$2"}\n')
print("LOOM-CODEX-DIRECT-RESULT " + json.dumps({"v": 1, "state": "$3", "thread_id": "$4",
      "turn_id": None, "terminal_at": "2026-01-01T00:00:00.000Z", "exit_code": $5,
      "ownership_retained": $6, "announced_thread_id": $7}, separators=(",", ":")))
sys.exit($5)
PY
	printf '%s\n' "$dir/codex-forward.sh"
}

# A result whose thread differs from the announced START fails closed with no END.
OUT="$d/mismatch.out"; status=0
FAKE=$(fake_supervisor mismatch thread-a succeeded thread-b 0 False '"thread-a"')
run_wrapper mismatch "$OUT" "$FAKE" || status=$?
[[ $status -eq 1 ]]
assert_absent '^LOOM-FORWARD-END ' "$OUT"
rg -qF 'START marker thread disagrees with the supervisor result' "$OUT"

# A START the supervisor could not write (announced_thread_id null) fails closed.
OUT="$d/unannounced.out"; status=0
FAKE=$(fake_supervisor unannounced '' succeeded thread-exact 0 False None)
run_wrapper unannounced "$OUT" "$FAKE" || status=$?
[[ $status -eq 1 ]]
assert_absent '^LOOM-FORWARD-(START|END) ' "$OUT"
rg -qF 'START marker was never written (announced_thread_id is null)' "$OUT"

# A retained owner (a descendant may still write) ends with an `unknown` END.
OUT="$d/retained.out"; status=0
FAKE=$(fake_supervisor retained thread-exact unknown thread-exact 125 True '"thread-exact"')
run_wrapper retained "$OUT" "$FAKE" || status=$?
[[ $status -eq 125 ]]
sed -n '2p' "$OUT" | jq -Rer '
	ltrimstr("LOOM-FORWARD-END ") | fromjson |
	. == {v:1,backend:"direct",thread_id:"thread-exact",outcome:"unknown",exit_code:125}' >/dev/null
rg -qFx 'state: unknown' "$OUT"

# Linux: a refused nested bubblewrap selects direct mode; a working one leaves
# the companion lane. The macOS probe is absent here (no sandbox-exec stub), so
# only the bwrap probe can decide. The mode line stays byte-identical; the note
# line names bubblewrap.
if [[ "$(uname -s)" == Linux ]]; then
	LINUX_BIN="$d/linux-bin"
	mkdir -p "$LINUX_BIN"
	cp "$BIN_DIR/codex" "$LINUX_BIN/codex"
	for status in 1 0; do
		printf '#!/usr/bin/env bash\nexit %s\n' "$status" >"$LINUX_BIN/bwrap"
		chmod +x "$LINUX_BIN/bwrap"
		OUT="$d/bwrap-$status.out"; ERR="$d/bwrap-$status.err"
		HOME="$d/home" PATH="$LINUX_BIN:$PATH" FAKE_CODEX_PID="$d/bwrap-$status.pid" \
			FAKE_CODEX_ARGV="$d/bwrap-$status.argv" LOOM_STAGE_ID=stage-one LOOM_SESSION_ID=session-one \
			bash "$WRAPPER" task 'bwrap probe' --model gpt-6.1-terra --effort xhigh --write \
			--unit-id unit-a --invocation-id "$INVOCATION" >"$OUT" 2>"$ERR" || true
		if [[ $status -eq 1 ]]; then
			rg -qF 'LOOM-FORWARD-START {"v":1,"backend":"direct"' "$OUT"
			rg -qFx 'mode: direct (codex exec --sandbox danger-full-access; nested Seatbelt refused)' "$OUT"
			rg -qF 'refuses a nested bubblewrap sandbox' "$OUT"
			assert_gone "$d/bwrap-$status.pid"
		else
			assert_absent -F '"backend":"direct"' "$OUT"
			[[ ! -e "$d/bwrap-$status.argv" ]]
		fi
	done
fi

printf '%s\n' PASS
