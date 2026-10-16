#!/usr/bin/env python3
"""Bounded supervisor for one direct ``codex exec --json`` invocation."""

from __future__ import annotations

import argparse
import ctypes
import errno
import os
import selectors
import signal
import subprocess
import sys
import time

from _codex_direct_events import DirectResult, EventState, JsonStream, utc_now

POLL_SECONDS = 0.05
MAX_STDERR_BYTES = 16_384


def _linux_snapshot() -> dict[int, tuple[int, str, str]] | None:
    result: dict[int, tuple[int, str, str]] = {}
    try:
        names = os.listdir("/proc")
    except OSError:
        return None
    for name in names:
        if not name.isdigit():
            continue
        try:
            with open(f"/proc/{name}/stat", encoding="ascii") as stat_file:
                data = stat_file.read()
            fields = data[data.rfind(")") + 2 :].split()
            result[int(name)] = (int(fields[1]), fields[19], fields[0])
        except (OSError, ValueError, IndexError):
            continue
    return result


def _ps_snapshot() -> dict[int, tuple[int, str, str]] | None:
    try:
        run = subprocess.run(
            ["/bin/ps", "-axo", "pid=,ppid=,lstart=,state="],
            check=True,
            capture_output=True,
            text=True,
            timeout=1,
        )
    except (OSError, subprocess.SubprocessError):
        return None
    result: dict[int, tuple[int, str, str]] = {}
    for line in run.stdout.splitlines():
        parts = line.split()
        if len(parts) < 8 or not parts[0].isdigit() or not parts[1].isdigit():
            continue
        result[int(parts[0])] = (int(parts[1]), " ".join(parts[2:7]), parts[7])
    return result


def _snapshot() -> dict[int, tuple[int, str, str]] | None:
    return _linux_snapshot() if sys.platform.startswith("linux") else _ps_snapshot()


class Descendants:
    def __init__(self, root_pid: int) -> None:
        self.root_pid = root_pid
        self.identities: dict[int, str] = {}
        self.probe_failed = not self.refresh()

    def refresh(self) -> bool:
        snapshot = _snapshot()
        if snapshot is None:
            self.probe_failed = True
            return False
        children: dict[int, list[int]] = {}
        for pid, (ppid, _, _) in snapshot.items():
            children.setdefault(ppid, []).append(pid)
        pending = [self.root_pid]
        seen: set[int] = set()
        while pending:
            pid = pending.pop()
            if pid in seen:
                continue
            seen.add(pid)
            if pid in snapshot:
                self.identities.setdefault(pid, snapshot[pid][1])
            pending.extend(children.get(pid, []))
        return True

    def living(self) -> list[int] | None:
        snapshot = _snapshot()
        if snapshot is None:
            self.probe_failed = True
            return None
        return [
            pid
            for pid, token in self.identities.items()
            if pid in snapshot and snapshot[pid][1] == token and snapshot[pid][2] != "Z"
        ]


def _enable_subreaper() -> None:
    if not sys.platform.startswith("linux"):
        return
    try:
        ctypes.CDLL(None, use_errno=True).prctl(36, 1, 0, 0, 0)
    except (AttributeError, OSError):
        pass


def _signal_group(pgid: int, sent: signal.Signals) -> bool:
    try:
        os.killpg(pgid, sent)
        return True
    except ProcessLookupError:
        return True
    except OSError:
        return False


def _probe_group(pgid: int) -> tuple[bool, bool]:
    try:
        os.killpg(pgid, 0)
        return True, True
    except ProcessLookupError:
        return True, False
    except PermissionError:
        return False, True
    except OSError as error:
        return (True, False) if error.errno == errno.ESRCH else (False, True)


def _drain(selector: selectors.BaseSelector, stream: JsonStream, stderr: list[int]) -> None:
    for key, _ in selector.select(timeout=POLL_SECONDS):
        try:
            chunk = os.read(key.fd, 8192)
        except BlockingIOError:
            continue
        if not chunk:
            selector.unregister(key.fileobj)
            if key.data == "stdout":
                stream.feed(b"", eof=True)
            continue
        if key.data == "stdout":
            stream.feed(chunk)
        elif stderr[0] < MAX_STDERR_BYTES:
            kept = chunk[: MAX_STDERR_BYTES - stderr[0]]
            sys.stderr.buffer.write(kept)
            sys.stderr.buffer.flush()
            stderr[0] += len(kept)


def _wait_until(proc: subprocess.Popen[bytes], deadline: float, tick) -> bool:
    while proc.poll() is None and time.monotonic() < deadline:
        tick()
    return proc.poll() is not None


def _reap_adopted() -> None:
    while True:
        try:
            pid, _ = os.waitpid(-1, os.WNOHANG)
        except (ChildProcessError, OSError):
            return
        if pid == 0:
            return


