"""Native process identities and sampled resource use for run-rust-tests.py."""

import ctypes
import os
import signal
import sys
import time
from dataclasses import dataclass
from pathlib import Path


@dataclass
class Process:
    pid: int
    parent: int
    group: int
    start: object
    threads: int = 0
    written: object = None
    zombie: bool = False

    @property
    def identity(self):
        return self.pid, self.start


class BsdInfo(ctypes.Structure):
    # sys/proc_info.h: PROC_PIDTBSDINFO (3), including PID birth time.
    _fields_ = [(name, ctypes.c_uint32) for name in (
        "flags", "status", "xstatus", "pid", "ppid", "uid", "gid", "ruid",
        "rgid", "svuid", "svgid", "reserved",
    )] + [("comm", ctypes.c_char * 16), ("name", ctypes.c_char * 32)] + [
        (name, ctypes.c_uint32) for name in (
            "nfiles", "pgid", "jobc", "tdev", "tpgid", "nice",
        )
    ] + [("start_sec", ctypes.c_uint64), ("start_usec", ctypes.c_uint64)]


class TaskInfo(ctypes.Structure):
    _fields_ = [(name, ctypes.c_uint64) for name in (
        "virtual", "resident", "user", "system", "threads_user", "threads_system",
    )] + [(name, ctypes.c_int32) for name in (
        "policy", "faults", "pageins", "cow", "sent", "received", "mach",
        "unix", "switches", "threads", "running", "priority",
    )]


class UsageInfo(ctypes.Structure):
    # sys/resource.h: rusage_info_v2, flavor 2. Disk fields exclude child usage.
    _fields_ = [("uuid", ctypes.c_uint8 * 16)] + [
        (name, ctypes.c_uint64) for name in (
            "user", "system", "idle", "interrupts", "pageins", "wired",
            "resident", "footprint", "start", "exit", "child_user",
            "child_system", "child_idle", "child_interrupts", "child_pageins",
            "child_elapsed", "read", "written",
        )
    ]


