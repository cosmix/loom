"""Deadline-limited stdin collection for Python hooks, including stock macOS."""
import os
import select
import sys
import time


def read_input(seconds=1, limit=1024 * 1024):
    deadline = time.monotonic() + seconds
    chunks = bytearray()
    fd = sys.stdin.fileno()
    while len(chunks) <= limit:
        remaining = deadline - time.monotonic()
        if remaining <= 0 or not select.select([fd], [], [], remaining)[0]:
            break
        chunk = os.read(fd, min(65536, limit + 1 - len(chunks)))
        if not chunk:
            break
        chunks.extend(chunk)
    if len(chunks) > limit:
        raise ValueError("hook input exceeds limit")
    return bytes(chunks)
