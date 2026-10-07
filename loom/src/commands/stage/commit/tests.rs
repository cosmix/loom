use super::*;
use std::fs;
use std::io::Write;
use std::os::unix::fs::PermissionsExt;
use std::path::Path;
use std::process::Command;

use tempfile::TempDir;

use crate::models::session::SessionType;
use crate::relay::emit::test_support::{context_for, RelayFixture};

#[derive(Default)]
struct VecSink {
    stdout: Vec<u8>,
    stderr: Vec<u8>,
}

impl RelaySink for VecSink {
    fn stdout(&mut self) -> &mut dyn Write {
        &mut self.stdout
    }

    fn stderr(&mut self) -> &mut dyn Write {
        &mut self.stderr
    }
}

fn git(root: &Path, args: &[&str]) -> String {
    let output = Command::new("git")
        .args(args)
        .current_dir(root)
        .env("GIT_CONFIG_GLOBAL", root.join("no-global-config"))
        .env("GIT_CONFIG_SYSTEM", root.join("no-system-config"))
        .env("GIT_CONFIG_NOSYSTEM", "1")
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "git {args:?} failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    String::from_utf8_lossy(&output.stdout).trim().to_string()
}

fn init_repo(root: &Path) {
    git(root, &["init", "-b", "main"]);
    for (key, value) in [
        ("user.name", "Tester"),
        ("user.email", "tester@example.com"),
        ("commit.gpgsign", "false"),
        ("core.hooksPath", ".git/hooks"),
    ] {
        git(root, &["config", key, value]);
    }
    fs::write(root.join("a.txt"), "base\n").unwrap();
    git(root, &["add", "a.txt"]);
    git(root, &["commit", "-m", "base"]);
}

fn stage(root: &Path) {
    fs::write(root.join("a.txt"), "changed\n").unwrap();
    git(root, &["add", "a.txt"]);
}

fn install_hook(root: &Path, name: &str, script: &str) {
    let path = root.join(".git/hooks").join(name);
    fs::write(&path, script).unwrap();
    fs::set_permissions(&path, fs::Permissions::from_mode(0o755)).unwrap();
}

fn relay_fixture() -> RelayFixture {
    let fixture = context_for(SessionType::Stage);
    init_repo(&fixture.cwd);
    fixture
}

fn relay_commit(fixture: &RelayFixture, message: &str, sink: &mut VecSink) -> Result<()> {
    commit_with(
        "stage-a",
        message,
        Some("stage-a"),
        RelayMode::Relay(fixture.context.clone()),
        &fixture.cwd,
        sink,
    )
}

fn ticket(fixture: &RelayFixture) -> crate::relay::Ticket {
    let path = fs::read_dir(&fixture.context.scratch_dir)
        .unwrap()
        .map(|entry| entry.unwrap().path())
        .find(|path| path.extension().and_then(|value| value.to_str()) == Some("req"))
        .unwrap();
    crate::relay::Ticket::decode(&fs::read(path).unwrap()).unwrap()
}

fn payload(fixture: &RelayFixture) -> CommitPayload {
    serde_json::from_value(ticket(fixture).payload).unwrap()
}

/// The error an operator-mode commit of `message` in `dir` fails with. `dir`
/// is no repository, so only the message check can word it as
/// `validate_commit_message` does: the hooks fail outside a repository.
fn operator_refusal(dir: &Path, message: &str) -> String {
    let mut sink = VecSink::default();
    let error = commit_with(
        "stage-a",
        message,
        None,
        RelayMode::Operator,
        dir,
        &mut sink,
    )
    .unwrap_err();
    format!("{error:#}")
}

#[test]
fn an_empty_message_is_refused() {
    let temp = TempDir::new().unwrap();
    let mut sink = VecSink::default();

    assert!(commit_with(
        "stage-a",
        "",
        None,
        RelayMode::Operator,
        temp.path(),
        &mut sink
    )
    .is_err());
    let refusal = operator_refusal(temp.path(), "");
    assert!(refusal.contains("the commit message is empty"), "{refusal}");
}

#[test]
fn a_nul_byte_is_refused() {
    let temp = TempDir::new().unwrap();
    let mut sink = VecSink::default();

    assert!(commit_with(
        "stage-a",
        "feat: a\0b",
        None,
        RelayMode::Operator,
        temp.path(),
        &mut sink
    )
    .is_err());
    let refusal = operator_refusal(temp.path(), "feat: a\0b");
    assert!(refusal.contains("contains a NUL byte"), "{refusal}");
}

#[test]
fn an_oversized_message_is_refused() {
    let temp = TempDir::new().unwrap();
    let mut sink = VecSink::default();
    let message = "a".repeat(16 * 1024 + 1);

    assert!(commit_with(
        "stage-a",
        &message,
        None,
        RelayMode::Operator,
        temp.path(),
        &mut sink
    )
    .is_err());
    let refusal = operator_refusal(temp.path(), &message);
    assert!(refusal.contains("the limit is 16384"), "{refusal}");
}

