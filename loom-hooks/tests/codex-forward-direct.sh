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

with open(os.environ["FAKE_CODEX_PID"], "w", encoding="ascii") as output:
    output.write(str(os.getpid()))
with open(os.environ["FAKE_CODEX_ARGV"], "w", encoding="utf-8") as output:
    json.dump(sys.argv[1:], output)

def emit(value):
    print(json.dumps(value, separators=(",", ":")), flush=True)

emit({"type": "thread.started", "thread_id": "thread-exact"})
emit({"type": "turn.started", "turn_id": "turn-exact"})
if os.environ.get("DIRECT_SCENARIO") == "timeout":
    signal.signal(signal.SIGTERM, signal.SIG_IGN)
    while True:
        signal.pause()
emit({"type": "item.completed", "item": {"type": "agent_message", "text": "direct final"}})
emit({"type": "turn.completed", "turn_id": "turn-exact"})
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
rg -qF 'LOOM-CODEX-DIRECT-RESULT {"v":1,"state":"succeeded","thread_id":"thread-exact","turn_id":"turn-exact"' "$OUT"
rg -qFx 'unit: unit-a' "$OUT"
rg -qFx "invocation: $INVOCATION" "$OUT"
rg -qFx 'state: succeeded' "$OUT"
jq -e '
	.[0:3] == ["exec","--json","--sandbox"] and
	index("danger-full-access") != null and index("--skip-git-repo-check") != null' \
	"$d/wrapper.argv" >/dev/null
assert_gone "$d/wrapper.pid"

# A timed-out child receives TERM, then KILL after the bounded grace and is reaped.
OUT="$d/timeout.out"; ERR="$d/timeout.err"; status=0
PATH="$BIN_DIR:$PATH" DIRECT_SCENARIO=timeout FAKE_CODEX_PID="$d/timeout.pid" \
	FAKE_CODEX_ARGV="$d/timeout.argv" python3 "$SUPERVISOR" --timeout-ms 80 --grace-ms 40 -- \
	codex exec --json -- test >"$OUT" 2>"$ERR" || status=$?
[[ $status -eq 124 && ! -s "$ERR" ]]
tail -n 1 "$OUT" | jq -Rer '
	ltrimstr("LOOM-CODEX-DIRECT-RESULT ") | fromjson |
	.state == "cancelled" and .thread_id == "thread-exact" and .turn_id == "turn-exact" and
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
spec.loader.exec_module(module)
module._signal_group = lambda _pgid, _signal: False
result = module.supervise(["codex", "exec", "--json", "--", "test"], 30, 20)
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
spec.loader.exec_module(module)
module.Descendants.living = lambda _self: [999999]
result = module.supervise(["codex", "exec", "--json", "--", "test"], 500, 20)
assert result.state == "unknown"
assert result.ownership_retained is True
assert result.exit_code == 125
PY
assert_gone "$d/survivor.pid"

printf '%s\n' PASS
