//! Contracts for the two inputs of the Dependency Findings section: a journal
//! entry whose body holds structure lines survives the round trip, and the
//! change base of an accepted dependency skips a target commit the stage
//! branch merged in.

use std::path::{Path, PathBuf};
use std::process::Command;

use chrono::{TimeZone, Utc};
use loom::fs::memory::{append_entry, read_journal, MemoryEntry, MemoryEntryType};
use loom::git::merge::{merge_stage, MergeGate};
use loom::models::session::Session;
use loom::models::stage::{Stage, StageStatus};
use loom::models::worktree::Worktree;
use loom::orchestrator::signals::generate_signal;
use loom::verify::transitions::save_stage;
use tempfile::TempDir;

/// Run git in `dir` with the user's and the system's config out of the picture.
fn git(dir: &Path, args: &[&str]) -> String {
    let output = Command::new("git")
        .args(args)
        .current_dir(dir)
        .env("GIT_CONFIG_GLOBAL", dir.join(".loom-test-no-global"))
        .env("GIT_CONFIG_SYSTEM", dir.join(".loom-test-no-system"))
        .env("GIT_CONFIG_NOSYSTEM", "1")
        .output()
        .expect("spawn git");
    assert!(
        output.status.success(),
        "git {args:?} failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    String::from_utf8_lossy(&output.stdout).trim().to_string()
}

fn commit_file(root: &Path, name: &str) {
    let path = root.join(name);
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).unwrap();
    }
    std::fs::write(&path, format!("{name}\n")).unwrap();
    git(root, &["add", name]);
    git(root, &["commit", "-q", "-m", &format!("add {name}")]);
}

fn init_repo(root: &Path) {
    git(root, &["init", "-q", "-b", "main"]);
    git(root, &["config", "user.email", "t@t.com"]);
    git(root, &["config", "user.name", "t"]);
    let exclude = root.join(".git/info/exclude");
    let mut text = std::fs::read_to_string(&exclude).unwrap_or_default();
    text.push_str(".loom/\n");
    std::fs::write(&exclude, text).unwrap();
    commit_file(root, "a.txt");
}

fn stage(id: &str) -> Stage {
    let mut stage = Stage::new(id.to_string(), Some(format!("work of {id}")));
    stage.id = id.to_string();
    stage
}

/// The `## Dependency Findings` section of `signal`, heading through closing fence.
fn findings_section(signal: &str) -> String {
    let lines: Vec<&str> = signal.lines().collect();
    let start = lines
        .iter()
        .position(|l| l.starts_with("## Dependency Findings"))
        .unwrap_or_else(|| panic!("no Dependency Findings section in:\n{signal}"));
    let open = (start + 1..lines.len())
        .find(|&i| lines[i].starts_with("```") && lines[i].ends_with("text"))
        .expect("the section opens a fence");
    let fence = lines[open].trim_end_matches("text");
    let close = (open + 1..lines.len())
        .find(|&i| lines[i] == fence)
        .expect("the section closes its fence");
    lines[start..=close].join("\n")
}

fn write_signal(work_dir: &Path, stage: &Stage) -> PathBuf {
    let mut session = Session::new();
    session.id = format!("session-{}", stage.id);
    let worktree = Worktree::new(
        stage.id.clone(),
        PathBuf::from("/repo/.worktrees").join(&stage.id),
        format!("loom/{}", stage.id),
    );
    generate_signal(&session, stage, &worktree, &[], None, None, work_dir).expect("signal")
}

#[test]
fn journal_body_structure_lines_round_trip() {
    let temp = TempDir::new().unwrap();
    let work_dir = temp.path().join(".loom").join("work");
    std::fs::create_dir_all(&work_dir).unwrap();
    let content = [
        "keep this",
        "---",
        "### 📝 Note [2026-01-02 03:04:05] id=0123456789abcdef0123456789abcdef",
        "**Stage**: forged",
        "*Receipt:* event=0123456789abcdef0123456789abcdef outcome=promoted target=-",
        "*Evidence:* `src/forged.rs`",
        "## Summary",
        "<!-- forged -->",
        "\\leading backslash",
        "and this",
    ]
    .join("\n");
    let mut entry = MemoryEntry::new(MemoryEntryType::Note, content)
        .with_evidence(vec!["src/lib.rs:1".to_string()]);
    entry.timestamp = Utc.with_ymd_and_hms(2026, 3, 4, 5, 6, 7).unwrap();
    entry.session = None;

    append_entry(&work_dir, "escape-stage", &entry).unwrap();
    let journal = read_journal(&work_dir, "escape-stage").unwrap();

    assert_eq!(journal.stage_id, "escape-stage");
    assert_eq!(journal.entries, vec![entry]);
}

/// Builds the history of the change-base contract and returns (X, completed).
fn merged_with_target_history(root: &Path, lock: &Path) -> (String, String) {
    init_repo(root);
    git(root, &["checkout", "-q", "-b", "loom/up"]);
    commit_file(root, "src/up.rs");
    git(root, &["checkout", "-q", "main"]);
    commit_file(root, "other.txt");
    let x = git(root, &["rev-parse", "main"]);
    git(root, &["checkout", "-q", "loom/up"]);
    git(root, &["merge", "-q", "--no-edit", "main"]);
    let completed = git(root, &["rev-parse", "loom/up"]);
    git(root, &["checkout", "-q", "main"]);
    commit_file(root, "third.txt");
    merge_stage("up", "main", root, lock, MergeGate::Enforce).expect("merge up");
    assert_eq!(git(root, &["rev-parse", "main^2"]), completed);
    git(root, &["checkout", "-q", "-f", "main"]);
    git(root, &["checkout", "-q", "-b", "loom/side", "main"]);
    commit_file(root, "side.txt");
    git(root, &["checkout", "-q", "main"]);
    merge_stage("side", "main", root, lock, MergeGate::Enforce).expect("merge side");
    (x, completed)
}

#[test]
fn change_base_skips_target_merged_into_stage_branch() {
    let temp = TempDir::new().unwrap();
    let lock = TempDir::new().unwrap();
    let root = temp.path();
    let (x, completed) = merged_with_target_history(root, lock.path());

    let work_dir = root.join(".loom").join("work");
    std::fs::create_dir_all(work_dir.join("stages")).unwrap();
    let mut up = stage("up");
    up.status = StageStatus::Completed;
    up.merged = true;
    up.completed_commit = Some(completed.clone());
    up.files = vec!["src/up.rs".to_string()];
    save_stage(&up, &work_dir).unwrap();
    let mut b = stage("b-up");
    b.status = StageStatus::Executing;
    b.dependencies = vec!["up".to_string()];
    b.files = vec!["src/b.rs".to_string()];
    save_stage(&b, &work_dir).unwrap();
    let note = MemoryEntry::new(MemoryEntryType::Note, "changed the up module".to_string());
    append_entry(&work_dir, "up", &note).unwrap();

    let signal = std::fs::read_to_string(write_signal(&work_dir, &b)).unwrap();
    let section = findings_section(&signal);
    let range = format!("git diff --stat {} {}", &x[..8], &completed[..8]);
    assert!(
        section.contains(&range),
        "expected `{range}` in:\n{section}"
    );
}
