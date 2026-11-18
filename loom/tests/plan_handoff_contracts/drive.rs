//! Calls into the run-handoff surface the contracts share, and the unpacking of
//! their outcomes.
#![allow(dead_code)]

use std::collections::BTreeSet;
use std::path::PathBuf;
use std::process::{Child, Command};

use loom::plan_server::run::abandon::{run_abandon, AbandonOutcome};
use loom::plan_server::run::changes::split_changes;
use loom::plan_server::run::commit::commit_doc_changes;
use loom::plan_server::wire::pressure::{PressureSlots, PressureStart};
use loom::plan_server::wire::run::ChangedPath;

use crate::support::{FakeLoom, Repo};

/// The first lines `print_startup` emits for a remote dashboard on a wildcard bind.
pub const REMOTE_STARTUP: [&str; 5] = [
    "loom dashboard listening on 0.0.0.0:7373 (remote access enabled; terminals enabled)",
    "  bootstrap from another machine at: http://10.20.30.40:7373/?token=abc123",
    "  bootstrap from this machine at: http://127.0.0.1:7373/?token=abc123",
    "  warning: plain HTTP; anyone who can reach this address can view the dashboard, and the token above grants settings and terminal access to anyone who has it",
    "  (Ctrl-C to stop)",
];

pub fn paths(changes: &[ChangedPath]) -> BTreeSet<String> {
    changes.iter().map(|c| c.path.clone()).collect()
}

pub fn ignore(_: &str) {}

/// Commits `ticked` against the doc changes [`split_changes`] reports now.
pub fn commit(repo: &Repo, ticked: &[&str], message: &str) -> anyhow::Result<i32> {
    let (current, _) = split_changes(&repo.root).expect("split the changes");
    let ticked: Vec<String> = ticked.iter().map(|p| p.to_string()).collect();
    commit_doc_changes(&repo.root, &ticked, message, &current, &mut ignore)
}

/// Runs the abandon sequence with the dialog having shown no unmerged branch.
pub fn abandon(repo: &Repo, fake: &FakeLoom) -> AbandonOutcome {
    run_abandon(&repo.root, &repo.runtime(), &fake.bin, &[], &mut ignore).expect("run abandon")
}

pub fn branches_changed(outcome: AbandonOutcome) -> Vec<(String, usize)> {
    match outcome {
        AbandonOutcome::BranchesChanged { now } => now,
        other => panic!("expected BranchesChanged, got {other:?}"),
    }
}

pub fn renamed(outcome: AbandonOutcome) -> PathBuf {
    match outcome {
        AbandonOutcome::Renamed { to } => to,
        other => panic!("expected Renamed, got {other:?}"),
    }
}

pub fn assert_refused(outcome: &AbandonOutcome, case: &str) {
    assert!(
        matches!(outcome, AbandonOutcome::Refused { .. }),
        "{case}: expected Refused, got {outcome:?}"
    );
}

/// A pressure start for `plan` with the default slots, every value inside its vocabulary.
pub fn start(plan: &str) -> PressureStart {
    let slots = PressureSlots {
        rounds: 2,
        claude_model: loom::claude::DEFAULT_PRESSURE_CLAUDE_MODEL.to_string(),
        claude_effort: loom::claude::DEFAULT_PRESSURE_CLAUDE_EFFORT.to_string(),
        codex_model: loom::codex::DEFAULT_PRESSURE_CODEX_MODEL.to_string(),
        codex_effort: loom::codex::DEFAULT_PRESSURE_CODEX_EFFORT.to_string(),
        address_model: loom::claude::DEFAULT_PRESSURE_CLAUDE_MODEL.to_string(),
        address_effort: loom::claude::DEFAULT_PRESSURE_ADDRESS_EFFORT.to_string(),
    };
    PressureStart {
        plan: plan.to_string(),
        slots,
    }
}

/// A live child process that sleeps until killed.
pub fn sleeper() -> Child {
    Command::new("sleep")
        .arg("30")
        .spawn()
        .expect("spawn sleep")
}
