//! Fixtures for the merge lifecycle end-to-end tests: a real repository with a
//! real stage worktree, an orchestrator over it, the relay path a Merge
//! session uses for `--resolved`, and a snapshot of the operator's checkout
//! that the invariants are asserted against after every step.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::process::Command;

use chrono::Utc;
use tempfile::TempDir;

use crate::fs::inbox::{read_ledger, write_entry, LedgerOutcome, WriteOutcome};
use crate::fs::work_dir::write_terminal_config;
use crate::git::merge::merge_head_exists;
use crate::models::session::{SessionBackendKind, SessionType, TerminalConfig};
use crate::models::stage::{Stage, StageStatus};
use crate::orchestrator::core::{Orchestrator, OrchestratorConfig};
use crate::orchestrator::merge_lifecycle::test_support::write_live_session;
use crate::plan::ExecutionGraph;
use crate::relay::{new_request_id, AgentRole, InboxEntry, RequestKind};
use crate::verify::transitions::{load_stage, save_stage};

pub(super) const ID: &str = "s";

/// Run `git` in `dir` with ambient configuration shut out; the output is
/// returned whatever the exit status.
pub(super) fn git_output(dir: &Path, args: &[&str]) -> std::process::Output {
    Command::new("git")
        .args(args)
        .current_dir(dir)
        .env("GIT_CONFIG_GLOBAL", dir.join(".loom-test-no-global"))
        .env("GIT_CONFIG_SYSTEM", dir.join(".loom-test-no-system"))
        .env("GIT_CONFIG_NOSYSTEM", "1")
        .env("GIT_AUTHOR_NAME", "t")
        .env("GIT_AUTHOR_EMAIL", "t@t.com")
        .env("GIT_COMMITTER_NAME", "t")
        .env("GIT_COMMITTER_EMAIL", "t@t.com")
        .output()
        .unwrap()
}

