//! Cwd-dependent command tests run in their own process. Rust's serial test
//! attribute does not protect other, unannotated tests from a cwd change.

use std::env;
use std::io::{Read, Write as _};
use std::ops::Deref;
use std::process::{Command, Stdio};
use tempfile::TempDir;

pub(super) fn run_isolated_test() -> bool {
    let test_name = std::thread::current().name().unwrap().to_string();
    if env::var("LOOM_MEMORY_TEST_CHILD").as_deref() == Ok(test_name.as_str()) {
        return false;
    }

    // Give this child a distinct temp root so a concurrent reader can detect
    // its fixture cwd without depending on other tests' cwd behavior.
    let fixture_root = TempDir::new().unwrap();
    let fixture_path = fixture_root.path().canonicalize().unwrap();
    let mut child = Command::new(env::current_exe().unwrap())
        .args(["--exact", &test_name])
        .env("LOOM_MEMORY_TEST_CHILD", &test_name)
        .env("LOOM_MEMORY_TEST_TMPDIR", &fixture_path)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    // A child that fills an unread pipe blocks forever, so the pipes drain
    // while this thread polls for the child's exit.
    let stdout = drain(child.stdout.take().unwrap());
    let stderr = drain(child.stderr.take().unwrap());
    let mut observed_child_cwd = false;
    let status = loop {
        observed_child_cwd |=
            std::fs::canonicalize(".").is_ok_and(|cwd| cwd.starts_with(&fixture_path));
        if let Some(status) = child.try_wait().unwrap() {
            break status;
        }
        std::thread::sleep(std::time::Duration::from_millis(1));
    };
    let stdout = stdout.join().unwrap();
    let stderr = stderr.join().unwrap();
    assert!(
        status.success(),
        "isolated test {test_name} failed:\n{stdout}\n{stderr}"
    );
    assert!(
        stdout.contains("1 passed"),
        "isolated test {test_name} did not run exactly once"
    );
    observed_child_cwd |=
        std::fs::canonicalize(".").is_ok_and(|cwd| cwd.starts_with(&fixture_path));
    assert!(!observed_child_cwd, "parent observed the child fixture cwd");
    true
}

fn drain(mut pipe: impl Read + Send + 'static) -> std::thread::JoinHandle<String> {
    std::thread::spawn(move || {
        let mut bytes = Vec::new();
        pipe.read_to_end(&mut bytes).unwrap();
        String::from_utf8_lossy(&bytes).into_owned()
    })
}

pub(super) struct CwdRestorer(std::path::PathBuf);

impl CwdRestorer {
    pub(super) fn new() -> Self {
        Self(env::current_dir().unwrap())
    }
}

impl Drop for CwdRestorer {
    fn drop(&mut self) {
        env::set_current_dir(&self.0).unwrap();
    }
}

pub(super) struct RepoFixture {
    // Fields drop in declaration order: restore cwd before TempDir removal.
    _cwd: CwdRestorer,
    dir: TempDir,
}

impl Deref for RepoFixture {
    type Target = TempDir;

    fn deref(&self) -> &Self::Target {
        &self.dir
    }
}

pub(super) fn init_git_repo() -> RepoFixture {
    let cwd = CwdRestorer::new();
    let temp_dir = isolated_temp_dir();
    let run_git = |args: &[&str]| {
        Command::new("git")
            .args(args)
            .current_dir(temp_dir.path())
            .output()
            .unwrap()
    };
    run_git(&["init", "--initial-branch=main"]);
    run_git(&["config", "user.email", "test@test.com"]);
    run_git(&["config", "user.name", "Test"]);
    RepoFixture {
        _cwd: cwd,
        dir: temp_dir,
    }
}

pub(super) fn isolated_temp_dir() -> TempDir {
    let root = env::var_os("LOOM_MEMORY_TEST_TMPDIR").expect("memory test must run in child");
    TempDir::new_in(std::path::PathBuf::from(root)).unwrap()
}

#[test]
fn child_output_beyond_a_pipe_buffer_does_not_stall_the_parent() {
    if run_isolated_test() {
        return;
    }
    let bulk = vec![b'x'; 256 * 1024];
    std::io::stdout().write_all(&bulk).unwrap();
    std::io::stderr().write_all(&bulk).unwrap();
}

#[test]
fn repo_fixture_restores_cwd_and_removes_tree_on_unwind() {
    if run_isolated_test() {
        return;
    }
    let original_dir = env::current_dir().unwrap();
    let mut repo_path = None;
    let failure = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        let repo = init_git_repo();
        repo_path = Some(repo.path().to_path_buf());
        env::set_current_dir(repo.path()).unwrap();
        panic!("exercise fixture cleanup");
    }));
    assert!(failure.is_err());
    assert_eq!(env::current_dir().unwrap(), original_dir);
    assert!(!repo_path.unwrap().exists());
}