class MacProcesses:
    def __init__(self):
        self.lib = ctypes.CDLL("/usr/lib/libproc.dylib", use_errno=True)
        self.lib.proc_listpids.argtypes = [
            ctypes.c_uint32, ctypes.c_uint32, ctypes.c_void_p, ctypes.c_int,
        ]
        self.lib.proc_pidinfo.argtypes = [
            ctypes.c_int, ctypes.c_int, ctypes.c_uint64, ctypes.c_void_p,
            ctypes.c_int,
        ]
        self.lib.proc_pid_rusage.argtypes = [
            ctypes.c_int, ctypes.c_int, ctypes.c_void_p,
        ]

    def read(self, pid, resources=False):
        info = BsdInfo()
        size = ctypes.sizeof(info)
        if self.lib.proc_pidinfo(pid, 3, 0, ctypes.byref(info), size) != size:
            return None
        process = Process(pid, info.ppid, info.pgid,
                          (info.start_sec, info.start_usec), zombie=info.status == 5)
        if resources:
            task, usage = TaskInfo(), UsageInfo()
            size = ctypes.sizeof(task)
            if self.lib.proc_pidinfo(pid, 4, 0, ctypes.byref(task), size) == size:
                process.threads = task.threads
            if self.lib.proc_pid_rusage(pid, 2, ctypes.byref(usage)) == 0:
                process.written = usage.written
        return process

    def snapshot(self):
        # PROC_UID_ONLY (4) avoids querying unrelated users and launches no ps.
        size = self.lib.proc_listpids(4, os.getuid(), None, 0)
        if size <= 0:
            raise RuntimeError("libproc could not enumerate this user's processes")
        pids = (ctypes.c_int * (size // 4 + 256))()
        count = self.lib.proc_listpids(4, os.getuid(), pids, ctypes.sizeof(pids))
        if count <= 0:
            raise RuntimeError("libproc process enumeration failed")
        return {pid: p for pid in pids[:count // 4] if pid > 0
                for p in [self.read(pid)] if p is not None}


class LinuxProcesses:
    def read(self, pid, resources=False):
        try:
            path = Path("/proc") / str(pid)
            fields = (path / "stat").read_text().rsplit(") ", 1)[1].split()
            process = Process(pid, int(fields[1]), int(fields[2]), fields[19],
                              int(fields[17]), zombie=fields[0] == "Z")
            if resources:
                values = dict(line.split(": ", 1)
                              for line in (path / "io").read_text().splitlines())
                process.written = int(values["write_bytes"])
            return process
        except (OSError, ValueError, IndexError, KeyError):
            return None

    def snapshot(self):
        return {int(name): p for name in os.listdir("/proc") if name.isdigit()
                for p in [self.read(int(name))] if p is not None}


class OwnedProcesses:
    """Own observed descendants by PID and birth time; sampling is not containment."""

    def __init__(self, pid, backend):
        self.pid, self.backend = pid, backend
        self.root_reaped, self.returncode = False, None
        first = backend.read(pid)
        self.known = {first.identity} if first else set()
        self.written = {}
        self.peak_threads = 0
        self.samples = 0
        self.missing_usage = 0
        self.cleanup_sampling_errors = []

    def sample(self):
        snapshot = self.backend.snapshot()
        if not self.root_reaped and self.pid in snapshot:
            self.known.add(snapshot[self.pid].identity)
        owned = {pid: p for pid, p in snapshot.items() if p.identity in self.known}
        # The first child may not have called setsid yet when it is registered.
        private_group = any(p.group == self.pid for p in owned.values())
        for _ in range(len(snapshot) + 1):
            added = {pid: p for pid, p in snapshot.items() if pid not in owned
                     and (p.parent in owned or (private_group and p.group == self.pid))}
            if not added:
                break
            owned.update(added)
        self.known.update(p.identity for p in owned.values())
        self._resources(owned)
        return [p for p in owned.values() if not p.zombie]

    def _resources(self, owned):
        threads = 0
        for process in owned.values():
            current = self.backend.read(process.pid, resources=True)
            if current is None or current.identity != process.identity:
                continue
            threads += current.threads
            if current.written is None:
                self.missing_usage += 1
            else:
                self.written[current.identity] = max(
                    current.written, self.written.get(current.identity, 0))
        self.peak_threads = max(self.peak_threads, threads)
        self.samples += 1

    def signal(self, processes, signum):
        for process in processes:
            current = self.backend.read(process.pid)
            if current is None or current.identity != process.identity:
                continue
            try:
                # This private group also covers children born between samples.
                if current.group == self.pid:
                    os.killpg(self.pid, signum)
                else:
                    os.kill(current.pid, signum)
            except ProcessLookupError:
                continue
        # An unreaped direct child's PID cannot be reused, even if libproc cannot
        # read its initial identity during exec. Keep this ownership independent.
        if not self.root_reaped:
            try:
                if os.getpgid(self.pid) == self.pid:
                    os.killpg(self.pid, signum)
                else:
                    os.kill(self.pid, signum)
            except ProcessLookupError:
                return

    def poll(self):
        if not self.root_reaped:
            waited, status = os.waitpid(self.pid, os.WNOHANG)
            if waited:
                self.root_reaped = True
                self.returncode = os.waitstatus_to_exitcode(status)
        return self.returncode

    def retire(self):
        remaining = self.cleanup_sample()
        leaked = [p.pid for p in remaining]
        for signum, grace in ((signal.SIGTERM, 2), (signal.SIGKILL, 3)):
            deadline = time.monotonic() + grace
            while remaining and time.monotonic() < deadline:
                self.signal(remaining, signum)
                time.sleep(0.1)
                remaining = self.cleanup_sample()
        return leaked, [p.pid for p in remaining]

    def cleanup_sample(self):
        try:
            remaining = self.sample()
        except Exception as error:
            self.cleanup_sampling_errors.append(str(error))
            remaining = [p for pid, birth in self.known
                         for p in [self.backend.read(pid)]
                         if p is not None and p.start == birth and not p.zombie]
        if self.poll() is None and not any(p.pid == self.pid for p in remaining):
            remaining.append(Process(self.pid, os.getpid(), self.pid, None))
        return remaining

    def metrics(self):
        return {
            "peak_threads_sampled": self.peak_threads,
            "disk_bytes_written_sampled": sum(self.written.values()),
            "process_identities_observed": len(self.known),
            "resource_samples": self.samples,
            "missing_usage_samples": self.missing_usage,
            "cleanup_sampling_errors": self.cleanup_sampling_errors,
            "metric_scope": "observed command descendants; sampled lower bounds",
        }


def process_backend():
    if sys.platform == "darwin":
        return MacProcesses()
    if sys.platform.startswith("linux"):
        return LinuxProcesses()
    raise RuntimeError("the bounded test launcher supports macOS and Linux")
