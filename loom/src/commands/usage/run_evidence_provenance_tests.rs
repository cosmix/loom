//! Accounting provenance and stage ordering of the run-evidence producer.
//! A child of `run_evidence_tests.rs` (size limit), whose fixtures it reuses.

use super::*;
use crate::commands::usage::provider_report;
use crate::commands::usage::provider_types::{ProviderDiagnostics, ProviderReport};

/// A provider report with no rows: `Partial` when `damaged`, else `Measured`.
fn report(provider: Provider, damaged: bool) -> ProviderReport {
    let diagnostics = ProviderDiagnostics {
        malformed_rows: usize::from(damaged),
        ..ProviderDiagnostics::default()
    };
    provider_report::build(provider, &[], diagnostics)
}

fn claude_ledger(providers: Vec<ProviderReport>) -> ProviderLedger {
    let measured = ProvenanceStatus::MeasuredCanonical;
    ProviderLedger {
        providers,
        ..ledger(vec![row(Provider::Claude, measured, Some(1))])
    }
}

#[test]
fn a_partial_report_of_an_assigned_provider_reads_partial() {
    let assigned = BTreeSet::from([Provider::Claude]);
    let damaged = report(Provider::Claude, true);
    assert_eq!(damaged.completeness.state, CompletenessState::Partial);
    let ledger = claude_ledger(vec![damaged]);
    let tokens = provider_tokens(&ledger, &assigned);

    assert_eq!(
        accounting_provenance(&ledger, &assigned, &tokens),
        AccountingProvenance::Partial
    );
}

#[test]
fn a_partial_report_of_an_unassigned_provider_does_not_change_provenance() {
    let assigned = BTreeSet::from([Provider::Claude]);
    let foreign = report(Provider::Codex, true);
    assert_eq!(foreign.completeness.state, CompletenessState::Partial);
    let ledger = claude_ledger(vec![report(Provider::Claude, false), foreign]);
    let tokens = provider_tokens(&ledger, &assigned);

    assert_eq!(
        accounting_provenance(&ledger, &assigned, &tokens),
        AccountingProvenance::Measured
    );
}

/// Creates two stages, each with one open finding, in `order`.
fn finding_ids_created_in(order: [&str; 2]) -> Vec<String> {
    let dir = TempDir::new().unwrap();
    for id in order {
        let created = stage(
            id,
            &[],
            "2026-10-14T06:12:00Z",
            "2026-10-14T06:20:00Z",
            0,
            0,
        );
        create_stage(&created, dir.path()).unwrap();
        write_round(dir.path(), id, &round_with("F-1-1", "major")).unwrap();
    }
    stage_evidence(dir.path(), &day())
        .unwrap()
        .unresolved_findings
        .into_iter()
        .map(|finding| finding.finding_id)
        .collect()
}

#[test]
fn findings_follow_stage_id_order_whatever_the_creation_order() {
    let expected = vec!["stage-a:F-1-1", "stage-b:F-1-1"];

    assert_eq!(finding_ids_created_in(["stage-a", "stage-b"]), expected);
    assert_eq!(finding_ids_created_in(["stage-b", "stage-a"]), expected);
}

#[test]
fn a_straddle_abort_names_the_lowest_straddling_stage_id() {
    for order in [["stage-s", "stage-l"], ["stage-l", "stage-s"]] {
        let dir = TempDir::new().unwrap();
        for id in order {
            let (start, end) = if id == "stage-s" {
                ("2026-10-13T23:50:00Z", "2026-10-14T00:10:00Z")
            } else {
                ("2026-10-14T23:50:00Z", "2026-10-15T00:10:00Z")
            };
            create_stage(&stage(id, &[], start, end, 0, 0), dir.path()).unwrap();
        }
        let error = stage_evidence(dir.path(), &day())
            .err()
            .unwrap()
            .to_string();
        assert!(
            error.contains("stage-l") && error.contains("--until"),
            "{error}"
        );
    }
}
