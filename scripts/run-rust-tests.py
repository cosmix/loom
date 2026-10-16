#!/usr/bin/python3
"""Run one complete Cargo test invocation with bounded lifetime and isolated homes."""

import argparse
import json
import os
import re
import shutil
import signal
import sys
import tempfile
import time
from pathlib import Path

sys.dont_write_bytecode = True
from rust_test_environment import OwnedTestRoot, prepare_wrappers, temp_base
from rust_test_processes import OwnedProcesses, process_backend

INTERRUPTED = 0


def interrupt(signum, _frame):
    global INTERRUPTED
    INTERRUPTED = signum


def positive(value):
    number = int(value)
    if number <= 0:
        raise argparse.ArgumentTypeError("must be greater than zero")
    return number


def arguments():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--doc", action="store_true", help="run doctests separately")
    parser.add_argument("--wall-seconds", type=positive, default=3600)
    parser.add_argument("--idle-seconds", type=positive, default=300,
                        help="maximum seconds without a change in command output")
    parser.add_argument("--test-threads", type=positive,
                        default=None)
    args = parser.parse_args()
    if args.test_threads is None:
        args.test_threads = positive(os.environ.get("RUST_TEST_THREADS", "1" if args.doc else "4"))
    return args


def spawn(command, environment, cwd, log):
    # Popen can wait for exec in its constructor. Fork exposes ownership first.
    descriptor = os.open(log, os.O_WRONLY | os.O_CREAT | os.O_TRUNC, 0o600)
    read_error, write_error = os.pipe()
    os.set_blocking(read_error, False)
    pid = os.fork()
    if pid != 0:
        os.close(descriptor)
        os.close(write_error)
        return pid, read_error
    try:
        os.close(read_error)
        os.setsid()
        os.dup2(descriptor, 1)
        os.dup2(descriptor, 2)
        os.close(descriptor)
        os.chdir(cwd)
        with open(os.devnull, "rb") as null:
            os.dup2(null.fileno(), 0)
        os.execvpe(command[0], command, environment)
    except BaseException as error:
        message = ("launcher exec failed: " + str(error) + "\n").encode()
        os.write(write_error, message[:4096])
        os.write(2, message)
        os._exit(127)


def watch(pid, owned, log, wall, idle):
    started = last_output = time.monotonic()
    size = 0
    while True:
        owned.sample()
        result = owned.poll()
        if result is not None:
            return result, None
        now = time.monotonic()
        current_size = log.stat().st_size if log.exists() else 0
        if current_size != size:
            size, last_output = current_size, now
        if INTERRUPTED:
            return None, "signal:" + str(INTERRUPTED)
        if now - started >= wall:
            return None, "wall_timeout"
        if now - last_output >= idle:
            return None, "idle_timeout"
        time.sleep(0.25)


def execute(command, environment, cwd, log, wall, idle, backend, root=None):
    started = time.monotonic()
    pid, error_pipe = spawn(command, environment, cwd, log)
    owned = OwnedProcesses(pid, backend)
    result, reason = None, None
    try:
        result, reason = watch(pid, owned, log, wall, idle)
    except Exception as error:
        reason = "monitor_error: " + str(error)
    finally:
        journal = {"cleanup_pending": True, "command_pid": pid,
                   "test_root": str(root) if root else None,
                   "identities": list(owned.known), "command_returncode": result}
        try:
            log.with_suffix(".cleanup.json").write_text(json.dumps(journal, indent=2) + "\n")
        except OSError as error:
            reason = "cleanup_journal_error: " + str(error)
        leaked, survivors = owned.retire()
        if result is None:
            result = owned.poll()
        try:
            launch_error = os.read(error_pipe, 4096).decode(errors="replace") or None
        except BlockingIOError:
            launch_error = None
        finally:
            os.close(error_pipe)
    report = {"command": command, "command_returncode": result,
              "launch_error": launch_error,
              "stop_reason": reason, "cleanup_signalled_pids": leaked,
              "surviving_pids": survivors, "elapsed_seconds": time.monotonic() - started}
    report.update(owned.metrics())
    journal.update(cleanup_pending=bool(survivors or owned.cleanup_sampling_errors),
                   surviving_pids=survivors, command_returncode=result)
    try:
        log.with_suffix(".cleanup.json").write_text(json.dumps(journal, indent=2) + "\n")
    except OSError as error:
        report["stop_reason"] = "cleanup_journal_error: " + str(error)
    return report


def result_code(report):
    reason = report["stop_reason"]
    if reason and reason.startswith("signal:"):
        return 128 + int(reason.split(":")[1])
    if reason in ("wall_timeout", "idle_timeout"):
        return 124
    if reason or report["surviving_pids"] or report["launch_error"]:
        return 125
    if report["cleanup_sampling_errors"]:
        return 125
    status = report["command_returncode"]
    if status:
        return status if status > 0 else 128 - status
    if report["cleanup_signalled_pids"]:
        return 125
    return 0


