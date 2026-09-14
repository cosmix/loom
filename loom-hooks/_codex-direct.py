#!/usr/bin/env python3
"""Bounded supervisor for one direct ``codex exec --json`` invocation."""

from __future__ import annotations

import argparse
import ctypes
import datetime as dt
import errno
import json
import os
import re
import selectors
import signal
import subprocess
import sys
import time
from dataclasses import dataclass

RESULT_PREFIX = "LOOM-CODEX-DIRECT-RESULT "
SAFE_ID = re.compile(r"^[A-Za-z0-9][A-Za-z0-9._-]{0,127}$")
MAX_EVENT_BYTES = 65_536
MAX_OUTPUT_BYTES = 65_536
MAX_STDERR_BYTES = 16_384
POLL_SECONDS = 0.05


def utc_now() -> str:
    value = dt.datetime.now(dt.timezone.utc).isoformat(timespec="milliseconds")
    return value.replace("+00:00", "Z")


@dataclass(frozen=True)
class DirectResult:
    state: str
    thread_id: str | None
    turn_id: str | None
    terminal_at: str
    exit_code: int
    ownership_retained: bool

    def line(self) -> str:
        value = {
            "v": 1,
            "state": self.state,
            "thread_id": self.thread_id,
            "turn_id": self.turn_id,
            "terminal_at": self.terminal_at,
            "exit_code": self.exit_code,
            "ownership_retained": self.ownership_retained,
        }
        return RESULT_PREFIX + json.dumps(value, separators=(",", ":"))


class EventState:
    def __init__(self) -> None:
        self.thread_id: str | None = None
        self.turn_id: str | None = None
        self.terminal: str | None = None
        self.terminal_at: str | None = None
        self.invalid = False

    def _bind(self, field: str, value: object) -> None:
        if not isinstance(value, str) or SAFE_ID.fullmatch(value) is None:
            self.invalid = True
            return
        known = getattr(self, field)
        if known is not None and known != value:
            self.invalid = True
            return
        setattr(self, field, value)

    def accept(self, event: object) -> None:
        if not isinstance(event, dict) or not isinstance(event.get("type"), str):
            self.invalid = True
            return
        kind = event["type"]
        if kind == "thread.started":
            self._bind("thread_id", event.get("thread_id"))
        if kind.startswith("turn.") and event.get("turn_id") is not None:
            self._bind("turn_id", event.get("turn_id"))
        if kind in {"turn.completed", "turn.failed", "turn.cancelled"}:
            if self.terminal is not None and self.terminal != kind:
                self.invalid = True
            self.terminal = kind
            self.terminal_at = self.terminal_at or utc_now()


class JsonStream:
    def __init__(self, events: EventState) -> None:
        self.events = events
        self.buffer = bytearray()
        self.discarding = False
        self.emitted = 0

    def feed(self, chunk: bytes, eof: bool = False) -> None:
        for part in chunk.splitlines(keepends=True):
            self._feed_part(part)
        if eof and (self.buffer or self.discarding):
            self.events.invalid = True
            self.buffer.clear()
            self.discarding = False

    def _feed_part(self, part: bytes) -> None:
        ended = part.endswith(b"\n")
        payload = part[:-1] if ended else part
        if not self.discarding:
            self.buffer.extend(payload)
            if len(self.buffer) > MAX_EVENT_BYTES:
                self.events.invalid = True
                self.buffer.clear()
                self.discarding = True
        if not ended:
            return
        if self.discarding:
            self.discarding = False
            return
        self._consume_line(bytes(self.buffer))
        self.buffer.clear()

    def _consume_line(self, line: bytes) -> None:
        try:
            event = json.loads(line)
        except (UnicodeDecodeError, json.JSONDecodeError):
            self.events.invalid = True
            return
        self.events.accept(event)
        encoded = json.dumps(event, separators=(",", ":")).encode() + b"\n"
        if self.emitted + len(encoded) <= MAX_OUTPUT_BYTES:
            sys.stdout.buffer.write(encoded)
            sys.stdout.buffer.flush()
            self.emitted += len(encoded)


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


def _retire(proc: subprocess.Popen[bytes], tracker: Descendants, grace_ms: int, tick) -> bool:
    signal_ok = _signal_group(proc.pid, signal.SIGTERM)
    joined = _wait_until(proc, time.monotonic() + grace_ms / 1000, tick)
    group_ok, group_alive = _probe_group(proc.pid)
    if group_alive:
        signal_ok = _signal_group(proc.pid, signal.SIGKILL) and signal_ok
    living = tracker.living()
    if living:
        for pid in living:
            try:
                os.kill(pid, signal.SIGKILL)
            except ProcessLookupError:
                continue
            except OSError:
                signal_ok = False
    if not joined:
        joined = _wait_until(proc, time.monotonic() + 2, tick)
    if joined:
        _reap_adopted()
    group_ok2, group_alive2 = _probe_group(proc.pid)
    living2 = tracker.living()
    probes_ok = group_ok and group_ok2 and living is not None and living2 is not None
    retired = joined and not group_alive2 and living2 == []
    return signal_ok and probes_ok and retired and not tracker.probe_failed


def _normal_result(proc: subprocess.Popen[bytes], events: EventState, tracker: Descendants) -> DirectResult:
    group_ok, group_alive = _probe_group(proc.pid)
    living = tracker.living()
    retained = not group_ok or group_alive or living is None or bool(living) or tracker.probe_failed
    complete = not events.invalid and events.thread_id is not None and events.turn_id is not None
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

    joined = _wait_until(proc, time.monotonic() + timeout_ms / 1000, tick)
    timed_out = not joined
    retired = _retire(proc, tracker, grace_ms, tick) if timed_out else False
    if joined:
        proc.wait()
        _reap_adopted()
    end = time.monotonic() + 0.25
    while selector.get_map() and time.monotonic() < end:
        _drain(selector, stream, stderr)
    tracker.refresh()
    if timed_out:
        result = DirectResult("cancelled" if retired else "unknown", events.thread_id,
                              events.turn_id, utc_now(), 124 if retired else 125, not retired)
    else:
        result = _normal_result(proc, events, tracker)
    selector.close()
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
