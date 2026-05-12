//! Tool-event analysis for stuck-session detection.
//!
//! Reads recent tool events for a session and computes whether the session
//! appears to be stuck by looking at the failure ratio within a rolling window.

use std::io;
use std::path::Path;

use chrono::{DateTime, Utc};

use crate::hooks::events::tail_tool_events;

/// Rolling window length in seconds.
pub const STUCK_WINDOW_SECS: u64 = 60;

/// Minimum number of failure-shaped events required to flag a session as stuck.
pub const STUCK_MIN_EVENTS: u32 = 5;

/// Minimum failure ratio (failure_count / window_events) to flag as stuck.
pub const STUCK_FAILURE_RATIO: f32 = 0.80;

/// Summary of recent tool-event activity for a single session.
#[derive(Debug, Clone)]
pub struct ToolAnalysis {
    /// Number of events within the rolling window.
    pub recent_events: usize,
    /// Number of failure-shaped events within the rolling window.
    pub recent_failure_count: u32,
    /// Ratio of failure-shaped events to total window events (0.0 when window is empty).
    pub recent_failure_ratio: f32,
    /// Seconds since the newest event observed for this session (0 when no events).
    pub last_event_age_secs: u64,
}

impl ToolAnalysis {
    /// Returns `true` when the session looks stuck based on failure heuristics.
    pub fn is_possibly_stuck(&self) -> bool {
        self.recent_failure_count >= STUCK_MIN_EVENTS
            && self.recent_failure_ratio >= STUCK_FAILURE_RATIO
    }
}

