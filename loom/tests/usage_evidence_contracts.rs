//! Contracts for stage evidence-and-replay: the declared net-total and latency
//! rules of `usage --compare`, quota jitter tolerance, and the RunEvidence
//! producer `usage --run-evidence`.

#[path = "integration/helpers.rs"]
#[allow(dead_code)]
mod helpers;
#[path = "usage_evidence_contracts/support.rs"]
mod support;

use serde_json::{json, Value};
use support::*;

const SIDES: [&str; 2] = ["baseline", "candidate"];

#[test]
fn quota_jitter_reads_as_one_interval() {
    let scratch = Scratch::new();
    let (claude, project) = (scratch.path("claude"), scratch.project("p"));
    write_jittered_quota_history(&project);
    let output = scratch.run(&[
        "usage",
        "--project",
        path_str(&project),
        "--claude-root",
        path_str(&claude),
        "--since",
        "2026-10-14",
        "--until",
        UNTIL,
        "--json",
    ]);
    assert!(output.status.success(), "usage failed: {output:?}");
    let report = stdout_json(&output);
    let claude_history = report["provider_ledger"]["quota_history"]["providers"]
        .as_array()
        .unwrap_or_else(|| panic!("no quota_history.providers in {report}"))
        .iter()
        .find(|entry| entry["provider"] == "claude")
        .unwrap_or_else(|| panic!("no claude quota history in {report}"));
    let continuity: Vec<&str> = claude_history["points"]
        .as_array()
        .expect("claude quota points")
        .iter()
        .map(|point| point["continuity"].as_str().unwrap_or("<absent>"))
        .collect();
    let mut expected = vec!["initial"];
    expected.extend(["same-reset"; 5]);
    assert_eq!(
        continuity, expected,
        "jittered resets_at must read as one interval"
    );
}

#[test]
fn net_total_supports_a_known_reduction() {
    let base = base_artifact();
    let pairs = [30, 28, 32, 29]
        .iter()
        .enumerate()
        .map(|(index, output)| {
            let mut pair = numbered_pair(&base, index);
            pair["candidate"]["provider_tokens"]["claude"]["output_tokens"] = json!(output);
            for side in SIDES {
                pair[side]["quota_observations"] = Value::Null;
            }
            pair
        })
        .collect();
    let artifact = with_pairs(&base, pairs, json!({"token_rule": "net-total"}));
    let report = stdout_json(&Scratch::new().compare(&artifact));
    let net = &report["net_total"];
    assert_eq!(net["verdict"], "supported-candidate", "report: {report}");
    let providers = net["providers"].as_array().expect("net_total.providers");
    assert_eq!(providers.len(), 1, "report: {report}");
    assert_eq!(providers[0]["provider"], "claude");
    assert_eq!(providers[0]["pairs"], 4);
    assert_eq!(providers[0]["classification"], "reduction");
    assert_eq!(report["verdict"], "supported-candidate", "report: {report}");
}

#[test]
fn net_total_reads_an_aa_set_inconclusive() {
    let base = base_artifact();
    let pairs = (0..4)
        .map(|index| {
            let mut pair = numbered_pair(&base, index);
            pair["candidate"] = pair["baseline"].clone();
            pair["candidate"]["run_id"] = json!(format!("cand-{index}"));
            pair
        })
        .collect();
    let fields = json!({
        "candidate_revision": "base-rev",
        "changed_dimensions": ["implementation-revision"],
        "token_rule": "net-total"
    });
    let artifact = with_pairs(&base, pairs, fields);
    let output = Scratch::new().compare(&artifact);
    let report = stdout_json(&output);
    assert_eq!(output.status.code(), Some(2), "report: {report}");
    assert_eq!(
        report["net_total"]["verdict"], "inconclusive",
        "report: {report}"
    );
    assert!(
        has_code(
            &report["net_total"]["reason_codes"],
            "net-total-within-noise"
        ),
        "report: {report}"
    );
}

#[test]
fn single_pair_reads_insufficient_pairs() {
    let base = base_artifact();
    let pairs = vec![base["pairs"][0].clone()];
    let artifact = with_pairs(&base, pairs, json!({"token_rule": "net-total"}));
    let output = Scratch::new().compare(&artifact);
    let report = stdout_json(&output);
    assert_eq!(output.status.code(), Some(2), "report: {report}");
    assert_eq!(
        report["net_total"]["verdict"], "inconclusive",
        "report: {report}"
    );
    assert!(
        has_code(&report["net_total"]["reason_codes"], "insufficient-pairs"),
        "report: {report}"
    );
}

