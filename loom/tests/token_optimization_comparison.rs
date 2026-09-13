use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};

use serde::Deserialize;
use serde_json::{json, Value};

const LOOM: &str = env!("CARGO_BIN_EXE_loom");
const FIXTURES: &str = "tests/fixtures/token_optimization/comparison";
static SCRATCH_SEQUENCE: AtomicU64 = AtomicU64::new(0);

#[derive(Debug, Deserialize)]
struct MatrixCase {
    name: String,
    mutation: String,
    exit_code: i32,
    verdict: String,
    token_proxy_verdict: String,
    subscription_verdict: String,
    reason_codes: Vec<String>,
}

#[test]
fn comparison_defaults_support_valid_improvement() {
    let scratch = scratch_dir("valid");
    let artifact = fixture_path("valid_improvement.json");

    let output = run_compare(&scratch, &artifact, &[]);
    let report = decoded_stdout(&output);

    assert_eq!(output.status.code(), Some(0));
    assert_report(
        &report,
        "supported-candidate",
        "supported-candidate",
        "supported-candidate",
        &[],
    );
    assert_eq!(report["schema_version"], 1);
    assert_eq!(report["claim_scope"], "finite-fixture-set");
    assert_eq!(report["pairs"][0]["work_unit_id"], "success-case");
    assert_eq!(
        report["pairs"][0]["baseline_provider_tokens"]["claude"]["input_tokens"],
        100
    );
    assert_eq!(
        report["pairs"][0]["candidate_provider_tokens"]["claude"]["input_tokens"],
        90
    );
    assert_eq!(
        report["pairs"][0]["quota_comparisons"],
        json!([{
            "provider": "claude",
            "window": "five-hour",
            "baseline_consumption_percent": 4.0,
            "candidate_consumption_percent": 2.0,
            "combined_uncertainty_percent": 0.5
        }])
    );
}

#[test]
fn comparison_rejects_every_explicit_selection_flag() {
    let scratch = scratch_dir("conflicts");
    let artifact = fixture_path("valid_improvement.json");
    let scratch_text = scratch.to_string_lossy().into_owned();
    let cases = selection_flag_cases(&scratch_text);

    for args in cases {
        let output = run_compare(&scratch, &artifact, &args);
        assert_eq!(output.status.code(), Some(2), "args={args:?}");
        assert_eq!(
            decoded_stdout(&output),
            error_report("comparison-selection-conflict"),
            "args={args:?}"
        );
    }
}

#[test]
fn comparison_fixture_matrix_has_exact_outcomes() {
    let scratch = scratch_dir("matrix");
    let base: Value =
        serde_json::from_slice(&fs::read(fixture_path("valid_improvement.json")).unwrap()).unwrap();
    let cases: Vec<MatrixCase> =
        serde_json::from_slice(&fs::read(fixture_path("matrix.json")).unwrap()).unwrap();

    for case in cases {
        let artifact = mutate(base.clone(), &case.mutation);
        let path = scratch.join(format!("{}.json", case.name));
        fs::write(&path, serde_json::to_vec_pretty(&artifact).unwrap()).unwrap();
        let output = run_compare(&scratch, &path, &[]);
        let report = decoded_stdout(&output);
        assert_eq!(
            output.status.code(),
            Some(case.exit_code),
            "case={}",
            case.name
        );
        let reason_codes = case
            .reason_codes
            .iter()
            .map(String::as_str)
            .collect::<Vec<_>>();
        assert_report(
            &report,
            &case.verdict,
            &case.token_proxy_verdict,
            &case.subscription_verdict,
            &reason_codes,
        );
        assert_pair_evidence(&report, &artifact, &case.name);
    }
}

#[test]
fn malformed_and_oversized_inputs_emit_json_reason_codes() {
    let scratch = scratch_dir("invalid");
    let malformed = run_compare(&scratch, &fixture_path("malformed_schema.json"), &[]);
    let malformed_report = decoded_stdout(&malformed);
    assert_eq!(malformed.status.code(), Some(2));
    assert_eq!(malformed_report, error_report("malformed-input"));

    let oversized_path = create_oversized(&scratch);
    let oversized = run_compare(&scratch, &oversized_path, &[]);
    let oversized_report = decoded_stdout(&oversized);
    assert_eq!(oversized.status.code(), Some(2));
    assert_eq!(oversized_report, error_report("input-too-large"));
}

fn error_report(reason: &str) -> Value {
    json!({
        "schema_version": 1,
        "verdict": "inconclusive",
        "token_proxy_verdict": "inconclusive",
        "subscription_verdict": "inconclusive",
        "reason_codes": [reason],
        "pairs": [],
        "claim_scope": "finite-fixture-set"
    })
}