#[test]
fn ai_attribution_is_refused() {
    let temp = TempDir::new().unwrap();
    for message in [
        "feat: change\n\nCo-Authored-By: Claude <noreply@anthropic.com>",
        "feat: change\n\nSigned-off-by: Claude",
        "feat: change\n\nGenerated with Claude Code",
    ] {
        let mut sink = VecSink::default();
        assert!(commit_with(
            "stage-a",
            message,
            None,
            RelayMode::Operator,
            temp.path(),
            &mut sink
        )
        .is_err());
        let refusal = operator_refusal(temp.path(), message);
        assert!(refusal.contains("carries AI attribution"), "{refusal}");
    }
}

#[test]
fn a_plain_message_naming_claude_passes() {
    let fixture = relay_fixture();
    let mut sink = VecSink::default();

    relay_commit(
        &fixture,
        "feat(auth): add the claude login probe",
        &mut sink,
    )
    .unwrap();
    assert_eq!(
        payload(&fixture).message,
        "feat(auth): add the claude login probe\n"
    );
}

#[test]
fn a_stage_id_other_than_the_session_stage_is_refused() {
    let fixture = relay_fixture();
    let mut sink = VecSink::default();

    assert!(commit_with(
        "stage-b",
        "feat: change",
        Some("stage-a"),
        RelayMode::Relay(fixture.context.clone()),
        &fixture.cwd,
        &mut sink
    )
    .is_err());
    assert_eq!(
        fs::read_dir(&fixture.context.scratch_dir).unwrap().count(),
        0
    );
}

#[test]
fn the_payload_carries_the_tree_written_after_a_restaging_pre_commit_hook() {
    let fixture = relay_fixture();
    stage(&fixture.cwd);
    let before = git(&fixture.cwd, &["write-tree"]);
    install_hook(
        &fixture.cwd,
        "pre-commit",
        "#!/bin/sh\nprintf 'hook\\n' >> a.txt\ngit add a.txt\n",
    );
    let mut sink = VecSink::default();

    relay_commit(&fixture, "feat: change", &mut sink).unwrap();
    let request = payload(&fixture);
    assert_eq!(request.expected_tree, git(&fixture.cwd, &["write-tree"]));
    assert_ne!(request.expected_tree, before);
    assert_eq!(
        request.expected_head,
        git(&fixture.cwd, &["rev-parse", "HEAD"])
    );
}

#[test]
fn a_failing_pre_commit_hook_writes_no_ticket() {
    let fixture = relay_fixture();
    install_hook(&fixture.cwd, "pre-commit", "#!/bin/sh\nexit 1\n");
    let mut sink = VecSink::default();

    assert!(relay_commit(&fixture, "feat: change", &mut sink).is_err());
    assert_eq!(
        fs::read_dir(&fixture.context.scratch_dir).unwrap().count(),
        0
    );
}

#[test]
fn a_commit_msg_hook_rewrite_reaches_the_payload() {
    let fixture = relay_fixture();
    install_hook(
        &fixture.cwd,
        "commit-msg",
        "#!/bin/sh\nprintf 'feat: rewritten\\n' > \"$1\"\n",
    );
    let mut sink = VecSink::default();

    relay_commit(&fixture, "feat: original", &mut sink).unwrap();
    assert_eq!(payload(&fixture).message, "feat: rewritten\n");
}

#[test]
fn operator_mode_calls_commit_staged_in_process() {
    let temp = TempDir::new().unwrap();
    init_repo(temp.path());
    git(temp.path(), &["checkout", "-b", "loom/stage-a"]);
    stage(temp.path());
    let mut sink = VecSink::default();

    commit_with(
        "stage-a",
        "feat: commit",
        None,
        RelayMode::Operator,
        temp.path(),
        &mut sink,
    )
    .unwrap();
    let head = git(temp.path(), &["rev-parse", "HEAD"]);
    assert_eq!(
        git(temp.path(), &["log", "-1", "--format=%s"]),
        "feat: commit"
    );
    assert_eq!(
        String::from_utf8(sink.stdout).unwrap(),
        format!("committed {head}\n")
    );
}

#[test]
fn the_confirmation_names_request_status_wait() {
    let fixture = relay_fixture();
    let mut sink = VecSink::default();

    relay_commit(&fixture, "feat: change", &mut sink).unwrap();
    let stderr = String::from_utf8(sink.stderr).unwrap();
    assert!(stderr.contains("loom request status") && stderr.contains("--wait 90"));
}

#[test]
fn the_message_is_stripspaced_after_the_hooks() {
    let fixture = relay_fixture();
    install_hook(
        &fixture.cwd,
        "commit-msg",
        "#!/bin/sh\nprintf '   \\n\\n' >> \"$1\"\n",
    );
    let mut sink = VecSink::default();

    relay_commit(&fixture, "feat: clean", &mut sink).unwrap();
    assert_eq!(payload(&fixture).message, "feat: clean\n");
}
