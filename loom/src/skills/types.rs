//! Type definitions for skill metadata and matching

use serde::Deserialize;

/// Metadata extracted from a SKILL.md file's YAML frontmatter
#[derive(Debug, Clone, Deserialize)]
pub struct SkillMetadata {
    /// Skill name (e.g., "auth", "testing")
    pub name: String,
    /// Human-readable description of what the skill does
    pub description: String,
    /// List of trigger words/phrases that activate this skill (YAML array format)
    #[serde(default)]
    pub triggers: Vec<String>,
    /// Comma-separated trigger keywords (CSV string in YAML frontmatter)
    /// Field name in YAML is `trigger-keywords` (with hyphen)
    #[serde(default, alias = "trigger-keywords")]
    pub trigger_keywords: Option<String>,
}

/// A matched skill with its relevance score
#[derive(Debug, Clone)]
pub struct SkillMatch {
    /// Name of the matched skill
    pub name: String,
    /// Description of the skill
    pub description: String,
    /// Relevance score (higher = more relevant)
    pub score: f32,
    /// Which triggers matched
    pub matched_triggers: Vec<String>,
}

impl SkillMatch {
    /// Create a new skill match
    pub fn new(
        name: String,
        description: String,
        score: f32,
        matched_triggers: Vec<String>,
    ) -> Self {
        Self {
            name,
            description,
            score,
            matched_triggers,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_skill_metadata_deserialize() {
        let yaml = r#"
name: auth
description: Authentication patterns
triggers:
  - login
  - password
"#;
        let metadata: SkillMetadata = crate::yaml::from_str(yaml).unwrap();
        assert_eq!(metadata.name, "auth");
        assert_eq!(metadata.description, "Authentication patterns");
        assert_eq!(metadata.triggers.len(), 2);
    }

    #[test]
    fn test_skill_metadata_empty_triggers() {
        let yaml = r#"
name: test-skill
description: A test skill
"#;
        let metadata: SkillMetadata = crate::yaml::from_str(yaml).unwrap();
        assert_eq!(metadata.name, "test-skill");
        assert!(metadata.triggers.is_empty());
    }

    #[test]
    fn test_skill_match_creation() {
        let skill_match = SkillMatch::new(
            "auth".to_string(),
            "Authentication".to_string(),
            5.0,
            vec!["login".to_string()],
        );
        assert_eq!(skill_match.name, "auth");
        assert_eq!(skill_match.score, 5.0);
        assert_eq!(skill_match.matched_triggers.len(), 1);
    }
}
