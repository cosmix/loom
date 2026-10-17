"""Exercise dev-install build settings without compiling or installing anything."""
import json
import os
from pathlib import Path
import shutil
import subprocess
import sys
import tempfile
import unittest


DEV_INSTALL = Path(__file__).resolve().parents[2] / "dev-install.sh"
BUILD_ENV = (
    "CARGO_BUILD_JOBS",
    "CARGO_PROFILE_RELEASE_LTO",
    "CARGO_PROFILE_RELEASE_CODEGEN_UNITS",
    "CARGO_PROFILE_RELEASE_INCREMENTAL",
)


class DevInstallTests(unittest.TestCase):
    def setUp(self):
        self.temp = tempfile.TemporaryDirectory()
        self.addCleanup(self.temp.cleanup)
        self.root = Path(self.temp.name)
        (self.root / "loom").mkdir()
        self.bin = self.root / "bin"
        self.bin.mkdir()
        shutil.copyfile(DEV_INSTALL, self.root / "dev-install.sh")
        self.stub("pgrep", "#!/bin/sh\nexit 1\n")
        self.stub("nproc", "#!/bin/sh\necho 12\n")
        self.stub("getconf", "#!/bin/sh\necho 8\n")
        self.stub("cargo", f"#!{sys.executable}\n" + """
import json, os, sys
from pathlib import Path
Path(os.environ['BUILD_TRACE']).write_text(json.dumps({
    'args': sys.argv[1:], 'cwd': os.getcwd(),
    'env': {key: value for key, value in os.environ.items()
            if key.startswith('CARGO_')},
}))
sys.exit(int(os.environ.get('BUILD_EXIT', '0')))
""")
        (self.root / "install.sh").write_text(
            'printf "%s\\n" "$@" > "$INSTALL_TRACE"\n'
            'printf "%s\\n" "$CARGO_PROFILE_RELEASE_LTO" >> "$INSTALL_TRACE"\n'
        )

    def stub(self, name, content):
        path = self.bin / name
        path.write_text(content)
        path.chmod(0o755)

    def run_install(self, **overrides):
        env = dict(os.environ)
        for key in BUILD_ENV:
            env.pop(key, None)
        env.update(
            PATH=f"{self.bin}:{env['PATH']}", HOME=str(self.root),
            BUILD_TRACE=str(self.root / "build.json"),
            INSTALL_TRACE=str(self.root / "install.trace"),
            CARGO_PROFILE_RELEASE_LTO="fat",
            **overrides,
        )
        return subprocess.run(
            ["bash", str(self.root / "dev-install.sh"), "--skills", "all"],
            env=env, capture_output=True, text=True, timeout=10,
        )

    def test_build_overrides_are_scoped_and_installer_receives_arguments(self):
        result = self.run_install()
        self.assertEqual(result.returncode, 0, result.stderr)
        build = json.loads((self.root / "build.json").read_text())
        self.assertEqual(build["args"], ["build", "--release"])
        self.assertEqual(build["cwd"], str(self.root / "loom"))
        self.assertEqual(
            [build["env"][key] for key in BUILD_ENV],
            ["12", "off", "16", "true"],
        )
        self.assertEqual((self.root / "install.trace").read_text(), "--skills\nall\nfat\n")

    def test_explicit_job_limit_wins(self):
        result = self.run_install(CARGO_BUILD_JOBS="2")
        self.assertEqual(result.returncode, 0, result.stderr)
        build = json.loads((self.root / "build.json").read_text())
        self.assertEqual(build["env"]["CARGO_BUILD_JOBS"], "2")

    def test_cpu_detection_fallbacks(self):
        self.stub("nproc", "#!/bin/sh\nexit 1\n")
        for getconf, expected in (("echo 8", "8"), ("exit 1", "4")):
            with self.subTest(expected=expected):
                self.stub("getconf", f"#!/bin/sh\n{getconf}\n")
                result = self.run_install()
                self.assertEqual(result.returncode, 0, result.stderr)
                build = json.loads((self.root / "build.json").read_text())
                self.assertEqual(build["env"]["CARGO_BUILD_JOBS"], expected)

    def test_failed_build_does_not_install(self):
        result = self.run_install(BUILD_EXIT="7")
        self.assertEqual(result.returncode, 7, result.stderr)
        self.assertFalse((self.root / "install.trace").exists())


if __name__ == "__main__":
    unittest.main()
