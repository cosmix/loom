use anyhow::Result;
use chrono::{TimeZone, Utc};

use super::*;

fn instant(hour: u32) -> DateTime<Utc> {
    Utc.with_ymd_and_hms(2026, 9, 12, hour, 0, 0)
        .single()
        .expect("valid fixture timestamp")
}

#[test]
fn inclusive_bounds_include_both_endpoints() -> Result<()> {
    let range = TimeRange::parse("2026-09-12", Some("2026-09-12T20:00:00Z"), instant(23))?;

    assert!(range.includes(instant(0)));
    assert!(range.includes(instant(20)));
    assert!(!range.includes(instant(21)));
    Ok(())
}

#[test]
fn duration_is_resolved_against_supplied_command_start() -> Result<()> {
    let range = TimeRange::parse("2h", None, instant(20))?;

    assert_eq!(range.since, instant(18));
    assert!(range.until.is_none());
    Ok(())
}

#[test]
fn malformed_naive_and_reversed_until_are_rejected() {
    let now = instant(20);

    assert!(TimeRange::parse("1h", Some("not-a-time"), now).is_err());
    assert!(TimeRange::parse("1h", Some("2026-09-12T20:00:00"), now).is_err());
    assert!(TimeRange::parse("1h", Some("2026-09-12T18:59:59Z"), now).is_err());
}

#[test]
fn offset_aware_until_is_normalized_to_utc() -> Result<()> {
    let range = TimeRange::parse("2026-09-12", Some("2026-09-12T22:00:00+02:00"), instant(23))?;

    assert_eq!(range.until, Some(instant(20)));
    Ok(())
}
