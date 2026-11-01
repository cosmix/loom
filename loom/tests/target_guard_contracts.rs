//! Contracts for the target guard: what `check` holds and accepts, and how
//! `merge_stage` consults it.
//!
//! Every git command here runs with the user's and system's config shut out
//! and `LOOM_SESSION_ID` removed, so the hook (when installed) can write the
//! ledger and attests every move made through git.

use loom::git::hooks::{install_reference_transaction_hook, HookInstall};
use loom::git::merge::{merge_stage, MergeBlock, MergeGate, MergeResult};
use loom::git::target_guard::{
    accepted_tip, attestation_mode, check, AttestationMode, GuardState, HoldReason,
};
use std::path::{Path, PathBuf};
use std::process::{Command, Output};
use tempfile::TempDir;

fn run(dir: &Path, args: &[&str], envs: &[(&str, &str)]) -> Output {
    let mut cmd = Command::new("git");
    cmd.args(args)
        .current_dir(dir)
        .env_remove("LOOM_SESSION_ID")
        .env("GIT_CONFIG_GLOBAL", dir.join(".loom-test-no-global"))
        .env("GIT_CONFIG_SYSTEM", dir.join(".loom-test-no-system"))
        .env("GIT_CONFIG_NOSYSTEM", "1")
        .env("GIT_AUTHOR_NAME", "t")
        .env("GIT_AUTHOR_EMAIL", "t@t.com")
        .env("GIT_COMMITTER_NAME", "t")
        .env("GIT_COMMITTER_EMAIL", "t@t.com");
    for (key, value) in envs {
        cmd.env(key, value);
    }
    cmd.output().unwrap()
}

