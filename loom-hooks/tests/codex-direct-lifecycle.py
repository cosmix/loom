#!/usr/bin/env python3
"""Lifecycle fixtures shared with Rust ownership tests: inherited output after exit."""
import importlib.util
import json
import os
from pathlib import Path
import signal
import subprocess
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
        handlers = {sig: signal.getsignal(sig) for sig in (signal.SIGTERM, signal.SIGHUP)}
        result = supervisor.supervise([sys.executable, "-c", code], 2000, 500)
        assert all(signal.getsignal(sig) == handler for sig, handler in handlers.items())
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


def signal_runner(pid_file, sent, during_spawn):
    child_code = "\n".join([
        "import json, os, subprocess, sys, time",
        "child = subprocess.Popen([sys.executable, '-c', 'import time; time.sleep(60)'])",
        f"path = {str(pid_file)!r}; open(path + '.tmp', 'w').write(json.dumps([os.getpid(), child.pid])); os.replace(path + '.tmp', path)",
        f"events = {EVENTS[:2]!r}",
        "for event in events: print(json.dumps(event), flush=True)",
        "while True: print(' ', flush=True); time.sleep(0.01)",
    ])
    # Raising the signal inside Popen covers the window before registration.
    startup = "\n".join([
        "original = module.subprocess.Popen",
        "def spawn(*args, **kwargs):",
        "    child = original(*args, **kwargs)",
        f"    path = {str(pid_file)!r}; open(path + '.tmp', 'w').write(json.dumps([child.pid])); os.replace(path + '.tmp', path)",
        f"    os.kill(os.getpid(), {sent})",
        "    return child",
        "module.subprocess.Popen = spawn",
    ]) if during_spawn else ""
    runner = "\n".join([
        "import dataclasses, importlib.util, json, os, sys",
        f"sys.path.insert(0, {str(HOOKS)!r})",
        f"spec = importlib.util.spec_from_file_location('direct_fixture', {str(HOOKS / '_codex-direct.py')!r})",
        "module = importlib.util.module_from_spec(spec); sys.modules[spec.name] = module; spec.loader.exec_module(module)",
        startup,
        f"result = module.supervise([sys.executable, '-c', {child_code!r}], 60000, 5000)",
        "print(json.dumps(dataclasses.asdict(result)), flush=True)",
    ])
    return runner


def run_signal_case(directory, sent, during_spawn=False):
    pid_file = directory / f"signal-{sent}-{during_spawn}"
    runner = signal_runner(pid_file, sent, during_spawn)
    process = subprocess.Popen([sys.executable, "-c", runner], stdout=subprocess.PIPE,
                               stderr=subprocess.PIPE, text=True)
    try:
        deadline = time.monotonic() + 5
        while not pid_file.exists() and process.poll() is None and time.monotonic() < deadline:
            time.sleep(0.01)
        assert pid_file.exists(), "child did not start"
        pids = json.loads(pid_file.read_text())
        if not during_spawn:
            time.sleep(0.1)  # The supervisor has begun draining the live child's output.
            os.kill(process.pid, sent)
        stdout, stderr = process.communicate(timeout=8)
        assert process.returncode == 0, stderr
        result = json.loads(stdout.strip().splitlines()[-1])
        assert result["state"] == "cancelled" and not result["ownership_retained"], result
        assert supervisor._probe_group(pids[0]) == (True, False)
        for pid in pids:
            try:
                os.kill(pid, 0)
            except ProcessLookupError:
                continue
            raise AssertionError(f"owned process {pid} survived")
    finally:
        if process.poll() is None:
            process.kill()
            process.wait()
        if pid_file.exists():
            try:
                os.killpg(json.loads(pid_file.read_text())[0], signal.SIGKILL)
            except ProcessLookupError:
                pass


def main():
    with tempfile.TemporaryDirectory(prefix="loom-direct-lifecycle-") as temp:
        for terminal in (True, False):
            run_case(Path(temp), terminal)
        for sent in (signal.SIGTERM, signal.SIGHUP):
            run_signal_case(Path(temp), sent)
            run_signal_case(Path(temp), sent, during_spawn=True)
    print("PASS")


if __name__ == "__main__":
    main()
