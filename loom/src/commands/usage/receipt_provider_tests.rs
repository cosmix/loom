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

fn receipt(version: u64, request_id: Option<&str>) -> serde_json::Value {
    json!({
        "schema_version": version,
        "provider": "claude",
        "observed_at": "2026-09-12T20:00:00Z",
        "request_id": request_id,
        "usage": {
            "input_tokens": 10,
            "output_tokens": 2
        }
    })
}

#[test]
fn receipt_optional_usage_stays_unknown_and_stage_is_not_applicable() -> Result<()> {
    let root = tempfile::tempdir()?;
    fs::write(root.path().join("one.json"), receipt(1, None).to_string())?;

    let normalized = normalize(Some(root.path()), &range());
    let row = &normalized.rows[0];

    assert_eq!(row.tokens.cache_creation_input_tokens, None);
    assert_eq!(row.tokens.cache_read_input_tokens, None);
    assert_eq!(row.tokens.resident_input_tokens, None);
    assert_eq!(
        row.attribution.stage_state,
        StageAttributionState::NotApplicable
    );
    assert_eq!(
        normalized.diagnostics[&Provider::Claude].unattributable_receipts,
        1
    );
    Ok(())
}

#[test]
fn malformed_and_unsupported_receipts_are_sanitized_counts() -> Result<()> {
    let root = tempfile::tempdir()?;
    fs::write(root.path().join("bad.json"), "not-json")?;
    fs::write(
        root.path().join("future.json"),
        receipt(2, Some("id")).to_string(),
    )?;

    let normalized = normalize(Some(root.path()), &range());
    let diagnostics = &normalized.diagnostics[&Provider::Claude];

    assert!(normalized.rows.is_empty());
    assert_eq!(diagnostics.malformed_receipts, 1);
    assert_eq!(diagnostics.unsupported_receipt_versions, 1);
    assert_eq!(diagnostics.receipt_files_seen, 2);
    Ok(())
}

#[test]
fn missing_receipts_root_is_empty_with_diagnostic() -> Result<()> {
    let root = tempfile::tempdir()?;

    let normalized = normalize(Some(&root.path().join("missing")), &range());

    assert!(normalized.rows.is_empty());
    assert_eq!(normalized.diagnostics[&Provider::Claude].missing_roots, 1);
    assert_eq!(normalized.diagnostics[&Provider::Codex].missing_roots, 1);
    Ok(())
}

#[test]
fn codex_receipt_with_unordered_cache_keeps_other_measured_fields() -> Result<()> {
    let root = tempfile::tempdir()?;
    let mut value = receipt(1, Some("response-1"));
    value["provider"] = json!("codex");
    value["usage"]["cache_read_input_tokens"] = json!(11);
    fs::write(root.path().join("one.json"), value.to_string())?;

    let normalized = normalize(Some(root.path()), &range());
    let row = &normalized.rows[0];

    assert_eq!(row.provenance, ProvenanceStatus::MeasuredCanonical);
    assert_eq!(row.tokens.fresh_input_tokens, None);
    assert_eq!(row.tokens.output_tokens, Some(2));
    assert_eq!(
        normalized.diagnostics[&Provider::Codex].invalid_cache_relations,
        1
    );
    Ok(())
}

#[test]
fn inconsistent_receipt_ttl_split_is_unknown_and_diagnostic() -> Result<()> {
    let root = tempfile::tempdir()?;
    let mut value = receipt(1, Some("request-1"));
    value["usage"]["cache_creation_input_tokens"] = json!(10);
    value["usage"]["cache_write_5m_input_tokens"] = json!(4);
    value["usage"]["cache_write_1h_input_tokens"] = json!(5);
    fs::write(root.path().join("one.json"), value.to_string())?;

    let normalized = normalize(Some(root.path()), &range());
    let row = &normalized.rows[0];

    assert_eq!(row.provenance, ProvenanceStatus::MeasuredCanonical);
    assert_eq!(row.tokens.cache_creation_input_tokens, Some(10));
    assert_eq!(row.tokens.cache_write_5m_input_tokens, None);
    assert_eq!(row.tokens.cache_write_1h_input_tokens, None);
    assert_eq!(
        normalized.diagnostics[&Provider::Claude].invalid_cache_relations,
        1
    );
    Ok(())
}

#[test]
fn exact_receipt_request_id_copy_keeps_one_canonical_row() -> Result<()> {
    let root = tempfile::tempdir()?;
    let value = receipt(1, Some("request-1")).to_string();
    fs::write(root.path().join("one.json"), &value)?;
    fs::write(root.path().join("two.json"), value)?;

    let normalized = normalize(Some(root.path()), &range());
    let statuses = normalized
        .rows
        .iter()
        .map(|row| row.provenance)
        .collect::<Vec<_>>();

    assert_eq!(
        statuses,
        vec![
            ProvenanceStatus::MeasuredCanonical,
            ProvenanceStatus::DuplicateExact
        ]
    );
    Ok(())
}

#[test]
fn conflicting_receipt_request_id_marks_every_row_ambiguous() -> Result<()> {
    let root = tempfile::tempdir()?;
    let first = receipt(1, Some("request-1"));
    let mut second = first.clone();
    second["usage"]["output_tokens"] = json!(3);
    fs::write(root.path().join("one.json"), first.to_string())?;
    fs::write(root.path().join("two.json"), second.to_string())?;

    let normalized = normalize(Some(root.path()), &range());

    assert!(normalized
        .rows
        .iter()
        .all(|row| row.provenance == ProvenanceStatus::AmbiguousConflict));
    Ok(())
}