fn fixture_path(name: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join(FIXTURES)
        .join(name)
}

fn scratch_dir(label: &str) -> PathBuf {
    let sequence = SCRATCH_SEQUENCE.fetch_add(1, Ordering::Relaxed);
    let timestamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("system clock before Unix epoch")
        .as_nanos();
    let root = std::env::temp_dir().join(format!(
        "loom-comparison-{}-{}-{}-{}",
        std::process::id(),
        label,
        timestamp,
        sequence
    ));
    fs::create_dir_all(&root).expect("create unique comparison scratch directory");
    fs::write(root.join("config.toml"), "[update]\ncheck = false\n").unwrap();
    root
}

fn run_compare(scratch: &Path, artifact: &Path, extra: &[String]) -> Output {
    let mut command = Command::new(LOOM);
    command.args(["usage", "--compare"]);
    command.arg(artifact);
    command.args(extra);
    command.arg("--json");
    command.env("HOME", scratch).env("LOOM_HOME", scratch);
    command.current_dir(scratch);
    command.output().expect("execute loom usage comparison")
}

fn decoded_stdout(output: &Output) -> Value {
    serde_json::from_slice(&output.stdout).unwrap_or_else(|error| {
        panic!(
            "comparison stdout must be JSON: {error}; stderr={}",
            String::from_utf8_lossy(&output.stderr)
        )
    })
}

fn assert_report(
    report: &Value,
    verdict: &str,
    token_proxy: &str,
    subscription: &str,
    reasons: &[&str],
) {
    let expected_reasons = reasons
        .iter()
        .map(|reason| json!(reason))
        .collect::<Vec<_>>();
    assert_eq!(report["verdict"], verdict);
    assert_eq!(report["token_proxy_verdict"], token_proxy);
    assert_eq!(report["subscription_verdict"], subscription);
    assert_eq!(report["reason_codes"], Value::Array(expected_reasons));
}

fn assert_pair_evidence(report: &Value, artifact: &Value, case: &str) {
    assert_eq!(report["schema_version"], 1, "case={case}");
    assert_eq!(report["claim_scope"], "finite-fixture-set", "case={case}");
    assert_eq!(
        report["pairs"].as_array().unwrap().len(),
        artifact["pairs"].as_array().unwrap().len()
    );
    for (result, source) in report["pairs"]
        .as_array()
        .unwrap()
        .iter()
        .zip(artifact["pairs"].as_array().unwrap())
    {
        assert_eq!(
            result["work_unit_id"], source["work_unit_id"],
            "case={case}"
        );
        assert_eq!(
            result["baseline_provider_tokens"], source["baseline"]["provider_tokens"],
            "case={case}"
        );
        assert_eq!(
            result["candidate_provider_tokens"], source["candidate"]["provider_tokens"],
            "case={case}"
        );
    }
    if case == "averaged-regression" {
        assert_eq!(report["pairs"][0]["verdict"], "supported-candidate");
        assert_eq!(report["pairs"][1]["verdict"], "rejected");
    } else {
        assert_eq!(report["pairs"][0]["verdict"], report["verdict"]);
        assert_eq!(
            report["pairs"][0]["token_proxy_verdict"],
            report["token_proxy_verdict"]
        );
        assert_eq!(
            report["pairs"][0]["subscription_verdict"],
            report["subscription_verdict"]
        );
        assert_eq!(report["pairs"][0]["reason_codes"], report["reason_codes"]);
    }
    if case == "mixed-window-regression" {
        assert_eq!(report["subscription_verdict"], "rejected");
        assert!(report["reason_codes"]
            .as_array()
            .unwrap()
            .contains(&json!("subscription-consumption-regression")));
    }
}

fn selection_flag_cases(scratch: &str) -> Vec<Vec<String>> {
    [
        vec!["--since", "1d"],
        vec!["--until", "2026-09-12T00:00:00Z"],
        vec!["--provider", "codex"],
        vec!["--claude-root", scratch],
        vec!["--codex-root", scratch],
        vec!["--receipts-root", scratch],
        vec!["--forward-receipts-root", scratch],
        vec!["--project", scratch],
        vec!["--all"],
        vec!["--stage", "stage-a"],
        vec!["--plan", "plan-a"],
        vec!["--windows", "5h"],
    ]
    .into_iter()
    .map(|args| args.into_iter().map(str::to_owned).collect())
    .collect()
}

