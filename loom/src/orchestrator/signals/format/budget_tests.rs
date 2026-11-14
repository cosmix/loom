use super::*;

const NOW: i64 = 1_000_000;

fn reading(percent: f64, reset: Option<i64>) -> ProviderQuota {
    ProviderQuota {
        observed_at: NOW,
        windows: vec![QuotaWindow {
            kind: WindowKind::SevenDay,
            used_percent: percent,
            resets_at: reset,
        }],
        plan: None,
        error: None,
    }
}

fn claude(reading: ProviderQuota) -> QuotaSnapshot {
    QuotaSnapshot {
        claude: Some(reading),
        codex: None,
    }
}

#[test]
fn missing_stale_and_failed_readings_are_omitted() {
    assert_eq!(
        format_model_budget_section(&QuotaSnapshot::default(), NOW),
        None
    );
    let mut stale = reading(80.0, None);
    stale.observed_at = NOW - quota::STALE_AFTER_SECS;
    let mut failed = reading(80.0, None);
    failed.error = Some("failed".into());
    let snapshot = QuotaSnapshot {
        claude: Some(stale),
        codex: Some(failed),
    };
    assert_eq!(format_model_budget_section(&snapshot, NOW), None);
}

#[test]
fn claude_pressure_depends_on_percent_and_reset_boundaries() {
    for (percent, seconds, expected) in [
        (80.0, 172_800, true),
        (80.0, 600, false),
        (75.0, 1801, true),
        (74.9, 1801, false),
        (80.0, 1800, false),
        (80.0, 1801, true),
    ] {
        let text = format_model_budget_section(&claude(reading(percent, Some(NOW + seconds))), NOW)
            .unwrap();
        assert_eq!(
            text.contains(CLAUDE_ROUTING),
            expected,
            "{percent}%, {seconds}s: {text}"
        );
    }
    assert!(
        format_model_budget_section(&claude(reading(80.0, None)), NOW)
            .unwrap()
            .contains(CLAUDE_ROUTING)
    );
}

#[test]
fn codex_and_both_providers_have_distinct_routing() {
    let mut snapshot = QuotaSnapshot {
        claude: None,
        codex: Some(reading(80.0, None)),
    };
    assert!(format_model_budget_section(&snapshot, NOW)
        .unwrap()
        .contains(CODEX_ROUTING));
    snapshot.claude = Some(reading(80.0, None));
    let text = format_model_budget_section(&snapshot, NOW).unwrap();
    assert!(text.contains(BOTH_ROUTING));
    assert!(!text.contains(CLAUDE_ROUTING));
    assert!(!text.contains(CODEX_ROUTING));
}

#[test]
fn windows_are_ordered_and_reset_times_render_correctly() {
    let mut provider = reading(81.0, Some(NOW + 3 * 86_400 + 4 * 3600));
    provider.windows.push(QuotaWindow {
        kind: WindowKind::FiveHour,
        used_percent: 62.4,
        resets_at: Some(NOW + 2 * 3600 + 10 * 60),
    });
    let text = format_model_budget_section(&claude(provider), NOW).unwrap();
    assert!(text.contains("Claude: 5h 62% used, resets in 2h10m; 7d 81% used, resets in 3d4h."));
    for reset in [NOW, NOW - 1] {
        let text = format_model_budget_section(&claude(reading(80.0, Some(reset))), NOW).unwrap();
        assert!(text.contains("7d 80% used, resets now."));
        assert!(!text.contains("Routing:"));
    }
}
