//! Contract tests for the shared text, hash and time helpers in `crate::utils`.

use chrono::{TimeZone, Utc};

use crate::utils::text::single_line;
use crate::utils::{parse_rfc3339_utc, sha256_hex, truncate_for_display};

#[test]
fn sha256_hex_known_digest() {
    let digest = sha256_hex(b"abc");
    assert_eq!(
        digest,
        "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad"
    );
    assert_eq!(digest.len(), 64, "full digest, no prefix or truncation");
}

#[test]
fn rfc3339_offset_normalized_to_utc() {
    let expected = Utc
        .with_ymd_and_hms(2026, 1, 2, 1, 4, 5)
        .single()
        .expect("valid UTC instant");
    let parsed = parse_rfc3339_utc("2026-01-02T03:04:05+02:00");
    assert_eq!(parsed, Some(expected));
    assert_eq!(
        parsed.map(|value| value.to_rfc3339()),
        Some("2026-01-02T01:04:05+00:00".to_string())
    );
    assert_eq!(parse_rfc3339_utc("2026-01-02 03:04:05"), None);
}

#[test]
fn single_line_strips_bidi_and_zero_width() {
    let flattened = single_line("a\u{202E}b\u{200B}c\n\t d");
    assert_eq!(flattened, "abc d");
    assert!(!flattened.contains('\u{202E}'));
    assert!(!flattened.contains('\u{200B}'));
}

#[test]
fn display_truncation_collapses_whitespace() {
    let cut = truncate_for_display("alpha\t\tbeta  gamma", 10);
    assert_eq!(cut, "alpha bet\u{2026}");
    assert_eq!(cut.chars().count(), 10, "ellipsis counts toward the budget");
}
