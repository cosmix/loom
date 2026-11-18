//! The operator commands around a held target, through the built binary:
//! `loom clean --state` and `loom clean --all` refuse while the target holds a
//! move loom did not accept and delete nothing, `loom target accept` refuses
//! inside a loom session, and `loom target status` still reports when the
//! target branch is gone. Every command runs from the root of a scratch
//! repository holding `.loom/work/` (no `config.toml`, so the target is
//! `main`).

#[path = "integration/helpers.rs"]
#[allow(dead_code)]
mod helpers;

use loom::git::target_guard::{accepted_tip, check, recorded_hold, GuardState, RECORD_FILE};
use std::path::{Path, PathBuf};
use std::process::Output;
use tempfile::TempDir;

use helpers::test_support::git_repo::{git, TestRepo};

/// A repository whose `main` the guard holds, and the files a refused
/// command must leave behind.
struct Held {
    _dir: TempDir,
    root: PathBuf,
    work_dir: PathBuf,
    accepted: String,
    record: Vec<u8>,
    survivors: Vec<PathBuf>,
}

/// A repository on `main` with one commit and `.loom/work/` created; the
/// guard records `main`, then an operator commit adds
/// `.claude/settings.json` and the guard holds it. A stage worktree directory
/// and a file in the state directory stand for what `loom clean` would delete.
fn held() -> Held {
    let dir = TempDir::new().unwrap();
    let root = dir.path().canonicalize().unwrap();
    TestRepo::builder()
        .identity("t", "t@t.com")
        .seed("README.md", "readme\n", "seed")
        .init_in(&root);
    std::fs::write(root.join(".git/info/exclude"), ".loom/\n.worktrees/\n").unwrap();
    let work_dir = root.join(".loom/work");
    std::fs::create_dir_all(&work_dir).unwrap();
    let first = check(&root, &work_dir, "main").unwrap();
    assert!(matches!(first, Some(GuardState::Clear { .. })));
    let accepted = git(&root, &["rev-parse", "main"]);

    std::fs::create_dir_all(root.join(".claude")).unwrap();
    std::fs::write(root.join(".claude/settings.json"), "{}\n").unwrap();
    git(&root, &["add", ".claude/settings.json"]);
    git(&root, &["commit", "-q", "-m", "settings"]);
    let second = check(&root, &work_dir, "main").unwrap();
    assert!(matches!(second, Some(GuardState::Held(_))), "{second:?}");

    let worktree_file = root.join(".worktrees/stage-a/keep.txt");
    std::fs::create_dir_all(worktree_file.parent().unwrap()).unwrap();
    std::fs::write(&worktree_file, "work\n").unwrap();
    let state_file = work_dir.join("keep.txt");
    std::fs::write(&state_file, "state\n").unwrap();
    let record = std::fs::read(work_dir.join(RECORD_FILE)).unwrap();
    Held {
        _dir: dir,
        root,
        work_dir,
        accepted,
        record,
        survivors: vec![worktree_file, state_file],
    }
}

fn loom(root: &Path, args: &[&str]) -> Output {
    helpers::loom_cmd()
        .args(args)
        .current_dir(root)
        .output()
        .unwrap()
}

fn text(bytes: &[u8]) -> String {
    String::from_utf8_lossy(bytes).into_owned()
}

/// `out` is a refusal that names the hold and the record's review command, and
/// the held repository still has its record and everything else.
fn assert_refused_and_untouched(out: &Output, held: &Held) {
    let (stdout, stderr) = (text(&out.stdout), text(&out.stderr));
    assert!(!out.status.success(), "the command ran; stdout: {stdout}");
    assert!(
        stderr.contains("the target branch main has a move loom did not accept"),
        "stderr: {stderr}"
    );
    assert!(stderr.contains("touches .claude/settings.json"), "{stderr}");
    assert!(stderr.contains("loom target status"), "{stderr}");
    assert!(
        !stdout.contains("Base graphs"),
        "the refusal must come before the base graph prune: {stdout}"
    );
    assert_eq!(
        std::fs::read(held.work_dir.join(RECORD_FILE)).unwrap(),
        held.record,
        "the guard record changed"
    );
    for path in &held.survivors {
        assert!(path.exists(), "{} was deleted", path.display());
    }
}

#[test]
fn clean_state_refuses_while_the_target_is_held() {
    let held = held();

    let out = loom(&held.root, &["clean", "--state"]);

    assert_refused_and_untouched(&out, &held);
}

#[test]
fn clean_all_refuses_while_the_target_is_held() {
    let held = held();

    let out = loom(&held.root, &["clean", "--all"]);

    assert_refused_and_untouched(&out, &held);
}

#[test]
fn target_accept_inside_a_loom_session_is_refused() {
    let held = held();
    let moved = git(&held.root, &["rev-parse", "main"]);

    let out = helpers::loom_cmd()
        .args(["target", "accept", "--to", &moved])
        .env("LOOM_SESSION_ID", "session-7")
        .current_dir(&held.root)
        .output()
        .unwrap();

    let stderr = text(&out.stderr);
    assert!(!out.status.success(), "a session accepted the move");
    assert!(
        stderr.contains("this is loom session session-7"),
        "{stderr}"
    );
    assert!(
        stderr.contains("a stage agent cannot accept it"),
        "{stderr}"
    );
    assert!(recorded_hold(&held.work_dir, "main").unwrap().is_some());
    assert_eq!(
        accepted_tip(&held.work_dir, "main").unwrap(),
        Some(held.accepted.clone())
    );
    assert_eq!(
        std::fs::read(held.work_dir.join(RECORD_FILE)).unwrap(),
        held.record
    );
}

#[test]
fn target_status_reports_a_deleted_target_branch() {
    let held = held();
    git(&held.root, &["checkout", "-q", "--detach"]);
    git(&held.root, &["branch", "-q", "-D", "main"]);

    let out = loom(&held.root, &["target", "status"]);

    assert!(out.status.success(), "stderr: {}", text(&out.stderr));
    let stdout = text(&out.stdout);
    assert!(
        stdout.contains("State: refs/heads/main does not resolve"),
        "{stdout}"
    );
    assert!(
        stdout.contains(&format!(
            "Restore: git update-ref refs/heads/main {}",
            held.accepted
        )),
        "{stdout}"
    );
}
