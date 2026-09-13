use super::history_store::record_observation_at;
use super::tests::{append, dual_quota, quota, row, write_rows, NOW};
use super::*;
use tempfile::tempdir;

#[test]
fn writer_rejects_duplicate_timestamps() {
    let dir = tempdir().unwrap();
    append(
        dir.path(),
        "claude",
        &quota(NOW - 10, 10.0, Some(500), None),
    );
    append(
        dir.path(),
        "claude",
        &quota(NOW - 10, 20.0, Some(500), None),
    );

    let history = read_history(dir.path(), "claude", 0, None);
    assert_eq!(history.points.len(), 1);
}

#[test]
fn unchanged_same_reset_observations_are_coalesced() {
    let dir = tempdir().unwrap();
    append(
        dir.path(),
        "claude",
        &quota(NOW - 20, 10.0, Some(500), None),
    );
    append(
        dir.path(),
        "claude",
        &quota(NOW - 10, 10.0, Some(500), None),
    );

    assert_eq!(read_history(dir.path(), "claude", 0, None).points.len(), 1);
}

#[test]
fn a_changed_plan_is_preserved_even_when_windows_match() {
    let dir = tempdir().unwrap();
    append(
        dir.path(),
        "codex",
        &quota(NOW - 20, 10.0, Some(500), Some("free")),
    );
    append(
        dir.path(),
        "codex",
        &quota(NOW - 10, 10.0, Some(500), Some("pro")),
    );

    let history = read_history(dir.path(), "codex", 0, None);
    assert_eq!(
        history
            .points
            .iter()
            .filter_map(|point| point.plan.as_deref())
            .collect::<Vec<_>>(),
        vec!["free", "pro"]
    );
}

#[test]
fn either_window_reset_starts_a_reset_segment() {
    let dir = tempdir().unwrap();
    append(dir.path(), "claude", &dual_quota(NOW - 30, 500, 900));
    append(dir.path(), "claude", &dual_quota(NOW - 20, 600, 900));
    append(dir.path(), "claude", &dual_quota(NOW - 10, 600, 1_000));

    let history = read_history(dir.path(), "claude", 0, None);
    assert_eq!(
        history
            .points
            .iter()
            .map(|point| point.continuity)
            .collect::<Vec<_>>(),
        vec![
            HistoryContinuity::Initial,
            HistoryContinuity::Reset,
            HistoryContinuity::Reset
        ]
    );
}

#[test]
fn a_same_reset_decrease_is_marked_unknown() {
    let dir = tempdir().unwrap();
    append(
        dir.path(),
        "claude",
        &quota(NOW - 20, 50.0, Some(500), None),
    );
    append(
        dir.path(),
        "claude",
        &quota(NOW - 10, 40.0, Some(500), None),
    );

    let history = read_history(dir.path(), "claude", 0, None);
    assert_eq!(history.points[1].continuity, HistoryContinuity::Unknown);
}

#[test]
fn missing_reset_boundaries_are_marked_unknown() {
    let dir = tempdir().unwrap();
    append(dir.path(), "claude", &quota(NOW - 20, 10.0, None, None));
    append(dir.path(), "claude", &quota(NOW - 10, 20.0, None, None));

    let history = read_history(dir.path(), "claude", 0, None);
    assert_eq!(history.points[1].continuity, HistoryContinuity::Unknown);
}

#[test]
fn an_ambiguous_window_prevents_a_simultaneous_reset_from_winning() {
    let dir = tempdir().unwrap();
    append(dir.path(), "claude", &dual_quota(NOW - 20, 500, 900));
    let ambiguous = ProviderQuota {
        observed_at: NOW - 10,
        windows: vec![
            QuotaWindow {
                kind: WindowKind::FiveHour,
                used_percent: 50.0,
                resets_at: Some(600),
            },
            QuotaWindow {
                kind: WindowKind::SevenDay,
                used_percent: 30.0,
                resets_at: None,
            },
        ],
        plan: None,
        error: None,
    };
    append(dir.path(), "claude", &ambiguous);

    let history = read_history(dir.path(), "claude", 0, None);
    assert_eq!(history.points[1].continuity, HistoryContinuity::Unknown);
}

#[test]
fn append_prunes_rows_older_than_thirty_utc_days() {
    let dir = tempdir().unwrap();
    let old = NOW - 30 * 24 * 60 * 60 - 1;
    write_rows(dir.path(), "claude", &[row(old, 10.0, Some(500))]);

    append(
        dir.path(),
        "claude",
        &quota(NOW - 10, 20.0, Some(500), None),
    );

    let history = read_history(dir.path(), "claude", 0, None);
    assert_eq!(
        history
            .points
            .iter()
            .map(|point| point.observed_at)
            .collect::<Vec<_>>(),
        vec![NOW - 10]
    );
}

#[test]
fn retention_evicts_the_oldest_row_across_both_providers() {
    let dir = tempdir().unwrap();
    let plan = "x".repeat(200);
    let first = quota(NOW - 20, 10.0, Some(500), Some(&plan));
    let second = quota(NOW - 10, 20.0, Some(500), Some(&plan));
    record_observation_at(dir.path(), "claude", &first, NOW, 500).unwrap();
    record_observation_at(dir.path(), "codex", &second, NOW, 500).unwrap();

    assert!(read_history(dir.path(), "claude", 0, None)
        .points
        .is_empty());
    assert_eq!(
        read_history(dir.path(), "codex", 0, None).points[0].observed_at,
        NOW - 10
    );
}

#[test]
fn an_oversized_newest_record_leaves_existing_bounded_history_intact() {
    let dir = tempdir().unwrap();
    let existing = quota(NOW - 20, 10.0, Some(500), None);
    record_observation_at(dir.path(), "claude", &existing, NOW, 200).unwrap();
    let plan = "x".repeat(200);
    let newest = quota(NOW - 10, 20.0, Some(500), Some(&plan));

    record_observation_at(dir.path(), "claude", &newest, NOW, 200).unwrap();

    let history = read_history(dir.path(), "claude", 0, None);
    assert_eq!(
        history
            .points
            .iter()
            .map(|point| point.observed_at)
            .collect::<Vec<_>>(),
        vec![NOW - 20]
    );
}

#[test]
fn nonpositive_observations_do_not_create_history() {
    let dir = tempdir().unwrap();

    assert!(
        record_observation_at(dir.path(), "claude", &quota(0, 10.0, None, None), NOW, 200,).is_ok()
    );
    assert!(
        record_observation_at(dir.path(), "claude", &quota(-1, 20.0, None, None), NOW, 200,)
            .is_ok()
    );

    assert!(read_history(dir.path(), "claude", i64::MIN, None)
        .points
        .is_empty());
    assert!(!history_path(dir.path(), "claude").exists());
}

#[test]
fn unknown_provider_does_not_create_any_history_file() {
    let dir = tempdir().unwrap();
    let observation = quota(NOW, 10.0, None, None);

    assert!(record_observation_at(dir.path(), "gemini", &observation, NOW, 200).is_err());

    assert!(!history_path(dir.path(), "claude").exists());
    assert!(!history_path(dir.path(), "codex").exists());
}
