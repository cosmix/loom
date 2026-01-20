//! Skill index for loading and matching skills from SKILL.md files

use anyhow::{Context, Result};
use std::collections::HashMap;
use std::fs;
use std::path::Path;

use super::matcher::{match_skills, normalize_text};
use super::types::{SkillMatch, SkillMetadata};

/// Index of available skills with trigger-to-skill mapping
#[derive(Debug, Clone, Default)]
pub struct SkillIndex {
    /// All loaded skill metadata
    skills: Vec<SkillMetadata>,
    /// Map from normalized trigger to skill names
    trigger_map: HashMap<String, Vec<String>>,
    /// Map from skill name to description (for output)
    descriptions: HashMap<String, String>,
}

impl SkillIndex {
    /// Create an empty skill index
    pub fn new() -> Self {
        Self::default()
    }

    /// Load skills from a directory containing skill subdirectories
    ///
    /// Expected structure:
    /// ```text
    /// skills_dir/
    ///   auth/
    ///     SKILL.md
    ///   testing/
    ///     SKILL.md
    ///   ...
    /// ```
    pub fn load_from_directory(path: &Path) -> Result<Self> {
        let mut index = Self::new();

        if !path.exists() {
            return Ok(index);
        }

        let entries = fs::read_dir(path)
            .with_context(|| format!("Failed to read skills directory: {}", path.display()))?;

        for entry in entries.flatten() {
            let skill_dir = entry.path();
            if !skill_dir.is_dir() {
                continue;
            }

            let skill_file = skill_dir.join("SKILL.md");
            if !skill_file.exists() {
                continue;
            }

            match Self::parse_skill_file(&skill_file) {
                Ok(metadata) => {
                    index.add_skill(metadata);
                }
                Err(e) => {
                    // Log warning but continue loading other skills
                    eprintln!(
                        "Warning: Failed to parse skill file {}: {}",
                        skill_file.display(),
                        e
                    );
                }
            }
        }

        Ok(index)
    }

    /// Parse a SKILL.md file to extract metadata from YAML frontmatter
    fn parse_skill_file(path: &Path) -> Result<SkillMetadata> {
        let content = fs::read_to_string(path)
            .with_context(|| format!("Failed to read skill file: {}", path.display()))?;

        Self::parse_frontmatter(&content)
            .with_context(|| format!("Failed to parse frontmatter in: {}", path.display()))
    }

    /// Parse YAML frontmatter from skill file content
    fn parse_frontmatter(content: &str) -> Result<SkillMetadata> {
        // Find frontmatter delimiters
        let lines: Vec<&str> = content.lines().collect();

        if lines.is_empty() || lines[0].trim() != "---" {
            anyhow::bail!("Missing opening frontmatter delimiter");
        }

        // Find closing delimiter
        let end_idx = lines
            .iter()
            .skip(1)
            .position(|line| line.trim() == "---")
            .ok_or_else(|| anyhow::anyhow!("Missing closing frontmatter delimiter"))?
            + 1;

        // Extract YAML content
        let yaml_content = lines[1..end_idx].join("\n");

        let metadata: SkillMetadata =
            serde_yaml::from_str(&yaml_content).context("Failed to parse YAML frontmatter")?;

        Ok(metadata)
    }

    /// Add a skill to the index
    fn add_skill(&mut self, metadata: SkillMetadata) {
        // Store description for output
        self.descriptions
            .insert(metadata.name.clone(), metadata.description.clone());

        // Build trigger map
        for trigger in &metadata.triggers {
            let normalized = normalize_text(trigger);
            self.trigger_map
                .entry(normalized)
                .or_default()
                .push(metadata.name.clone());
        }

        self.skills.push(metadata);
    }

    /// Match skills against input text
    ///
    /// Returns up to `max` skills sorted by relevance score (descending).
    /// Only skills with score >= 2.0 are returned (at least one phrase match
    /// or two word matches).
    pub fn match_skills(&self, text: &str, max: usize) -> Vec<SkillMatch> {
        const SCORE_THRESHOLD: f32 = 2.0;
        match_skills(
            text,
            &self.trigger_map,
            &self.descriptions,
            max,
            SCORE_THRESHOLD,
        )
    }

