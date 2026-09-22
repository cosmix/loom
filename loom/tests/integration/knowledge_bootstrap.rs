//! `loom knowledge bootstrap` end to end, against fake `claude` scripts.
//!
//! Each loom and git command sets its own cwd and environment
//! (`knowledge_bootstrap_support::Sandbox`); the test process's env and cwd
//! are never touched.

use std::fs;
use std::path::Path;

use tempfile::TempDir;

use super::helpers::init_test_repo;
use super::knowledge_bootstrap_support::{
    combined, core_util_files, leftover_briefs, numbered, receipt_ids, success, table_ids,
    table_rows, Sandbox, COMPLETING_FAKE, EXIT_1, LOG_AND_EXIT, MARKER_THEN_EXIT,
};

const CORE_UTIL_IDS: [&str; 4] = [".", "src", "src/core", "src/util"];

#[test]
fn structural_only_scaffolds_and_reports() {
    let sandbox = Sandbox::new(LOG_AND_EXIT);
    let repo = init_test_repo();
    let stdout = success(&sandbox.bootstrap(repo.path(), &["--structural-only"]));

    assert!(stdout.contains("coverage:"), "{stdout}");
    assert!(stdout.contains("structural-only"), "{stdout}");
    assert!(repo.path().join("doc/loom/knowledge/INDEX.md").exists());
    assert!(repo.path().join(".loom/.gitignore").exists());
    let status = sandbox.git(repo.path(), &["status", "--porcelain"]);
    assert!(
        !status.lines().any(|line| line.contains(".loom")),
        "{status}"
    );
    assert!(receipt_ids(repo.path()).is_none());
    assert!(!sandbox.log_exists());
}

#[test]
fn dry_run_prints_command_without_spawning() {
    let sandbox = Sandbox::new(LOG_AND_EXIT);
    let repo = sandbox.fixture(&core_util_files());
    let stdout = success(&sandbox.bootstrap(repo.path(), &["--dry-run"]));

    assert!(
        stdout.contains("--disallowedTools Edit,Write,NotebookEdit"),
        "{stdout}"
    );
    let brief = stdout
        .lines()
        .find_map(|line| line.strip_prefix("brief: "))
        .expect("brief line");
    assert!(Path::new(brief).exists(), "{brief}");
    assert_eq!(table_ids(&stdout), CORE_UTIL_IDS);
    let rows = table_rows(&stdout);
    assert!(!rows
        .iter()
        .flatten()
        .any(|cell| cell.contains("doc/loom/knowledge")));
    assert!(!sandbox.log_exists());
}

#[test]
fn session_completion_writes_receipt_and_refresh_is_idempotent() {
    let sandbox = Sandbox::new(COMPLETING_FAKE);
    let repo = sandbox.fixture(&core_util_files());
    success(&sandbox.bootstrap(repo.path(), &["--model", "sonnet", "--effort", "low"]));

    assert_eq!(sandbox.invocations(), 1);
    let args = sandbox.recorded_args();
    let tools = args.iter().position(|arg| arg == "--disallowedTools");
    assert_eq!(
        args[tools.expect("--disallowedTools") + 1],
        "Edit,Write,NotebookEdit"
    );
    let last = args.last().expect("argv");
    assert!(last.contains(".loom/work/bootstrap/brief-"), "{last}");
    assert!(receipt_ids(repo.path()).is_some_and(|ids| !ids.is_empty()));
    let architecture = repo.path().join("doc/loom/knowledge/architecture.md");
    let architecture = fs::read_to_string(architecture).expect("architecture.md");
    assert!(
        architecture.contains("Fake architecture Entry"),
        "{architecture}"
    );
    assert_eq!(leftover_briefs(repo.path()), 0);

    let stdout = success(&sandbox.bootstrap(repo.path(), &["--refresh"]));
    assert!(stdout.contains("knowledge is current"), "{stdout}");
    assert_eq!(sandbox.invocations(), 1);

    let edited = "pub fn edited() -> usize {\n    7\n}\n";
    fs::write(repo.path().join("src/util/a.rs"), edited).expect("edit a.rs");
    sandbox.git(repo.path(), &["commit", "-qam", "edit util"]);
    let stdout = success(&sandbox.bootstrap(repo.path(), &["--refresh", "--dry-run"]));
    let statuses: Vec<String> = table_rows(&stdout)
        .into_iter()
        .map(|row| format!("{} {}", row[0], row[3]))
        .collect();
    let expected = [
        ". unchanged",
        "src unchanged",
        "src/core unchanged",
        "src/util changed",
    ];
    assert_eq!(statuses, expected, "{stdout}");
}

