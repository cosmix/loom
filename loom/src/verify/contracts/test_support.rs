//! Fixtures shared by the contract tests: one cargo-test contract, a stage
//! carrying it, and a real git repository with the stage worktree where loom
//! puts it.

use std::path::{Path, PathBuf};
use std::process::Command;

use crate::daemon::ContractRunReport;
use crate::git::worktree::WorktreeGit;
use crate::models::stage::{Stage, StageStatus};
use crate::plan::schema::ContractSpec;
use crate::verify::contracts::store::{write_freeze, FreezeRecord, FREEZE_RECORD_VERSION};

pub(crate) const CONTRACT_ID: &str = "rejects-x";
pub(crate) const CONTRACT_FILE: &str = "tests/x_contract.rs";
pub(crate) const CONTRACT_CONTENT: &[u8] = b"#[test]\nfn rejects_x() { assert!(false); }\n";

pub(crate) fn contract() -> ContractSpec {
    ContractSpec {
        id: CONTRACT_ID.to_string(),
        file: CONTRACT_FILE.to_string(),
        test: "tests::rejects_x".to_string(),
        runner: Some("cargo-test".to_string()),
        cwd: None,
        scenario: "x arrives".to_string(),
        rejects: "accepting x".to_string(),
    }
}

/// The manifest of the one package [`contract_worktree`]'s base holds.
pub(crate) const PACKAGE_MANIFEST: &str =
    "[package]\nname = \"fx\"\nversion = \"0.1.0\"\nedition = \"2021\"\n";

pub(crate) const LIQUIDMON_FILE: &str = "src/device_lock/contract_tests.rs";

/// A contract inside a module the base does not declare yet: its file needs
/// `mod device_lock;` in `src/lib.rs` and a new `src/device_lock/mod.rs`.
pub(crate) fn liquidmon_contract() -> ContractSpec {
    ContractSpec {
        file: LIQUIDMON_FILE.to_string(),
        test: "device_lock::contract_tests::rejects_x".to_string(),
        ..contract()
    }
}

/// Put the liquidmon shape in `worktree` (a [`contract_worktree`]): the
/// contract file, the declaration in `src/lib.rs` and a declaration-only
/// `src/device_lock/mod.rs`; the plain fixture contract file is removed.
pub(crate) fn write_liquidmon(worktree: &Path) {
    std::fs::remove_file(worktree.join(CONTRACT_FILE)).unwrap();
    let contract = std::str::from_utf8(CONTRACT_CONTENT).unwrap();
    write_files(
        worktree,
        &[
            ("src/lib.rs", "mod device_lock;\n"),
            (
                "src/device_lock/mod.rs",
                "#[cfg(test)]\nmod contract_tests;\n",
            ),
            (LIQUIDMON_FILE, contract),
        ],
    );
}

/// Write each `(path, content)` beneath `root`, creating its directories.
pub(crate) fn write_files(root: &Path, files: &[(&str, &str)]) {
    for (path, content) in files {
        let path = root.join(path);
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(path, content).unwrap();
    }
}

/// An executing v2 stage owned by `session_id`, with [`contract`], whose
/// worktree is `.worktrees/<stage_id>`.
pub(crate) fn contract_stage(stage_id: &str, session_id: &str) -> Stage {
    Stage {
        id: stage_id.to_string(),
        status: StageStatus::Executing,
        plan_version: 2,
        session: Some(session_id.to_string()),
        worktree: Some(stage_id.to_string()),
        contracts: vec![contract()],
        ..Stage::default()
    }
}

/// The report of a contract session whose one contract failed.
pub(crate) fn red_reports() -> Vec<ContractRunReport> {
    vec![ContractRunReport {
        contract_id: CONTRACT_ID.to_string(),
        adapter: Some("cargo-test".to_string()),
        outcome: "failed".to_string(),
        exit_code: Some(101),
    }]
}

/// Leave the freeze record `loom stage contracts freeze` writes (DESIGN D8)
/// for `stage_id`, frozen by `session_id`, with no frozen files.
pub(crate) fn write_test_freeze(work_dir: &Path, stage_id: &str, session_id: &str) {
    let record = FreezeRecord {
        version: FREEZE_RECORD_VERSION,
        stage_id: stage_id.to_string(),
        session_id: session_id.to_string(),
        frozen_at: chrono::Utc::now(),
        base: "main".to_string(),
        files: Vec::new(),
        contracts: Vec::new(),
    };
    write_freeze(work_dir, &record, &[]).unwrap();
}

