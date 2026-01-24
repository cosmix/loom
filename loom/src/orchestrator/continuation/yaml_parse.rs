//! YAML frontmatter parsing for stage files.
//!
//! This module provides stage-specific parsing that delegates to the canonical
//! frontmatter parser in `crate::parser::frontmatter`.

use anyhow::Result;

use crate::models::stage::Stage;
use crate::parser::frontmatter::parse_from_markdown;

/// Parse a Stage from markdown with YAML frontmatter
pub fn parse_stage_from_markdown(content: &str) -> Result<Stage> {
    parse_from_markdown(content, "Stage")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::parser::frontmatter::extract_yaml_frontmatter;

    #[test]
    fn test_parse_stage_from_markdown() {
        use crate::models::stage::{Stage, StageStatus};
        use crate::verify::transitions::serialize_stage_to_markdown;

        // Create a Stage and serialize it to ensure valid YAML
        let mut stage = Stage::new(
            "Test Name".to_string(),
            Some("Test description".to_string()),
        );
        stage.id = "test-id".to_string();
        stage.status = StageStatus::Queued;
        let content = serialize_stage_to_markdown(&stage).expect("Should serialize");

        // Parse it back
        let parsed = parse_stage_from_markdown(&content).expect("Should parse stage");
        assert_eq!(parsed.id, "test-id");
        assert_eq!(parsed.name, "Test Name");
    }

    #[test]
    fn test_extract_yaml_frontmatter() {
        let content = r#"---
id: test-id
name: Test Name
---

# Content here
"#;

        let yaml = extract_yaml_frontmatter(content).expect("Should extract YAML");
        let id = yaml["id"].as_str().unwrap();
        assert_eq!(id, "test-id");
    }

    #[test]
    fn test_extract_yaml_frontmatter_missing() {
        let content = "# No frontmatter here";
        let result = extract_yaml_frontmatter(content);
        assert!(result.is_err());
        assert!(result
            .unwrap_err()
            .to_string()
            .contains("No frontmatter delimiter"));
    }
}
