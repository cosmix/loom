use std::fs;

use anyhow::Result;
use chrono::{TimeZone, Utc};
use serde_json::json;

use super::*;

fn range() -> super::super::time_range::TimeRange {
    super::super::time_range::TimeRange {
        since: Utc
            .with_ymd_and_hms(2026, 9, 12, 0, 0, 0)
            .single()
            .expect("valid fixture timestamp"),
        until: None,
    }
}

fn assistant(id: &str, output: u64, model: &str) -> String {
    json!({
        "type": "assistant",
        "timestamp": "2026-09-12T20:00:00Z",
        "message": {
            "id": id,
            "model": model,
            "content": [],
            "usage": {"input_tokens": 10, "output_tokens": output}
        }
    })
    .to_string()
}

fn assistant_with_cache(id: &str, total: u64, five_minutes: u64, one_hour: u64) -> String {
    json!({
        "type": "assistant",
        "timestamp": "2026-09-12T20:00:00Z",
        "message": {
            "id": id,
            "model": "claude",
            "content": [],
            "usage": {
                "input_tokens": 10,
                "cache_creation_input_tokens": total,
                "cache_creation": {
                    "ephemeral_5m_input_tokens": five_minutes,
                    "ephemeral_1h_input_tokens": one_hour
                },
                "output_tokens": 2
            }
        }
    })
    .to_string()
}

fn file(path: std::path::PathBuf, session: &str) -> super::super::discovery::DiscoveredFile {
    super::super::discovery::DiscoveredFile {
        path,
        project_slug: "project".to_owned(),
        scope: Scope::Main,
        session_id: session.to_owned(),
        agent_id: None,
    }
}

fn parsed_pair(left: &str, right: &str) -> Result<(tempfile::TempDir, Vec<Transcript>)> {
    let root = tempfile::tempdir()?;
    let left_path = root.path().join("left.jsonl");
    let right_path = root.path().join("right.jsonl");
    fs::write(&left_path, format!("{left}\n"))?;
    fs::write(&right_path, format!("{right}\n"))?;
    let transcripts = vec![
        super::super::transcript::parse(&file(left_path, "one"), &range())?,
        super::super::transcript::parse(&file(right_path, "two"), &range())?,
    ];
    Ok((root, transcripts))
}

fn parsed_one(line: &str) -> Result<(tempfile::TempDir, Vec<Transcript>)> {
    let root = tempfile::tempdir()?;
    let path = root.path().join("one.jsonl");
    fs::write(&path, format!("{line}\n"))?;
    let transcript = super::super::transcript::parse(&file(path, "one"), &range())?;
    Ok((root, vec![transcript]))
}

#[test]
fn global_exact_message_id_copy_collapses_once() -> Result<()> {
    let line = assistant("response-1", 5, "claude");
    let (_root, mut transcripts) = parsed_pair(&line, &line)?;

    let normalized = normalize(&mut transcripts);

    assert_eq!(
        transcripts
            .iter()
            .map(Transcript::total_usage)
            .map(|usage| usage.output)
            .sum::<u64>(),
        5
    );
    assert_eq!(
        normalized
            .rows
            .iter()
            .filter(|row| row.provenance == ProvenanceStatus::DuplicateExact)
            .count(),
        1
    );
    Ok(())
}

#[test]
fn global_conflicting_terminal_vector_is_ambiguous() -> Result<()> {
    let (_root, mut transcripts) = parsed_pair(
        &assistant("response-1", 5, "claude"),
        &assistant("response-1", 6, "claude"),
    )?;

    let normalized = normalize(&mut transcripts);

    assert_eq!(
        transcripts
            .iter()
            .map(|transcript| transcript.requests().count())
            .sum::<usize>(),
        0
    );
    assert!(normalized
        .rows
        .iter()
        .all(|row| row.provenance == ProvenanceStatus::AmbiguousConflict));
    Ok(())
}

#[test]
fn synthetic_row_is_separate_from_measured_responses() -> Result<()> {
    let root = tempfile::tempdir()?;
    let path = root.path().join("rows.jsonl");
    fs::write(
        &path,
        format!(
            "{}\n{}\n",
            assistant("synthetic", 0, SYNTHETIC_MODEL),
            assistant("real", 2, "claude")
        ),
    )?;
    let mut transcripts = vec![super::super::transcript::parse(
        &file(path, "one"),
        &range(),
    )?];

    let normalized = normalize(&mut transcripts);
    let report = super::super::provider_report::build(
        Provider::Claude,
        &normalized.rows,
        normalized.diagnostics,
    );

    assert_eq!(report.coverage.synthetic_rows, 1);
    assert_eq!(report.coverage.measured_responses, 1);
    Ok(())
}

#[test]
fn inconsistent_transcript_ttl_split_is_unknown_and_diagnostic() -> Result<()> {
    let line = assistant_with_cache("response-1", 100, 30, 40);
    let (_root, mut transcripts) = parsed_one(&line)?;
    let legacy = transcripts[0]
        .requests()
        .next()
        .expect("fixture has one request");
    assert_eq!(
        (
            legacy.usage.cache_creation,
            legacy.usage.ephemeral_5m,
            legacy.usage.ephemeral_1h,
        ),
        (100, 100, 0)
    );

    let normalized = normalize(&mut transcripts);
    let row = &normalized.rows[0];

    assert_eq!(row.provenance, ProvenanceStatus::MeasuredCanonical);
    assert_eq!(row.tokens.cache_creation_input_tokens, Some(100));
    assert_eq!(row.tokens.cache_write_5m_input_tokens, None);
    assert_eq!(row.tokens.cache_write_1h_input_tokens, None);
    assert_eq!(normalized.diagnostics.invalid_cache_relations, 1);
    let report = super::super::provider_report::build(
        Provider::Claude,
        &normalized.rows,
        normalized.diagnostics,
    );
    assert_eq!(
        (
            report.totals.cache_creation.value,
            report.totals.cache_creation.measured_rows,
            report.totals.cache_creation.unknown_rows,
        ),
        (100, 1, 0)
    );
    assert_eq!(
        (
            report.totals.cache_write_5m.value,
            report.totals.cache_write_5m.measured_rows,
            report.totals.cache_write_5m.unknown_rows,
            report.totals.cache_write_1h.value,
            report.totals.cache_write_1h.measured_rows,
            report.totals.cache_write_1h.unknown_rows,
        ),
        (0, 0, 1, 0, 0, 1)
    );
    Ok(())
}

#[test]
fn consistent_transcript_ttl_split_is_preserved() -> Result<()> {
    let line = assistant_with_cache("response-1", 100, 30, 70);
    let (_root, mut transcripts) = parsed_one(&line)?;

    let normalized = normalize(&mut transcripts);
    let row = &normalized.rows[0];

    assert_eq!(row.provenance, ProvenanceStatus::MeasuredCanonical);
    assert_eq!(
        (
            row.tokens.cache_creation_input_tokens,
            row.tokens.cache_write_5m_input_tokens,
            row.tokens.cache_write_1h_input_tokens,
        ),
        (Some(100), Some(30), Some(70))
    );
    assert_eq!(normalized.diagnostics.invalid_cache_relations, 0);
    Ok(())
}
