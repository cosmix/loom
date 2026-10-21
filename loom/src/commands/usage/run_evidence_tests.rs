use std::collections::BTreeSet;

use chrono::{DateTime, Utc};
use tempfile::TempDir;

use super::*;
use crate::commands::usage::comparison_schema::FindingSeverity;
use crate::commands::usage::provider_types::{
    Attribution, ProvenanceStatus, SourceKind, StageAttributionState, TimeRangeView,
    PROVIDER_LEDGER_SCHEMA_VERSION,
};
use crate::commands::usage::run_evidence_stages::stage_evidence;
use crate::models::stage::{Stage, StageStatus};
use crate::verify::review::report::Finding;
use crate::verify::review::store::{write_round, CarriedFinding, RecordedFinding, ReviewRound};
use crate::verify::review::verdict_records::append_carried;
use crate::verify::transitions::create_stage;

fn at(text: &str) -> DateTime<Utc> {
    text.parse().unwrap()
}

fn range(since: &str, until: &str) -> TimeRange {
    TimeRange {
        since: at(since),
        until: Some(at(until)),
    }
}

fn day() -> TimeRange {
    range("2026-10-14T00:00:00Z", "2026-10-15T00:00:00Z")
}

fn stage(id: &str, deps: &[&str], start: &str, end: &str, retries: u32, fixes: u32) -> Stage {
    Stage {
        id: id.to_string(),
        name: id.to_string(),
        status: StageStatus::Completed,
        dependencies: deps.iter().map(|dep| dep.to_string()).collect(),
        started_at: Some(at(start)),
        completed_at: Some(at(end)),
        retry_count: retries,
        fix_attempts: fixes,
        ..Stage::default()
    }
}

fn two_stages(dir: &TempDir) {
    let a = stage(
        "stage-a",
        &[],
        "2026-10-14T06:00:00Z",
        "2026-10-14T06:10:00Z",
        1,
        2,
    );
    let b = stage(
        "stage-b",
        &["stage-a"],
        "2026-10-14T06:12:00Z",
        "2026-10-14T06:20:00Z",
        0,
        1,
    );
    create_stage(&a, dir.path()).unwrap();
    create_stage(&b, dir.path()).unwrap();
}

fn round_with(id: &str, severity: &str) -> ReviewRound {
    let finding = Finding {
        severity: severity.to_string(),
        file: "src/lib.rs".to_string(),
        line: 1,
        claim: "claim".to_string(),
        scenario: Some("scenario".to_string()),
        rule: None,
    };
    ReviewRound {
        version: 1,
        round: 1,
        agent_id: "agent".to_string(),
        harvested_at: at("2026-10-14T06:09:00Z"),
        fingerprint: format!("sha256:{}", "a".repeat(64)),
        files: Default::default(),
        malformed: None,
        findings: vec![RecordedFinding {
            id: id.to_string(),
            finding,
        }],
        resolved: Vec::new(),
        unresolved: Vec::new(),
        suggestion_memory_ids: Vec::new(),
    }
}

#[test]
fn critical_path_sums_the_longest_chain() {
    let dir = TempDir::new().unwrap();
    two_stages(&dir);
    let evidence = stage_evidence(dir.path(), &day()).unwrap();
    assert_eq!(evidence.critical_path_ms, Some(1_080_000));
    assert_eq!(evidence.retry_count, 1);
    assert_eq!(evidence.fix_count, 3);
}

#[test]
fn unfinished_or_absent_stages_have_no_critical_path() {
    let dir = TempDir::new().unwrap();
    assert_eq!(
        stage_evidence(dir.path(), &day()).unwrap().critical_path_ms,
        None
    );
    let mut running = stage(
        "stage-a",
        &[],
        "2026-10-14T06:00:00Z",
        "2026-10-14T06:10:00Z",
        0,
        0,
    );
    running.status = StageStatus::Executing;
    create_stage(&running, dir.path()).unwrap();
    assert_eq!(
        stage_evidence(dir.path(), &day()).unwrap().critical_path_ms,
        None
    );
}

