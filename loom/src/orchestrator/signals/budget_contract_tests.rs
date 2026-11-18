//! Contracts for the `## Model Budget` signal section: stale or failed quota
//! readings are never printed as current, and the pressure rule weighs how
//! soon the pressed window resets.

use super::format::budget::format_model_budget_section;
use crate::quota::{ProviderQuota, QuotaSnapshot, QuotaWindow, WindowKind};

const NOW: i64 = 1_800_000_000;

fn reading(observed_at: i64, window: QuotaWindow, error: Option<&str>) -> ProviderQuota {
    ProviderQuota {
        observed_at,
        windows: vec![window],
        plan: None,
        error: error.map(str::to_owned),
    }
}

fn window(kind: WindowKind, used_percent: f64, resets_at: Option<i64>) -> QuotaWindow {
    QuotaWindow {
        kind,
        used_percent,
        resets_at,
    }
}

#[test]
fn stale_or_failed_readings_are_not_printed_as_current() {
    let snapshot = QuotaSnapshot {
        claude: Some(reading(
            NOW - 900,
            window(WindowKind::FiveHour, 90.0, Some(NOW + 3600)),
            None,
        )),
        codex: Some(reading(
            NOW,
            window(WindowKind::SevenDay, 50.0, Some(NOW + 172_800)),
            Some("timeout"),
        )),
    };

    let section = format_model_budget_section(&snapshot, NOW);

    assert_eq!(
        section, None,
        "a reading observed 900 s ago and a failed reading must both be omitted"
    );
}

#[test]
fn pressure_needs_a_window_that_does_not_reset_soon() {
    let pressed = |resets_in: i64| QuotaSnapshot {
        claude: Some(reading(
            NOW,
            window(WindowKind::SevenDay, 80.0, Some(NOW + resets_in)),
            None,
        )),
        codex: None,
    };

    let far = format_model_budget_section(&pressed(172_800), NOW)
        .expect("a fresh claude reading gives a section");
    assert!(far.contains("## Model Budget"), "section={far}");
    assert!(
        far.contains("Routing: Claude quota is under pressure."),
        "80% with two days to reset is pressure: section={far}"
    );

    let soon = format_model_budget_section(&pressed(600), NOW)
        .expect("a fresh claude reading gives a section");
    assert!(soon.contains("## Model Budget"), "section={soon}");
    assert!(
        !soon.contains("Routing:"),
        "a window resetting in 600 s is no pressure: section={soon}"
    );
}
