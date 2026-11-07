//! Contracts for the criteria-authoring stage: the freeze's static gate.
//! Only pure checks run at freeze, compiler-backed lints only once the
//! contracts compile, a failure is the writer's only when its output names a
//! written file as a whole path, and a static refusal goes back to the
//! waiting writer until the third one of its session, which is filed.

#[path = "criteria_authoring_contracts/support.rs"]
mod support;

use loom::host_verify::apply::Applied;
use loom::host_verify::freeze::apply_freeze_result;
use loom::host_verify::JobRecord;
use loom::verify::contracts::format_gate::{attribute_failure, is_pure_check, runs_at_freeze};
use loom::verify::contracts::refusal::{
    record_static_refusal, JobRefusalOrigin, StaticRefusal, STATIC_RETURN_LIMIT,
};
use support::{format_only, job_id, outcome, Jobs, FORMAT_SUMMARY, STAGE};
use tempfile::TempDir;

#[test]
fn autofix_forms_are_not_pure_checks() {
    let dir = TempDir::new().unwrap();
    let pure = [
        "cargo clippy --all-targets -- -D warnings",
        "eslint src",
        "biome lint .",
        "ruff check .",
        "tsc --noEmit",
        "go vet ./...",
        "golangci-lint run",
        "! rg -n FIXME tests/",
    ];
    for command in pure {
        assert!(
            is_pure_check(command, dir.path()),
            "{command:?} is a pure check"
        );
    }
    let not_pure = [
        "cargo clippy --fix",
        "eslint --fix .",
        "biome check --write",
        "ruff check --fix",
        "tsc",
        "cargo build",
        "cargo clippy && cargo test",
    ];
    for command in not_pure {
        assert!(
            !is_pure_check(command, dir.path()),
            "{command:?} is not a pure check"
        );
    }
}

#[test]
fn failure_attribution_needs_a_whole_path() {
    let written: Vec<String> = [
        "tests/foo_contracts.rs",
        "/w/pkg/tests/foo_contracts.rs",
        "src/lib.rs",
        "/w/pkg/src/lib.rs",
    ]
    .iter()
    .map(|path| path.to_string())
    .collect();
    let blamed = |output: &str| attribute_failure(output, &written);
    let clippy = "error: unused variable\n  --> tests/foo_contracts.rs:3:5\n";
    assert_eq!(blamed(clippy), vec!["tests/foo_contracts.rs".to_string()]);
    let elsewhere = "error: unused import\n  --> src/other.rs:1:1\n";
    assert!(
        blamed(elsewhere).is_empty(),
        "a file the writer never touched"
    );
    let prefixed = "error: unused\n  --> my_tests/foo_contracts.rs:3:5\n";
    assert!(
        blamed(prefixed).is_empty(),
        "my_tests/foo_contracts.rs is another file"
    );
    let absolute = "warning: dead code\n  --> /w/pkg/src/lib.rs:9:1\n";
    assert_eq!(blamed(absolute), vec!["/w/pkg/src/lib.rs".to_string()]);
    let dotted = "./tests/foo_contracts.rs:3:5: result of fmt.Sprintf call not used\n";
    assert_eq!(blamed(dotted), vec!["tests/foo_contracts.rs".to_string()]);
    let both = "tests/foo_contracts.rs:1:1\nsrc/lib.rs:2:2\ntests/foo_contracts.rs:4:4\n";
    assert_eq!(
        blamed(both),
        vec![
            "src/lib.rs".to_string(),
            "tests/foo_contracts.rs".to_string()
        ],
        "sorted and deduplicated"
    );
}

#[test]
fn compiling_lints_wait_for_compiling_contracts() {
    let dir = TempDir::new().unwrap();
    let cwd = dir.path();
    for lint in ["cargo clippy --all-targets", "go vet ./..."] {
        assert!(
            !runs_at_freeze(lint, cwd, false),
            "{lint:?} waits for compiling contracts"
        );
        assert!(
            runs_at_freeze(lint, cwd, true),
            "{lint:?} runs once the contracts compile"
        );
    }
    for compiles in [false, true] {
        assert!(
            !runs_at_freeze("tsc --noEmit", cwd, compiles),
            "tsc --noEmit never runs at freeze (contracts_compile {compiles})"
        );
    }
    assert!(
        runs_at_freeze("eslint src", cwd, false),
        "a syntactic lint always runs"
    );
    assert!(
        runs_at_freeze("cargo fmt --check", cwd, false),
        "a formatter check always runs"
    );
}