    /// Get the number of loaded skills
    pub fn skill_count(&self) -> usize {
        self.skills.len()
    }

    /// Check if the index is empty
    pub fn is_empty(&self) -> bool {
        self.skills.is_empty()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;
    use tempfile::TempDir;

    fn create_test_skill(dir: &Path, name: &str, description: &str, triggers: &[&str]) {
        let skill_dir = dir.join(name);
        fs::create_dir_all(&skill_dir).unwrap();

        let skill_file = skill_dir.join("SKILL.md");
        let mut file = fs::File::create(&skill_file).unwrap();

        writeln!(file, "---").unwrap();
        writeln!(file, "name: {}", name).unwrap();
        writeln!(file, "description: {}", description).unwrap();
        writeln!(file, "triggers:").unwrap();
        for trigger in triggers {
            writeln!(file, "  - {}", trigger).unwrap();
        }
        writeln!(file, "---").unwrap();
        writeln!(file, "# {}", name).unwrap();
    }

    #[test]
    fn test_load_from_directory() {
        let temp_dir = TempDir::new().unwrap();
        let skills_dir = temp_dir.path();

        create_test_skill(
            skills_dir,
            "auth",
            "Authentication patterns",
            &["login", "password"],
        );
        create_test_skill(
            skills_dir,
            "testing",
            "Testing patterns",
            &["test", "unit test"],
        );

        let index = SkillIndex::load_from_directory(skills_dir).unwrap();

        assert_eq!(index.skill_count(), 2);
        assert!(!index.is_empty());
    }

    #[test]
    fn test_load_from_nonexistent_directory() {
        let index = SkillIndex::load_from_directory(Path::new("/nonexistent/path")).unwrap();
        assert!(index.is_empty());
    }

    #[test]
    fn test_parse_frontmatter() {
        let content = r#"---
name: test-skill
description: A test skill description
triggers:
  - trigger1
  - trigger2
---
# Test Skill Content
"#;

        let metadata = SkillIndex::parse_frontmatter(content).unwrap();
        assert_eq!(metadata.name, "test-skill");
        assert_eq!(metadata.description, "A test skill description");
        assert_eq!(metadata.triggers.len(), 2);
    }

    #[test]
    fn test_parse_frontmatter_missing_open() {
        let content = "name: test\n---\nContent";
        assert!(SkillIndex::parse_frontmatter(content).is_err());
    }

    #[test]
    fn test_parse_frontmatter_missing_close() {
        let content = "---\nname: test\nContent";
        assert!(SkillIndex::parse_frontmatter(content).is_err());
    }

    #[test]
    fn test_match_skills_integration() {
        let temp_dir = TempDir::new().unwrap();
        let skills_dir = temp_dir.path();

        create_test_skill(
            skills_dir,
            "auth",
            "Authentication patterns",
            &["login", "password", "token", "refresh token"],
        );
        create_test_skill(
            skills_dir,
            "testing",
            "Testing patterns",
            &["test", "unit test", "integration test"],
        );

        let index = SkillIndex::load_from_directory(skills_dir).unwrap();

        // Match with phrase trigger
        let matches = index.match_skills("implement refresh token rotation", 5);
        assert_eq!(matches.len(), 1);
        assert_eq!(matches[0].name, "auth");
        assert!(matches[0].score >= 2.0); // phrase match

        // Match with multiple word triggers
        let matches = index.match_skills("add login and password validation", 5);
        assert_eq!(matches.len(), 1);
        assert_eq!(matches[0].name, "auth");
        assert!(matches[0].score >= 2.0); // two word matches
    }

    #[test]
    fn test_match_skills_below_threshold() {
        let temp_dir = TempDir::new().unwrap();
        let skills_dir = temp_dir.path();

        create_test_skill(skills_dir, "auth", "Authentication patterns", &["login"]);

        let index = SkillIndex::load_from_directory(skills_dir).unwrap();

        // Single word match should not pass threshold of 2.0
        let matches = index.match_skills("implement login", 5);
        assert!(matches.is_empty());
    }

    #[test]
    fn test_empty_index_match() {
        let index = SkillIndex::new();
        let matches = index.match_skills("any text", 5);
        assert!(matches.is_empty());
    }
}
