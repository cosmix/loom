//! Contract for stage hooks-and-delivery: declining the overwrite prompt in
//! `install.sh` (`confirm_overwrites`) stops the install with a non-zero exit,
//! so a wrapper that runs the installer and then restarts loom does not go on
//! as if the install had succeeded. Accepting the prompt still returns 0.
//!
//! The installer is sourced with `LOOM_INSTALL_LIB_ONLY=1`, so only the prompt
//! runs. Requires `bash` on PATH.

use std::io::Write;
use std::path::Path;
use std::process::{Command, Stdio};

use tempfile::TempDir;

struct PromptRun {
    code: Option<i32>,
    stdout: String,
    stderr: String,
}

/// Answers the overwrite prompt with `answer` under a temp HOME that already
/// holds `.claude/agents`, so the prompt is shown.
fn answer_prompt(answer: &str) -> PromptRun {
    let installer = Path::new(env!("CARGO_MANIFEST_DIR")).join("../install.sh");
    let home = TempDir::new().expect("create temp home");
    std::fs::create_dir_all(home.path().join(".claude/agents")).expect("create agents dir");
    let mut child = Command::new("bash")
        .arg("-c")
        .arg(r#"LOOM_INSTALL_LIB_ONLY=1 source "$INSTALLER"; confirm_overwrites"#)
        .env_clear()
        .env("PATH", std::env::var("PATH").unwrap_or_default())
        .env("HOME", home.path())
        .env("INSTALLER", &installer)
        .current_dir(home.path())
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("spawn bash");
    let mut stdin = child.stdin.take().expect("bash stdin");
    stdin
        .write_all(format!("{answer}\n").as_bytes())
        .expect("write answer");
    drop(stdin);
    let out = child.wait_with_output().expect("wait for bash");
    PromptRun {
        code: out.status.code(),
        stdout: String::from_utf8_lossy(&out.stdout).into_owned(),
        stderr: String::from_utf8_lossy(&out.stderr).into_owned(),
    }
}

#[test]
fn declined_install_exits_non_zero() {
    let declined = answer_prompt("n");
    assert!(
        declined.stdout.contains("cancelled"),
        "declining must report the cancellation\nstdout: {}\nstderr: {}",
        declined.stdout,
        declined.stderr
    );
    assert!(
        declined.code.is_some_and(|code| code != 0),
        "declining the overwrite prompt must exit non-zero, got {:?}\nstdout: {}",
        declined.code,
        declined.stdout
    );

    let accepted = answer_prompt("y");
    assert_eq!(
        accepted.code,
        Some(0),
        "accepting the overwrite prompt must exit 0\nstdout: {}\nstderr: {}",
        accepted.stdout,
        accepted.stderr
    );
}