fn mutate(mut artifact: Value, mutation: &str) -> Value {
    match mutation {
        "mixed-window-regression" => return fixture_json("mixed_window_regression.json"),
        "cross-provider-transfer" => mutate_cross_provider(&mut artifact),
        "output-increase-input-decrease" => mutate_token_tradeoff(&mut artifact),
        "reset-crossing" => {
            candidate(&mut artifact)["quota_observations"][0]["reset_id"] = json!("reset-b")
        }
        "coarse-quota" => mutate_coarse_quota(&mut artifact),
        "missing-telemetry" => mutate_missing_telemetry(&mut artifact),
        "model-effort-mismatch" => mutate_assignment(&mut artifact),
        "failed-review" => {
            candidate(&mut artifact)["reviewed_dimensions"][0]["verdict"] = json!("failed")
        }
        "omitted-requirement" => {
            candidate(&mut artifact)["omitted_requirements"] = json!(["required-doc"])
        }
        "slower-critical-path" => candidate(&mut artifact)["critical_path_ms"] = json!(1001),
        "slower-notification" => candidate(&mut artifact)["notification_latency_ms"] = json!(101),
        "incomparable-workload" => candidate(&mut artifact)["workload_id"] = json!("workload-b"),
        "duplicate-run-id" => candidate(&mut artifact)["run_id"] = json!("base-success"),
        "averaged-regression" => mutate_averaged_regression(&mut artifact),
        unknown => panic!("unknown matrix mutation: {unknown}"),
    }
    artifact
}

fn fixture_json(name: &str) -> Value {
    serde_json::from_slice(&fs::read(fixture_path(name)).unwrap()).unwrap()
}

fn candidate(artifact: &mut Value) -> &mut Value {
    &mut artifact["pairs"][0]["candidate"]
}

fn mutate_cross_provider(artifact: &mut Value) {
    artifact["intervention"]["intervention_id"] = json!("provider-transfer");
    artifact["intervention"]["changed_dimensions"] = json!([
        "implementation-revision",
        "provider-assignment",
        "model-assignment"
    ]);
    let assignment = &mut candidate(artifact)["assignments"][0];
    assignment["provider"] = json!("codex");
    assignment["model"] = json!("gpt-5.6-sol");
    candidate(artifact)["provider_tokens"] = json!({
        "codex": {
            "input_tokens": 90,
            "fresh_input_tokens": 70,
            "cache_creation_input_tokens": null,
            "cache_read_input_tokens": 20,
            "cache_write_5m_input_tokens": null,
            "cache_write_1h_input_tokens": null,
            "output_tokens": 35,
            "thinking_output_tokens": 8,
            "resident_input_tokens": 90,
            "total_tokens": 125
        }
    });
    artifact["pairs"][0]["baseline"]["quota_observations"] = Value::Null;
    candidate(artifact)["quota_observations"] = Value::Null;
}

fn mutate_token_tradeoff(artifact: &mut Value) {
    let tokens = &mut candidate(artifact)["provider_tokens"]["claude"];
    tokens["input_tokens"] = json!(80);
    tokens["fresh_input_tokens"] = json!(80);
    tokens["output_tokens"] = json!(50);
    tokens["resident_input_tokens"] = json!(120);
    artifact["pairs"][0]["baseline"]["quota_observations"] = Value::Null;
    candidate(artifact)["quota_observations"] = Value::Null;
}

fn mutate_missing_telemetry(artifact: &mut Value) {
    let run = candidate(artifact);
    run["critical_path_ms"] = Value::Null;
    run["accounting_complete"] = json!(false);
    run["accounting_provenance"] = json!("partial");
    run["quota_observations"] = Value::Null;
}

fn mutate_coarse_quota(artifact: &mut Value) {
    artifact["pairs"][0]["baseline"]["quota_observations"][0]["precision_percent"] = json!(1.0);
    let quota = &mut candidate(artifact)["quota_observations"][0];
    quota["used_percent_end"] = json!(24.0);
    quota["precision_percent"] = json!(1.0);
}

fn mutate_assignment(artifact: &mut Value) {
    let assignment = &mut candidate(artifact)["assignments"][0];
    assignment["model"] = json!("claude-sonnet");
    assignment["effort"] = json!("medium");
}

fn mutate_averaged_regression(artifact: &mut Value) {
    candidate(artifact)["notification_latency_ms"] = json!(80);
    let mut regression = artifact["pairs"][0].clone();
    regression["work_unit_id"] = json!("regression-case");
    regression["baseline"]["run_id"] = json!("base-regression");
    regression["candidate"]["run_id"] = json!("candidate-regression");
    regression["candidate"]["notification_latency_ms"] = json!(110);
    artifact["pairs"].as_array_mut().unwrap().push(regression);
}

fn create_oversized(scratch: &Path) -> PathBuf {
    let path = scratch.join("oversized.json");
    let mut bytes = fs::read(fixture_path("oversized_seed.json")).unwrap();
    bytes.resize(16 * 1024 * 1024 + 1, b' ');
    fs::write(&path, bytes).unwrap();
    path
}
