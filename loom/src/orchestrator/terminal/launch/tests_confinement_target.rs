//! The target guard under a stage capsule (`doc/plans/PLAN-target-ref-guard.md`):
//! a session that moves its target through git is refused by loom's
//! `reference-transaction` hook, a session that turns hooks off moves it
//! without a ledger line, and the guard holds that move once the host looks.
//!
//! Built on the confinement e2e's fixture and srt harness
//! (`tests_confinement_e2e.rs`, `tests_confinement_srt.rs`) and skipped with
//! them where srt cannot run. That fixture's repository has no commit and its
//! worktree's administrative directory no `HEAD` or `commondir`, so this file
//! completes both before anything runs git.

// bespoke-git-fixture: one git_env (config pointed at a missing file, NOSYSTEM, fixed author) is exported both to host git and into the srt shell command line, so the host and the capsule read the same config; the shared runner cannot be handed to the sandboxed shell.

use super::tests_confinement_e2e::srt::{diagnostics, skip, write_rc, Confined, RC_PREFIX};
use super::tests_confinement_e2e::{confine, fixture, stage, Fixture, STAGE_ID};
use crate::git::hooks::{install_reference_transaction_hook, HookInstall};
use crate::git::target_guard::{
    self, attestation_mode, AttestationMode, GuardState, HoldReason, LEDGER_FILE, RECORD_FILE,
    REFS_FILE,
};
use crate::models::session::SessionType;
use crate::models::stage::{Implementer, StageType};
use serial_test::serial;
use shell_escape::escape;
use std::borrow::Cow;
use std::path::{Path, PathBuf};
use std::process::Stdio;

/// The session id every sandboxed git runs under, as a loom session's shell
/// carries one: the hook refuses a move it cannot attest only in a session.
const SESSION_ID: &str = "probe";
/// The file the stage commit adds, outside `doc/loom/knowledge/`, so neither
/// the hook's nor the guard's knowledge exception covers the move.
const STAGE_FILE: &str = "src/stage.txt";
/// What the positive control writes into the worktree.
const CONTROL: &str = "target-guard-control";

/// The confinement fixture as a working git setup whose `main` the guard
/// records.
struct Guarded {
    f: Fixture,
    /// The stage branch, `loom/<stage id>`, checked out in the worktree.
    branch: String,
    /// `main`'s tip: the tip the guard accepted.
    seed: String,
    /// The stage branch's one commit beyond `seed`.
    stage_commit: String,
}

/// The environment every git here runs with, on the host or under srt:
/// global and system configuration pointed at a missing file (an operator's
/// own `core.hooksPath` would keep the hook from running) and a fixed
/// identity.
fn git_env(f: &Fixture) -> Vec<(&'static str, String)> {
    let missing = f.base.join("no-git-config").display().to_string();
    vec![
        ("GIT_CONFIG_GLOBAL", missing.clone()),
        ("GIT_CONFIG_SYSTEM", missing),
        ("GIT_CONFIG_NOSYSTEM", "1".to_string()),
        ("GIT_AUTHOR_NAME", "t".to_string()),
        ("GIT_AUTHOR_EMAIL", "t@t.com".to_string()),
        ("GIT_COMMITTER_NAME", "t".to_string()),
        ("GIT_COMMITTER_EMAIL", "t@t.com".to_string()),
    ]
}

/// Run `git` on the host in `dir`, assert it succeeded, and return its
/// trimmed stdout.
fn git(f: &Fixture, dir: &Path, args: &[&str]) -> String {
    let output = crate::test_git::git_command(dir)
        .args(args)
        .envs(git_env(f))
        .stdin(Stdio::null())
        .output()
        .expect("run git");
    assert!(
        output.status.success(),
        "git {args:?} in {}: {}",
        dir.display(),
        diagnostics(&output)
    );
    String::from_utf8_lossy(&output.stdout).trim().to_string()
}