/// Run `git` in `dir`, assert it succeeded, and return its trimmed stdout.
pub(super) fn git(dir: &Path, args: &[&str]) -> String {
    let out = git_output(dir, args);
    assert!(
        out.status.success(),
        "git {args:?} failed: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    String::from_utf8_lossy(&out.stdout).trim().to_string()
}

pub(super) fn commit_file(dir: &Path, name: &str, text: &str) {
    let file = dir.join(name);
    std::fs::create_dir_all(file.parent().unwrap()).unwrap();
    std::fs::write(file, text).unwrap();
    git(dir, &["add", name]);
    git(dir, &["commit", "-q", "-m", name]);
}

/// Twenty numbered lines: a file a stage and an operator can edit apart.
pub(super) fn twenty_lines() -> String {
    (1..=20).map(|n| format!("line {n}\n")).collect()
}

/// A repository on `main` holding `seed.txt` and `other.txt`, with the state
/// directories excluded from its status as in a real project, and the stage
/// worktree `.worktrees/s` on a new branch `loom/s` at `main`.
pub(super) fn repo_with_worktree() -> TempDir {
    let repo = TempDir::new().unwrap();
    let root = repo.path();
    git(root, &["init", "-q", "-b", "main"]);
    git(root, &["config", "user.email", "t@t.com"]);
    git(root, &["config", "user.name", "t"]);
    std::fs::write(root.join(".git/info/exclude"), ".loom/\n.worktrees/\n").unwrap();
    std::fs::write(root.join("seed.txt"), "seed").unwrap();
    std::fs::write(root.join("other.txt"), "other\n").unwrap();
    git(root, &["add", "seed.txt", "other.txt"]);
    git(root, &["commit", "-q", "-m", "seed"]);
    let worktree = worktree(root);
    let path = worktree.to_str().unwrap();
    git(root, &["worktree", "add", "-q", "-b", "loom/s", path]);
    repo
}

pub(super) fn worktree(root: &Path) -> PathBuf {
    root.join(".worktrees").join(ID)
}

pub(super) fn main_tip(root: &Path) -> String {
    git(root, &["rev-parse", "main"])
}

pub(super) fn branch_tip(root: &Path) -> String {
    git(root, &["rev-parse", "loom/s"])
}

/// The stage and `main` edit `seed.txt` differently, so merging conflicts.
pub(super) fn make_conflict(root: &Path) {
    commit_file(&worktree(root), "seed.txt", "stage side");
    commit_file(root, "seed.txt", "main side");
}

/// What a resolver does in the worktree: merge `main` (which conflicts),
/// write `resolution` to `seed.txt`, and commit.
pub(super) fn resolve_by_merging(root: &Path, resolution: &str) {
    let wt = worktree(root);
    let merge = git_output(&wt, &["merge", "main"]);
    assert!(!merge.status.success(), "the merge must conflict");
    std::fs::write(wt.join("seed.txt"), resolution).unwrap();
    git(&wt, &["add", "seed.txt"]);
    git(&wt, &["commit", "-q", "-m", "resolve"]);
}

/// A new orchestrator over `root` on the tmux lane, so building it never
/// probes the host for a terminal emulator, with an empty graph.
pub(super) fn orchestrator(root: &Path) -> Orchestrator {
    orchestrator_with_graph(root, ExecutionGraph::build(Vec::new()).unwrap())
}

pub(super) fn orchestrator_with_graph(root: &Path, graph: ExecutionGraph) -> Orchestrator {
    let work_dir = root.join(".loom").join("work");
    let backend = SessionBackendKind::Tmux;
    write_terminal_config(&work_dir, &TerminalConfig { backend }).unwrap();
    let config = OrchestratorConfig {
        work_dir,
        repo_root: root.to_path_buf(),
        base_branch: Some("main".to_string()),
        enable_skill_routing: false,
        ..Default::default()
    };
    Orchestrator::new(config, graph).unwrap()
}

/// An orchestrator holding stage `s` as `Completed` and not merged.
pub(super) fn orchestrator_with_completed_stage(root: &Path) -> Orchestrator {
    let orchestrator = orchestrator(root);
    let stage = Stage {
        id: ID.to_string(),
        status: StageStatus::Completed,
        completed_commit: Some(branch_tip(root)),
        ..Stage::default()
    };
    save_stage(&stage, &orchestrator.config.work_dir).unwrap();
    orchestrator
}

pub(super) fn on_disk(orchestrator: &Orchestrator) -> Stage {
    load_stage(ID, &orchestrator.config.work_dir).unwrap()
}

/// Relay `merge-resolved` from a live Merge session of stage `s` through the
/// relay inbox and drain it with the daemon's own pass. Returns the outcome
/// the ledger holds and the session, to be finished by the caller.
pub(super) fn resolve_from_inbox(
    orchestrator: &mut Orchestrator,
) -> (LedgerOutcome, crate::models::session::Session) {
    let work_dir = orchestrator.config.work_dir.clone();
    let session = write_live_session(&work_dir, ID, SessionType::Merge);
    let entry = InboxEntry {
        v: 1,
        id: new_request_id(),
        kind: RequestKind::MergeResolved,
        relayed_at: Utc::now(),
        session_id: session.id.clone(),
        stage_id: ID.to_string(),
        agent: AgentRole::Main,
        tool_use_id: None,
        payload: serde_json::json!({}),
    };
    assert!(matches!(
        write_entry(&work_dir, &entry).unwrap(),
        WriteOutcome::Written
    ));
    orchestrator.drain_session_inboxes();
    let outcome = read_ledger(&work_dir, &session.id)
        .unwrap()
        .into_iter()
        .rev()
        .find(|record| record.id == entry.id && record.outcome.is_some())
        .and_then(|record| record.outcome)
        .expect("the drain settles the request");
    (outcome, session)
}

/// Everything about the operator's checkout the lifecycle must not disturb.
pub(super) struct Checkout {
    pub(super) head: String,
    branch: String,
    status: String,
    files: BTreeMap<String, Vec<u8>>,
}

pub(super) fn checkout(root: &Path) -> Checkout {
    let mut files = BTreeMap::new();
    collect_files(root, root, &mut files);
    Checkout {
        head: git(root, &["rev-parse", "HEAD"]),
        branch: git(root, &["rev-parse", "--abbrev-ref", "HEAD"]),
        status: git(root, &["status", "--porcelain"]),
        files,
    }
}

/// Every regular file under `dir` outside `.git` and the loom directories.
fn collect_files(root: &Path, dir: &Path, files: &mut BTreeMap<String, Vec<u8>>) {
    for entry in std::fs::read_dir(dir).unwrap() {
        let path = entry.unwrap().path();
        let name = path.file_name().unwrap().to_string_lossy().into_owned();
        if matches!(name.as_str(), ".git" | ".loom" | ".worktrees") {
            continue;
        }
        if path.is_dir() {
            collect_files(root, &path, files);
        } else {
            let relative = path.strip_prefix(root).unwrap().to_string_lossy();
            files.insert(relative.into_owned(), std::fs::read(&path).unwrap());
        }
    }
}

/// The step changed nothing in the operator's checkout: no `MERGE_HEAD`, the
/// same `HEAD`, branch, status and file bytes.
pub(super) fn assert_untouched(root: &Path, before: &Checkout) {
    assert!(
        !merge_head_exists(root).unwrap(),
        "MERGE_HEAD in the checkout"
    );
    let now = checkout(root);
    assert_eq!(now.head, before.head, "HEAD moved");
    assert_eq!(now.branch, before.branch, "the checked-out branch changed");
    assert_eq!(now.status, before.status, "status changed");
    assert_eq!(now.files, before.files, "working files changed");
}

/// The step advanced the checked-out target and nothing else: no
/// `MERGE_HEAD`, `HEAD` is now `main`'s tip, the status is the same, and
/// only the files in `changed` differ in bytes.
pub(super) fn assert_advanced_only(root: &Path, before: &Checkout, changed: &[&str]) {
    assert!(
        !merge_head_exists(root).unwrap(),
        "MERGE_HEAD in the checkout"
    );
    let now = checkout(root);
    assert_ne!(now.head, before.head, "HEAD did not advance");
    assert_eq!(now.head, main_tip(root), "HEAD is not the target's tip");
    assert_eq!(now.branch, before.branch);
    assert_eq!(now.status, before.status, "status changed");
    let strip = |files: &BTreeMap<String, Vec<u8>>| -> BTreeMap<String, Vec<u8>> {
        files
            .iter()
            .filter(|(name, _)| !changed.contains(&name.as_str()))
            .map(|(name, bytes)| (name.clone(), bytes.clone()))
            .collect()
    };
    assert_eq!(
        strip(&now.files),
        strip(&before.files),
        "working files changed"
    );
}

/// The number of commit objects in the repository, loose and packed.
pub(super) fn commit_objects(root: &Path) -> usize {
    let out = git(
        root,
        &[
            "cat-file",
            "--batch-all-objects",
            "--batch-check=%(objecttype)",
        ],
    );
    out.lines().filter(|kind| *kind == "commit").count()
}

pub(super) fn text_of(root: &Path, name: &str) -> String {
    std::fs::read_to_string(root.join(name)).unwrap()
}

pub(super) fn stage_file_text(orchestrator: &Orchestrator) -> String {
    let stages = orchestrator.config.work_dir.join("stages");
    let file = crate::fs::stage_files::find_stage_file(&stages, ID)
        .unwrap()
        .unwrap();
    std::fs::read_to_string(file).unwrap()
}
