use std::fs;

use anyhow::Result;
use chrono::{TimeZone, Utc};
use serde_json::json;

use super::*;

fn range() -> TimeRange {
    TimeRange {
        since: Utc
            .with_ymd_and_hms(2026, 9, 12, 0, 0, 0)
            .single()
            .expect("valid fixture timestamp"),
        until: None,
    }
}

fn write(lines: &[Value]) -> Result<(tempfile::TempDir, PathBuf)> {
    let root = tempfile::tempdir()?;
    let path = root.path().join("rollout.jsonl");
    let content = lines
        .iter()
        .map(Value::to_string)
        .collect::<Vec<_>>()
        .join("\n");
    fs::write(&path, format!("{content}\n"))?;
    Ok((root, path))
}

fn direct(id: &str, input: u64, cached: u64, output: u64) -> Value {
    json!({
        "timestamp": "2026-09-12T20:00:00Z",
        "type": "token_usage_record",
        "payload": {
            "response_id": id,
            "model": "gpt-codex",
            "usage": {
                "input_tokens": input,
                "cached_input_tokens": cached,
                "output_tokens": output,
                "reasoning_output_tokens": 3,
                "total_tokens": input + output
            }
        }
    })
}

fn fallback(last_output: u64, cumulative_output: u64) -> Value {
    json!({
        "timestamp": "2026-09-12T20:01:00Z",
        "type": "event_msg",
        "payload": {
            "type": "token_count",
            "info": {
                "last_token_usage": {
                    "input_tokens": 50,
                    "cached_input_tokens": 20,
                    "output_tokens": last_output,
                    "total_tokens": 50 + last_output
                },
                "total_token_usage": {"output_tokens": cumulative_output}
            }
        }
    })
}

fn session_meta(id: &str) -> Value {
    json!({"type": "session_meta", "payload": {"id": id}})
}

#[test]
fn unsafe_session_meta_thread_id_leaves_row_unattributed() -> Result<()> {
    let (_root, path) = write(&[session_meta("../unsafe"), direct("response-1", 100, 40, 20)])?;

    let normalized = normalize(&[path], &range());
    let row = &normalized.rows[0];

    assert!(row.codex_thread_id.is_none());
    assert!(row.codex_thread_conflict);
    Ok(())
}

#[test]
fn differing_session_meta_thread_ids_mark_file_rows_conflicting() -> Result<()> {
    let (_root, path) = write(&[
        session_meta("thread-a"),
        direct("response-1", 100, 40, 20),
        session_meta("thread-b"),
    ])?;

    let normalized = normalize(&[path], &range());
    let row = &normalized.rows[0];

    assert!(row.codex_thread_id.is_none());
    assert!(row.codex_thread_conflict);
    Ok(())
}

#[test]
fn direct_record_uses_explicit_vector_and_codex_input_semantics() -> Result<()> {
    let (_root, path) = write(&[direct("response-1", 100, 40, 20)])?;

    let normalized = normalize(&[path], &range());
    let row = &normalized.rows[0];

    assert_eq!(row.tokens.resident_input_tokens, Some(100));
    assert_eq!(row.tokens.fresh_input_tokens, Some(60));
    assert_eq!(row.tokens.cache_read_input_tokens, Some(40));
    assert_eq!(row.tokens.cache_creation_input_tokens, None);
    assert_eq!(row.tokens.thinking_output_tokens, Some(3));
    Ok(())
}

#[test]
fn fallback_same_last_with_changed_total_is_preserved() -> Result<()> {
    let first = fallback(5, 5);
    let second = fallback(5, 10);
    let third = second.clone();
    let (_root, path) = write(&[first, second, third])?;

    let normalized = normalize(&[path], &range());

    assert_eq!(normalized.rows.len(), 2);
    assert_eq!(normalized.diagnostics.fallback_duplicate_snapshots, 1);
    assert!(normalized
        .rows
        .iter()
        .all(|row| row.provenance == ProvenanceStatus::MeasuredCanonical));
    Ok(())
}

#[test]
fn mixed_direct_and_fallback_are_not_fused() -> Result<()> {
    let (_root, path) = write(&[fallback(5, 5), direct("response-1", 100, 40, 20)])?;

    let normalized = normalize(&[path], &range());
    let report = super::super::provider_report::build(
        Provider::Codex,
        &normalized.rows,
        normalized.diagnostics,
    );

    assert_eq!(report.coverage.measured_responses, 1);
    assert_eq!(report.coverage.fallback_coverage_rows, 1);
    assert_eq!(report.totals.output.value, 20);
    assert_eq!(report.fallback_coverage_totals.output.value, 5);
    assert_eq!(report.diagnostics.mixed_schema_files, 1);
    Ok(())
}

#[test]
fn mixed_schema_detection_uses_physical_file_not_only_in_range_rows() -> Result<()> {
    let mut old_direct = direct("response-1", 100, 40, 20);
    old_direct["timestamp"] = json!("2026-09-11T20:00:00Z");
    let (_root, path) = write(&[old_direct, fallback(5, 5)])?;

    let normalized = normalize(&[path], &range());

    assert_eq!(normalized.rows.len(), 1);
    assert_eq!(
        normalized.rows[0].provenance,
        ProvenanceStatus::FallbackCoverage
    );
    assert_eq!(normalized.diagnostics.mixed_schema_files, 1);
    assert_eq!(normalized.diagnostics.unknown_fallback_relations, 1);
    Ok(())
}

#[test]
fn duplicate_global_response_id_conflict_is_excluded() -> Result<()> {
    let (_left_root, left) = write(&[direct("same", 100, 40, 20)])?;
    let (_right_root, right) = write(&[direct("same", 100, 40, 21)])?;

    let normalized = normalize(&[left, right], &range());

    assert!(normalized
        .rows
        .iter()
        .all(|row| row.provenance == ProvenanceStatus::AmbiguousConflict));
    Ok(())
}

#[test]
fn exact_global_response_id_copy_is_counted_once() -> Result<()> {
    let (_left_root, left) = write(&[direct("same", 100, 40, 20)])?;
    let (_right_root, right) = write(&[direct("same", 100, 40, 20)])?;

    let normalized = normalize(&[left, right], &range());
    let report = super::super::provider_report::build(
        Provider::Codex,
        &normalized.rows,
        normalized.diagnostics,
    );

    assert_eq!(report.coverage.measured_responses, 1);
    assert_eq!(report.coverage.duplicate_exact_rows, 1);
    assert_eq!(report.totals.output.value, 20);
    Ok(())
}

#[test]
fn direct_total_mismatch_is_diagnostic_only() -> Result<()> {
    let mut record = direct("response-1", 100, 40, 20);
    record["payload"]["usage"]["total_tokens"] = json!(999);
    let (_root, path) = write(&[record])?;

    let normalized = normalize(&[path], &range());

    assert_eq!(
        normalized.rows[0].provenance,
        ProvenanceStatus::MeasuredCanonical
    );
    assert_eq!(normalized.diagnostics.total_token_mismatches, 1);
    Ok(())
}