#[test]
fn declared_latency_tolerance_judges_the_mean() {
    let base = base_artifact();
    let pairs = [1001, 997]
        .iter()
        .enumerate()
        .map(|(index, critical_path)| {
            let mut pair = numbered_pair(&base, index);
            pair["baseline"]["critical_path_ms"] = json!(1000);
            pair["candidate"]["critical_path_ms"] = json!(critical_path);
            pair
        })
        .collect();
    let artifact = with_pairs(&base, pairs, json!({"latency_tolerance_ms": 0}));
    let report = stdout_json(&Scratch::new().compare(&artifact));
    let latency = &report["latency"];
    assert_eq!(
        latency["verdict"], "supported-candidate",
        "report: {report}"
    );
    assert_eq!(
        latency["critical_path"]["mean_increase_ms"].as_f64(),
        Some(-1.0),
        "report: {report}"
    );
    assert!(
        !has_code(&report["reason_codes"], "critical-path-regression"),
        "report: {report}"
    );
    assert_eq!(report["verdict"], "supported-candidate", "report: {report}");
}

#[test]
fn compare_tolerates_quota_jitter_and_interval() {
    let mut artifact = base_artifact();
    let quota = &mut artifact["pairs"][0]["candidate"]["quota_observations"][0];
    quota["resets_at"] = json!(2001);
    quota["observed_end_at"] = json!(1400);
    let report = stdout_json(&Scratch::new().compare(&artifact));
    let codes = &report["reason_codes"];
    assert!(!has_code(codes, "quota-reset-crossing"), "report: {report}");
    assert!(
        !has_code(codes, "quota-interval-mismatch"),
        "report: {report}"
    );
    assert_eq!(report["verdict"], "supported-candidate", "report: {report}");
}

#[test]
fn run_evidence_carries_the_measured_run() {
    let scratch = Scratch::new();
    let (claude, codex) = (scratch.path("claude"), scratch.path("codex"));
    let project = producer_project(&scratch, "pa", &claude, 7);
    let output = scratch.run_evidence(&declaration("run-a", "base-rev"), &project, &claude, &codex);
    assert_eq!(output.status.code(), Some(0), "producer: {output:?}");
    let run = stdout_json(&output);
    assert_eq!(run["run_id"], "run-a", "run: {run}");
    assert_eq!(run["retry_count"], 1, "run: {run}");
    assert_eq!(run["fix_count"], 3, "run: {run}");
    assert_eq!(run["critical_path_ms"], 1_080_000, "run: {run}");
    assert_eq!(run["notification_latency_ms"], Value::Null, "run: {run}");
    assert_eq!(
        run["unresolved_findings"],
        json!([{"finding_id": "stage-a:F-1-1", "severity": "high"}]),
        "run: {run}"
    );
    assert_eq!(
        run["provider_tokens"]["claude"]["output_tokens"], 7,
        "run: {run}"
    );
    assert_eq!(run["accounting_provenance"], "measured", "run: {run}");
    assert_eq!(run["accounting_complete"], true, "run: {run}");
}

#[test]
fn run_evidence_refuses_an_undeclared_provider() {
    let scratch = Scratch::new();
    let (claude, codex) = (scratch.path("claude"), scratch.path("codex"));
    let project = producer_project(&scratch, "pa", &claude, 7);
    codex_evidence(&project, &codex);
    let output = scratch.run_evidence(&declaration("run-a", "base-rev"), &project, &claude, &codex);
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(!output.status.success(), "producer must refuse: {output:?}");
    assert!(output.stdout.is_empty(), "stdout must be empty: {output:?}");
    assert!(
        stderr.contains("undeclared provider codex: 1 in-scope usage rows"),
        "stderr: {stderr}"
    );
}

#[test]
fn run_evidence_round_trips_through_compare() {
    let scratch = Scratch::new();
    let (claude, codex) = (scratch.path("claude"), scratch.path("codex"));
    let project_a = producer_project(&scratch, "pa", &claude, 7);
    let project_b = producer_project(&scratch, "pb", &claude, 5);
    let produce = |decl: Value, project: &std::path::Path| {
        let output = scratch.run_evidence(&decl, project, &claude, &codex);
        assert_eq!(output.status.code(), Some(0), "producer: {output:?}");
        stdout_json(&output)
    };
    let run_a = produce(declaration("run-a", "base-rev"), &project_a);
    let run_b = produce(declaration("run-b", "cand-rev"), &project_b);
    let artifact = json!({
        "schema_version": 1,
        "intervention": {
            "intervention_id": "rt",
            "baseline_revision": "base-rev",
            "candidate_revision": "cand-rev",
            "changed_dimensions": ["implementation-revision"]
        },
        "pairs": [{"work_unit_id": "unit-1", "baseline": run_a, "candidate": run_b}]
    });
    let output = scratch.compare(&artifact);
    let report = stdout_json(&output);
    assert_eq!(output.status.code(), Some(2), "report: {report}");
    assert!(
        !has_code(&report["reason_codes"], "malformed-input"),
        "report: {report}"
    );
    assert!(
        !has_code(&report["reason_codes"], "incomparable-environment"),
        "report: {report}"
    );
    let pairs = report["pairs"].as_array().expect("report pairs");
    assert_eq!(pairs.len(), 1, "report: {report}");
    assert_eq!(
        pairs[0]["baseline_provider_tokens"],
        run_a["provider_tokens"]
    );
    assert_eq!(
        pairs[0]["candidate_provider_tokens"],
        run_b["provider_tokens"]
    );
}
