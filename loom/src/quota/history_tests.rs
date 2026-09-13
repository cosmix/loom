use super::history_store::record_observation_at;
use super::*;
use tempfile::tempdir;

pub(super) const NOW: i64 = 100_000_000;

pub(super) fn quota(
    observed_at: i64,
    used_percent: f64,
    resets_at: Option<i64>,
    plan: Option<&str>,
) -> ProviderQuota {
    ProviderQuota {
        observed_at,
        windows: vec![QuotaWindow {
            kind: WindowKind::FiveHour,
            used_percent,
            resets_at,
        }],
        plan: plan.map(str::to_string),
        error: Some("discarded error".to_string()),
    }
}

pub(super) fn dual_quota(observed_at: i64, five_reset: i64, seven_reset: i64) -> ProviderQuota {
    ProviderQuota {
        observed_at,
        windows: vec![
            QuotaWindow {
                kind: WindowKind::FiveHour,
                used_percent: 50.0,
                resets_at: Some(five_reset),
            },
            QuotaWindow {
                kind: WindowKind::SevenDay,
                used_percent: 30.0,
                resets_at: Some(seven_reset),
            },
        ],
        plan: None,
        error: None,
    }
}

pub(super) fn append(dir: &Path, provider: &str, quota: &ProviderQuota) {
    record_observation_at(dir, provider, quota, NOW, 16 * 1024 * 1024).unwrap();
}

pub(super) fn write_rows(dir: &Path, provider: &str, rows: &[String]) {
    let path = history_path(dir, provider);
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    std::fs::write(path, rows.join("\n")).unwrap();
}

pub(super) fn row(observed_at: i64, used_percent: f64, resets_at: Option<i64>) -> String {
    serde_json::json!({
        "schema_version": 1,
        "observed_at": observed_at,
        "windows": [{
            "kind": "five-hour",
            "used_percent": used_percent,
            "resets_at": resets_at
        }],
        "plan": null
    })
    .to_string()
}

#[test]
fn successful_observations_are_read_for_both_fixed_providers() {
    let dir = tempdir().unwrap();
    append(
        dir.path(),
        "claude",
        &quota(NOW - 20, 20.0, Some(500), None),
    );
    append(
        dir.path(),
        "codex",
        &quota(NOW - 10, 30.0, Some(600), Some("pro")),
    );

    assert_eq!(read_history(dir.path(), "claude", 0, None).points.len(), 1);
    assert_eq!(read_history(dir.path(), "codex", 0, None).points.len(), 1);
    assert!(!std::fs::read_to_string(history_path(dir.path(), "claude"))
        .unwrap()
        .contains("discarded error"));
}

#[test]
fn read_history_reads_a_direct_schema_v1_fixture_without_snapshot_credentials_or_poller() {
    let dir = tempdir().unwrap();
    let path = dir.path().join("quota/history/claude.jsonl");
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    std::fs::write(&path, format!("{}\n", row(10, 25.0, Some(100)))).unwrap();

    let history = read_history(dir.path(), "claude", 0, None);

    assert_eq!(history.points.len(), 1);
    assert_eq!(history.points[0].observed_at, 10);
}

#[test]
fn missing_and_unknown_provider_inputs_are_diagnostic_states() {
    let dir = tempdir().unwrap();

    assert_eq!(
        read_history(dir.path(), "claude", 0, None)
            .diagnostics
            .source,
        HistorySourceState::Missing
    );
    assert_eq!(
        read_history(dir.path(), "other", 0, None)
            .diagnostics
            .source,
        HistorySourceState::UnknownProvider
    );
}

#[test]
fn an_empty_valid_history_is_distinct_from_a_missing_one() {
    let dir = tempdir().unwrap();
    write_rows(dir.path(), "claude", &[]);

    let history = read_history(dir.path(), "claude", 0, None);
    assert_eq!(history.diagnostics.source, HistorySourceState::Empty);
}

#[test]
fn malformed_and_torn_rows_do_not_hide_valid_history() {
    let dir = tempdir().unwrap();
    write_rows(
        dir.path(),
        "claude",
        &[
            row(10, 10.0, Some(100)),
            "{\"schema_version\":1".to_string(),
        ],
    );

    let history = read_history(dir.path(), "claude", 0, None);
    assert_eq!(history.points.len(), 1);
    assert_eq!(history.diagnostics.source, HistorySourceState::Read);
    assert_eq!(history.diagnostics.malformed_rows, 1);
}

#[test]
fn a_file_with_only_malformed_rows_is_corrupt() {
    let dir = tempdir().unwrap();
    write_rows(dir.path(), "claude", &["not json".to_string()]);

    let history = read_history(dir.path(), "claude", 0, None);
    assert_eq!(history.diagnostics.source, HistorySourceState::Corrupt);
}

#[test]
fn unsupported_schema_versions_are_counted_separately() {
    let dir = tempdir().unwrap();
    let unsupported = serde_json::json!({ "schema_version": 2 }).to_string();
    write_rows(dir.path(), "claude", &[unsupported]);

    let history = read_history(dir.path(), "claude", 0, None);
    assert_eq!(history.diagnostics.unsupported_schema_rows, 1);
    assert_eq!(history.diagnostics.source, HistorySourceState::Corrupt);
}

#[test]
fn oversized_and_symlinked_history_files_are_unreadable() {
    let oversized = tempdir().unwrap();
    let path = history_path(oversized.path(), "claude");
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    std::fs::File::create(&path)
        .unwrap()
        .set_len((16 * 1024 * 1024 + 1) as u64)
        .unwrap();
    assert_eq!(
        read_history(oversized.path(), "claude", 0, None)
            .diagnostics
            .source,
        HistorySourceState::Unreadable
    );

    let linked = tempdir().unwrap();
    let target = linked.path().join("outside.jsonl");
    std::fs::write(&target, row(10, 10.0, Some(100))).unwrap();
    let link = history_path(linked.path(), "claude");
    std::fs::create_dir_all(link.parent().unwrap()).unwrap();
    std::os::unix::fs::symlink(&target, link).unwrap();
    assert_eq!(
        read_history(linked.path(), "claude", 0, None)
            .diagnostics
            .source,
        HistorySourceState::Unreadable
    );
}

#[test]
fn reader_uses_inclusive_utc_epoch_bounds() {
    let dir = tempdir().unwrap();
    write_rows(
        dir.path(),
        "claude",
        &[
            row(10, 10.0, Some(100)),
            row(20, 20.0, Some(100)),
            row(30, 30.0, Some(100)),
        ],
    );

    let history = read_history(dir.path(), "claude", 20, Some(20));
    assert_eq!(
        history
            .points
            .iter()
            .map(|point| point.observed_at)
            .collect::<Vec<_>>(),
        vec![20]
    );
}

#[test]
fn reader_drops_nonmonotonic_rows_without_reordering() {
    let dir = tempdir().unwrap();
    write_rows(
        dir.path(),
        "claude",
        &[
            row(10, 10.0, Some(100)),
            row(9, 20.0, Some(100)),
            row(20, 30.0, Some(100)),
        ],
    );

    let history = read_history(dir.path(), "claude", 0, None);
    assert_eq!(
        history
            .points
            .iter()
            .map(|point| point.observed_at)
            .collect::<Vec<_>>(),
        vec![10, 20]
    );
    assert_eq!(history.diagnostics.nonmonotonic_rows, 1);
}