fn git_env(dir: &Path, args: &[&str], envs: &[(&str, &str)]) -> String {
    let out = run(dir, args, envs);
    assert!(
        out.status.success(),
        "git {args:?} failed: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    String::from_utf8_lossy(&out.stdout).trim().to_string()
}

fn git(dir: &Path, args: &[&str]) -> String {
    git_env(dir, args, &[])
}

/// A repository on `main` with one commit, `.loom/` and `.worktrees/`
/// excluded, and the state directory `root/.loom/work` created.
fn fixture() -> (TempDir, PathBuf, PathBuf) {
    let dir = TempDir::new().unwrap();
    let root = dir.path().canonicalize().unwrap();
    git(&root, &["init", "-b", "main"]);
    git(&root, &["config", "user.name", "t"]);
    git(&root, &["config", "user.email", "t@t.com"]);
    std::fs::write(root.join(".git/info/exclude"), ".loom/\n.worktrees/\n").unwrap();
    commit(&root, "README.md", false);
    let work = root.join(".loom/work");
    std::fs::create_dir_all(&work).unwrap();
    (dir, root, work)
}

/// Commit `path` in the checkout at `dir`; `hooks_off` skips every hook.
fn commit(dir: &Path, path: &str, hooks_off: bool) -> String {
    let file = dir.join(path);
    std::fs::create_dir_all(file.parent().unwrap()).unwrap();
    std::fs::write(&file, path).unwrap();
    git(dir, &["add", path]);
    let mut args = if hooks_off {
        vec!["-c", "core.hooksPath=/dev/null"]
    } else {
        Vec::new()
    };
    args.extend(["commit", "-m", path]);
    git(dir, &args);
    git(dir, &["rev-parse", "HEAD"])
}

/// A commit on no branch whose tree is `base`'s tree plus `path`; its parent
/// is `base` when `parent` is set, and it has none otherwise.
fn crafted(root: &Path, base: &str, path: &str, parent: bool) -> String {
    let index = root.join(".loom/crafted-index");
    let _ = std::fs::remove_file(&index);
    let index_env = [("GIT_INDEX_FILE", index.to_str().unwrap())];
    let blob_src = root.join(".loom/crafted-blob");
    std::fs::write(&blob_src, path).unwrap();
    let blob = git(root, &["hash-object", "-w", blob_src.to_str().unwrap()]);
    git_env(root, &["read-tree", base], &index_env);
    let entry = format!("100644,{blob},{path}");
    git_env(
        root,
        &["update-index", "--add", "--cacheinfo", &entry],
        &index_env,
    );
    let tree = git_env(root, &["write-tree"], &index_env);
    let mut args = vec!["commit-tree", tree.as_str(), "-m", "crafted"];
    if parent {
        args.extend(["-p", base]);
    }
    git(root, &args)
}

/// Move `main` to `commit` with hooks disabled: no attestation line.
fn move_main(root: &Path, commit: &str) {
    let args = [
        "-c",
        "core.hooksPath=/dev/null",
        "update-ref",
        "refs/heads/main",
        commit,
    ];
    git(root, &args);
}

fn tip(root: &Path) -> String {
    git(root, &["rev-parse", "refs/heads/main"])
}

/// `.worktrees/s` on `loom/s`, with one commit per file; returns the commits.
fn stage_branch(root: &Path, files: &[&str]) -> Vec<String> {
    let wt = root.join(".worktrees/s");
    git(
        root,
        &["worktree", "add", "-b", "loom/s", wt.to_str().unwrap()],
    );
    files.iter().map(|f| commit(&wt, f, false)).collect()
}

fn install_hook(root: &Path, work: &Path) {
    let installed = install_reference_transaction_hook(root).unwrap();
    assert_eq!(installed, HookInstall::Installed);
    assert_eq!(
        attestation_mode(root, work),
        AttestationMode::Active,
        "attestation must be on (is core.hooksPath set at global or system scope?)"
    );
}

fn check_now(root: &Path, work: &Path) -> GuardState {
    check(root, work, "main")
        .unwrap()
        .expect("no other merge lock holder")
}

fn record(root: &Path, work: &Path) -> String {
    let state = check_now(root, work);
    let main = tip(root);
    assert_eq!(
        state,
        GuardState::Clear {
            accepted: main.clone()
        }
    );
    main
}

fn reasons(state: GuardState) -> Vec<HoldReason> {
    match state {
        GuardState::Held(hold) => hold.reasons,
        other => panic!("expected a hold, got {other:?}"),
    }
}

fn has_control_path(reasons: &[HoldReason], path: &str) -> bool {
    reasons
        .iter()
        .any(|r| matches!(r, HoldReason::ControlPaths { paths } if paths.iter().any(|p| p == path)))
}

fn has_unattested(reasons: &[HoldReason], path: &str) -> bool {
    reasons.iter().any(
        |r| matches!(r, HoldReason::Unattested { paths, .. } if paths.iter().any(|p| p == path)),
    )
}

#[test]
fn control_path_move_is_held() {
    let (_dir, root, work) = fixture();
    record(&root, &work);
    commit(&root, ".claude/settings.json", false);
    let reasons = reasons(check_now(&root, &work));
    assert!(
        has_control_path(&reasons, ".claude/settings.json"),
        "{reasons:?}"
    );
}

#[test]
fn unattested_move_is_held() {
    let (_dir, root, work) = fixture();
    install_hook(&root, &work);
    let a = record(&root, &work);
    let x = crafted(&root, &a, "src/crafted.rs", true);
    move_main(&root, &x);
    let reasons = reasons(check_now(&root, &work));
    assert!(has_unattested(&reasons, "src/crafted.rs"), "{reasons:?}");
}

#[test]
fn attested_operator_commit_is_accepted() {
    let (_dir, root, work) = fixture();
    install_hook(&root, &work);
    record(&root, &work);
    let new_main = commit(&root, "src/y.rs", false);
    let state = check_now(&root, &work);
    assert_eq!(
        state,
        GuardState::Clear {
            accepted: new_main.clone()
        }
    );
    assert_eq!(accepted_tip(&work, "main").unwrap(), Some(new_main));
}

#[test]
fn unattested_knowledge_only_move_is_accepted() {
    let (_dir, root, work) = fixture();
    install_hook(&root, &work);
    record(&root, &work);
    let new_main = commit(&root, "doc/loom/knowledge/x.md", true);
    let state = check_now(&root, &work);
    assert_eq!(state, GuardState::Clear { accepted: new_main });
}

#[test]
fn fast_forward_into_unmerged_stage_branch_is_held() {
    let (_dir, root, work) = fixture();
    let stage = stage_branch(&root, &["a.txt", "b.txt"]);
    record(&root, &work);
    git(&root, &["update-ref", "refs/heads/main", &stage[0]]);
    let reasons = reasons(check_now(&root, &work));
    let stage_work = reasons.iter().any(|r| {
        matches!(r, HoldReason::StageWork { branches } if branches.iter().any(|b| b == "loom/s"))
    });
    assert!(stage_work, "{reasons:?}");
}

#[test]
fn history_rewrite_is_held() {
    let (_dir, root, work) = fixture();
    install_hook(&root, &work);
    commit(&root, "second.txt", false);
    record(&root, &work);
    git(&root, &["reset", "--hard", "HEAD~1"]);
    commit(&root, "third.txt", false);
    let reasons = reasons(check_now(&root, &work));
    assert!(reasons.contains(&HoldReason::NotFastForward), "{reasons:?}");
}

#[test]
fn tracked_hooks_dir_change_is_held() {
    let (_dir, root, work) = fixture();
    git(&root, &["config", "core.hooksPath", "hooks"]);
    record(&root, &work);
    commit(&root, "hooks/pre-commit", false);
    let reasons = reasons(check_now(&root, &work));
    assert!(
        has_control_path(&reasons, "hooks/pre-commit"),
        "{reasons:?}"
    );
}

#[test]
fn normalized_hook_paths_are_held_at_the_target_boundary() {
    for spelling in [".", "././.githooks", "x/../.githooks", "absolute"] {
        let (_dir, root, work) = fixture();
        std::fs::create_dir_all(root.join("x")).unwrap();
        let absolute = root.join("x/../.githooks");
        let configured = if spelling == "absolute" {
            absolute.to_str().unwrap()
        } else {
            spelling
        };
        git(&root, &["config", "core.hooksPath", configured]);
        record(&root, &work);
        let path = if spelling == "." {
            "pre-commit"
        } else {
            ".githooks/pre-commit"
        };
        commit(&root, path, false);
        let held = reasons(check_now(&root, &work));
        assert!(has_control_path(&held, path), "{spelling}: {held:?}");
    }
}

#[test]
fn merge_stage_refuses_a_held_target() {
    let (_dir, root, work) = fixture();
    stage_branch(&root, &["a.txt"]);
    let a = record(&root, &work);
    let x = crafted(&root, &a, ".claude/settings.json", true);
    move_main(&root, &x);
    let result = merge_stage("s", "main", &root, &work, MergeGate::Enforce).unwrap();
    match result {
        MergeResult::Blocked(MergeBlock::TargetHeld {
            target,
            accepted,
            observed,
        }) => {
            assert_eq!(target, "main");
            assert_eq!(accepted, a);
            assert_eq!(observed, x);
        }
        other => panic!("expected TargetHeld, got {other:?}"),
    }
    assert_eq!(tip(&root), x);
}

#[test]
fn merge_stage_does_not_settle_on_an_agent_fast_forward() {
    let (_dir, root, work) = fixture();
    let stage = stage_branch(&root, &["a.txt"]);
    record(&root, &work);
    move_main(&root, &stage[0]);
    let result = merge_stage("s", "main", &root, &work, MergeGate::Enforce).unwrap();
    assert!(
        matches!(result, MergeResult::Blocked(MergeBlock::TargetHeld { .. })),
        "{result:?}"
    );
    assert_eq!(tip(&root), stage[0]);
}

#[test]
fn loom_merge_keeps_the_accepted_tip_current() {
    let (_dir, root, work) = fixture();
    stage_branch(&root, &["a.txt"]);
    record(&root, &work);
    let result = merge_stage("s", "main", &root, &work, MergeGate::Enforce).unwrap();
    assert!(matches!(result, MergeResult::Success { .. }), "{result:?}");
    let merged = tip(&root);
    assert_eq!(accepted_tip(&work, "main").unwrap(), Some(merged.clone()));
    let op = crafted(&root, &merged, "op.txt", true);
    move_main(&root, &op);
    assert_eq!(check_now(&root, &work), GuardState::Clear { accepted: op });
}

#[test]
fn replace_ref_does_not_hide_an_unattested_change() {
    let (_dir, root, work) = fixture();
    install_hook(&root, &work);
    let a = record(&root, &work);
    let x = crafted(&root, &a, "src/crafted.rs", true);
    let d = crafted(&root, &a, "doc/loom/knowledge/decoy.md", true);
    git(&root, &["replace", &x, &d]);
    move_main(&root, &x);
    let reasons = reasons(check_now(&root, &work));
    assert!(has_unattested(&reasons, "src/crafted.rs"), "{reasons:?}");
}

#[test]
fn graft_does_not_hide_a_history_rewrite() {
    let (_dir, root, work) = fixture();
    let a = record(&root, &work);
    let x = crafted(&root, &a, "src/x.rs", false);
    std::fs::write(root.join(".git/info/grafts"), format!("{x} {a}\n")).unwrap();
    move_main(&root, &x);
    let reasons = reasons(check_now(&root, &work));
    assert!(reasons.contains(&HoldReason::NotFastForward), "{reasons:?}");
}

#[test]
fn attestation_downgrade_still_holds_an_unattested_move() {
    let (_dir, root, work) = fixture();
    install_hook(&root, &work);
    let a = record(&root, &work);
    git(&root, &["config", "core.hooksPath", "/dev/null"]);
    let mode = attestation_mode(&root, &work);
    assert!(matches!(mode, AttestationMode::Off { .. }), "{mode:?}");
    let x = crafted(&root, &a, "src/crafted.rs", true);
    move_main(&root, &x);
    let reasons = reasons(check_now(&root, &work));
    assert!(has_unattested(&reasons, "src/crafted.rs"), "{reasons:?}");
}
