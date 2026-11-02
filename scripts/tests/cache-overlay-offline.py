#!/usr/bin/env python3
"""Exercise existing uv and Bun cache entries without network or host-cache writes."""
import concurrent.futures
import hashlib
import os
from pathlib import Path
import shutil
import subprocess
import tempfile

home = Path.home()
uv = shutil.which("uv")
bun = shutil.which("bun")
uv_cache = home / ".cache/uv"
bun_cache = home / ".bun/install/cache"
assert uv and bun and uv_cache.is_dir() and bun_cache.is_dir()
uv_canary = uv_cache / "simple-v18/pypi/requests.rkyv"
bun_canary = bun_cache / "is-number@7.0.0/package.json"
assert uv_canary.is_file() and bun_canary.is_file(), "required warm fixture entries absent"

def digest(path):
    return hashlib.sha256(path.read_bytes()).hexdigest()

before = (digest(uv_canary), digest(bun_canary))
with tempfile.TemporaryDirectory(prefix=".loom-overlay-offline-", dir=os.getcwd()) as tmp:
    root = Path(tmp)
    def run(kind, index, warm):
        runroot = root / f"{kind}-{index}"
        runroot.mkdir()
        for name in ("lower", "upper", "work", "cache", "project"):
            (runroot / name).mkdir()
        project = runroot / "project"
        if kind == "uv":
            (project / "pyproject.toml").write_text('[project]\nname="overlay-fixture"\nversion="0.1.0"\nrequires-python=">=3.10"\ndependencies=["requests==2.32.5"]\n')
            source, key, args = uv_cache, "UV_CACHE_DIR", [uv, "lock", "--offline", "--no-config", "--default-index", "https://pypi.org/simple"]
        else:
            (project / "package.json").write_text('{"name":"overlay-fixture","version":"1.0.0","dependencies":{"is-number":"7.0.0"}}')
            (project / "bun.lock").write_text('''{"lockfileVersion":1,"configVersion":1,"workspaces":{"":{"name":"overlay-fixture","version":"1.0.0","dependencies":{"is-number":"7.0.0"}}},"packages":{"is-number":["is-number@7.0.0","",{},"sha512-41Cifkg6e8TylSpdtTpeLVMqvSBEVzTttHvERD741+pnZ8ANv0004MRL43QKPDlK9cGvNp6NZWZUBlbGXYxxng=="]}}''')
            source, key, args = bun_cache, "BUN_INSTALL_CACHE_DIR", [bun, "install", "--frozen-lockfile", "--ignore-scripts"]
        command = ["bwrap", "--unshare-all", "--ro-bind", "/", "/", "--proc", "/proc", "--dev", "/dev", "--bind", str(runroot), str(runroot), "--chdir", str(project), "--setenv", key, str(runroot / "cache")]
        if warm:
            command += ["--overlay-src", str(source), "--overlay", str(runroot / "upper"), str(runroot / "work"), str(runroot / "cache")]
        if warm:
            canary = "simple-v18/pypi/requests.rkyv" if kind == "uv" else "is-number@7.0.0/package.json"
            check = subprocess.run(command + ["--", "/usr/bin/test", "-f", str(runroot / "cache" / canary)], capture_output=True, text=True)
            assert check.returncode == 0, f"{kind} lower not visible: {check.stderr}"
        result = subprocess.run(command + ["--"] + args, capture_output=True, text=True, timeout=60)
        if (result.returncode == 0) != warm:
            raise AssertionError(f"{kind} warm={warm}: exit {result.returncode}\n{result.stderr}")
        if warm and kind == "bun":
            (project / "node_modules/is-number/package.json").write_text("changed")
        return f"{kind} {'warm' if warm else 'cold'} exit={result.returncode}"
    with concurrent.futures.ThreadPoolExecutor(max_workers=4) as pool:
        for result in pool.map(lambda item: run(*item), [("uv", 1, True), ("uv", 2, True), ("bun", 1, True), ("bun", 2, True)]):
            print(result)
    print(run("uv", 3, False))
    print(run("bun", 3, False))
assert before == (digest(uv_canary), digest(bun_canary)), "host cache canary changed"
print("host cache canaries unchanged")
