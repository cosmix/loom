use chrono::{DateTime, Duration, TimeZone, Utc};

use super::*;
use crate::models::session::Session;

fn at(hour: u32) -> DateTime<Utc> {
    Utc.with_ymd_and_hms(2026, 10, 14, hour, 0, 0)
        .single()
        .expect("valid fixture timestamp")
}

fn range() -> TimeRange {
    TimeRange {
        since: at(10),
        until: Some(at(14)),
    }
}

fn write_record(
    work_dir: &Path,
    id: &str,
    stage: &str,
    transcript: Option<&str>,
    active: DateTime<Utc>,
) {
    let mut session = Session::new();
    session.id = id.to_owned();
    session.stage_id = Some(stage.to_owned());
    session.transcript_path = transcript.map(str::to_owned);
    session.created_at = active - Duration::minutes(5);
    session.last_active = active;
    let directory = work_dir.join("sessions");
    std::fs::create_dir_all(&directory).unwrap();
    std::fs::write(
        directory.join(format!("{id}.md")),
        crate::fs::session_files::session_to_markdown(&session),
    )
    .unwrap();
}

fn inventory(
    work_dir: &Path,
    read: &[PathBuf],
    stages: Option<&HashSet<String>>,
) -> SessionInventory {
    missed_claude_transcripts(Some(work_dir), read, &range(), stages)
}

#[test]
fn unread_transcript_inside_the_window_counts() {
    let temp = tempfile::tempdir().unwrap();
    write_record(temp.path(), "s1", "stage-a", Some("/gone/a.jsonl"), at(11));

    assert_eq!(
        inventory(temp.path(), &[], None),
        SessionInventory {
            missed: 1,
            unreadable: 0
        }
    );
}

#[test]
fn transcript_outside_the_window_or_already_read_or_absent_does_not_count() {
    let temp = tempfile::tempdir().unwrap();
    write_record(
        temp.path(),
        "old",
        "stage-a",
        Some("/gone/old.jsonl"),
        at(8),
    );
    write_record(
        temp.path(),
        "read",
        "stage-a",
        Some("/gone/read.jsonl"),
        at(11),
    );
    write_record(temp.path(), "none", "stage-a", None, at(11));

    let read = [PathBuf::from("/gone/read.jsonl")];

    assert_eq!(
        inventory(temp.path(), &read, None),
        SessionInventory::default()
    );
}

#[test]
fn read_path_matches_through_canonicalization() {
    let temp = tempfile::tempdir().unwrap();
    let transcript = temp.path().join("t.jsonl");
    std::fs::write(&transcript, "").unwrap();
    let dotted = temp.path().join("./t.jsonl");
    write_record(
        temp.path(),
        "s1",
        "stage-a",
        Some(&transcript.display().to_string()),
        at(11),
    );

    assert_eq!(inventory(temp.path(), &[dotted], None).missed, 0);
}

#[test]
fn unparseable_record_gives_missed_zero_and_unreadable_one() {
    let temp = tempfile::tempdir().unwrap();
    write_record(temp.path(), "s1", "stage-a", Some("/gone/a.jsonl"), at(11));
    let broken = "---\nid: s2\nstage_id: stage-a\n---\n";
    std::fs::write(temp.path().join("sessions/s2.md"), broken).unwrap();
    std::fs::remove_file(temp.path().join("sessions/s1.md")).unwrap();

    assert_eq!(
        inventory(temp.path(), &[], None),
        SessionInventory {
            missed: 0,
            unreadable: 1
        }
    );
}

#[test]
fn enumeration_error_gives_unreadable_one() {
    let temp = tempfile::tempdir().unwrap();
    // `sessions` as a regular file makes read_dir fail with a non-NotFound error.
    std::fs::write(temp.path().join("sessions"), "").unwrap();

    assert_eq!(
        inventory(temp.path(), &[], None),
        SessionInventory {
            missed: 0,
            unreadable: 1
        }
    );
}

#[test]
fn no_work_dir_reads_nothing() {
    assert_eq!(
        missed_claude_transcripts(None, &[], &range(), None),
        SessionInventory::default()
    );
}

#[test]
fn record_of_another_stage_is_not_counted() {
    let temp = tempfile::tempdir().unwrap();
    write_record(temp.path(), "s-b", "stage-b", Some("/gone/b.jsonl"), at(11));
    let only_a = HashSet::from(["stage-a".to_owned()]);

    assert_eq!(inventory(temp.path(), &[], Some(&only_a)).missed, 0);
    assert_eq!(inventory(temp.path(), &[], None).missed, 1);
}

#[test]
fn assess_reads_measured_with_no_reason() {
    let result = assess(
        Provider::Claude,
        &CoverageCounts::default(),
        &ProviderDiagnostics::default(),
    );

    assert_eq!(result.state, CompletenessState::Measured);
    assert!(result.reasons.is_empty());
}

#[test]
fn assess_orders_reasons_as_pinned() {
    let coverage = CoverageCounts {
        unknown_usage_rows: 1,
        ambiguous_rows: 1,
        fallback_coverage_rows: 4,
        ..CoverageCounts::default()
    };
    let diagnostics = ProviderDiagnostics {
        missing_roots: 1,
        unreadable_files: 1,
        malformed_rows: 1,
        unreadable_session_records: 1,
        missed_session_transcripts: 1,
        codex_scope_unavailable: 1,
        malformed_forward_receipts: 1,
        missing_forward_threads: 1,
        unthreaded_forward_receipts: 1,
        missing_timestamp_rows: 9,
        ..ProviderDiagnostics::default()
    };

    let result = assess(Provider::Codex, &coverage, &diagnostics);

    assert_eq!(result.state, CompletenessState::Partial);
    assert_eq!(
        result.reasons,
        [
            "missing-roots",
            "unreadable-files",
            "malformed-rows",
            "unreadable-session-records",
            "missed-session-transcripts",
            "unknown-usage-rows",
            "ambiguous-rows",
            "codex-scope-unavailable",
            "malformed-forward-receipts",
            "forward-thread-missing",
            "forward-receipt-unthreaded",
        ]
    );
}

#[test]
fn damaged_receipts_make_only_the_codex_report_partial() {
    let diagnostics = ProviderDiagnostics {
        truncated_forward_receipt_files: 1,
        ..ProviderDiagnostics::default()
    };
    let coverage = CoverageCounts::default();

    assert_eq!(
        assess(Provider::Codex, &coverage, &diagnostics).reasons,
        ["malformed-forward-receipts"]
    );
    assert_eq!(
        assess(Provider::Claude, &coverage, &diagnostics).state,
        CompletenessState::Measured
    );
}

#[test]
fn evidence_loss_reasons_each_read_partial() {
    for (diagnostics, reason) in [
        (
            ProviderDiagnostics {
                malformed_rows: 1,
                ..Default::default()
            },
            "malformed-rows",
        ),
        (
            ProviderDiagnostics {
                unreadable_session_records: 1,
                ..Default::default()
            },
            "unreadable-session-records",
        ),
    ] {
        let result = assess(Provider::Claude, &CoverageCounts::default(), &diagnostics);
        assert_eq!(result.reasons, [reason]);
    }
}
