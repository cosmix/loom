from fnmatch import fnmatchcase
from pathlib import Path
import re
import shutil
import subprocess
import tempfile
import unittest


ROOT = Path(__file__).resolve().parents[2]
WORKFLOWS = ROOT / ".github/workflows"


def block_end(lines, start, column):
    for index in range(start, len(lines)):
        line = lines[index]
        if line.strip() and len(line) - len(line.lstrip()) <= column:
            return index
    return len(lines)


def key_block(text, key):
    lines = text.splitlines()
    for index, line in enumerate(lines):
        if line.strip() == f"{key}:":
            end = block_end(lines, index + 1, line.index(key))
            return "\n".join(lines[index + 1:end])
    return ""


def run_bodies(workflow):
    lines = workflow.splitlines()
    index = 0
    while index < len(lines):
        match = re.fullmatch(r" *(?:- +)?(run): *(.*)", lines[index])
        index += 1
        if match is None:
            continue
        value = match[2].strip()
        if not value or re.fullmatch(r"[|>][1-9+-]{0,2}(?: +#.*)?", value):
            end = block_end(lines, index, match.start(1))
            yield "\n".join(lines[index:end])
            index = end
        else:
            yield value


def filter_patterns(ci, name):
    filters = key_block(key_block(ci, "changes"), name)
    return [
        line.strip()[2:].strip("\"'")
        for line in filters.splitlines()
        if line.strip().startswith("- ")
    ]


class WorkflowTests(unittest.TestCase):
    @classmethod
    def setUpClass(cls):
        cls.ci = (WORKFLOWS / "ci.yml").read_text(encoding="utf-8")
        cls.release = (WORKFLOWS / "release.yml").read_text(encoding="utf-8")

    def test_actions_are_pinned_with_tag_comments(self):
        for workflow in sorted(WORKFLOWS.glob("*.yml")):
            for number, line in enumerate(workflow.read_text(encoding="utf-8").splitlines(), 1):
                if re.match(r"^ *(?:- +)?uses:", line):
                    with self.subTest(workflow=workflow.name, line=number):
                        self.assertRegex(
                            line,
                            r"^ *(?:- +)?uses: +\S+@[0-9a-f]{40} # \S+\s*$",
                        )

    def test_ci_and_release_default_to_read_only_contents(self):
        for name, workflow in (("ci.yml", self.ci), ("release.yml", self.release)):
            with self.subTest(workflow=name):
                self.assertRegex(workflow, r"(?m)^permissions: *\n +contents: read *$")

    def test_release_run_bodies_have_no_workflow_expressions(self):
        bodies = list(run_bodies(self.release))
        self.assertTrue(bodies)
        for body in bodies:
            self.assertNotIn("${{", body)

    def test_minisign_is_checked_before_extraction(self):
        for body in run_bodies(self.release):
            download = re.search(r"(?m)^ *curl (.* -o minisign\.tar\.gz) *$", body)
            if download:
                break
        else:
            self.fail("Missing minisign download")
        self.assertIn("-fsSL", download[1].split())
        extract = re.search(r"(?m)^ *tar\b.*\bminisign\.tar\.gz\b", body)
        self.assertIsNotNone(extract, "Missing minisign extraction")
        self.assertGreater(extract.start(), download.end())
        self.assertRegex(
            body[download.end():extract.start()],
            r"(?m)^ *(?:echo .* \| +)?sha256sum +-c\b",
        )

    def test_ci_runs_every_python_regression(self):
        commands = "\n".join(run_bodies(self.ci))
        for test_file in sorted((ROOT / "scripts/tests").glob("test_*.py")):
            with self.subTest(test=test_file.name):
                self.assertIn(test_file.name, commands)

    def test_rust_filter_covers_embedded_assets(self):
        assets = (ROOT / "loom/build/assets.rs").read_text(encoding="utf-8")
        constants = re.findall(
            r'^ *const (\w+_(ROOT|TEMPLATE)): &str = "([^"]+)";',
            assets,
            re.MULTILINE,
        )
        self.assertIn("WEB_DIST_ROOT", {name for name, _, _ in constants})
        patterns = filter_patterns(self.ci, "rust")
        for name, kind, path in constants:
            with self.subTest(asset=name, path=path):
                self.assertTrue(
                    any(
                        pattern in (path, f"{path}/**")
                        or (kind == "TEMPLATE" and fnmatchcase(path, pattern))
                        for pattern in patterns
                    ),
                    f"{path} is missing from the rust filter",
                )

    def test_macos_requires_the_real_hook_binary(self):
        job = key_block(self.ci, "hooks-macos")
        self.assertIn("cargo build", job)
        self.assertIn("LOOM_HOOK_TEST_REQUIRE_BINARY=1", job)

    def test_ci_checks_knowledge_strictly(self):
        self.assertIn(
            "knowledge check --strict --baseline doc/loom/knowledge/check-baseline.txt",
            key_block(self.ci, "hook-syntax"),
        )
        self.assertIn("doc/loom/knowledge/**", filter_patterns(self.ci, "shell"))

    def test_dependabot_updates_actions(self):
        config = (ROOT / ".github/dependabot.yml").read_text(encoding="utf-8")
        self.assertRegex(config, r"""(?m)^ *- package-ecosystem: ["']?github-actions["']? *$""")

    def test_hook_syntax_checker_reports_invalid_scripts(self):
        cases = (
            (None, ""),
            ("install.sh", "if then\n"),
            ("dev-install.sh", "if then\n"),
            ("loom-hooks/bad.py", "def (\n"),
            ("loom-hooks/python-hook.sh", "#!/usr/bin/env python3\ndef (\n"),
        )
        for invalid_path, source in cases:
            with self.subTest(invalid_path=invalid_path), tempfile.TemporaryDirectory() as temp:
                repo = Path(temp)
                (repo / "scripts").mkdir()
                (repo / "loom-hooks").mkdir()
                checker = repo / "scripts/check-hook-syntax.sh"
                shutil.copyfile(ROOT / "scripts/check-hook-syntax.sh", checker)
                for path in ("loom-hooks/ok.sh", "install.sh", "dev-install.sh"):
                    (repo / path).write_text("true\n", encoding="utf-8")
                if invalid_path:
                    (repo / invalid_path).write_text(source, encoding="utf-8")

                result = subprocess.run(["bash", str(checker)], capture_output=True, text=True)
                self.assertEqual(result.returncode, 1 if invalid_path else 0, result.stderr)
                if invalid_path:
                    self.assertIn(f"SYNTAX ERROR: {invalid_path}\n", result.stderr)


if __name__ == "__main__":
    unittest.main()
