//! Captured Go package results reach the live acceptance gate.
use crate::models::stage::{AcceptanceCriterion, Stage};
use crate::testrun::command::shell_quote;
use crate::testrun::{registry, RunOutput};
use crate::verify::criteria::{run_acceptance, AcceptanceResult};
use tempfile::TempDir;

const EMPTY: &str = "?\texample.invalid/astra/empty\t[no test files]\n";
const EMPTY_JSON: &str = concat!(
    "{\"Action\":\"start\",\"Package\":\"example.invalid/astra/empty\"}\n",
    "{\"Action\":\"output\",\"Package\":\"example.invalid/astra/empty\",\"Output\":\"?\\texample.invalid/astra/empty\\t[no test files]\\n\"}\n",
    "{\"Action\":\"skip\",\"Package\":\"example.invalid/astra/empty\"}\n",
);

fn go_stage(output: &str, prefix: &str) -> Stage {
    let mut stage = Stage::new("go-evidence".into(), None);
    stage.plan_version = 2;
    // Keep real shell execution without launching a newly-created executable.
    stage.setup.push(format!(
        "go() {{ {prefix}printf '%s' {}; }}",
        shell_quote(output)
    ));
    stage.add_acceptance_criterion(AcceptanceCriterion::Simple("go test ./...".into()));
    stage
}

#[test]
fn empty_go_packages_text_and_json_fail_live_acceptance() {
    for output in [EMPTY, EMPTY_JSON] {
        let dir = TempDir::new().unwrap();
        let stage = go_stage(output, "");
        let result = run_acceptance(&stage, Some(dir.path())).unwrap();
        let AcceptanceResult::Failed { results, failures } = result else {
            panic!("empty Go package passed");
        };
        assert_eq!(results[0].exit_code, Some(0));
        assert!(!results[0].timed_out);
        assert!(results[0].output_complete);
        assert!(
            failures
                .iter()
                .any(|failure| failure.contains("selected zero tests (go-test)")),
            "{failures:?}"
        );
    }
}

#[test]
fn first_live_run_rejects_zero_test_summary_beyond_capture_cap() {
    let dir = TempDir::new().unwrap();
    let stage = go_stage(EMPTY, "head -c 10485761 /dev/zero\n");
    let result = run_acceptance(&stage, Some(dir.path())).unwrap();
    let AcceptanceResult::Failed { results, failures } = result else {
        panic!("a summary beyond the capture limit must not certify acceptance");
    };
    assert_eq!(results[0].exit_code, Some(0));
    assert!(!results[0].timed_out);
    assert!(!results[0].output_complete);
    assert!(results[0]
        .stdout
        .contains(crate::verify::criteria::executor::OUTPUT_TRUNCATED_MARKER));
    assert!(
        failures
            .iter()
            .any(|failure| failure.contains("output capture incomplete")),
        "{failures:?}"
    );
}

#[test]
fn mixed_go_packages_preserve_execution_evidence() {
    let dir = TempDir::new().unwrap();
    for output in [
        format!("{EMPTY}--- PASS: TestReal (0.00s)\nPASS\nok\texample.invalid/tested\t0.001s\n"),
        format!("{EMPTY_JSON}{{\"Action\":\"pass\",\"Package\":\"example.invalid/tested\",\"Test\":\"TestReal\"}}\n"),
    ] {
        let adapter = registry::recognize("go test ./...", dir.path()).unwrap();
        let summary = adapter.parse(&RunOutput { stdout: &output, stderr: "", exit_code: Some(0) });
        assert_eq!(summary.executed, Some(1));
        let stage = go_stage(&output, "");
        assert!(run_acceptance(&stage, Some(dir.path())).unwrap().all_passed());
    }
}

#[test]
fn nonverbose_tested_package_is_not_misreported_as_zero() {
    let dir = TempDir::new().unwrap();
    let output = format!("{EMPTY}ok\texample.invalid/tested\t0.001s\n");
    let adapter = registry::recognize("go test ./...", dir.path()).unwrap();
    assert_eq!(
        adapter
            .parse(&RunOutput {
                stdout: &output,
                stderr: "",
                exit_code: Some(0)
            })
            .executed,
        None
    );
}
