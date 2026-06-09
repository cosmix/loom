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
}
