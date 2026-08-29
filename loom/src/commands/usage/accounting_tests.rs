use super::super::transcript::{Request, TokenUsage};
use super::*;
use anyhow::Result;
use chrono::{DateTime, Utc};

fn request(stamp: &str, usage: TokenUsage) -> Result<Request> {
    Ok(Request {
        message_id: None,
        timestamp: stamp.parse::<DateTime<Utc>>()?,
        model: "model".to_owned(),
        usage,
        tool_uses: Vec::new(),
        thinking_chars: 0,
        text_chars: 0,
    })
}

#[test]
fn accounting_applies_known_s2_weights() {
    let usage = TokenUsage {
        input: 100,
        cache_creation: 30,
        cache_read: 50,
        output: 20,
        ephemeral_5m: 10,
        ephemeral_1h: 20,
    };
    let accounting = Accounting::of(&usage);
    assert_eq!(accounting.s1, 200);
    assert_eq!(accounting.s2, 257.5);
    assert_eq!(accounting.s3, 150);
}

#[test]
fn five_hour_gap_splits_but_four_hour_gap_does_not() -> Result<()> {
    let one = request("2026-08-01T00:00:00Z", TokenUsage::default())?;
    let two = request("2026-08-01T04:00:00Z", TokenUsage::default())?;
    let three = request("2026-08-01T09:00:00Z", TokenUsage::default())?;
    let windows = bucket(&[&three, &one, &two], Windowing::FiveHour);
    assert_eq!(windows.len(), 2);
    assert_eq!(windows[0].requests, 2);
    assert_eq!(windows[1].requests, 1);
    Ok(())
}

#[test]
fn iso_week_uses_iso_year_and_week_label() -> Result<()> {
    let request = request("2026-08-29T12:00:00Z", TokenUsage::default())?;
    let windows = bucket(&[&request], Windowing::IsoWeek);
    assert_eq!(windows[0].label, "2026-W35");
    Ok(())
}

#[test]
fn iso_week_windows_never_drop_a_request() -> Result<()> {
    // Every request must land in some window's total, even one whose ISO
    // week fails to resolve to a calendar Monday - a dropped request would
    // silently disagree with the report's own Totals section.
    let one = request("2026-01-01T00:00:00Z", TokenUsage::default())?;
    let two = request("2026-12-31T23:59:59Z", TokenUsage::default())?;
    let windows = bucket(&[&one, &two], Windowing::IsoWeek);
    let total_requests: usize = windows.iter().map(|window| window.requests).sum();
    assert_eq!(total_requests, 2);
    Ok(())
}
