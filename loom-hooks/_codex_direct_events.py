"""Codex exec JSONL evidence (0.162.1, commit 092d3acd6bec3e3a14bdc7e7a2810ab628ab759d).

Exec turn.started is empty; turn.completed carries usage, turn.failed error.
A provider turn ID is optional evidence and is never synthesized.
"""
from __future__ import annotations
import datetime as dt
import json
import re
import sys
from dataclasses import dataclass
from typing import Callable

RESULT_PREFIX = "LOOM-CODEX-DIRECT-RESULT "
SAFE_ID = re.compile(r"^[A-Za-z0-9][A-Za-z0-9._-]{0,127}$")
MAX_EVENT_BYTES = 65_536
MAX_OUTPUT_BYTES = 65_536
MAX_STDERR_BYTES = 16_384
# Item events carry whole command output and can exceed MAX_EVENT_BYTES (a
# `cat` of three skill files did). Lifecycle events (thread.*, turn.*) stay
# small, so an oversized line is skipped only when its prefix names an item
# event; any other oversized line still invalidates the stream.
OVERSIZED_ITEM = re.compile(rb'\{"type":"(item\.(?:started|updated|completed))","item":\{')


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
    announced_thread_id: str | None = None

    def line(self) -> str:
        value = {
            "v": 1,
            "state": self.state,
            "thread_id": self.thread_id,
            "turn_id": self.turn_id,
            "terminal_at": self.terminal_at,
            "exit_code": self.exit_code,
            "ownership_retained": self.ownership_retained,
            "announced_thread_id": self.announced_thread_id,
        }
        return RESULT_PREFIX + json.dumps(value, separators=(",", ":"))


class EventState:
    def __init__(self) -> None:
        self.thread_id: str | None = None
        self.turn_id: str | None = None
        self.terminal: str | None = None
        self.terminal_at: str | None = None
        self.invalid = False
        self.started = False

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
        if kind == "turn.started":
            if self.started or self.thread_id is None:
                self.invalid = True
            self.started = True
        if kind.startswith("turn.") and event.get("turn_id") is not None:
            self._bind("turn_id", event.get("turn_id"))
        if kind in {"turn.completed", "turn.failed", "turn.cancelled"}:
            if not self.started or self.terminal is not None:
                self.invalid = True
            self.terminal = kind
            self.terminal_at = self.terminal_at or utc_now()


class JsonStream:
    def __init__(self, events: EventState, on_thread: Callable[[str], None] | None = None) -> None:
        self.events = events
        self.on_thread = on_thread
        self.buffer = bytearray()
        self.discarding = False
        self.emitted = 0
        self.skipped_type: str | None = None
        self.skipped_bytes = 0

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
        if self.discarding:
            self.skipped_bytes += len(payload)
        else:
            self.buffer.extend(payload)
            if len(self.buffer) > MAX_EVENT_BYTES:
                self._start_discard()
        if not ended:
            return
        if self.discarding:
            self.discarding = False
            self._note_skipped()
            return
        self._consume_line(bytes(self.buffer))
        self.buffer.clear()

    def _start_discard(self) -> None:
        match = OVERSIZED_ITEM.match(self.buffer)
        if match is None:
            self.events.invalid = True
        self.skipped_type = None if match is None else match.group(1).decode()
        self.skipped_bytes = len(self.buffer)
        self.buffer.clear()
        self.discarding = True

    def _note_skipped(self) -> None:
        # stderr flows into the wrapper's provider output, so the skip is visible.
        if self.skipped_type is not None:
            sys.stderr.write(
                f"note: skipped a {self.skipped_bytes}-byte codex {self.skipped_type} "
                f"event over the {MAX_EVENT_BYTES}-byte line cap\n"
            )
            sys.stderr.flush()

    def _consume_line(self, line: bytes) -> None:
        try:
            event = json.loads(line)
        except (UnicodeDecodeError, json.JSONDecodeError):
            self.events.invalid = True
            return
        self.events.accept(event)
        if self.on_thread is not None and self.events.thread_id is not None:
            self.on_thread(self.events.thread_id)
        encoded = json.dumps(event, separators=(",", ":")).encode() + b"\n"
        if self.emitted + len(encoded) <= MAX_OUTPUT_BYTES:
            sys.stdout.buffer.write(encoded)
            sys.stdout.buffer.flush()
            self.emitted += len(encoded)


