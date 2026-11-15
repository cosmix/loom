use super::*;

#[test]
fn sha256_hex_matches_the_known_empty_digest() {
    assert_eq!(
        sha256_hex(b""),
        "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855"
    );
}

#[test]
fn parse_rfc3339_utc_normalizes_offsets() {
    let parsed = parse_rfc3339_utc("2026-01-02T03:04:05+02:00").unwrap();
    assert_eq!(parsed.to_rfc3339(), "2026-01-02T01:04:05+00:00");
}

#[test]
fn parse_rfc3339_utc_rejects_non_rfc3339_timestamps() {
    assert_eq!(parse_rfc3339_utc("2026-01-02 03:04:05"), None);
}

#[test]
fn truncate_for_display_collapses_every_whitespace_run() {
    assert_eq!(truncate_for_display("a\t\tb  c\nd", 20), "a b c d");
    assert_eq!(truncate_for_display("  a\u{a0}\tb  ", 3), "a b");
    assert_eq!(truncate_for_display("a\t\tb  c\nd", 5), "a b …");
}

#[test]
fn format_thousands_groups_from_the_right() {
    assert_eq!(format_thousands(0), "0");
    assert_eq!(format_thousands(999), "999");
    assert_eq!(format_thousands(1_000), "1,000");
    assert_eq!(format_thousands(147_000), "147,000");
    assert_eq!(format_thousands(1_234_567), "1,234,567");
}

#[test]
fn control_sequences_are_withheld_from_a_stdout_that_is_not_a_terminal() {
    let mut redirected = Vec::new();
    write_terminal_sequence(&mut redirected, false, ATTR_RESET.as_bytes());
    assert!(redirected.is_empty(), "a redirected stdout carries payload");

    let mut terminal = Vec::new();
    write_terminal_sequence(&mut terminal, true, ATTR_RESET.as_bytes());
    assert_eq!(terminal, ATTR_RESET.as_bytes());
}

#[test]
fn test_format_elapsed_seconds() {
    assert_eq!(format_elapsed(0), "0s");
    assert_eq!(format_elapsed(30), "30s");
    assert_eq!(format_elapsed(59), "59s");
}

#[test]
fn test_format_elapsed_minutes() {
    assert_eq!(format_elapsed(60), "1m0s");
    assert_eq!(format_elapsed(90), "1m30s");
    assert_eq!(format_elapsed(3599), "59m59s");
}

#[test]
fn test_format_elapsed_hours() {
    assert_eq!(format_elapsed(3600), "1h0m");
    assert_eq!(format_elapsed(3661), "1h1m");
    assert_eq!(format_elapsed(7200), "2h0m");
}

#[test]
fn test_format_elapsed_verbose_seconds() {
    assert_eq!(format_elapsed_verbose(0), "0s");
    assert_eq!(format_elapsed_verbose(30), "30s");
    assert_eq!(format_elapsed_verbose(59), "59s");
}

#[test]
fn test_format_elapsed_verbose_minutes() {
    assert_eq!(format_elapsed_verbose(60), "1m 0s");
    assert_eq!(format_elapsed_verbose(90), "1m 30s");
    assert_eq!(format_elapsed_verbose(3599), "59m 59s");
}

#[test]
fn test_format_elapsed_verbose_hours() {
    assert_eq!(format_elapsed_verbose(3600), "1h 0m 0s");
    assert_eq!(format_elapsed_verbose(3661), "1h 1m 1s");
    assert_eq!(format_elapsed_verbose(7200), "2h 0m 0s");
}

#[test]
fn test_truncate() {
    assert_eq!(truncate("hello", 10), "hello");
    assert_eq!(truncate("hello world", 8), "hello...");
    assert_eq!(truncate("12345", 5), "12345");
    assert_eq!(truncate("12345", 6), "12345");
}

#[test]
fn test_truncate_utf8() {
    let emoji_str = "Hello 🦀 world";
    let result = truncate(emoji_str, 10);
    assert_eq!(result, "Hello 🦀...");
    assert!(result.is_char_boundary(result.len()));
}

#[test]
fn test_truncate_very_short() {
    assert_eq!(truncate("hello", 3), "...");
    assert_eq!(truncate("hello", 2), "...");
}

#[test]
fn test_truncate_for_display() {
    assert_eq!(truncate_for_display("short", 10), "short");
    assert_eq!(
        truncate_for_display("this is a longer string", 10),
        "this is a…"
    );
    assert_eq!(
        truncate_for_display("line1\nline2\nline3", 20),
        "line1 line2 line3"
    );
}

#[test]
fn test_truncate_for_display_utf8() {
    let emoji_str = "Hello 🦀 world!";
    let result = truncate_for_display(emoji_str, 10);
    assert_eq!(result, "Hello 🦀 w…");
    assert!(result.is_char_boundary(result.len()));
}

#[test]
fn test_truncate_for_display_exact_length() {
    let s = "12345";
    assert_eq!(truncate_for_display(s, 5), "12345");
    assert_eq!(truncate_for_display(s, 6), "12345");
}
