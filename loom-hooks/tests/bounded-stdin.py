#!/usr/bin/env python3
"""Held-open input must finish without GNU timeout, preserving guard decisions."""
import os
from pathlib import Path
import shutil
import signal
import subprocess
import tempfile
import time

HOOKS = Path(__file__).resolve().parent.parent


def run_case(path, name, payload, expected):
    env = {key: value for key, value in os.environ.items() if not key.startswith("LOOM_")}
    env.update(PATH=str(path), HOME=str(path.parent), TMPDIR=str(path.parent))
    interpreter = "python3" if name == "skill-trigger.sh" else "bash"
    proc = subprocess.Popen([str(path / interpreter), str(HOOKS / name)], env=env,
                            stdin=subprocess.PIPE, stdout=subprocess.PIPE,
                            stderr=subprocess.PIPE, start_new_session=True)
    try:
        start = time.monotonic()
        proc.stdin.write(payload.encode())
        proc.stdin.flush()  # Deliberately keep the input writer open.
        assert proc.wait(timeout=8) == expected, name
        assert time.monotonic() - start < 8, name
        proc.stdin.close()
        proc.stdin = None
        proc.communicate(timeout=2)
    finally:
        try:
            os.killpg(proc.pid, signal.SIGKILL)
        except ProcessLookupError:
            pass
        proc.wait(timeout=2)
        for pipe in (proc.stdin, proc.stdout, proc.stderr):
            if pipe is not None:
                pipe.close()


def main():
    with tempfile.TemporaryDirectory(prefix="loom-stdin-") as temp:
        path = Path(temp) / "bin"
        path.mkdir()
        for name in ("bash", "cat", "dirname", "sleep", "jq", "python3"):
            executable = shutil.which(name)
            assert executable, name
            (path / name).symlink_to(executable)
        for name in ("read-guard.sh", "codex-forward-guard.sh", "stage-terminal-guard.sh"):
            run_case(path, name, "{}", 0)
        run_case(path, "plans-path-guard.sh",
                 '{"tool_name":"Write","tool_input":{"file_path":"/x/.claude/plans/a.md"}}', 2)
        run_case(path, "worktree-file-guard.sh", "{", 2)
        run_case(path, "skill-trigger.sh", "{}", 0)
    print("PASS")


if __name__ == "__main__":
    main()
