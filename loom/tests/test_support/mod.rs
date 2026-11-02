//! Shared subprocess policy for repository fixtures.
#![allow(dead_code)]

use std::path::Path;
use std::process::{Command, Output};

pub fn git_unchecked(dir: &Path, args: &[&str]) -> Output {
    git_with_program(Path::new("git"), dir, args)
}

fn git_with_program(program: &Path, dir: &Path, args: &[&str]) -> Output {
    let mut command = Command::new(program);
    for (key, _) in std::env::vars_os() {
        if key.to_string_lossy().starts_with("GIT_") {
            command.env_remove(key);
        }
    }
    command
        .env("GIT_CONFIG_NOSYSTEM", "1")
        .env("GIT_CONFIG_GLOBAL", "/dev/null")
        .args(args)
        .current_dir(dir);
    command
        .output()
        .unwrap_or_else(|error| panic!("git {args:?} in {}: {error}", dir.display()))
}

pub fn git_checked(dir: &Path, args: &[&str]) -> Output {
    let output = git_unchecked(dir, args);
    check_git(output, dir, args)
}

fn check_git(output: Output, dir: &Path, args: &[&str]) -> Output {
    assert!(
        output.status.success(),
        "git {args:?} in {} exited {}\nstdout: {}\nstderr: {}",
        dir.display(),
        output.status,
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    output
}

#[test]
fn failed_git_mutation_reports_command_and_output() {
    // The stand-in runs from a shell string: a script written here and executed
    // could fail with ETXTBSY when another test thread forks while the write
    // descriptor is open.
    const FAILING_GIT: &str = "echo fixture-error >&2; exit 42";
    let temp = tempfile::tempdir().unwrap();
    for action in ["commit", "merge"] {
        let args = ["-c", FAILING_GIT, action, "fixture"];
        let result = std::panic::catch_unwind(|| {
            check_git(
                git_with_program(Path::new("/bin/sh"), temp.path(), &args),
                temp.path(),
                &args,
            )
        });
        let panic = result.expect_err("failed git must reject fixture setup");
        let message = panic.downcast_ref::<String>().expect("panic message");
        assert!(
            message.contains(action) && message.contains("42") && message.contains("fixture-error"),
            "{message}"
        );
    }
}

pub fn init_git_repo(dir: &Path) {
    git_checked(dir, &["init"]);
    git_checked(dir, &["config", "user.email", "test@test.com"]);
    git_checked(dir, &["config", "user.name", "Test User"]);
    std::fs::write(dir.join("README.md"), "# Test Repository\n").expect("write README");
    git_checked(dir, &["add", "."]);
    git_checked(dir, &["commit", "-m", "Initial commit"]);
}

pub fn write_main_checkout_identity(repo: &Path, work_dir: &Path) {
    use std::os::unix::fs::MetadataExt;

    let checkout = repo.canonicalize().expect("canonical checkout");
    let git_dir = checkout
        .join(".git")
        .canonicalize()
        .expect("canonical git dir");
    let directory = |path: &Path| {
        let meta = std::fs::metadata(path).expect("checkout identity directory");
        serde_json::json!({
            "path": path,
            "device": meta.dev(),
            "inode": meta.ino(),
        })
    };
    let identity = serde_json::json!({
        "checkout": directory(&checkout),
        "git_dir": directory(&git_dir),
        "common_dir": directory(&git_dir),
    });
    std::fs::create_dir_all(work_dir).expect("create Loom work dir");
    std::fs::write(
        work_dir.join("main-checkout-identity.json"),
        serde_json::to_vec(&identity).expect("serialize checkout identity"),
    )
    .expect("write checkout identity");
}

pub fn clear_loom_env(command: &mut Command) {
    for (key, _) in std::env::vars_os() {
        if key.to_string_lossy().starts_with("LOOM_") {
            command.env_remove(key);
        }
    }
}
