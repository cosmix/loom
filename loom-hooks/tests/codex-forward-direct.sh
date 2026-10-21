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
with open(os.environ["FAKE_CODEX_PID"], "w", encoding="ascii") as output:
    output.write(str(os.getpid()))
with open(os.environ["FAKE_CODEX_ARGV"], "w", encoding="utf-8") as output:
    json.dump(sys.argv[1:], output)

def emit(value):
    print(json.dumps(value, separators=(",", ":")), flush=True)

emit({"type": "thread.started", "thread_id": "thread-exact"})
emit({"type": "turn.started"})
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
	bash "$WRAPPER" task 'direct prompt' --model gpt-5.6-terra --effort xhigh --write \
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
	bash "$WRAPPER" task 'direct failure' --model gpt-5.6-terra --effort xhigh --write \
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
			bash "$WRAPPER" task 'bwrap probe' --model gpt-5.6-terra --effort xhigh --write \
			--unit-id unit-a --invocation-id "$INVOCATION" >"$OUT" 2>"$ERR" || true
		if [[ $status -eq 1 ]]; then
			rg -qF 'LOOM-FORWARD-START {"v":1,"backend":"direct"' "$OUT"
			rg -qFx 'mode: direct (codex exec --sandbox danger-full-access; nested Seatbelt refused)' "$OUT"
			rg -qF 'refuses a nested bubblewrap sandbox' "$OUT"
			assert_gone "$d/bwrap-$status.pid"
		else
			! rg -qF '"backend":"direct"' "$OUT"
			[[ ! -e "$d/bwrap-$status.argv" ]]
		fi
	done
fi

printf '%s\n' PASS