/// Analyse the last 50 tool events for `session_id` and return a [`ToolAnalysis`].
///
/// Events are read from `<work_dir>/tool-events.jsonl`.  Returns an empty
/// analysis (all zeros) when no events exist for the session.
pub fn analyze_session(work_dir: &Path, session_id: &str) -> io::Result<ToolAnalysis> {
    let all_events = tail_tool_events(work_dir, 50)?;

    // Keep only events that belong to the requested session.
    let session_events: Vec<_> = all_events
        .into_iter()
        .filter(|e| e.session_id == session_id)
        .collect();

    if session_events.is_empty() {
        return Ok(ToolAnalysis {
            recent_events: 0,
            recent_failure_count: 0,
            recent_failure_ratio: 0.0,
            last_event_age_secs: 0,
        });
    }

    // Parse timestamps; skip events whose ts cannot be parsed.
    let parsed: Vec<(DateTime<Utc>, &crate::hooks::events::ToolEvent)> = session_events
        .iter()
        .filter_map(|e| e.ts.parse::<DateTime<Utc>>().ok().map(|ts| (ts, e)))
        .collect();

    if parsed.is_empty() {
        return Ok(ToolAnalysis {
            recent_events: 0,
            recent_failure_count: 0,
            recent_failure_ratio: 0.0,
            last_event_age_secs: 0,
        });
    }

    // Newest event timestamp.
    let newest_ts = parsed.iter().map(|(ts, _)| *ts).max().unwrap();

    // Age of the newest event relative to now.
    let last_event_age_secs = (Utc::now() - newest_ts).num_seconds().max(0) as u64;

    // Filter to events within the rolling window.
    let window_secs = STUCK_WINDOW_SECS as i64;
    let windowed: Vec<_> = parsed
        .iter()
        .filter(|(ts, _)| (newest_ts - *ts).num_seconds() <= window_secs)
        .collect();

    let recent_events = windowed.len();

    // Classify each windowed event as failure-shaped.
    // `is_error == true` → failure.
    // `output_bytes == Some(0)` → failure (empty output, likely an error).
    // `output_bytes == None`    → NOT a failure (field absent means unknown).
    let recent_failure_count = windowed
        .iter()
        .filter(|(_, e)| e.is_error || e.output_bytes == Some(0))
        .count() as u32;

    let recent_failure_ratio = if recent_events == 0 {
        0.0
    } else {
        recent_failure_count as f32 / recent_events as f32
    };

    Ok(ToolAnalysis {
        recent_events,
        recent_failure_count,
        recent_failure_ratio,
        last_event_age_secs,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;
    use tempfile::TempDir;

    /// Write a `tool-events.jsonl` file into the given temp directory.
    fn write_events(dir: &TempDir, lines: &[&str]) {
        let path = dir.path().join("tool-events.jsonl");
        let mut f = std::fs::File::create(path).unwrap();
        for line in lines {
            writeln!(f, "{line}").unwrap();
        }
    }

    /// Build a minimal JSON event line with the given fields.
    fn make_event(ts: &str, session_id: &str, is_error: bool, output_bytes: Option<u64>) -> String {
        let ob = match output_bytes {
            Some(n) => format!(",\"output_bytes\":{n}"),
            None => String::new(),
        };
        format!(
            r#"{{"ts":"{ts}","tool":"Bash","is_error":{is_error},"session_id":"{session_id}","stage_id":"st-1"{ob}}}"#
        )
    }

    // -----------------------------------------------------------------------
    // Test 1: no tool-events.jsonl file → all zeros
    // -----------------------------------------------------------------------
    #[test]
    fn empty_file() {
        let dir = TempDir::new().unwrap();
        // No file written — directory is empty.
        let analysis = analyze_session(dir.path(), "sess-1").unwrap();
        assert_eq!(analysis.recent_events, 0);
        assert_eq!(analysis.recent_failure_count, 0);
        assert_eq!(analysis.recent_failure_ratio, 0.0);
    }

    // -----------------------------------------------------------------------
    // Test 2: one non-failure event → recent_events=1, failure_count=0
    // -----------------------------------------------------------------------
    #[test]
    fn single_non_failure_event() {
        let dir = TempDir::new().unwrap();
        let line = make_event("2026-01-01T00:00:01Z", "sess-1", false, Some(100));
        write_events(&dir, &[&line]);

        let analysis = analyze_session(dir.path(), "sess-1").unwrap();
        assert_eq!(analysis.recent_events, 1);
        assert_eq!(analysis.recent_failure_count, 0);
        assert!(!analysis.is_possibly_stuck());
    }

    // -----------------------------------------------------------------------
    // Test 3: 6 failure events all within window → is_possibly_stuck() == true
    // -----------------------------------------------------------------------
    #[test]
    fn all_failure_window() {
        let dir = TempDir::new().unwrap();
        // All events share the same timestamp so they're trivially within window.
        let lines: Vec<String> = (0..6)
            .map(|_| make_event("2026-01-01T00:00:01Z", "sess-1", true, None))
            .collect();
        let refs: Vec<&str> = lines.iter().map(|s| s.as_str()).collect();
        write_events(&dir, &refs);

        let analysis = analyze_session(dir.path(), "sess-1").unwrap();
        assert_eq!(analysis.recent_events, 6);
        assert_eq!(analysis.recent_failure_count, 6);
        assert_eq!(analysis.recent_failure_ratio, 1.0);
        assert!(analysis.is_possibly_stuck());
    }

    // -----------------------------------------------------------------------
    // Test 4: 7 events, 6 failure-shaped → ratio ≈ 0.857 ≥ 0.80 → stuck
    // -----------------------------------------------------------------------
    #[test]
    fn mixed_above_ratio() {
        let dir = TempDir::new().unwrap();
        let ts = "2026-01-01T00:00:01Z";
        let mut lines: Vec<String> = (0..6)
            .map(|_| make_event(ts, "sess-1", true, None))
            .collect();
        // One non-failure event.
        lines.push(make_event(ts, "sess-1", false, Some(50)));
        let refs: Vec<&str> = lines.iter().map(|s| s.as_str()).collect();
        write_events(&dir, &refs);

        let analysis = analyze_session(dir.path(), "sess-1").unwrap();
        assert_eq!(analysis.recent_events, 7);
        assert_eq!(analysis.recent_failure_count, 6);
        // ratio = 6/7 ≈ 0.857
        assert!(analysis.recent_failure_ratio >= 0.80);
        assert!(analysis.is_possibly_stuck());
    }

    // -----------------------------------------------------------------------
    // Test 5: 7 events, 3 failure-shaped → ratio ≈ 0.43 < 0.80 → not stuck
    // -----------------------------------------------------------------------
    #[test]
    fn mixed_below_ratio() {
        let dir = TempDir::new().unwrap();
        let ts = "2026-01-01T00:00:01Z";
        let mut lines: Vec<String> = (0..3)
            .map(|_| make_event(ts, "sess-1", true, None))
            .collect();
        for _ in 0..4 {
            lines.push(make_event(ts, "sess-1", false, Some(100)));
        }
        let refs: Vec<&str> = lines.iter().map(|s| s.as_str()).collect();
        write_events(&dir, &refs);

        let analysis = analyze_session(dir.path(), "sess-1").unwrap();
        assert_eq!(analysis.recent_events, 7);
        assert_eq!(analysis.recent_failure_count, 3);
        // ratio = 3/7 ≈ 0.43
        assert!(analysis.recent_failure_ratio < 0.80);
        assert!(!analysis.is_possibly_stuck());
    }
}
