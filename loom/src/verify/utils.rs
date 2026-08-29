//! Shared utility functions for verification operations

use anyhow::{Context, Result};
use regex::{Regex, RegexBuilder};
use std::collections::HashSet;

/// Extract lines from output that match any of the given patterns.
///
/// Each pattern is treated as a regex. Lines are deduplicated while preserving order.
///
/// # Arguments
/// * `output` - The text to search through
/// * `patterns` - Regex patterns to match against each line
///
/// # Returns
/// A deduplicated Vec of matching lines, preserving order of first occurrence
pub fn extract_matching_lines(output: &str, patterns: &[String]) -> Result<Vec<String>> {
    if patterns.is_empty() {
        return Ok(Vec::new());
    }

    let mut matching_lines = Vec::new();
    let regexes: Vec<Regex> = patterns
        .iter()
        .map(|p| {
            RegexBuilder::new(p)
                .size_limit(1 << 20) // 1MB compiled size limit (matches verify/goal_backward/wiring.rs)
                .build()
                .with_context(|| format!("Invalid pattern: {p}"))
        })
        .collect::<Result<Vec<_>>>()?;

    for line in output.lines() {
        if regexes.iter().any(|re| re.is_match(line)) {
            matching_lines.push(line.to_string());
        }
    }

    // Deduplicate while preserving order
    let mut seen = HashSet::new();
    matching_lines.retain(|line| seen.insert(line.clone()));

    Ok(matching_lines)
}

/// Cap a grep stderr blob to a few lines / a few hundred bytes so that a tree with
/// many unreadable files (e.g. a sandbox that denies read on dotfiles) cannot flood
/// the caller's output. Grep exits 2 both for a genuine error and whenever any file
/// under the search root is unreadable; callers still need to parse stdout in that
/// case, but the raw stderr can be arbitrarily long, so it is only ever surfaced
/// through this bounded form. Shared by `duplicate_detection` and `wiring_detection`,
/// which both report this exact grep exit-2 warning.
pub(crate) fn bounded_stderr_warning(stderr: &str) -> String {
    const MAX_LINES: usize = 3;
    const MAX_CHARS: usize = 400;

    let all_lines: Vec<&str> = stderr.trim().lines().collect();
    let omitted_lines = all_lines.len().saturating_sub(MAX_LINES);
    let mut shown = all_lines
        .into_iter()
        .take(MAX_LINES)
        .collect::<Vec<_>>()
        .join("\n");

    let mut chars_truncated = false;
    if shown.chars().count() > MAX_CHARS {
        shown = shown.chars().take(MAX_CHARS).collect();
        chars_truncated = true;
    }

    if omitted_lines > 0 {
        let plural = if omitted_lines == 1 { "" } else { "s" };
        format!("{shown}\n… ({omitted_lines} more line{plural})")
    } else if chars_truncated {
        format!("{shown}…")
    } else {
        shown
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_extract_matching_lines() {
        let output = "line 1\nFAILED: test_foo\nline 3\nFAILED: test_bar\nline 5";
        let patterns = vec!["FAILED:".to_string()];

        let matches = extract_matching_lines(output, &patterns).unwrap();
        assert_eq!(matches.len(), 2);
        assert!(matches[0].contains("test_foo"));
        assert!(matches[1].contains("test_bar"));
    }

    #[test]
    fn test_extract_matching_lines_empty_patterns() {
        let output = "line 1\nFAILED: test_foo";
        let patterns: Vec<String> = vec![];

        let matches = extract_matching_lines(output, &patterns).unwrap();
        assert!(matches.is_empty());
    }

    #[test]
    fn test_extract_matching_lines_deduplication() {
        let output = "FAILED: test_foo\nFAILED: test_foo\nFAILED: test_foo";
        let patterns = vec!["FAILED:".to_string()];

        let matches = extract_matching_lines(output, &patterns).unwrap();
        assert_eq!(matches.len(), 1);
    }

    #[test]
    fn test_bounded_stderr_warning_short_input_unchanged() {
        let warning = bounded_stderr_warning("grep: ./foo: Permission denied");
        assert_eq!(warning, "grep: ./foo: Permission denied");
    }

    #[test]
    fn test_bounded_stderr_warning_caps_lines() {
        let stderr = (0..10)
            .map(|i| format!("grep: ./file{i}: Permission denied"))
            .collect::<Vec<_>>()
            .join("\n");
        let warning = bounded_stderr_warning(&stderr);
        assert_eq!(warning.lines().count(), 4); // 3 content lines + 1 "more" line
        assert!(warning.contains("(7 more line"));
    }
}