/// Apply `record` and return the refusal message it answers with.
fn refused(jobs: &Jobs, record: &JobRecord) -> String {
    match apply_freeze_result(&jobs.work, record).expect("apply the freeze job") {
        Applied::FreezeRefused(message) => message,
        other => panic!("expected the refusal returned to the writer, got {other:?}"),
    }
}

fn apply(jobs: &Jobs, record: &JobRecord) {
    apply_freeze_result(&jobs.work, record).expect("apply the freeze job");
}

#[test]
fn static_refusal_returns_to_the_writer() {
    let jobs = Jobs::create();
    for order in [1u64, 2] {
        let record = jobs.failed("s1", order, format_only(), None);
        let message = refused(&jobs, &record);
        assert!(
            message.contains("tests/a.rs"),
            "the failing output: {message}"
        );
        let refusal = jobs.refusal();
        assert_eq!(refusal.session_id, "s1");
        assert_eq!(
            refusal.returned, order as u32,
            "static refusal {order} of s1 is returned to the writer, not filed"
        );
    }

    let red = vec![
        outcome("contract:c1", false, "c1 passed on the unimplemented tree"),
        outcome("format", false, FORMAT_SUMMARY),
    ];
    apply(&jobs, &jobs.failed("s2", 3, red, None));
    let refusal = jobs.refusal();
    assert_eq!(refusal.session_id, "s2");
    assert_eq!(
        refusal.returned, 0,
        "a failed contract outcome is a standing refusal"
    );

    let stopped = jobs.failed("s3", 4, format_only(), Some("the contract run stopped"));
    apply(&jobs, &stopped);
    let refusal = jobs.refusal();
    assert_eq!(refusal.session_id, "s3");
    assert_eq!(
        refusal.returned, 0,
        "a result with an error is a standing refusal"
    );
}

#[test]
fn third_static_refusal_files_the_dispute() {
    assert_eq!(STATIC_RETURN_LIMIT, 3);
    let jobs = Jobs::create();
    let first = jobs.failed("s1", 1, format_only(), None);
    apply(&jobs, &first);
    apply(&jobs, &first);
    assert_eq!(
        jobs.refusal().returned,
        1,
        "one job applied twice counts once"
    );
    apply(&jobs, &jobs.failed("s1", 2, format_only(), None));
    assert_eq!(jobs.refusal().returned, 2);
    apply(&jobs, &jobs.failed("s1", 3, format_only(), None));
    let third = jobs.refusal();
    assert_eq!(
        (third.session_id.as_str(), third.returned),
        ("s1", 0),
        "the third is filed"
    );

    let other = jobs.failed("s2", 4, format_only(), None);
    apply(&jobs, &other);
    let fresh = jobs.refusal();
    assert_eq!(
        (fresh.session_id.as_str(), fresh.returned),
        ("s2", 1),
        "a new session starts at 1"
    );
    direct_records_count_once(&jobs, &other);
}

/// `record_static_refusal` over the job `applied` already recorded for s2.
fn direct_records_count_once(jobs: &Jobs, applied: &JobRecord) {
    let problems = vec![format!("format: {FORMAT_SUMMARY}")];
    let record = |order: u64, id: String| {
        let origin = JobRefusalOrigin {
            job_id: id,
            submission_order: order,
        };
        record_static_refusal(&jobs.work, STAGE, "s2", origin, &problems).expect("record")
    };
    for _ in 0..2 {
        assert_eq!(
            record(4, applied.id.clone()),
            StaticRefusal::Returned { count: 1 }
        );
    }
    assert_eq!(
        record(2, job_id(40)),
        StaticRefusal::Superseded,
        "an older job loses"
    );
    assert_eq!(jobs.refusal().returned, 1);
    assert_eq!(record(5, job_id(5)), StaticRefusal::Returned { count: 2 });
    assert_eq!(record(6, job_id(6)), StaticRefusal::Filed);
    assert_eq!(
        jobs.refusal().returned,
        0,
        "the filed record is a standing refusal"
    );
}