#[test]
fn a_dependency_outside_the_run_has_no_critical_path() {
    let dir = TempDir::new().unwrap();
    let earlier = stage(
        "stage-z",
        &[],
        "2026-10-13T06:00:00Z",
        "2026-10-13T06:10:00Z",
        5,
        4,
    );
    let b = stage(
        "stage-b",
        &["stage-z"],
        "2026-10-14T06:12:00Z",
        "2026-10-14T06:20:00Z",
        0,
        1,
    );
    create_stage(&earlier, dir.path()).unwrap();
    create_stage(&b, dir.path()).unwrap();
    let evidence = stage_evidence(dir.path(), &day()).unwrap();
    assert_eq!(evidence.critical_path_ms, None);
    assert_eq!((evidence.retry_count, evidence.fix_count), (0, 1));
}

#[test]
fn a_stage_before_the_window_is_left_out_of_every_count() {
    let dir = TempDir::new().unwrap();
    two_stages(&dir);
    let earlier = stage(
        "stage-z",
        &[],
        "2026-10-13T06:00:00Z",
        "2026-10-13T06:10:00Z",
        5,
        4,
    );
    create_stage(&earlier, dir.path()).unwrap();
    write_round(dir.path(), "stage-z", &round_with("F-1-1", "major")).unwrap();
    let evidence = stage_evidence(dir.path(), &day()).unwrap();
    assert_eq!((evidence.retry_count, evidence.fix_count), (1, 3));
    assert_eq!(evidence.critical_path_ms, Some(1_080_000));
    assert!(evidence.unresolved_findings.is_empty());
}

#[test]
fn a_stage_straddling_the_window_aborts() {
    let dir = TempDir::new().unwrap();
    let early = stage(
        "stage-s",
        &[],
        "2026-10-13T23:50:00Z",
        "2026-10-14T00:10:00Z",
        0,
        0,
    );
    create_stage(&early, dir.path()).unwrap();
    let error = stage_evidence(dir.path(), &day())
        .err()
        .unwrap()
        .to_string();
    assert!(
        error.contains("stage-s") && error.contains("--since"),
        "{error}"
    );

    let dir = TempDir::new().unwrap();
    let late = stage(
        "stage-l",
        &[],
        "2026-10-14T23:50:00Z",
        "2026-10-15T00:10:00Z",
        0,
        0,
    );
    create_stage(&late, dir.path()).unwrap();
    let error = stage_evidence(dir.path(), &day())
        .err()
        .unwrap()
        .to_string();
    assert!(
        error.contains("stage-l") && error.contains("--until"),
        "{error}"
    );
}

#[test]
fn findings_map_ids_and_severities() {
    let cases = [
        (
            "F-1-1",
            "critical",
            "stage-b:F-1-1",
            FindingSeverity::Critical,
        ),
        ("F-1-1", "major", "stage-b:F-1-1", FindingSeverity::High),
        ("F-1-1", "minor", "stage-b:F-1-1", FindingSeverity::Medium),
        (
            "F-1-1",
            "unspecified",
            "stage-b:F-1-1",
            FindingSeverity::Low,
        ),
    ];
    for (id, severity, expected_id, expected) in cases {
        // Findings accumulate across rounds, so each case gets its own work dir.
        let dir = TempDir::new().unwrap();
        let b = stage(
            "stage-b",
            &[],
            "2026-10-14T06:12:00Z",
            "2026-10-14T06:20:00Z",
            0,
            0,
        );
        create_stage(&b, dir.path()).unwrap();
        write_round(dir.path(), "stage-b", &round_with(id, severity)).unwrap();
        let found = stage_evidence(dir.path(), &day())
            .unwrap()
            .unresolved_findings
            .into_iter()
            .map(|finding| (finding.finding_id, finding.severity))
            .collect::<Vec<_>>();
        assert_eq!(found, vec![(expected_id.to_string(), expected)]);
    }
}

