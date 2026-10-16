"""Build invocation-local test wrappers without copying caches or executables."""

import os
import shlex
import shutil
import tempfile
from pathlib import Path


class OwnedTestRoot:
    """Remove the aggregate fixture tree only after owned processes are retired."""

    def __init__(self, parent):
        self.path = Path(tempfile.mkdtemp(prefix="r-", dir=parent))
        self.preserve = False

    def __enter__(self):
        return self

    def __exit__(self, _type, _value, _traceback):
        if not self.preserve:
            shutil.rmtree(self.path)


def temp_base(repository):
    # Short enough for Darwin Unix sockets, outside the /tmp sandbox write grant.
    base = Path("/var/tmp").resolve()
    home = Path.home().resolve()
    if base == home or home in base.parents or base == repository or repository in base.parents:
        raise RuntimeError("OS temporary directory must be outside HOME and the checkout")
    base = base / ("loom-tests-" + str(os.getuid()))
    base.mkdir(mode=0o700, exist_ok=True)
    if base.is_symlink() or base.stat().st_uid != os.getuid():
        raise RuntimeError("test temporary parent must be owned by this user, without a symlink")
    if base.stat().st_mode & 0o077:
        raise RuntimeError("test temporary parent must have mode 0700")
    return base


def test_environment(root, toolchain, threads):
    # Trusted confinement fixtures must remain outside the writable TMPDIR tree.
    (root / "trusted").mkdir(mode=0o700)
    locations = {
        "HOME": "home", "XDG_CONFIG_HOME": "home/.config",
        "XDG_CACHE_HOME": "home/.cache", "XDG_DATA_HOME": "home/.local/share",
        "XDG_STATE_HOME": "home/.local/state", "XDG_RUNTIME_DIR": "runtime",
        "CARGO_HOME": "cargo", "RUSTUP_HOME": "rustup",
        "TMPDIR": "t", "TMP": "t", "TEMP": "t", "TMUX_TMPDIR": "t",
    }
    values = {key: str(root / suffix) for key, suffix in locations.items()}
    for directory in values.values():
        Path(directory).mkdir(mode=0o700, parents=True, exist_ok=True)
    values.update({
        "PATH": str(toolchain / "bin") + os.pathsep + os.environ.get("PATH", os.defpath),
        "RUSTC": str(toolchain / "bin/rustc"),
        "RUSTDOC": str(toolchain / "bin/rustdoc"),
        "RUST_TEST_THREADS": str(threads), "LOOM_TEST_ROOT": str(root),
        "LOOM_TEST_TOOLCHAIN": str(toolchain),
        "GIT_CONFIG_NOSYSTEM": "1",
        "PYTHONDONTWRITEBYTECODE": "1",
    })
    for key in ("LOOM_E2E_REQUIRE_TMUX", "LOOM_TEST_REQUIRE_SANDBOX_FREE"):
        if key in os.environ:
            values[key] = os.environ[key]
    return values


def write_wrapper(path, values, executable=None):
    prefixes = ("LOOM_", "CODEX_", "CLAUDE_", "XDG_", "GIT_", "CARGO_TARGET_")
    removed = [key for key in os.environ if key.startswith(prefixes)]
    removed += ["RUSTC_WRAPPER", "RUSTC_WORKSPACE_WRAPPER", "RUSTUP_TOOLCHAIN",
                "CARGO_TARGET_DIR", "CARGO_BUILD_TARGET", "CARGO_BUILD_TARGET_DIR",
                "BASH_ENV", "ENV", "ZDOTDIR", "SSH_AUTH_SOCK", "TMUX"]
    lines = ["#!/bin/sh", "set -eu"]
    lines.extend("unset " + shlex.quote(key) for key in sorted(set(removed)))
    lines.extend("export " + key + "=" + shlex.quote(value)
                 for key, value in values.items())
    command = shlex.quote(str(executable)) + ' "$@"' if executable else '"$@"'
    lines.append("exec " + command)
    path.write_text("\n".join(lines) + "\n")
    path.chmod(0o700)


def prepare_wrappers(root, toolchain, threads):
    values = test_environment(root, toolchain, threads)
    runner, rustdoc = root / "harness", root / "rustdoc"
    write_wrapper(runner, values)
    write_wrapper(rustdoc, values, toolchain / "bin/rustdoc")
    return runner, rustdoc