/// The shell command that runs `git args` under srt with [`git_env`] and
/// `LOOM_SESSION_ID` set to [`SESSION_ID`].
fn sandboxed_git(f: &Fixture, args: &[&str]) -> String {
    let mut words: Vec<String> = git_env(f)
        .into_iter()
        .chain([("LOOM_SESSION_ID", SESSION_ID.to_string())])
        .map(|(name, value)| format!("{name}={}", escape(Cow::Owned(value))))
        .collect();
    words.push("git".to_string());
    words.extend(
        args.iter()
            .map(|arg| escape(Cow::Borrowed(*arg)).into_owned()),
    );
    words.join(" ")
}

fn main_tip(f: &Fixture) -> String {
    git(f, &f.repo, &["rev-parse", "refs/heads/main"])
}

/// The guard's three state files.
fn state_files(f: &Fixture) -> Vec<PathBuf> {
    [RECORD_FILE, LEDGER_FILE, REFS_FILE]
        .iter()
        .map(|name| f.work_dir.join(name))
        .collect()
}

/// `f` made a working git setup: `main` at an empty seed commit, and the
/// stage branch checked out in the stage worktree, one commit ahead of it.
fn with_stage_commit(f: Fixture) -> Guarded {
    git(&f, &f.repo, &["symbolic-ref", "HEAD", "refs/heads/main"]);
    git(
        &f,
        &f.repo,
        &["commit", "-q", "--allow-empty", "-m", "seed"],
    );
    let branch = format!("loom/{STAGE_ID}");
    git(&f, &f.repo, &["branch", branch.as_str()]);
    let admin = f.repo.join(".git/worktrees").join(STAGE_ID);
    std::fs::write(admin.join("HEAD"), format!("ref: refs/heads/{branch}\n")).unwrap();
    std::fs::write(admin.join("commondir"), "../..\n").unwrap();
    git(&f, &f.worktree, &["reset", "-q"]);
    let checked_out = git(&f, &f.worktree, &["rev-parse", "--abbrev-ref", "HEAD"]);
    assert_eq!(
        checked_out, branch,
        "the fixture's worktree must have the stage branch checked out"
    );
    std::fs::write(f.worktree.join(STAGE_FILE), "stage work\n").unwrap();
    git(&f, &f.worktree, &["add", STAGE_FILE]);
    git(&f, &f.worktree, &["commit", "-q", "-m", "stage work"]);
    let seed = main_tip(&f);
    let stage_commit = git(&f, &f.worktree, &["rev-parse", "HEAD"]);
    Guarded {
        f,
        branch,
        seed,
        stage_commit,
    }
}

/// [`with_stage_commit`] with loom's hook installed and `main` recorded by
/// the guard, which writes its state files before any session starts: srt
/// mounts a denied path that does not exist from `/dev/null`, where a write
/// exits 0 and lands nowhere, so a refusal could not be told apart.
fn guarded() -> Guarded {
    let g = with_stage_commit(fixture());
    let f = &g.f;
    let installed = install_reference_transaction_hook(&f.repo).unwrap();
    assert_eq!(installed, HookInstall::Installed);
    // The guard requires a ledger line only while attestation is on; a
    // `core.hooksPath` in this host's own git configuration turns it off.
    assert_eq!(
        attestation_mode(&f.repo, &f.work_dir),
        AttestationMode::Active
    );
    let state = target_guard::check(&f.repo, &f.work_dir, "main").unwrap();
    let accepted = GuardState::Clear {
        accepted: g.seed.clone(),
    };
    assert_eq!(state, Some(accepted));
    for file in state_files(f) {
        assert!(file.is_file(), "the guard did not write {}", file.display());
    }
    g
}