def toolchain_info(environment, repository, output, backend):
    rustc = environment.get("RUSTC") or shutil.which("rustc")
    if not rustc:
        raise RuntimeError("rustc is not on PATH")
    answers = []
    for name, flags in (("sysroot", ["--print", "sysroot"]), ("host", ["-vV"])):
        log = output / (name + ".log")
        report = execute([rustc] + flags, environment, repository, log, 30, 30, backend)
        (output / (name + ".json")).write_text(json.dumps(report, indent=2) + "\n")
        if result_code(report):
            raise RuntimeError("toolchain discovery failed; see " + str(log))
        answers.append(log.read_text().strip())
    toolchain = Path(answers[0])
    host = re.search(r"^host: ([a-zA-Z0-9_-]+)$", answers[1], re.MULTILINE)
    if not host or not all((toolchain / "bin" / name).is_file()
                           for name in ("cargo", "rustc", "rustdoc")):
        raise RuntimeError("could not identify concrete cargo/rustc/rustdoc binaries")
    return toolchain, host.group(1)


def cargo_command(args, root, toolchain, host, environment):
    runner, rustdoc = prepare_wrappers(root, toolchain, args.test_threads)
    if environment.get("CARGO_BUILD_TARGET", host) != host:
        raise RuntimeError("cross-target CARGO_BUILD_TARGET is unsupported by this native runner")
    command = [str(toolchain / "bin/cargo"), "test", "--no-fail-fast", "--color", "never"]
    if args.doc:
        command.extend(["--doc", "--workspace", "--all-features"])
        environment["RUSTDOC"] = str(rustdoc)
    else:
        command.extend(["--all-targets", "--config",
                        "target." + host + ".runner=" + json.dumps(str(runner))])
    environment["RUST_TEST_THREADS"] = str(args.test_threads)
    command.extend(["--", "--test-threads=" + str(args.test_threads)])
    if sys.platform == "darwin":
        return ["/usr/bin/time", "-l"] + command
    return ["/usr/bin/time", "-v"] + command


def test_counts(log):
    totals = {"passed": 0, "failed": 0, "ignored": 0, "summaries": 0}
    pattern = re.compile(r"^test result: .*? (\d+) passed; (\d+) failed; (\d+) ignored;")
    with log.open(errors="replace") as stream:
        for line in stream:
            match = pattern.match(line)
            if match:
                for key, value in zip(("passed", "failed", "ignored"), match.groups()):
                    totals[key] += int(value)
                totals["summaries"] += 1
    return totals


def run(args, repository, output, backend):
    environment = os.environ.copy()
    toolchain, host = toolchain_info(environment, repository, output, backend)
    with OwnedTestRoot(temp_base(repository)) as owner:
        root = owner.path
        if any(character.isspace() for character in str(root)):
            raise RuntimeError("Cargo's string runner requires a temporary path without spaces")
        command = cargo_command(args, root, toolchain, host, environment)
        log = output / "cargo.log"
        print("Test temporary root: " + str(root), flush=True)
        owner.preserve = True
        report = execute(command, environment, repository / "loom", log,
                         args.wall_seconds, args.idle_seconds, backend, root)
        owner.preserve = bool(report["surviving_pids"] or report["cleanup_sampling_errors"])
        report.update({"test_root": str(root), "test_threads": args.test_threads,
                       "scope": "doctests" if args.doc else "all-targets",
                       "wall_limit_seconds": args.wall_seconds,
                       "idle_limit_seconds": args.idle_seconds, "tests": test_counts(log)})
        report["exit_code"] = result_code(report)
        if report["exit_code"] == 0 and report["tests"]["passed"] + report["tests"]["failed"] == 0:
            report.update(exit_code=98, stop_reason="zero_executed_tests")
        (output / "summary.json").write_text(json.dumps(report, indent=2) + "\n")
    report["test_root_removed"] = not root.exists()
    (output / "summary.json").write_text(json.dumps(report, indent=2) + "\n")
    print(json.dumps(report, indent=2))
    return report["exit_code"]


def main():
    args = arguments()
    repository = Path(__file__).resolve().parent.parent
    output_parent = repository / "loom/target/test-runs"
    output_parent.mkdir(parents=True, exist_ok=True)
    output = Path(tempfile.mkdtemp(prefix=time.strftime("%Y%m%d-%H%M%S-"), dir=output_parent))
    print("Retained logs and metrics: " + str(output), flush=True)
    for signum in (signal.SIGINT, signal.SIGTERM):
        signal.signal(signum, interrupt)
    try:
        return run(args, repository, output, process_backend())
    except Exception as error:
        (output / "launcher-error.txt").write_text(str(error) + "\n")
        print("launcher error: " + str(error), file=sys.stderr)
        return 125


if __name__ == "__main__":
    sys.exit(main())
