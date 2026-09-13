//! The `LOOM_RELAY_V1` line a CLI ticket writer appends to its own stdout,
//! and the relay hook scans a captured tool call's output for.

use super::kind::RequestKind;
use super::{MAX_LINES_PER_CALL, MAX_LINE_BYTES};
use std::collections::HashSet;

const PREFIX: &str = "LOOM_RELAY_V1";
const ID_HEX_LEN: usize = 32;
const SHA256_HEX_LEN: usize = 64;
const MAX_BYTES_DIGITS: usize = 7;

/// One parsed relay line: a request ticket's kind, id and integrity guard.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RelayLine {
    pub kind: RequestKind,
    pub id: String,
    pub sha256: String,
    pub bytes: u32,
}

impl RelayLine {
    /// Render the wire form the CLI writes as its last stdout line.
    pub fn format(&self) -> String {
        format!(
            "{PREFIX} kind={} id={} sha256={} bytes={}",
            self.kind, self.id, self.sha256, self.bytes
        )
    }

    /// Parse one whole line. `line` must not itself contain a `\n`; a single
    /// trailing `\r` (as splitting CRLF text on `\n` leaves behind) is
    /// stripped first.
    pub fn parse(line: &str) -> Option<RelayLine> {
        let line = line.strip_suffix('\r').unwrap_or(line);
        if !line.is_ascii() || line.len() > MAX_LINE_BYTES {
            return None;
        }
        let rest = line.strip_prefix(PREFIX)?.strip_prefix(' ')?;
        let mut fields = rest.split(' ');
        let kind = strip_field(fields.next()?, "kind=")?
            .parse::<RequestKind>()
            .ok()?;
        let id = parse_hex_field(fields.next()?, "id=", ID_HEX_LEN)?;
        let sha256 = parse_hex_field(fields.next()?, "sha256=", SHA256_HEX_LEN)?;
        let bytes = parse_bytes_field(strip_field(fields.next()?, "bytes=")?)?;
        if fields.next().is_some() {
            return None;
        }
        Some(RelayLine {
            kind,
            id,
            sha256,
            bytes,
        })
    }

    /// Extract every whole `LOOM_RELAY_V1` line from `text`, keyed by id
    /// (first occurrence wins), capped at [`MAX_LINES_PER_CALL`].
    pub fn extract(text: &str) -> Vec<RelayLine> {
        let mut seen = HashSet::new();
        let mut lines = Vec::new();
        for raw in text.split('\n') {
            if lines.len() >= MAX_LINES_PER_CALL {
                break;
            }
            if let Some(parsed) = RelayLine::parse(raw) {
                if seen.insert(parsed.id.clone()) {
                    lines.push(parsed);
                }
            }
        }
        lines
    }
}

fn strip_field<'a>(field: &'a str, prefix: &str) -> Option<&'a str> {
    field.strip_prefix(prefix)
}

fn parse_hex_field(field: &str, prefix: &str, expected_len: usize) -> Option<String> {
    let value = strip_field(field, prefix)?;
    let is_lowercase_hex = value.len() == expected_len
        && value
            .bytes()
            .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b));
    is_lowercase_hex.then(|| value.to_string())
}

/// `1-7` digits, no leading zero except a lone `0` — which is itself invalid,
/// since a ticket is never empty.
fn parse_bytes_field(value: &str) -> Option<u32> {
    if value.is_empty()
        || value.len() > MAX_BYTES_DIGITS
        || !value.bytes().all(|b| b.is_ascii_digit())
    {
        return None;
    }
    if value.len() > 1 && value.starts_with('0') {
        return None;
    }
    let parsed: u32 = value.parse().ok()?;
    (parsed != 0).then_some(parsed)
}

#[cfg(test)]
mod tests {
    use super::*;

    const VALID: &str = "LOOM_RELAY_V1 kind=memory id=4f1c9e0a7b2d4c6e8f00112233445566 \
        sha256=e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855 bytes=42";

    fn valid_line() -> RelayLine {
        RelayLine {
            kind: RequestKind::Memory,
            id: "4f1c9e0a7b2d4c6e8f00112233445566".to_string(),
            sha256: "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855".to_string(),
            bytes: 42,
        }
    }

    #[test]
    fn format_round_trips_through_parse() {
        let line = valid_line();
        assert_eq!(RelayLine::parse(&line.format()), Some(line));
    }

    #[test]
    fn accepts_a_trailing_carriage_return_left_by_a_crlf_split() {
        let with_cr = format!("{VALID}\r");
        assert_eq!(RelayLine::parse(&with_cr), Some(valid_line()));
    }

    #[test]
    fn extract_dedupes_by_id_and_caps_at_sixteen() {
        let mut text = String::new();
        for _ in 0..20 {
            text.push_str(VALID);
            text.push('\n');
        }
        let lines = RelayLine::extract(&text);
        assert_eq!(lines.len(), 1, "20 identical ids collapse to one");

        let mut distinct = String::new();
        for n in 0..20u32 {
            distinct.push_str(&format!(
                "LOOM_RELAY_V1 kind=memory id={:032x} \
                 sha256=e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855 bytes=1\n",
                n
            ));
        }
        assert_eq!(RelayLine::extract(&distinct).len(), MAX_LINES_PER_CALL);
    }

    #[test]
    fn rejects_malformed_lines() {
        let cases: &[(&str, String)] = &[
            ("prefix mid-line", format!("x {VALID}")),
            ("trailing text", format!("{VALID} oops")),
            (
                "uppercase hex",
                VALID.replacen(
                    "id=4f1c9e0a7b2d4c6e8f00112233445566",
                    "id=4F1C9E0A7B2D4C6E8F00112233445566",
                    1,
                ),
            ),
            (
                "wrong field order",
                VALID.replacen(
                    "kind=memory id=4f1c9e0a7b2d4c6e8f00112233445566",
                    "id=4f1c9e0a7b2d4c6e8f00112233445566 kind=memory",
                    1,
                ),
            ),
            (
                "161 bytes",
                format!("{VALID}{}", "a".repeat(161 - VALID.len())),
            ),
            (
                "non-ASCII",
                VALID.replacen("sha256=e3b0c44298", "sha256=\u{e9}3b0c44298", 1),
            ),
            (
                "missing field",
                VALID.trim_end_matches(" bytes=42").to_string(),
            ),
            ("extra field", format!("{VALID} bytes=42")),
            ("leading space", format!(" {VALID}")),
            ("bytes=0", VALID.replacen("bytes=42", "bytes=0", 1)),
            (
                "bytes with 8 digits",
                VALID.replacen("bytes=42", "bytes=12345678", 1),
            ),
        ];

        for (name, input) in cases {
            assert_eq!(RelayLine::parse(input), None, "case: {name}");
        }
    }
}