#[test]
fn exit_without_marker_writes_no_receipt() {
    let sandbox = Sandbox::new(LOG_AND_EXIT);
    let repo = init_test_repo();
    let text = success(&sandbox.bootstrap(repo.path(), &[]));
    assert!(text.contains("receipt not written"), "{text}");
    assert!(receipt_ids(repo.path()).is_none());
    assert_eq!(sandbox.invocations(), 1);
    assert_eq!(leftover_briefs(repo.path()), 0);

    let failing = Sandbox::new(EXIT_1);
    let repo = init_test_repo();
    let output = failing.bootstrap(repo.path(), &[]);
    assert!(!output.status.success(), "{}", combined(&output));
    assert!(receipt_ids(repo.path()).is_none());
    assert_eq!(leftover_briefs(repo.path()), 0);
}

#[test]
fn refuses_inside_stage() {
    let sandbox = Sandbox::new(LOG_AND_EXIT);
    let repo = init_test_repo();
    let output = sandbox
        .command(repo.path(), &[])
        .env("LOOM_STAGE_ID", "s")
        .output()
        .expect("run loom");
    assert!(!output.status.success());
    assert!(
        combined(&output).contains("operator command"),
        "{}",
        combined(&output)
    );
}

#[test]
fn refuses_outside_git() {
    let sandbox = Sandbox::new(LOG_AND_EXIT);
    let dir = TempDir::new().expect("temp dir");
    let parent = dir.path().parent().expect("temp dir has a parent");
    let output = sandbox
        .command(dir.path(), &[])
        .env("GIT_CEILING_DIRECTORIES", parent)
        .output()
        .expect("run loom");
    let text = combined(&output);
    assert!(!output.status.success(), "{text}");
    assert!(text.contains("must run inside a git repository"), "{text}");
}

#[test]
fn refuses_without_commits() {
    let sandbox = Sandbox::new(LOG_AND_EXIT);
    let dir = TempDir::new().expect("temp dir");
    sandbox.git(dir.path(), &["init", "-q"]);
    let output = sandbox.bootstrap(dir.path(), &[]);
    assert!(!output.status.success());
    let text = combined(&output);
    assert!(text.contains("needs at least one commit"), "{text}");
}

#[test]
fn removal_only_refresh_briefs_removed_cluster() {
    let sandbox = Sandbox::new(COMPLETING_FAKE);
    let mut files = numbered("a", "a", 0..20);
    files.extend(numbered("b", "b", 0..21));
    files.extend(numbered("c", "c", 0..8));
    let repo = sandbox.fixture(&files);
    success(&sandbox.bootstrap(repo.path(), &[]));
    assert_eq!(sandbox.invocations(), 1);
    assert_eq!(
        receipt_ids(repo.path()).expect("receipt"),
        [".", "a", "b", "c"]
    );

    sandbox.git(repo.path(), &["rm", "-rq", "c"]);
    sandbox.git(repo.path(), &["commit", "-qm", "drop c"]);
    success(&sandbox.bootstrap(repo.path(), &["--refresh"]));
    assert_eq!(sandbox.invocations(), 2);
    assert!(sandbox.briefs().contains("- removed: c"));
    assert_eq!(receipt_ids(repo.path()).expect("receipt"), [".", "a", "b"]);

    let stdout = success(&sandbox.bootstrap(repo.path(), &["--refresh"]));
    assert!(stdout.contains("knowledge is current"), "{stdout}");
    assert_eq!(sandbox.invocations(), 2);
}

#[test]
fn marker_then_exit_writes_receipt() {
    let sandbox = Sandbox::new(MARKER_THEN_EXIT);
    let repo = init_test_repo();
    success(&sandbox.bootstrap(repo.path(), &[]));
    assert!(receipt_ids(repo.path()).is_some());
}

#[test]
fn refresh_is_current_after_commit_and_in_clone() {
    let sandbox = Sandbox::new(COMPLETING_FAKE);
    let repo = sandbox.fixture(&core_util_files());
    success(&sandbox.bootstrap(repo.path(), &[]));
    sandbox.git(repo.path(), &["add", "-A"]);
    sandbox.git(repo.path(), &["commit", "-qm", "k"]);
    let stdout = success(&sandbox.bootstrap(repo.path(), &["--refresh"]));
    assert!(stdout.contains("knowledge is current"), "{stdout}");

    let clones = TempDir::new().expect("temp dir");
    let clone = clones.path().join("clone");
    let source = repo.path().to_str().expect("utf-8 path");
    let target = clone.to_str().expect("utf-8 path");
    sandbox.git(clones.path(), &["clone", "-q", source, target]);
    let stdout = success(&sandbox.bootstrap(&clone, &["--refresh"]));
    assert!(stdout.contains("knowledge is current"), "{stdout}");
    assert_eq!(sandbox.invocations(), 1);
}
