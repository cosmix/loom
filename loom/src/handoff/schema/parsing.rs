//! Handoff parsing logic supporting both V2 and V1 formats.

use super::v2::HandoffV2;

/// Result of attempting to parse a handoff file.
///
/// Supports both V2 (structured YAML) and V1 (prose markdown) formats.
#[derive(Debug)]
pub enum ParsedHandoff {
    /// Successfully parsed V2 structured handoff
    V2(Box<HandoffV2>),
    /// Fell back to V1 prose format (raw markdown content)
    V1Fallback(String),
}

impl ParsedHandoff {
    /// Try to parse a handoff file, falling back to V1 if V2 parsing fails
    pub fn parse(content: &str) -> Self {
        // Try to extract YAML frontmatter or find YAML block
        if let Some(yaml_content) = extract_yaml_from_handoff(content) {
            if let Ok(handoff) = HandoffV2::from_yaml(&yaml_content) {
                return ParsedHandoff::V2(Box::new(handoff));
            }
        }

        // Fall back to V1 prose format
        ParsedHandoff::V1Fallback(content.to_string())
    }

    /// Check if this is a V2 handoff
    pub fn is_v2(&self) -> bool {
        matches!(self, ParsedHandoff::V2(_))
    }

    /// Get V2 handoff if available
    pub fn as_v2(&self) -> Option<&HandoffV2> {
        match self {
            ParsedHandoff::V2(h) => Some(h),
            ParsedHandoff::V1Fallback(_) => None,
        }
    }

    /// Get raw content (V1) if this is a fallback
    pub fn as_v1(&self) -> Option<&str> {
        match self {
            ParsedHandoff::V2(_) => None,
            ParsedHandoff::V1Fallback(content) => Some(content),
        }
    }
}

/// Extract YAML content from a handoff file.
///
/// Supports two formats:
/// 1. YAML frontmatter (content between --- markers at start of file)
/// 2. Pure YAML (file starts with "version:")
fn extract_yaml_from_handoff(content: &str) -> Option<String> {
    let trimmed = content.trim();

    // Check for YAML frontmatter
    if let Some(after_first) = trimmed.strip_prefix("---") {
        if let Some(end_idx) = after_first.find("---") {
            let yaml = after_first[..end_idx].trim();
            return Some(yaml.to_string());
        }
    }

    // Check if the file is pure YAML (starts with version:)
    if trimmed.starts_with("version:") {
        return Some(trimmed.to_string());
    }

    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parsed_handoff_v2() {
        let yaml = r#"
version: 2
session_id: session-test
stage_id: stage-test
context_percent: 70.0
"#;
        let parsed = ParsedHandoff::parse(yaml);
        assert!(parsed.is_v2());
        let v2 = parsed.as_v2().unwrap();
        assert_eq!(v2.session_id, "session-test");
    }

    #[test]
    fn test_parsed_handoff_with_frontmatter() {
        let content = r#"---
version: 2
session_id: session-fm
stage_id: stage-fm
context_percent: 80.0
---

# Additional markdown content
"#;
        let parsed = ParsedHandoff::parse(content);
        assert!(parsed.is_v2());
        let v2 = parsed.as_v2().unwrap();
        assert_eq!(v2.session_id, "session-fm");
    }

    #[test]
    fn test_parsed_handoff_v1_fallback() {
        let content = "# Handoff: my-stage\n\n## Completed Work\n\n- Did stuff";
        let parsed = ParsedHandoff::parse(content);
        assert!(!parsed.is_v2());
        assert!(parsed.as_v1().is_some());
    }

    #[test]
    fn test_extract_yaml_from_handoff_frontmatter() {
        let content = "---\nversion: 2\n---\n# Rest of file";
        let yaml = extract_yaml_from_handoff(content).unwrap();
        assert_eq!(yaml, "version: 2");
    }

    #[test]
    fn test_extract_yaml_from_handoff_pure_yaml() {
        let content = "version: 2\nsession_id: test";
        let yaml = extract_yaml_from_handoff(content).unwrap();
        assert!(yaml.contains("version: 2"));
    }

    #[test]
    fn test_extract_yaml_from_handoff_prose() {
        let content = "# Handoff\n\nSome prose content";
        assert!(extract_yaml_from_handoff(content).is_none());
    }
}
