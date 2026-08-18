//! Compact knowledge summary for embedding in agent signals.
//!
//! This is the flat, tier-1-only digest that predates the tiered hierarchy and
//! is what a `Legacy` knowledge directory reduces to. A `Hierarchical`
//! directory has a generated `INDEX.md` instead — see [`super::index`].

use super::types::KnowledgeFile;
use anyhow::Result;
use std::fs;
use std::path::Path;

const SUMMARY_PREAMBLE: &str =
    "## Knowledge Summary\n\n> Curated knowledge to help you navigate the codebase.\n\n";

/// Generate a compact summary of all tier-1 knowledge files under `root`.
///
/// Returns an empty string when no file has any content worth summarizing.
pub fn generate_summary(root: &Path) -> Result<String> {
    let mut summary = String::from(SUMMARY_PREAMBLE);

    for file_type in KnowledgeFile::all() {
        let path = root.join(file_type.filename());
        if !path.exists() {
            continue;
        }
        let Ok(content) = fs::read_to_string(&path) else {
            continue;
        };
        // Extract just the headers and first-level items for a compact summary
        let compact = extract_compact_summary(&content);
        if !compact.is_empty() {
            summary.push_str(&format!("### {}\n\n", file_type.description()));
            summary.push_str(&compact);
            summary.push_str("\n\n");
        }
    }

    if summary.len() <= SUMMARY_PREAMBLE.len() {
        return Ok(String::new());
    }

    Ok(summary.trim_end().to_string())
}

/// Extract a compact summary from a knowledge file: every `## ` header, plus
/// the first few list items under each.
fn extract_compact_summary(content: &str) -> String {
    let mut summary = String::new();
    let mut in_section = false;
    let mut line_count = 0;
    const MAX_LINES_PER_SECTION: usize = 5;

    for line in content.lines() {
        // Skip the title and intro lines
        if line.starts_with("# ") || line.starts_with("> ") {
            continue;
        }

        // Track section headers
        if line.starts_with("## ") {
            if in_section {
                summary.push('\n');
            }
            summary.push_str(line);
            summary.push('\n');
            in_section = true;
            line_count = 0;
            continue;
        }

        // Include only first few items per section
        if in_section
            && line_count < MAX_LINES_PER_SECTION
            && (line.starts_with("- ") || line.starts_with("* "))
        {
            summary.push_str(line);
            summary.push('\n');
            line_count += 1;
        }
    }

    summary
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_extract_compact_summary_keeps_headers_and_caps_items() {
        let content =
            "# Title\n\n> blurb\n\n## Section A\n\n- one\n- two\n- three\n- four\n- five\n- six\n";
        let compact = extract_compact_summary(content);
        assert!(compact.contains("## Section A"));
        assert!(compact.contains("- five"));
        assert!(
            !compact.contains("- six"),
            "only the first 5 items per section are kept"
        );
        assert!(!compact.contains("# Title"));
    }

    #[test]
    fn test_generate_summary_empty_when_no_content() {
        let temp = tempfile::TempDir::new().unwrap();
        assert_eq!(generate_summary(temp.path()).unwrap(), "");
    }
}
