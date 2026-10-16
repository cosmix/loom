#!/usr/bin/env python3
"""Lifecycle fixtures shared with Rust ownership tests: inherited output after exit."""
import importlib.util
import json
import os
from pathlib import Path
import signal
import sys
import tempfile
import time

HOOKS = Path(__file__).resolve().parent.parent
sys.path.insert(0, str(HOOKS))
spec = importlib.util.spec_from_file_location("codex_direct", HOOKS / "_codex-direct.py")
supervisor = importlib.util.module_from_spec(spec)
sys.modules[spec.name] = supervisor
spec.loader.exec_module(supervisor)

# Pinned exec_events.rs schema: Codex 0.162.1, commit
# 092d3acd6bec3e3a14bdc7e7a2810ab628ab759d. No event invents turn_id.
EVENTS = [
    {"type": "thread.started", "thread_id": "thread-fixture"},
    {"type": "turn.started"},
    {"type": "turn.completed", "usage": {
        "input_tokens": 10, "cached_input_tokens": 2, "output_tokens": 3}},
]


def run_case(directory, terminal):
    pid_file = directory / ("completed" if terminal else "incomplete")
    code = "\n".join([
        "import json, os, subprocess, sys",
        "child = subprocess.Popen([sys.executable, '-c', 'import time; time.sleep(60)'])",
        f"open({str(pid_file)!r}, 'w').write(json.dumps([os.getpid(), child.pid]))",
        f"events = {EVENTS if terminal else EVENTS[:2]!r}",
        "for event in events: print(json.dumps(event), flush=True)",
        "os._exit(0)",
    ])
    try:
        start = time.monotonic()
        result = supervisor.supervise([sys.executable, "-c", code], 2000, 500)
        assert time.monotonic() - start < 3
        assert result.state == ("succeeded" if terminal else "unknown"), result
        assert result.ownership_retained is False, result
        assert result.turn_id is None
        leader, child = json.loads(pid_file.read_text())
        assert supervisor._probe_group(leader) == (True, False)
    finally:
        if pid_file.exists():
            leader, child = json.loads(pid_file.read_text())
            try:
                os.killpg(leader, signal.SIGKILL)
            except ProcessLookupError:
                pass
            supervisor._reap_adopted()


def main():
    with tempfile.TemporaryDirectory(prefix="loom-direct-lifecycle-") as temp:
        for terminal in (True, False):
            run_case(Path(temp), terminal)
    print("PASS")


if __name__ == "__main__":
    main()
