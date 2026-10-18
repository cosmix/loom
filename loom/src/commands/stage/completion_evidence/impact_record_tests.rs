//! The impact-test record a failed completion prints and the broker keeps.

use super::*;
use crate::verify::impact_tests::{load_observed, FailureKind, ImpactFailure};
use tempfile::TempDir;

const ID: &str = "IT-cargo-test:.";

fn observed(output: &str) -> ObservedImpact {
    ObservedImpact::of(&[ImpactFailure {
        id: ID.to_string(),
        command: "cargo test -- a".to_string(),
        tests: vec!["a".to_string()],
        kind: FailureKind::Fails { failed: Some(2) },
        base: "it passes at the stage's base 0123abcd".to_string(),
        accepted: None,
        output: output.to_string(),
    }])
}

fn standard_stage() -> Stage {
    Stage {
        id: "s".to_string(),
        plan_version: 2,
        stage_type: StageType::Standard,
        ..Stage::default()
    }
}

#[test]
fn a_printed_record_parses_back_after_other_output() {
    let record = observed("a line\nLOOM_CONTROL_IMPACT_EOF inside JSON stays one line");
    let printed = format!(
        "Running impact-selected tests...\n{}\n\n",
        format_impact_record(&record).unwrap()
    );

    assert_eq!(parse_impact_record(&printed).unwrap(), Some(record));
    assert_eq!(parse_impact_record("no record here\n").unwrap(), None);
}

/// The record must be the last two non-empty lines and appear once: a record
/// the stage's own code printed before loom's makes two and neither is kept,
/// as is a record followed by more output, a lone line of either kind, or a
/// record loom would not write.
#[test]
fn a_record_not_ending_the_output_or_broken_is_refused() {
    let real = format_impact_record(&observed("real")).unwrap();
    let planted = format_impact_record(&ObservedImpact::default()).unwrap();
    for output in [
        format!("{planted}{real}"),
        format!("{real}{real}"),
        format!("{real}trailing output\n"),
        format!("{IMPACT_RECORD_PREFIX}{{}}\n"),
        format!("{IMPACT_EOF_MARKER}\n"),
        real.replace(IMPACT_EOF_MARKER, &format!("between\n{IMPACT_EOF_MARKER}")),
        format!("{IMPACT_RECORD_PREFIX}{{\"version\":1,\"failures\":[],\"extra\":1}}\n{IMPACT_EOF_MARKER}\n"),
    ] {
        assert!(parse_impact_record(&output).is_err(), "{output}");
    }
}

#[test]
fn the_broker_keeps_a_standard_stage_s_record_only() {
    let work = TempDir::new().unwrap();
    let output = format_impact_record(&observed("seen")).unwrap();
    let knowledge = Stage {
        stage_type: StageType::Knowledge,
        ..standard_stage()
    };

    record_from_output(&knowledge, work.path(), &output).unwrap();
    assert!(load_observed(work.path(), "s").unwrap().failures.is_empty());

    record_from_output(&standard_stage(), work.path(), &output).unwrap();
    assert_eq!(load_observed(work.path(), "s").unwrap(), observed("seen"));
}

/// A failing run that reports no failure, prints no record, or prints one
/// that cannot be read clears the record: a dispute names only failures of
/// the latest run.
#[test]
fn a_later_run_without_failures_clears_the_record() {
    let seen = format_impact_record(&observed("seen")).unwrap();
    let empty = format_impact_record(&ObservedImpact::default()).unwrap();
    for (later, refused) in [
        (empty, false),
        (
            "the completion failed before its gates\n".to_string(),
            false,
        ),
        (format!("{seen}{seen}"), true),
    ] {
        let work = TempDir::new().unwrap();
        record_from_output(&standard_stage(), work.path(), &seen).unwrap();

        let result = record_from_output(&standard_stage(), work.path(), &later);

        assert_eq!(result.is_err(), refused, "{later}");
        assert_eq!(
            load_observed(work.path(), "s").unwrap(),
            ObservedImpact::default(),
            "{later}"
        );
    }
}

/// Stage code that prints a forged record and ends loom before loom prints
/// its own leaves the forgery as the last lines, and the broker keeps it:
/// the record is a claim. What it can excuse once accepted is bounded by
/// `verify::impact_tests::accepted` (its report tests).
#[test]
fn a_forged_record_ending_the_output_is_kept_as_a_claim() {
    let work = TempDir::new().unwrap();
    let mut forged = observed("forged");
    forged.failures[0].kind = FailureKind::Fails { failed: Some(500) };
    let output = format!("test output\n{}", format_impact_record(&forged).unwrap());

    record_from_output(&standard_stage(), work.path(), &output).unwrap();

    assert_eq!(load_observed(work.path(), "s").unwrap(), forged);
}
