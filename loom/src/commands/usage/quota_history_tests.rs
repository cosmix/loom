use std::fs;
use std::path::Path;

use chrono::{TimeZone, Utc};
use serde_json::json;

use super::*;
use crate::commands::usage::{
    attach_quota_history, provider, time_range::TimeRange, ProviderSelection,
};

fn range() -> TimeRange {
    TimeRange {
        since: Utc
            .with_ymd_and_hms(2026, 9, 12, 0, 0, 0)
            .single()
            .expect("valid fixture timestamp"),
        until: None,
    }
}

fn normalized(
    selection: ProviderSelection,
    receipts_root: Option<&Path>,
) -> provider::NormalizedProviderEvents {
    provider::normalize_provider_events(provider::ProviderEventInput {
        selection,
        range: range(),
        claude_transcripts: Vec::new(),
        claude_discovered_files: 0,
        claude_missing_roots: 0,
        codex_files: &[],
        codex_missing_roots: 0,
        codex_unreadable_directories: 0,
        receipts_root,
    })
}

fn write_history(root: &Path, provider: &str, observed_at: i64) {
    let directory = root.join("quota/history");
    fs::create_dir_all(&directory).expect("history directory");
    let point = json!({
        "schema_version": 1,
        "observed_at": observed_at,
        "windows": [{
            "kind": "five-hour",
            "used_percent": 42.5,
            "resets_at": 2_000_000_000
        }],
        "plan": "pro"
    });
    fs::write(
        directory.join(format!("{provider}.jsonl")),
        format!("{point}\nnot-json\n"),
    )
    .expect("history fixture");
}

#[test]
fn section_contains_provider_points_and_diagnostics() {
    let root = tempfile::tempdir().expect("temporary work root");
    let bounded = TimeRange {
        until: Some(range().since),
        ..range()
    };
    let observed_at = bounded.since.timestamp();
    write_history(root.path(), "claude", observed_at);
    let mut normalized = normalized(ProviderSelection::Claude, None);

    attach_quota_history(
        &mut normalized.ledger,
        ProviderSelection::Claude,
        Some(root.path()),
        bounded,
    );
    let value = serde_json::to_value(normalized.ledger.quota_history).unwrap();

    assert_eq!(
        value,
        json!({
            "schema_version": 1,
            "providers": [{
                "provider": "claude",
                "source": "read",
                "diagnostics": {
                    "malformed_rows": 1,
                    "unsupported_schema_rows": 0,
                    "nonmonotonic_rows": 0
                },
                "points": [{
                    "observed_at": observed_at,
                    "windows": [{
                        "kind": "five-hour",
                        "used_percent": 42.5,
                        "resets_at": 2_000_000_000
                    }],
                    "plan": "pro",
                    "continuity": "initial"
                }]
            }]
        })
    );
}

#[test]
fn missing_history_remains_missing_without_zero_usage_claim() {
    let root = tempfile::tempdir().expect("temporary work root");
    let mut normalized = normalized(ProviderSelection::Claude, None);

    attach_quota_history(
        &mut normalized.ledger,
        ProviderSelection::Claude,
        Some(root.path()),
        range(),
    );
    let history = &normalized.ledger.quota_history.as_ref().unwrap().providers[0];

    assert_eq!(history.source, QuotaHistorySourceState::Missing);
    assert!(history.points.is_empty());
}

#[test]
fn unavailable_history_is_explicit_for_each_selected_provider() {
    let project = tempfile::tempdir().expect("project without loom state");
    assert!(super::super::usage_work_dir(None, true).is_none());
    assert!(super::super::usage_work_dir(Some(project.path()), false).is_none());
    let mut normalized = normalized(ProviderSelection::All, None);

    attach_quota_history(
        &mut normalized.ledger,
        ProviderSelection::All,
        None,
        range(),
    );
    let history = normalized.ledger.quota_history.as_ref().unwrap();

    assert_eq!(history.providers.len(), 2);
    assert!(history.providers.iter().all(|provider| {
        provider.source == QuotaHistorySourceState::Unavailable && provider.points.is_empty()
    }));
}

#[test]
fn joining_history_does_not_change_measured_token_totals() {
    let receipts = tempfile::tempdir().expect("temporary receipt root");
    let receipt = json!({
        "schema_version": 1,
        "provider": "claude",
        "observed_at": "2026-09-12T20:00:00Z",
        "request_id": "request-1",
        "usage": {"input_tokens": 10, "output_tokens": 2}
    });
    fs::write(receipts.path().join("receipt.json"), receipt.to_string()).unwrap();
    let work_root = tempfile::tempdir().expect("temporary work root");
    write_history(work_root.path(), "claude", range().since.timestamp());
    let mut normalized = normalized(ProviderSelection::Claude, Some(receipts.path()));
    let before = serde_json::to_value(&normalized.ledger.providers).unwrap();

    attach_quota_history(
        &mut normalized.ledger,
        ProviderSelection::Claude,
        Some(work_root.path()),
        range(),
    );
    let after = serde_json::to_value(&normalized.ledger.providers).unwrap();

    assert_eq!(before, after);
    assert_eq!(normalized.ledger.providers[0].totals.output.value, 2);
}
