use serde::Deserialize;

use super::{is_safe_id, ForwardBackend, ForwardState};

pub const START_PREFIX: &str = "LOOM-FORWARD-START ";
pub const END_PREFIX: &str = "LOOM-FORWARD-END ";
pub const SEPARATOR: &str = "--- LOOM-FORWARD-OUTPUT ---";
pub const MAX_PREFIX_BYTES: usize = 4096;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StartMarker {
    pub backend: ForwardBackend,
    pub backend_id: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EndMarker {
    pub backend: ForwardBackend,
    pub backend_id: String,
    pub outcome: ForwardState,
    pub exit_code: i32,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum MarkerChannel {
    Absent,
    Streaming(StartMarker),
    Started(StartMarker),
    Finished(StartMarker, EndMarker),
    Invalid(&'static str),
}

pub fn decode_marker_channel(text: &str) -> MarkerChannel {
    let prefix = bounded_prefix(text);
    if prefix.is_empty() {
        return MarkerChannel::Absent;
    }
    let (first, rest) = split_first_line(prefix);
    if first.starts_with(END_PREFIX) {
        return MarkerChannel::Invalid("end before start");
    }
    if !first.starts_with(START_PREFIX) {
        return MarkerChannel::Absent;
    }
    let start = match parse_start(first) {
        Ok(start) => start,
        Err(ParseFailure::Incomplete) if rest.is_none() => {
            return MarkerChannel::Invalid("incomplete start marker");
        }
        Err(_) => return MarkerChannel::Invalid("invalid start marker"),
    };
    let Some(rest) = rest else {
        return MarkerChannel::Streaming(start);
    };
    decode_after_start(start, rest)
}

fn decode_after_start(start: StartMarker, mut remaining: &str) -> MarkerChannel {
    let mut end = None;
    loop {
        let (line, rest) = split_first_line(remaining);
        if line == SEPARATOR {
            return match end {
                Some(end) => MarkerChannel::Finished(start, end),
                None => MarkerChannel::Started(start),
            };
        }
        if line.starts_with(START_PREFIX) {
            return MarkerChannel::Invalid("duplicate start marker");
        }
        if line.starts_with(END_PREFIX) {
            if end.is_some() {
                return MarkerChannel::Invalid("duplicate end marker");
            }
            match parse_end(line) {
                Ok(parsed)
                    if parsed.backend != start.backend || parsed.backend_id != start.backend_id =>
                {
                    return MarkerChannel::Invalid("end marker identity mismatch");
                }
                Ok(parsed) => end = Some(parsed),
                Err(ParseFailure::Incomplete) if rest.is_none() => {
                    return MarkerChannel::Streaming(start)
                }
                Err(_) => return MarkerChannel::Invalid("invalid end marker"),
            }
        } else if rest.is_none() && incomplete_allowed(line) {
            return MarkerChannel::Streaming(start);
        } else {
            return MarkerChannel::Invalid("unexpected marker prefix line");
        }
        let Some(rest) = rest else {
            return MarkerChannel::Streaming(start);
        };
        remaining = rest;
    }
}

fn bounded_prefix(text: &str) -> &str {
    let mut end = text.len().min(MAX_PREFIX_BYTES);
    while !text.is_char_boundary(end) {
        end -= 1;
    }
    &text[..end]
}

fn split_first_line(text: &str) -> (&str, Option<&str>) {
    match text.find('\n') {
        Some(index) => (
            text[..index].strip_suffix('\r').unwrap_or(&text[..index]),
            Some(&text[index + 1..]),
        ),
        None => (text.strip_suffix('\r').unwrap_or(text), None),
    }
}

fn incomplete_allowed(line: &str) -> bool {
    line.is_empty()
        || START_PREFIX.starts_with(line)
        || END_PREFIX.starts_with(line)
        || SEPARATOR.starts_with(line)
}

#[derive(Debug, Clone, Copy)]
enum ParseFailure {
    Incomplete,
    Invalid,
}

fn parse_start(line: &str) -> Result<StartMarker, ParseFailure> {
    let body = line
        .strip_prefix(START_PREFIX)
        .ok_or(ParseFailure::Invalid)?;
    let _: serde_json::Value = serde_json::from_str(body).map_err(classify_json_error)?;
    if let Ok(wire) = serde_json::from_str::<CompanionStart>(body) {
        return validate_start(wire.v, wire.backend, ForwardBackend::Companion, wire.job_id);
    }
    let wire: DirectStart = serde_json::from_str(body).map_err(|_| ParseFailure::Invalid)?;
    validate_start(wire.v, wire.backend, ForwardBackend::Direct, wire.thread_id)
}

fn validate_start(
    version: u16,
    actual: ForwardBackend,
    expected: ForwardBackend,
    backend_id: String,
) -> Result<StartMarker, ParseFailure> {
    if version != 1 || actual != expected || !is_safe_id(&backend_id) {
        return Err(ParseFailure::Invalid);
    }
    Ok(StartMarker {
        backend: actual,
        backend_id,
    })
}

fn parse_end(line: &str) -> Result<EndMarker, ParseFailure> {
    let body = line.strip_prefix(END_PREFIX).ok_or(ParseFailure::Invalid)?;
    let _: serde_json::Value = serde_json::from_str(body).map_err(classify_json_error)?;
    if let Ok(wire) = serde_json::from_str::<CompanionEnd>(body) {
        return validate_end(
            wire.v,
            wire.backend,
            ForwardBackend::Companion,
            wire.job_id,
            wire.outcome,
            wire.exit_code,
        );
    }
    let wire: DirectEnd = serde_json::from_str(body).map_err(|_| ParseFailure::Invalid)?;
    validate_end(
        wire.v,
        wire.backend,
        ForwardBackend::Direct,
        wire.thread_id,
        wire.outcome,
        wire.exit_code,
    )
}

fn validate_end(
    version: u16,
    actual: ForwardBackend,
    expected: ForwardBackend,
    backend_id: String,
    outcome: ForwardState,
    exit_code: i32,
) -> Result<EndMarker, ParseFailure> {
    if version != 1 || actual != expected || !is_safe_id(&backend_id) || !outcome.is_terminal() {
        return Err(ParseFailure::Invalid);
    }
    Ok(EndMarker {
        backend: actual,
        backend_id,
        outcome,
        exit_code,
    })
}

fn classify_json_error(error: serde_json::Error) -> ParseFailure {
    if error.is_eof() {
        ParseFailure::Incomplete
    } else {
        ParseFailure::Invalid
    }
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct CompanionStart {
    v: u16,
    backend: ForwardBackend,
    job_id: String,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct DirectStart {
    v: u16,
    backend: ForwardBackend,
    thread_id: String,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct CompanionEnd {
    v: u16,
    backend: ForwardBackend,
    job_id: String,
    outcome: ForwardState,
    exit_code: i32,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct DirectEnd {
    v: u16,
    backend: ForwardBackend,
    thread_id: String,
    outcome: ForwardState,
    exit_code: i32,
}

#[cfg(test)]
#[path = "marker_tests.rs"]
mod tests;