/// Make `repo_root` a repository on `main` with one commit, add the stage
/// worktree on `loom/<stage_id>`, and write the contract file into it,
/// untracked. The base is a Cargo package (`Cargo.toml` and an empty
/// `src/lib.rs`), so the freeze's scaffold check finds the manifest that
/// owns the contract file (`tests/x_contract.rs` is an integration-test
/// root). The state directory `<repo_root>/.loom/work` is left holding
/// the singleton lock of a daemon that has stopped, as one that made the
/// worktree leaves it: no daemon runs, so a process other than the daemon may
/// compute the worktree's change fingerprint. Returns the worktree root.
pub(crate) fn contract_worktree(repo_root: &Path, stage_id: &str) -> PathBuf {
    let base = [("Cargo.toml", PACKAGE_MANIFEST), ("src/lib.rs", "")];
    worktree_over(repo_root, stage_id, &base)
}

/// [`contract_worktree`] over a base holding only `README.md`: no package
/// owns the contract file, so a contract without `runner` detects no
/// adapter and its `test` runs as a shell command.
pub(crate) fn shell_contract_worktree(repo_root: &Path, stage_id: &str) -> PathBuf {
    worktree_over(repo_root, stage_id, &[])
}

fn worktree_over(repo_root: &Path, stage_id: &str, base: &[(&str, &str)]) -> PathBuf {
    std::fs::create_dir_all(repo_root).unwrap();
    git(repo_root, &["init", "-q", "-b", "main"]);
    std::fs::write(repo_root.join("README.md"), "base\n").unwrap();
    write_files(repo_root, base);
    let mut add = vec!["add", "README.md"];
    add.extend(base.iter().map(|(path, _)| *path));
    git(repo_root, &add);
    git(repo_root, &["commit", "-q", "-m", "base"]);
    let branch = format!("loom/{stage_id}");
    let relative = format!(".worktrees/{stage_id}");
    git(
        repo_root,
        &["worktree", "add", "-q", "-b", &branch, &relative],
    );
    let worktree = repo_root.join(relative);
    let contract_file = worktree.join(CONTRACT_FILE);
    std::fs::create_dir_all(contract_file.parent().unwrap()).unwrap();
    std::fs::write(contract_file, CONTRACT_CONTENT).unwrap();
    let work_dir = repo_root.join(".loom").join("work");
    std::fs::create_dir_all(&work_dir).unwrap();
    std::fs::write(work_dir.join("orchestrator.lock"), b"").unwrap();
    worktree
}

/// Git pinned to the stage worktree [`contract_worktree`] made, as the daemon
/// runs it.
pub(crate) fn pinned(worktree: &Path) -> WorktreeGit {
    let repo_root = worktree.parent().and_then(Path::parent).unwrap();
    WorktreeGit::pinned(repo_root, worktree).unwrap()
}

/// Repoint the `.git` file of a worktree [`contract_worktree`] made at a git
/// directory the stage made itself: a bare clone of the repository, checked
/// out at the stage branch, whose configuration defines a clean filter that
/// creates a marker file whenever it runs. `.gitattributes` applies the
/// filter to every path, and the tracked `README.md` is left stat-dirty, so
/// git that follows the `.git` file runs the filter to re-hash it. Returns
/// the marker's path, beside the repository.
pub(crate) fn plant_foreign_git_dir(worktree: &Path) -> PathBuf {
    let repo_root = worktree.parent().and_then(Path::parent).unwrap();
    let outside = repo_root.parent().unwrap();
    let foreign = outside.join("foreign.git");
    let marker = outside.join("filter-ran");
    let stage_id = worktree.file_name().unwrap().to_str().unwrap();
    let (source, target) = (repo_root.to_str().unwrap(), foreign.to_str().unwrap());
    git(outside, &["clone", "-q", "--bare", source, target]);
    git(
        &foreign,
        &[
            "symbolic-ref",
            "HEAD",
            &format!("refs/heads/loom/{stage_id}"),
        ],
    );
    git(&foreign, &["config", "core.bare", "false"]);
    let filter = format!("touch '{}'; cat", marker.display());
    git(&foreign, &["config", "filter.evil.clean", &filter]);
    std::fs::write(worktree.join(".git"), format!("gitdir: {target}\n")).unwrap();
    git(worktree, &["read-tree", "HEAD"]);
    std::fs::write(worktree.join(".gitattributes"), "* filter=evil\n").unwrap();
    std::fs::write(worktree.join("README.md"), "changed\n").unwrap();
    marker
}

fn git(dir: &Path, args: &[&str]) {
    let output = Command::new("git")
        .args(["-c", "user.name=t", "-c", "user.email=t@t"])
        .args([
            "-c",
            "commit.gpgsign=false",
            "-c",
            "core.hooksPath=/dev/null",
        ])
        .args(args)
        .current_dir(dir)
        .output()
        .unwrap();
    assert!(output.status.success(), "git {args:?}: {output:?}");
}
