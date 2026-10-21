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

#[test]
fn since_accepts_rfc3339_instants_in_any_offset() -> Result<()> {
    let now = instant(23);
    let utc = TimeRange::parse("2026-10-14T06:30:00Z", None, now)?;
    let offset = TimeRange::parse("2026-10-14T09:30:00+03:00", None, now)?;

    assert_eq!(utc.since, offset.since);
    assert_eq!(
        utc.since,
        Utc.with_ymd_and_hms(2026, 10, 14, 6, 30, 0)
            .single()
            .unwrap()
    );
    Ok(())
}

#[test]
fn since_date_and_duration_forms_are_unchanged() -> Result<()> {
    let now = instant(23);

    assert_eq!(
        TimeRange::parse("2026-10-14", None, now)?.since,
        Utc.with_ymd_and_hms(2026, 10, 14, 0, 0, 0)
            .single()
            .unwrap()
    );
    assert_eq!(
        TimeRange::parse("7d", None, now)?.since,
        now - chrono::Duration::days(7)
    );
    Ok(())
}
