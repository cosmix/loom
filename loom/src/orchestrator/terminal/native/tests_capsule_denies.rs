//! The state-directory and git administrative directory denies of every
//! session kind's capsule, in both layers (`sandbox.filesystem.denyWrite` for Bash,
//! `permissions.deny` for the native file tools).

use super::*;
use crate::git::runner::run_git;
use crate::models::session::SessionType;
use crate::orchestrator::terminal::native::tests_session_settings::{
    checkout, read_capsule, sandbox_with, strings, write_capsule,
};

const DENY_WRITE: &str = "/sandbox/filesystem/denyWrite";
const DENY_RULES: &str = "/permissions/deny";

/// Every session kind with the directory it runs in: stage, contract and
/// merge sessions in the stage worktree, knowledge and base-conflict sessions
/// in the checkout. The adjudication row is a judge whose stage worktree is
/// gone, which falls back to the checkout (`judge_cwd`).
fn kinds_with_cwd(repo: &Path, worktree: &Path) -> [(SessionType, PathBuf); 6] {
    [
        (SessionType::Stage, worktree.to_path_buf()),
        (SessionType::Contract, worktree.to_path_buf()),
        (SessionType::Merge, worktree.to_path_buf()),
        (SessionType::Knowledge, repo.to_path_buf()),
        (SessionType::Adjudication, repo.to_path_buf()),
        (SessionType::BaseConflict, repo.to_path_buf()),
    ]
}

fn git(dir: &Path, args: &[&str]) {
    let output = run_git(args, dir).unwrap();
    assert!(output.status.success(), "git {args:?}: {output:?}");
}

/// Make `repo` a git repository with the registered worktrees `s1` and `s2`.
fn register_worktrees(repo: &Path) {
    git(repo, &["init", "-q", "-b", "main"]);
    git(
        repo,
        &[
            "-c",
            "user.email=t@t",
            "-c",
            "user.name=t",
            "-c",
            "commit.gpgsign=false",
            "commit",
            "-q",
            "--allow-empty",
            "-m",
            "base",
        ],
    );
    git(
        repo,
        &["worktree", "add", "-q", "-b", "loom/s1", ".worktrees/s1"],
    );
    git(
        repo,
        &["worktree", "add", "-q", "-b", "loom/s2", ".worktrees/s2"],
    );
}

/// `human-review --force-complete` has no operator proof of its own: it rests
/// on no session being able to write the stage file it changes.
#[test]
fn every_session_kind_denies_writes_to_the_state_directory() {
    let checkout = checkout();
    let sandbox = sandbox_with(vec![]);
    let state = checkout.repo.join(".loom");
    for (kind, cwd) in kinds_with_cwd(&checkout.repo, &checkout.worktree) {
        let path = write_capsule(
            &checkout,
            "session-st1",
            kind,
            &cwd,
            &sandbox,
            Some(&checkout.hooks_dir),
        )
        .unwrap();
        let capsule = read_capsule(&path);

        let deny_write = strings(&capsule, DENY_WRITE);
        let rules = strings(&capsule, DENY_RULES);
        assert!(
            deny_write.contains(state.to_str().unwrap()),
            "{kind}: {capsule}"
        );
        assert!(
            rules.contains(&format!("Edit(/{}/**)", state.display())),
            "{kind}: {capsule}"
        );
        assert!(rules.contains("Edit(.loom/**)"), "{kind}: {capsule}");
    }
}

/// A session in `s1` (Stage, Contract, Merge, and a judge whose stage
/// worktree exists) keeps `s1` writable but its `config.worktree` and denies
/// `s2` whole; a session in the checkout denies both whole. The Merge row is
/// the proof that the resolver keeps its own directory.
#[test]
fn every_session_kind_denies_the_admin_dirs_it_does_not_own() {
    let checkout = checkout();
    let sandbox = sandbox_with(vec![]);
    register_worktrees(&checkout.repo);
    let registry = checkout.repo.join(".git").join("worktrees");
    let judge_in_worktree = (SessionType::Adjudication, checkout.worktree.clone());
    let rows = kinds_with_cwd(&checkout.repo, &checkout.worktree)
        .into_iter()
        .chain([judge_in_worktree]);
    for (kind, cwd) in rows {
        let path = write_capsule(
            &checkout,
            "session-cw1",
            kind,
            &cwd,
            &sandbox,
            Some(&checkout.hooks_dir),
        )
        .unwrap();
        let capsule = read_capsule(&path);
        let deny_write = strings(&capsule, DENY_WRITE);
        let rules = strings(&capsule, DENY_RULES);

        for name in ["s1", "s2"] {
            let own = cwd == checkout.worktree && name == "s1";
            let dir = registry.join(name);
            let config = dir.join("config.worktree");
            let (dir, config) = (dir.to_str().unwrap(), config.to_str().unwrap());
            let at = format!("{kind} in {}, {name}: {capsule}", cwd.display());
            assert_eq!(deny_write.contains(dir), !own, "{at}");
            assert_eq!(rules.contains(&format!("Edit(/{dir}/**)")), !own, "{at}");
            assert_eq!(deny_write.contains(config), own, "{at}");
            assert_eq!(rules.contains(&format!("Edit(/{config})")), own, "{at}");
        }
    }
}

/// A worktree-rooted session's own administrative directory must be known to
/// leave it writable, so the spawn is refused when no registry entry names its
/// directory (`.worktrees/ghost` exists on disk beside registered worktrees) or
/// two entries name it. The Adjudication row is a judge inside `.worktrees`.
#[test]
fn a_worktree_rooted_capsule_refuses_an_unclaimed_or_doubly_claimed_worktree() {
    let checkout = checkout();
    let sandbox = sandbox_with(vec![]);
    register_worktrees(&checkout.repo);
    let registry = checkout.repo.join(".git").join("worktrees");
    let ghost = checkout.repo.join(".worktrees").join("ghost");
    std::fs::create_dir_all(&ghost).unwrap();
    let kinds = [
        SessionType::Stage,
        SessionType::Contract,
        SessionType::Merge,
        SessionType::Adjudication,
    ];
    let refusal = |kind: SessionType, cwd: &Path| {
        let hooks = Some(checkout.hooks_dir.as_path());
        let result = write_capsule(&checkout, "session-rf1", kind, cwd, &sandbox, hooks);
        format!("{:#}", result.expect_err("the spawn must be refused"))
    };

    for kind in kinds {
        let message = refusal(kind, &ghost);
        assert!(
            message.contains("is not a registered worktree"),
            "{kind}: {message}"
        );
        assert!(
            message.contains(ghost.to_str().unwrap()),
            "{kind}: {message}"
        );
    }

    let claim = std::fs::read(registry.join("s1").join("gitdir")).unwrap();
    std::fs::write(registry.join("s2").join("gitdir"), claim).unwrap();
    for kind in kinds {
        let message = refusal(kind, &checkout.worktree);
        assert!(
            message.contains("more than one worktree"),
            "{kind}: {message}"
        );
        assert!(
            message.contains(checkout.worktree.to_str().unwrap()),
            "{kind}: {message}"
        );
    }
}

#[cfg(target_os = "linux")]
#[path = "tests_capsule_bwrap.rs"]
mod bwrap;
