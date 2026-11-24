//! Contracts for the pre-commit hook's ledger step: a ledger the maintainability test lowered is
//! staged with the commit, and a ledger that already held unstaged edits is never swept in.
//! A fake `cargo` stands in for the test run and removes one ledger row in tighten mode.

use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::process::{Command, Output, Stdio};
use std::time::Duration;
use tempfile::TempDir;

const LEDGER: &str = "loom/maintainability-baseline.txt";
const TIGHTENED_ROW: &str = "function src/a.rs f 60";

const FAKE_CARGO: &str = r#"#!/usr/bin/env bash
if [ "${1:-}" = test ] && [ "${LOOM_LEDGER_TIGHTEN:-}" = 1 ]; then
    grep -v -x -F 'function src/a.rs f 60' maintainability-baseline.txt > maintainability-baseline.txt.tmp || true
    mv maintainability-baseline.txt.tmp maintainability-baseline.txt
fi
exit 0
"#;

const FAKE_BUNX: &str = "#!/usr/bin/env bash\necho 'markdownlint-cli2 v0.0.0'\nexit 0\n";

struct Fixture {
    _temp: TempDir,
    repo: PathBuf,
    bin: PathBuf,
    home: PathBuf,
    tmpdir: PathBuf,
}

fn write_exec(path: &Path, body: &str) {
    fs::write(path, body).expect("write fake");
    fs::set_permissions(path, fs::Permissions::from_mode(0o755)).expect("chmod fake");
}

impl Fixture {
    fn new() -> Self {
        let temp = tempfile::tempdir().expect("create temporary root");
        let root = fs::canonicalize(temp.path()).expect("physical root");
        let (repo, bin, home, tmpdir) = (
            root.join("repo"),
            root.join("bin"),
            root.join("home"),
            root.join("tmpdir"),
        );
        for dir in [&repo, &bin, &home, &tmpdir] {
            fs::create_dir_all(dir).expect("create fixture dir");
        }
        write_exec(&bin.join("cargo"), FAKE_CARGO);
        write_exec(&bin.join("bunx"), FAKE_BUNX);
        let fx = Fixture {
            _temp: temp,
            repo,
            bin,
            home,
            tmpdir,
        };
        fs::create_dir_all(fx.repo.join("loom/src")).expect("create loom/src");
        fs::write(fx.repo.join("loom/src/a.rs"), "fn f() {}\n").expect("write a.rs");
        let ledger = format!(
            "# Maintainability ledger\n# Rows only shrink.\nfile src/a.rs 500\n{TIGHTENED_ROW}\n"
        );
        fs::write(fx.repo.join(LEDGER), ledger).expect("write ledger");
        fx.git(&["init", "-q"]);
        fx.git(&["config", "user.name", "Ledger Test"]);
        fx.git(&["config", "user.email", "ledger@example.invalid"]);
        fx.git(&["config", "commit.gpgsign", "false"]);
        fx.git(&["add", "."]);
        fx.git(&["commit", "-q", "-m", "base"]);
        fx
    }

    /// `program` from the repository root with a scrubbed environment: no `LOOM_*` or `GIT_*`
    /// variable, the fakes first on PATH, and HOME and TMPDIR inside the fixture.
    fn command(&self, program: &str) -> Command {
        let mut command = Command::new(program);
        for (key, _) in std::env::vars_os() {
            let name = key.to_string_lossy();
            if name.starts_with("LOOM_") || name.starts_with("GIT_") {
                command.env_remove(&key);
            }
        }
        let path = std::env::var("PATH").unwrap_or_default();
        command
            .current_dir(&self.repo)
            .env("PATH", format!("{}:{path}", self.bin.display()))
            .env("HOME", &self.home)
            .env("TMPDIR", &self.tmpdir)
            .env("GIT_CONFIG_NOSYSTEM", "1")
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());
        command
    }

    fn output(&self, mut command: Command, what: &str) -> Output {
        loom::process::run_bounded_output(&mut command, Duration::from_secs(60), what)
            .unwrap_or_else(|error| panic!("run {what}: {error:#}"))
    }

    fn git(&self, args: &[&str]) -> String {
        let mut command = self.command("git");
        command.args(args);
        let output = self.output(command, "git");
        assert!(
            output.status.success(),
            "git {args:?}: {}",
            String::from_utf8_lossy(&output.stderr)
        );
        String::from_utf8_lossy(&output.stdout).into_owned()
    }

    fn stage_source_edit(&self) {
        fs::write(
            self.repo.join("loom/src/a.rs"),
            "fn f() {\n    let _x = 1;\n}\n",
        )
        .expect("edit a.rs");
        self.git(&["add", "loom/src/a.rs"]);
    }

    /// Run the real pre-commit hook; returns its exit code and both streams.
    fn run_hook(&self) -> (i32, String) {
        let hook = Path::new(env!("CARGO_MANIFEST_DIR")).join(".githooks/pre-commit");
        let mut command = self.command("bash");
        command.arg(&hook);
        let output = self.output(command, "pre-commit hook");
        let streams = format!(
            "stdout: {}\nstderr: {}",
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        );
        (output.status.code().unwrap_or(-1), streams)
    }

    fn staged(&self) -> Vec<String> {
        self.git(&["diff", "--cached", "--name-only"])
            .lines()
            .map(str::to_string)
            .collect()
    }
}

#[test]
fn a_ledger_the_hook_lowered_is_staged() {
    let fx = Fixture::new();
    fx.stage_source_edit();

    let (code, streams) = fx.run_hook();
    assert_eq!(code, 0, "the hook refused the commit\n{streams}");
    let staged = fx.staged();
    assert!(
        staged.iter().any(|path| path == LEDGER),
        "the lowered ledger was not staged; staged: {staged:?}\n{streams}"
    );
}

#[test]
fn a_ledger_with_unstaged_edits_refuses_the_commit() {
    let fx = Fixture::new();
    let ledger = fx.repo.join(LEDGER);
    let mut text = fs::read_to_string(&ledger).expect("read ledger");
    text.push_str("# an unstaged note\n");
    fs::write(&ledger, text).expect("edit ledger");
    fx.stage_source_edit();

    let (code, streams) = fx.run_hook();
    assert_ne!(
        code, 0,
        "the hook passed with unstaged ledger edits\n{streams}"
    );
    let staged = fx.staged();
    assert!(
        !staged.iter().any(|path| path == LEDGER),
        "the ledger with unstaged edits was staged; staged: {staged:?}\n{streams}"
    );
}