#[test]
fn carried_finding_ids_have_no_slash() {
    let dir = TempDir::new().unwrap();
    let b = stage(
        "stage-b",
        &[],
        "2026-10-14T06:12:00Z",
        "2026-10-14T06:20:00Z",
        0,
        0,
    );
    create_stage(&b, dir.path()).unwrap();
    let mut finding = round_with("F-1-1", "major").findings.remove(0).finding;
    finding.claim = "carried".to_string();
    let carried = CarriedFinding {
        id: "stage-x/F-1-2".to_string(),
        origin_stage: "stage-x".to_string(),
        finding,
        dispute: 1,
    };
    append_carried(dir.path(), "stage-b", &[carried]).unwrap();
    let found = stage_evidence(dir.path(), &day())
        .unwrap()
        .unresolved_findings;
    assert_eq!(found.len(), 1);
    assert_eq!(found[0].finding_id, "stage-b:stage-x:F-1-2");
    assert_eq!(found[0].severity, FindingSeverity::High);
}

fn row(provider: Provider, provenance: ProvenanceStatus, output: Option<u64>) -> NormalizedEvent {
    NormalizedEvent {
        provider,
        event_timestamp: at("2026-10-14T06:05:00Z"),
        source_kind: SourceKind::ClaudeTranscript,
        request_id: None,
        source_ordinal: 1,
        model: "m".to_owned(),
        project: "p".to_owned(),
        tokens: ProviderTokenVector {
            output_tokens: output,
            ..ProviderTokenVector::default()
        },
        attribution: Attribution {
            scope: "main".to_owned(),
            role: None,
            stage_id: None,
            loom_session_id: None,
            stage_state: StageAttributionState::Unknown,
        },
        provenance,
        observation_count: 1,
        changed_usage_fields: 0,
        first_observed_in_range: true,
        true_fresh_start: None,
        tool_names: Vec::new(),
        codex_thread_id: None,
        codex_thread_conflict: false,
        forward_candidate: false,
        forward_receipt: None,
    }
}

fn ledger(rows: Vec<NormalizedEvent>) -> ProviderLedger {
    ProviderLedger {
        schema_version: PROVIDER_LEDGER_SCHEMA_VERSION,
        range: TimeRangeView {
            since: at("2026-10-14T00:00:00Z"),
            until: None,
        },
        providers: Vec::new(),
        rows,
        quota_history: None,
    }
}

#[test]
fn an_undeclared_provider_with_rows_is_refused() {
    let assigned = BTreeSet::from([Provider::Claude]);
    let measured = ProvenanceStatus::MeasuredCanonical;
    let rows = vec![
        row(Provider::Claude, measured, Some(1)),
        row(Provider::Codex, ProvenanceStatus::UnknownUsage, None),
    ];
    let error = refuse_undeclared(&ledger(rows), &assigned)
        .unwrap_err()
        .to_string();
    assert_eq!(
        error,
        "undeclared provider codex: 1 in-scope usage rows; assign it in the declaration"
    );
    let only_claude = ledger(vec![row(Provider::Claude, measured, Some(1))]);
    assert!(refuse_undeclared(&only_claude, &assigned).is_ok());
}

#[test]
fn token_sums_keep_unknown_fields_unknown() {
    let assigned = BTreeSet::from([Provider::Claude, Provider::Codex]);
    let measured = ProvenanceStatus::MeasuredCanonical;
    let rows = vec![
        row(Provider::Claude, measured, Some(3)),
        row(Provider::Claude, measured, Some(4)),
        row(
            Provider::Claude,
            ProvenanceStatus::AmbiguousConflict,
            Some(100),
        ),
    ];
    let tokens = provider_tokens(&ledger(rows), &assigned);
    assert_eq!(tokens[&Provider::Claude].output_tokens, Some(7));
    assert_eq!(tokens[&Provider::Claude].input_tokens, None);
    assert!(!tokens.contains_key(&Provider::Codex));
    let provenance = accounting_provenance(&ledger(Vec::new()), &assigned, &tokens);
    assert_eq!(provenance, AccountingProvenance::Unknown);
}

#[test]
fn the_window_flags_must_be_rfc3339() {
    assert!(rfc3339_flag("--since", Some("7d"))
        .unwrap_err()
        .to_string()
        .contains("--since"));
    assert!(rfc3339_flag("--until", Some("2026-10-15"))
        .unwrap_err()
        .to_string()
        .contains("--until"));
    assert!(rfc3339_flag("--since", Some("2026-10-14T00:00:00Z")).is_ok());
}

#[path = "run_evidence_provenance_tests.rs"]
mod provenance_tests;
