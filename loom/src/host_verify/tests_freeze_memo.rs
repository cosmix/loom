//! Tests for sharing a failed build between the contracts of one job.

use std::cell::RefCell;

use serial_test::serial;
use std::path::PathBuf;

use anyhow::Result;

use super::apply::Applied;
use super::freeze_memo::{run_memoized, Verdict};
use super::tests_freeze::{confinement_unavailable, verdicts, Fixture};
use super::{store, JobState};
use crate::daemon::ContractRunReport;
use crate::plan::schema::ContractSpec;
use crate::testrun::{registry, RunOutput};
use crate::verify::contracts::failed_builds::BuildKey;
use crate::verify::contracts::freeze::JudgedRun;
use crate::verify::contracts::run_dir::resolve_run;
use crate::verify::contracts::test_support::{contract, CONTRACT_FILE};

fn spec(id: &str) -> ContractSpec {
    ContractSpec {
        id: id.to_string(),
        ..contract()
    }
}

fn report(id: &str, outcome: &str) -> ContractRunReport {
    ContractRunReport {
        contract_id: id.to_string(),
        adapter: Some("cargo-test".to_string()),
        outcome: outcome.to_string(),
        exit_code: Some(101),
    }
}

/// Contract `id` runs the build `build` names (`None`: no adapter applies)
/// and ends with `outcome`, a run per contract recorded in `runs`.
struct Plan<'a> {
    build: Option<&'a str>,
    outcome: &'a str,
    compile_failed: bool,
}

