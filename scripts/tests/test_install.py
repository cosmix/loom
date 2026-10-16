"""Installer fault injection; run with python3 -m unittest discover -s scripts/tests."""
import os
from pathlib import Path
import subprocess
import tempfile
import unittest

INSTALLER = Path(__file__).resolve().parents[2] / "install.sh"
KEY = "RWTHfjV12CKdjuXF6DPYXsOoneV6zG4nt4Qd1DFe7JzSIXTXKfRJPHjJ"


class InstallerTests(unittest.TestCase):
    def setUp(self):
        self.temp = tempfile.TemporaryDirectory()
        self.addCleanup(self.temp.cleanup)
        self.root = Path(self.temp.name)
        self.destination = self.root / ".local/bin/loom"
        self.destination.parent.mkdir(parents=True)
        self.destination.write_text("#!/bin/sh\necho old-working\n")
        self.destination.chmod(0o755)
        self.candidate = self.root / "candidate"
        self.candidate.write_text(
            '#!/bin/sh\necho candidate-help >> "$TRACE"\n'
            'test "${FAIL_VALIDATION:-0}" != 1\n'
        )
        self.candidate.chmod(0o755)
        local = self.root / "loom/target/release/loom"
        local.parent.mkdir(parents=True)
        local.write_bytes(self.candidate.read_bytes())
        local.chmod(0o755)
        self.trace = self.root / "trace"

    def run_install(self, body, **overrides):
        env = dict(os.environ, HOME=str(self.root), TRACE=str(self.trace),
                   CANDIDATE=str(self.candidate), INSTALLER=str(INSTALLER), **overrides)
        return subprocess.run(
            ["bash", "-c", 'LOOM_INSTALL_LIB_ONLY=1 source "$INSTALLER"\n' + body],
            env=env, capture_output=True, text=True, timeout=10,
        )

    def assert_preserved(self, result):
        self.assertNotEqual(result.returncode, 0, result.stdout)
        self.assertEqual(subprocess.check_output([self.destination], text=True).strip(), "old-working")
        self.assertEqual(list(self.destination.parent.glob(".loom-*")), [])

    def test_local_copy_chmod_validation_and_rename_failures_preserve_old_binary(self):
        for fault in ("cp() { return 1; }", "chmod() { return 1; }",
                      "export FAIL_VALIDATION=1", "mv() { return 1; }"):
            with self.subTest(fault=fault):
                self.assert_preserved(self.run_install(fault + '\nSCRIPT_DIR="$HOME"\ninstall_loom_local'))

    def test_valid_candidate_replaces_destination_only_after_validation(self):
        result = self.run_install('install_candidate "$CANDIDATE"')
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertEqual(self.trace.read_text(), "candidate-help\n")
        self.assertEqual(self.destination.read_bytes(), self.candidate.read_bytes())
        self.assertEqual(self.destination.stat().st_mode & 0o777, 0o755)
        previous = self.destination.with_name("loom.previous")
        self.assertEqual(subprocess.check_output([previous], text=True).strip(), "old-working")
        self.assertEqual(list(self.destination.parent.glob(".loom-*")), [])

    def test_backup_copy_failure_preserves_active_binary(self):
        self.assert_preserved(self.run_install('''
cp() { [[ "$1" != -p ]] || return 1; command cp "$@"; }
install_candidate "$CANDIDATE"
'''))

    def test_final_rename_failure_preserves_active_and_rollback_binary(self):
        self.assert_preserved(self.run_install('''
mv() { [[ "$2" != */loom ]] || return 1; command mv "$@"; }
install_candidate "$CANDIDATE"
'''))
        previous = self.destination.with_name("loom.previous")
        self.assertEqual(subprocess.check_output([previous], text=True).strip(), "old-working")

    def remote_body(self, reject):
        return f'''
uname() {{ if [[ "$1" == -s ]]; then echo Linux; else echo x86_64; fi; }}
minisign() {{
    test "$1" = -Vm && test "$3" = -x && test "$5" = -P || return 9
    test "$6" = "{KEY}" || return 9
    echo verified >> "$TRACE"
    return {1 if reject else 0}
}}
download_release_file() {{
    case "$1" in
        *.minisig) printf 'signature' > "$2" ;;
        *) cp "$CANDIDATE" "$2" ;;
    esac
}}
install_loom_remote
'''

    def test_rejected_signature_never_executes_or_replaces_candidate(self):
        self.assert_preserved(self.run_install(self.remote_body(True)))
        self.assertEqual(self.trace.read_text(), "verified\n")

    def test_rejected_signature_fails_closed_in_conditional_caller(self):
        body = self.remote_body(True).replace(
            "\ninstall_loom_remote\n",
            "\nif install_loom_remote; then exit 0; else exit 1; fi\n",
        )
        self.assert_preserved(self.run_install(body))
        self.assertEqual(self.trace.read_text(), "verified\n")

    def test_signature_verification_precedes_any_candidate_execution(self):
        result = self.run_install(self.remote_body(False))
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertEqual(self.trace.read_text(), "verified\ncandidate-help\n")

    def test_missing_trusted_verifier_fails_before_download_or_execution(self):
        body = '\n'.join([
            'command() { [[ "$*" != "-v minisign" ]] || return 1; builtin command "$@"; }',
            'install_loom_remote',
        ])
        self.assert_preserved(self.run_install(body))
        self.assertFalse(self.trace.exists())

    def test_missing_signature_preserves_old_binary_without_execution(self):
        body = self.remote_body(False).replace("printf 'signature' > \"$2\"", "return 1")
        self.assert_preserved(self.run_install(body))
        self.assertFalse(self.trace.exists())


if __name__ == "__main__":
    unittest.main()