/// A session's `git update-ref` of `main` with loom's hook in place: the
/// hook must refuse it and `main` stay at the seed. A write into the
/// worktree later in the same run must land: the shell ran past the move,
/// and the worktree is writable under this capsule.
fn hooked_move_missed(g: &Guarded, confined: &Confined) -> Vec<String> {
    let f = &g.f;
    let control = f.worktree.join("src/control");
    let update = sandboxed_git(
        f,
        &["update-ref", "refs/heads/main", g.stage_commit.as_str()],
    );
    let control_arg = escape(Cow::Owned(control.display().to_string()));
    let command = format!("{update}; echo {RC_PREFIX}$?; printf {CONTROL} > {control_arg}");
    let output = confined.run_alive(&command);
    let rc = write_rc(&output);
    let main = main_tip(f);
    let refusal = format!("loom: refusing to move refs/heads/main from loom session {SESSION_ID}");
    let by_hook = String::from_utf8_lossy(&output.stderr).contains(&refusal);
    let mut missed = Vec::new();
    if !(rc.is_some_and(|rc| rc != 0) && main == g.seed && by_hook) {
        missed.push(format!(
            "{}: the hook did not refuse a session's move of main (git exit code {rc:?}, \
             main at {main}, accepted {}, hook refusal printed: {by_hook}): {}",
            confined.label,
            g.seed,
            diagnostics(&output)
        ));
    }
    if std::fs::read_to_string(&control).ok().as_deref() != Some(CONTROL) {
        missed.push(format!(
            "{}: the control write to {} in the same run did not land: {}",
            confined.label,
            control.display(),
            diagnostics(&output)
        ));
    }
    missed
}

/// The matched control: the same move with hooks off must land. The capsule
/// lets a session write the target's ref (Claude Code grants a linked
/// worktree its whole git common directory), so the hook alone refused the
/// move above, and a move that skips it leaves no ledger line.
fn unhooked_move_missed(g: &Guarded, confined: &Confined) -> Vec<String> {
    let f = &g.f;
    let args = [
        "-c",
        "core.hooksPath=/dev/null",
        "update-ref",
        "refs/heads/main",
        g.stage_commit.as_str(),
    ];
    let update = sandboxed_git(f, &args);
    let output = confined.run_alive(&format!("{update}; echo {RC_PREFIX}$?"));
    let rc = write_rc(&output);
    let main = main_tip(f);
    if rc == Some(0) && main == g.stage_commit {
        return Vec::new();
    }
    vec![format!(
        "{}: a session's move of main with hooks off must land (git exit code {rc:?}, \
         main at {main}, expected {}): {}",
        confined.label,
        g.stage_commit,
        diagnostics(&output)
    )]
}

#[test]
#[serial]
fn a_sandboxed_target_move_is_refused_or_left_unattested_and_held() {
    if skip("a_sandboxed_target_move_is_refused_or_left_unattested_and_held") {
        return;
    }
    let g = guarded();
    let f = &g.f;
    let lanes = vec![Implementer::Claude];
    let confined = confine(f, SessionType::Stage, &stage(StageType::Standard, lanes));

    let mut missed = hooked_move_missed(&g, &confined);
    missed.extend(unhooked_move_missed(&g, &confined));
    // The worktree write is the positive control for the refusals.
    missed.extend(confined.refusals_missed(&state_files(f)));
    missed.extend(confined.writes_missed(&[f.worktree.join("src/x")]));
    assert!(missed.is_empty(), "{}", missed.join("\n"));

    // Back on the host: no ledger line covers the move, so the guard holds it.
    let ledger = std::fs::read_to_string(f.work_dir.join(LEDGER_FILE)).unwrap();
    assert!(
        !ledger
            .lines()
            .any(|line| line.contains(g.stage_commit.as_str())),
        "the ledger records the session's move of main:\n{ledger}"
    );
    let state = target_guard::check(&f.repo, &f.work_dir, "main").unwrap();
    let hold = match state {
        Some(GuardState::Held(hold)) => hold,
        other => panic!("the guard must hold main after an unattested move, got {other:?}"),
    };
    let unattested = HoldReason::Unattested {
        from: g.seed.clone(),
        to: g.stage_commit.clone(),
        paths: vec![STAGE_FILE.to_string()],
    };
    let stage_work = HoldReason::StageWork {
        branches: vec![g.branch.clone()],
    };
    assert_eq!(hold.reasons, vec![unattested, stage_work]);
}