fn run_all(plans: &[(&str, Plan<'_>)]) -> (Vec<String>, Vec<(String, Verdict)>) {
    let contracts: Vec<ContractSpec> = plans.iter().map(|(id, _)| spec(id)).collect();
    let runs = RefCell::new(Vec::new());
    let mut verdicts = Vec::new();
    let find = |id: &str| plans.iter().find(|(name, _)| *name == id).unwrap();
    run_memoized(
        &contracts,
        |contract| -> Result<String> { Ok(contract.id.clone()) },
        |id| -> Option<BuildKey> {
            let build = find(id).1.build?;
            Some((PathBuf::from("/w"), build.to_string()))
        },
        |contract, id| {
            runs.borrow_mut().push(id.clone());
            let plan = &find(id).1;
            Ok(JudgedRun {
                judged: Ok(report(&contract.id, plan.outcome)),
                compile_failed: plan.compile_failed,
            })
        },
        |contract, verdict| verdicts.push((contract.id.clone(), verdict)),
    )
    .unwrap();
    (runs.into_inner(), verdicts)
}

fn plan<'a>(build: Option<&'a str>, outcome: &'a str) -> Plan<'a> {
    Plan {
        build,
        outcome,
        compile_failed: outcome == "build_failed",
    }
}

#[test]
fn a_failed_build_is_shared_within_one_build() {
    let (runs, verdicts) = run_all(&[
        ("a", plan(Some("t"), "build_failed")),
        ("b", plan(Some("t"), "build_failed")),
    ]);
    assert_eq!(runs, ["a"], "the second contract does not run");
    assert_eq!(verdicts.len(), 2);
    let (id, second) = &verdicts[1];
    assert_eq!(id, "b");
    assert_eq!(second.shared_from.as_deref(), Some("a"));
    let shared = second.judged.as_ref().unwrap();
    assert_eq!(shared, &report("b", "build_failed"));
    assert_eq!(verdicts[0].1.shared_from, None);
}

#[test]
fn builds_that_differ_are_not_shared() {
    let (runs, _) = run_all(&[
        ("a", plan(Some("t1"), "build_failed")),
        ("b", plan(Some("t2"), "build_failed")),
    ]);
    assert_eq!(runs, ["a", "b"]);
}

#[test]
fn only_build_failed_is_shared() {
    let (runs, _) = run_all(&[
        ("a", plan(Some("t"), "failed")),
        ("b", plan(Some("t"), "failed")),
    ]);
    assert_eq!(runs, ["a", "b"]);
}

#[test]
fn a_contract_without_a_build_key_is_never_shared() {
    let (runs, _) = run_all(&[
        ("a", plan(None, "build_failed")),
        ("b", plan(None, "build_failed")),
    ]);
    assert_eq!(runs, ["a", "b"]);
}

#[test]
fn a_build_failed_the_compiler_did_not_report_is_not_shared() {
    let transient = Plan {
        compile_failed: false,
        ..plan(Some("t"), "build_failed")
    };
    let (runs, _) = run_all(&[("a", transient), ("b", plan(Some("t"), "build_failed"))]);
    assert_eq!(
        runs,
        ["a", "b"],
        "a killed rustc reds only its own contract"
    );
}

#[test]
fn runners_do_not_share_builds() {
    let (runs, _) = run_all(&[
        ("a", plan(Some("cargo nextest run t"), "build_failed")),
        ("b", plan(Some("cargo test t"), "build_failed")),
    ]);
    assert_eq!(runs, ["a", "b"]);
}

/// Gradle output the adapter reads as a rejected build is shared by a later
/// contract of the same project, so the command runs once, though the runner
/// is no cargo.
#[test]
fn a_gradle_compile_failure_is_shared() {
    let tmp = tempfile::TempDir::new().unwrap();
    std::fs::write(tmp.path().join("ATest.java"), "").unwrap();
    let gradle = registry::by_name("gradle").unwrap();
    let rejected = RunOutput {
        stdout: "> Compilation failed; see the compiler error output for details.\n",
        stderr: "",
        exit_code: Some(1),
    };
    assert!(gradle.compile_rejected(&rejected));
    let contracts: Vec<ContractSpec> = ["a", "b"]
        .iter()
        .map(|id| ContractSpec {
            file: "ATest.java".to_string(),
            test: format!("ATest.{id}"),
            runner: Some("gradle".to_string()),
            ..spec(id)
        })
        .collect();
    let runs = RefCell::new(Vec::new());
    run_memoized(
        &contracts,
        |c| resolve_run(c, Some(gradle), tmp.path(), tmp.path(), None),
        |run| run.build_key(),
        |c, _| {
            runs.borrow_mut().push(c.id.clone());
            Ok(JudgedRun {
                judged: Ok(report(&c.id, "build_failed")),
                compile_failed: gradle.compile_rejected(&rejected),
            })
        },
        |_, _| {},
    )
    .unwrap();
    assert_eq!(runs.into_inner(), ["a"]);
}

/// Two contracts of one cargo target whose file does not compile: the job
/// runs the first, the second takes its `build_failed`, and the freeze the
/// reports of both builds is accepted.
#[test]
#[serial]
fn two_contracts_in_one_scope_share_a_failed_build_and_freeze() {
    if confinement_unavailable("two_contracts_in_one_scope_share_a_failed_build_and_freeze") {
        return;
    }
    let fx = Fixture::cargo(|stage| {
        stage.contracts = vec![
            ContractSpec {
                test: "rejects_x".to_string(),
                ..spec("a")
            },
            ContractSpec {
                test: "rejects_y".to_string(),
                ..spec("b")
            },
        ];
    });
    std::fs::write(fx.worktree.join(CONTRACT_FILE), "fn broken( {\n").unwrap();
    // The cargo builds write `target/`, which the tree digest would see.
    let exclude = std::process::Command::new("git")
        .args(["rev-parse", "--git-path", "info/exclude"])
        .current_dir(&fx.worktree)
        .output()
        .unwrap();
    let exclude = fx
        .worktree
        .join(String::from_utf8(exclude.stdout).unwrap().trim());
    std::fs::create_dir_all(exclude.parent().unwrap()).unwrap();
    std::fs::write(exclude, "target/\nCargo.lock\n").unwrap();
    let (record, result) = fx.run(fx.job(JobState::Running));
    assert_eq!(
        verdicts(&result),
        [("contract:a", true), ("contract:b", true), ("format", true)],
        "{result:?}"
    );
    let progress = store::read_progress(&fx.work_dir, &record.stage_id, &record.id).unwrap();
    assert!(
        progress[1]
            .summary
            .contains("shares the failed build of contract `a`"),
        "{progress:?}"
    );
    let applied = fx.apply(&record);
    assert!(
        matches!(applied, Applied::FreezeAccepted { files: 2 }),
        "{applied:?}"
    );
}
