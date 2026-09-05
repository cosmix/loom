use super::*;
use tempfile::tempdir;

fn sample_quota() -> ProviderQuota {
    ProviderQuota {
        observed_at: 1_788_523_200,
        windows: vec![
            QuotaWindow {
                kind: WindowKind::FiveHour,
                used_percent: 48.0,
                resets_at: Some(1_788_531_180),
            },
            QuotaWindow {
                kind: WindowKind::SevenDay,
                used_percent: 31.0,
                resets_at: Some(1_788_876_000),
            },
        ],
        plan: None,
        error: None,
    }
}

#[test]
fn a_written_quota_round_trips_through_read_provider() {
    let dir = tempdir().unwrap();
    let quota = sample_quota();
    write_provider(dir.path(), "claude", &quota).unwrap();
    assert_eq!(read_provider(dir.path(), "claude"), Some(quota));
}

#[test]
fn write_provider_refuses_a_symlinked_target_and_leaves_it_untouched() {
    let dir = tempdir().unwrap();
    let real_target = dir.path().join("elsewhere.json");
    std::fs::write(&real_target, "untouched").unwrap();

    std::fs::create_dir_all(quota_dir(dir.path())).unwrap();
    let link_path = provider_path(dir.path(), "claude");
    std::os::unix::fs::symlink(&real_target, &link_path).unwrap();

    let result = write_provider(dir.path(), "claude", &sample_quota());
    assert!(result.is_err());
    assert_eq!(std::fs::read_to_string(&real_target).unwrap(), "untouched");
}

#[test]
fn write_provider_unlinks_a_planted_symlink_at_the_tmp_path_instead_of_following_it() {
    let dir = tempdir().unwrap();
    let real_target = dir.path().join("elsewhere.json");
    std::fs::write(&real_target, "untouched").unwrap();

    std::fs::create_dir_all(quota_dir(dir.path())).unwrap();
    let mut tmp_os = provider_path(dir.path(), "claude").into_os_string();
    tmp_os.push(".tmp");
    std::os::unix::fs::symlink(&real_target, PathBuf::from(tmp_os)).unwrap();

    let quota = sample_quota();
    write_provider(dir.path(), "claude", &quota).unwrap();

    assert_eq!(std::fs::read_to_string(&real_target).unwrap(), "untouched");
    assert_eq!(read_provider(dir.path(), "claude"), Some(quota));
}

#[test]
fn an_oversized_file_reads_as_none_but_is_left_in_place() {
    let dir = tempdir().unwrap();
    std::fs::create_dir_all(quota_dir(dir.path())).unwrap();
    let path = provider_path(dir.path(), "claude");
    let padding = "{".repeat(70 * 1024);
    std::fs::write(&path, &padding).unwrap();

    assert_eq!(read_provider(dir.path(), "claude"), None);
    assert!(path.exists());
}

#[test]
fn malformed_json_reads_as_none() {
    let dir = tempdir().unwrap();
    std::fs::create_dir_all(quota_dir(dir.path())).unwrap();
    let path = provider_path(dir.path(), "claude");
    std::fs::write(&path, "not json").unwrap();

    assert_eq!(read_provider(dir.path(), "claude"), None);
}

#[test]
fn an_absent_file_reads_as_none() {
    let dir = tempdir().unwrap();
    assert_eq!(read_provider(dir.path(), "claude"), None);
}

#[test]
fn record_failure_preserves_the_last_good_windows_and_sets_error() {
    let dir = tempdir().unwrap();
    let quota = sample_quota();
    write_provider(dir.path(), "claude", &quota).unwrap();

    record_failure(dir.path(), "claude", "HTTP 500").unwrap();

    let after = read_provider(dir.path(), "claude").unwrap();
    assert_eq!(after.observed_at, quota.observed_at);
    assert_eq!(after.windows, quota.windows);
    assert_eq!(after.error, Some("HTTP 500".to_string()));
}

#[test]
fn record_failure_writes_nothing_when_no_prior_file_exists() {
    let dir = tempdir().unwrap();
    record_failure(dir.path(), "claude", "no claude.ai login").unwrap();
    assert!(!provider_path(dir.path(), "claude").exists());
}

#[test]
fn a_successful_write_after_a_failure_clears_the_error() {
    let dir = tempdir().unwrap();
    let mut quota = sample_quota();
    quota.error = Some("stale error".to_string());
    write_provider(dir.path(), "claude", &quota).unwrap();
    assert_eq!(
        read_provider(dir.path(), "claude").unwrap().error,
        Some("stale error".to_string())
    );

    let fresh = sample_quota();
    write_provider(dir.path(), "claude", &fresh).unwrap();
    assert_eq!(read_provider(dir.path(), "claude").unwrap().error, None);
}

#[test]
fn an_out_of_range_percentage_is_clamped_on_read() {
    let dir = tempdir().unwrap();
    std::fs::create_dir_all(quota_dir(dir.path())).unwrap();
    let path = provider_path(dir.path(), "claude");
    std::fs::write(
        &path,
        r#"{
            "observed_at": 100,
            "windows": [
                { "kind": "seven-day", "used_percent": 151.0, "resets_at": null }
            ],
            "plan": null,
            "error": null
        }"#,
    )
    .unwrap();

    let quota = read_provider(dir.path(), "claude").unwrap();
    assert_eq!(quota.windows.len(), 1);
    assert_eq!(quota.windows[0].kind, WindowKind::SevenDay);
    assert_eq!(quota.windows[0].used_percent, 100.0);
}

#[test]
fn sanitize_drops_a_non_finite_window() {
    // JSON has no literal for NaN/Infinity, so a non-finite `used_percent`
    // can only arise from an in-process value (e.g. a future caller that
    // builds a `ProviderQuota` directly) - exercise `sanitize` itself rather
    // than going through a JSON file.
    let quota = ProviderQuota {
        observed_at: 100,
        windows: vec![
            QuotaWindow {
                kind: WindowKind::FiveHour,
                used_percent: f64::NAN,
                resets_at: None,
            },
            QuotaWindow {
                kind: WindowKind::SevenDay,
                used_percent: 40.0,
                resets_at: None,
            },
        ],
        plan: None,
        error: None,
    };

    let sanitized = sanitize(quota);
    assert_eq!(sanitized.windows.len(), 1);
    assert_eq!(sanitized.windows[0].kind, WindowKind::SevenDay);
}

#[test]
fn at_most_one_window_per_kind_is_kept_five_hour_first() {
    let dir = tempdir().unwrap();
    std::fs::create_dir_all(quota_dir(dir.path())).unwrap();
    let path = provider_path(dir.path(), "claude");
    std::fs::write(
        &path,
        r#"{
            "observed_at": 100,
            "windows": [
                { "kind": "seven-day", "used_percent": 10.0, "resets_at": null },
                { "kind": "five-hour", "used_percent": 20.0, "resets_at": null },
                { "kind": "five-hour", "used_percent": 30.0, "resets_at": null }
            ],
            "plan": null,
            "error": null
        }"#,
    )
    .unwrap();

    let quota = read_provider(dir.path(), "claude").unwrap();
    assert_eq!(quota.windows.len(), 2);
    assert_eq!(quota.windows[0].kind, WindowKind::FiveHour);
    assert_eq!(quota.windows[0].used_percent, 20.0);
    assert_eq!(quota.windows[1].kind, WindowKind::SevenDay);
}