def _retire(proc: subprocess.Popen[bytes], tracker: Descendants, deadline: float, tick) -> bool:
    # Same ownership contract as Rust OwnedProcess: kill the owned group on
    # timeout or leader exit, reap and drain within one cleanup deadline.
    # Snapshot identities also cover observed descendants that escaped the group.
    signal_ok = _signal_group(proc.pid, signal.SIGKILL)
    living = tracker.living()
    for pid in living or []:
        try:
            os.kill(pid, signal.SIGKILL)
        except ProcessLookupError:
            continue
        except OSError:
            signal_ok = False
    joined = _wait_until(proc, deadline, tick)
    while joined:
        _reap_adopted()
        group_ok, group_alive = _probe_group(proc.pid)
        living_now = tracker.living()
        if group_ok and not group_alive and living_now == []:
            return signal_ok and living is not None and not tracker.probe_failed
        if time.monotonic() >= deadline:
            break
        tick()
    return False


def _normal_result(proc: subprocess.Popen[bytes], events: EventState, tracker: Descendants) -> DirectResult:
    group_ok, group_alive = _probe_group(proc.pid)
    living = tracker.living()
    retained = not group_ok or group_alive or living is None or bool(living) or tracker.probe_failed
    complete = not events.invalid and events.thread_id is not None and events.started
    if retained or not complete:
        state, code = "unknown", 125
    elif proc.returncode == 0 and events.terminal == "turn.completed":
        state, code = "succeeded", 0
    elif events.terminal == "turn.failed":
        state, code = "failed", proc.returncode if 0 < proc.returncode < 126 else 1
    elif events.terminal == "turn.cancelled":
        state, code = "cancelled", 124
    else:
        state, code = "unknown", 125
    return DirectResult(state, events.thread_id, events.turn_id, events.terminal_at or utc_now(), code, retained)


def _close_streams(proc: subprocess.Popen[bytes], selector: selectors.BaseSelector) -> None:
    selector.close()
    for pipe in (proc.stdout, proc.stderr):
        if pipe is not None:
            pipe.close()


def supervise(command: list[str], timeout_ms: int, grace_ms: int = 5000) -> DirectResult:
    _enable_subreaper()
    events, stderr = EventState(), [0]
    try:
        proc = subprocess.Popen(
            command,
            stdin=subprocess.DEVNULL,
            stdout=subprocess.PIPE,
            stderr=subprocess.PIPE,
            start_new_session=True,
            close_fds=True,
        )
    except OSError as error:
        print(f"codex direct launch failed: {error}", file=sys.stderr)
        return DirectResult("unknown", None, None, utc_now(), 125, False)
    tracker, stream = Descendants(proc.pid), JsonStream(events)
    selector = selectors.DefaultSelector()
    for pipe, name in [(proc.stdout, "stdout"), (proc.stderr, "stderr")]:
        if pipe is not None:
            os.set_blocking(pipe.fileno(), False)
            selector.register(pipe, selectors.EVENT_READ, name)

    def tick() -> None:
        tracker.refresh()
        _drain(selector, stream, stderr)

    try:
        return _collect_result(proc, tracker, events, selector, stream, stderr,
                               timeout_ms, grace_ms, tick)
    except BaseException:
        _retire(proc, tracker, time.monotonic() + grace_ms / 1000,
                lambda: time.sleep(0.005))
        raise
    finally:
        _close_streams(proc, selector)


def _collect_result(proc, tracker, events, selector, stream, stderr, timeout_ms, grace_ms, tick):
    joined = _wait_until(proc, time.monotonic() + timeout_ms / 1000, tick)
    timed_out = not joined
    end = time.monotonic() + grace_ms / 1000
    retired = _retire(proc, tracker, end, tick)
    while selector.get_map() and time.monotonic() < end:
        _drain(selector, stream, stderr)
    retired = retired and not selector.get_map()
    tracker.refresh()
    if timed_out:
        result = DirectResult("cancelled" if retired else "unknown", events.thread_id,
                              events.turn_id, utc_now(), 124 if retired else 125, not retired)
    else:
        result = _normal_result(proc, events, tracker)
        if not retired:
            result = DirectResult("unknown", events.thread_id, events.turn_id,
                                  events.terminal_at or utc_now(), 125, True)
    return result


def parse_args() -> argparse.Namespace:
    parser = argparse.ArgumentParser()
    parser.add_argument("--timeout-ms", type=int, required=True)
    parser.add_argument("--grace-ms", type=int, default=5000)
    parser.add_argument("command", nargs=argparse.REMAINDER)
    args = parser.parse_args()
    if args.command[:1] == ["--"]:
        args.command = args.command[1:]
    valid_command = len(args.command) >= 3 and args.command[:2] == ["codex", "exec"]
    if not valid_command or "--json" not in args.command:
        parser.error("command must be codex exec --json ...")
    if not 1 <= args.timeout_ms <= 540_000 or not 1 <= args.grace_ms <= 5000:
        parser.error("timeouts exceed the direct supervisor bounds")
    return args


def main() -> int:
    args = parse_args()
    result = supervise(args.command, args.timeout_ms, args.grace_ms)
    print(result.line(), flush=True)
    return result.exit_code


if __name__ == "__main__":
    raise SystemExit(main())
