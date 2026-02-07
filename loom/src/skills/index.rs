//! Skill index for loading and matching skills from SKILL.md files

use anyhow::{Context, Result};
use std::collections::HashMap;
use std::fs;
use std::path::Path;

use crate::parser::frontmatter::extract_yaml_frontmatter;

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
        let yaml = extract_yaml_frontmatter(content)?;
        serde_yaml::from_value(yaml).context("Failed to parse SkillMetadata from frontmatter")
    }

    /// Add a skill to the index
    fn add_skill(&mut self, metadata: SkillMetadata) {
        // Store description for output
        self.descriptions
            .insert(metadata.name.clone(), metadata.description.clone());

        // Collect triggers from all three sources with priority:
        // 1. YAML triggers array (highest priority)
        // 2. trigger-keywords CSV field
        // 3. Description-embedded "Trigger keywords:" or "TRIGGERS:" (fallback)
        let triggers: Vec<String> = if !metadata.triggers.is_empty() {
            metadata.triggers.clone()
        } else if let Some(ref csv) = metadata.trigger_keywords {
            parse_csv_triggers(csv)
        } else {
            extract_description_triggers(&metadata.description)
        };

        // Build trigger map from collected triggers
        for trigger in &triggers {
            let normalized = normalize_text(trigger);
            if !normalized.is_empty() {
                self.trigger_map
                    .entry(normalized)
                    .or_default()
                    .push(metadata.name.clone());
            }
        }

        self.skills.push(metadata);
    }

    /// Get a skill by exact name lookup
    pub fn get_by_name(&self, name: &str) -> Option<&SkillMetadata> {
        self.skills.iter().find(|s| s.name == name)
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

/// Parse a CSV string of trigger keywords into individual triggers
fn parse_csv_triggers(csv: &str) -> Vec<String> {
    csv.split(',')
        .map(|s| {
            s.trim()
                .trim_end_matches(|c: char| c.is_ascii_punctuation() && c != '-' && c != '_')
                .to_string()
        })
        .filter(|s| !s.is_empty())
        .collect()
}

/// Extract triggers from description text by looking for known patterns:
/// - "TRIGGERS:" followed by comma-separated list
/// - "Trigger keywords:" followed by comma-separated list
fn extract_description_triggers(description: &str) -> Vec<String> {
    // Try "TRIGGERS:" first (case-insensitive search for the marker)
    for line in description.lines() {
        let trimmed = line.trim();

        // Check for "TRIGGERS:" pattern
        if let Some(pos) = trimmed.to_uppercase().find("TRIGGERS:") {
            let after = &trimmed[pos + "TRIGGERS:".len()..];
            let triggers = parse_csv_triggers(after);
            if !triggers.is_empty() {
                return triggers;
            }
        }

        // Check for "Trigger keywords:" pattern
        if let Some(pos) = trimmed.to_lowercase().find("trigger keywords:") {
            let after = &trimmed[pos + "trigger keywords:".len()..];
            let triggers = parse_csv_triggers(after);
            if !triggers.is_empty() {
                return triggers;
            }
        }
    }

    Vec::new()
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
        writeln!(file, "name: {name}").unwrap();
        writeln!(file, "description: {description}").unwrap();
        writeln!(file, "triggers:").unwrap();
        for trigger in triggers {
            writeln!(file, "  - {trigger}").unwrap();
        }
        writeln!(file, "---").unwrap();
        writeln!(file, "# {name}").unwrap();
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

    // --- CSV trigger_keywords parsing tests ---

    #[test]
    fn test_parse_csv_triggers_basic() {
        let result = parse_csv_triggers("login, password, token");
        assert_eq!(result, vec!["login", "password", "token"]);
    }

    #[test]
    fn test_parse_csv_triggers_whitespace() {
        let result = parse_csv_triggers("  login , password ,  token  ");
        assert_eq!(result, vec!["login", "password", "token"]);
    }

    #[test]
    fn test_parse_csv_triggers_empty_entries() {
        let result = parse_csv_triggers("login,,password,  ,token");
        assert_eq!(result, vec!["login", "password", "token"]);
    }

    #[test]
    fn test_parse_csv_triggers_empty_string() {
        let result = parse_csv_triggers("");
        assert!(result.is_empty());
    }

    // --- Description-embedded trigger extraction tests ---

    #[test]
    fn test_extract_description_triggers_uppercase() {
        let desc = "Authentication patterns.\n\nTRIGGERS: login, logout, password";
        let result = extract_description_triggers(desc);
        assert_eq!(result, vec!["login", "logout", "password"]);
    }

    #[test]
    fn test_extract_description_triggers_keyword_format() {
        let desc = "Debugging tool. Trigger keywords: debug, bug, error, crash";
        let result = extract_description_triggers(desc);
        assert_eq!(result, vec!["debug", "bug", "error", "crash"]);
    }

    #[test]
    fn test_extract_description_triggers_none() {
        let desc = "A simple skill with no trigger information";
        let result = extract_description_triggers(desc);
        assert!(result.is_empty());
    }

    #[test]
    fn test_extract_description_triggers_multiline() {
        let desc = "Some description.\n\nUSE WHEN: things.\n\nTRIGGERS: a, b, c.";
        let result = extract_description_triggers(desc);
        // Trailing punctuation is stripped from triggers
        assert_eq!(result, vec!["a", "b", "c"]);
    }

    // --- trigger-keywords YAML field tests ---

    fn create_test_skill_with_trigger_keywords(
        dir: &Path,
        name: &str,
        description: &str,
        trigger_keywords: &str,
    ) {
        let skill_dir = dir.join(name);
        fs::create_dir_all(&skill_dir).unwrap();

        let skill_file = skill_dir.join("SKILL.md");
        let mut file = fs::File::create(&skill_file).unwrap();

        writeln!(file, "---").unwrap();
        writeln!(file, "name: {name}").unwrap();
        writeln!(file, "description: {description}").unwrap();
        writeln!(file, "trigger-keywords: {trigger_keywords}").unwrap();
        writeln!(file, "---").unwrap();
        writeln!(file, "# {name}").unwrap();
    }

    fn create_test_skill_description_triggers(dir: &Path, name: &str, description: &str) {
        let skill_dir = dir.join(name);
        fs::create_dir_all(&skill_dir).unwrap();

        let skill_file = skill_dir.join("SKILL.md");
        let mut file = fs::File::create(&skill_file).unwrap();

        // Use YAML quoted string to avoid colon parsing issues
        writeln!(file, "---").unwrap();
        writeln!(file, "name: {name}").unwrap();
        writeln!(
            file,
            "description: \"{}\"",
            description.replace('"', "\\\"")
        )
        .unwrap();
        writeln!(file, "---").unwrap();
        writeln!(file, "# {name}").unwrap();
    }

    #[test]
    fn test_load_trigger_keywords_csv() {
        let temp_dir = TempDir::new().unwrap();
        let skills_dir = temp_dir.path();

        create_test_skill_with_trigger_keywords(
            skills_dir,
            "ci-cd",
            "CI/CD pipeline management",
            "pipeline, deploy, build, GitHub Actions",
        );

        let index = SkillIndex::load_from_directory(skills_dir).unwrap();
        assert_eq!(index.skill_count(), 1);

        // Two word matches should exceed threshold
        let matches = index.match_skills("deploy a new build pipeline", 5);
        assert_eq!(matches.len(), 1);
        assert_eq!(matches[0].name, "ci-cd");
        assert!(matches[0].score >= 2.0);
    }

    #[test]
    fn test_load_description_embedded_triggers() {
        let temp_dir = TempDir::new().unwrap();
        let skills_dir = temp_dir.path();

        create_test_skill_description_triggers(
            skills_dir,
            "debugging",
            "Systematic debugging. Trigger keywords: debug, bug, error, crash, investigate",
        );

        let index = SkillIndex::load_from_directory(skills_dir).unwrap();
        assert_eq!(index.skill_count(), 1);

        let matches = index.match_skills("debug the crash in login", 5);
        assert_eq!(matches.len(), 1);
        assert_eq!(matches[0].name, "debugging");
        assert!(matches[0].score >= 2.0);
    }

    #[test]
    fn test_yaml_triggers_take_priority_over_csv() {
        let temp_dir = TempDir::new().unwrap();
        let skills_dir = temp_dir.path();

        // Skill with both triggers array AND trigger-keywords — triggers should win
        let skill_dir = skills_dir.join("auth");
        fs::create_dir_all(&skill_dir).unwrap();
        let skill_file = skill_dir.join("SKILL.md");
        let mut file = fs::File::create(&skill_file).unwrap();
        writeln!(file, "---").unwrap();
        writeln!(file, "name: auth").unwrap();
        writeln!(file, "description: Auth patterns").unwrap();
        writeln!(file, "triggers:").unwrap();
        writeln!(file, "  - login").unwrap();
        writeln!(file, "  - password").unwrap();
        writeln!(file, "trigger-keywords: deploy, build").unwrap();
        writeln!(file, "---").unwrap();

        let index = SkillIndex::load_from_directory(skills_dir).unwrap();

        // "login password" should match (from triggers array)
        let matches = index.match_skills("login with password", 5);
        assert_eq!(matches.len(), 1);
        assert_eq!(matches[0].name, "auth");

        // "deploy build" should NOT match (trigger-keywords ignored when triggers exists)
        let matches = index.match_skills("deploy a build", 5);
        assert!(matches.is_empty());
    }

    #[test]
    fn test_get_by_name() {
        let temp_dir = TempDir::new().unwrap();
        let skills_dir = temp_dir.path();

        create_test_skill(skills_dir, "auth", "Authentication patterns", &["login"]);
        create_test_skill(skills_dir, "testing", "Testing patterns", &["test"]);

        let index = SkillIndex::load_from_directory(skills_dir).unwrap();

        let auth = index.get_by_name("auth");
        assert!(auth.is_some());
        assert_eq!(auth.unwrap().name, "auth");
        assert_eq!(auth.unwrap().description, "Authentication patterns");

        let testing = index.get_by_name("testing");
        assert!(testing.is_some());

        let nonexistent = index.get_by_name("nonexistent");
        assert!(nonexistent.is_none());
    }

    #[test]
    fn test_csv_triggers_take_priority_over_description() {
        let temp_dir = TempDir::new().unwrap();
        let skills_dir = temp_dir.path();

        // Skill with trigger-keywords CSV AND description triggers — CSV should win
        let skill_dir = skills_dir.join("mixed");
        fs::create_dir_all(&skill_dir).unwrap();
        let skill_file = skill_dir.join("SKILL.md");
        let mut file = fs::File::create(&skill_file).unwrap();
        writeln!(file, "---").unwrap();
        writeln!(file, "name: mixed").unwrap();
        writeln!(
            file,
            "description: \"A skill. Trigger keywords: alpha, beta\""
        )
        .unwrap();
        writeln!(file, "trigger-keywords: gamma, delta").unwrap();
        writeln!(file, "---").unwrap();

        let index = SkillIndex::load_from_directory(skills_dir).unwrap();

        // "gamma delta" should match (from trigger-keywords CSV)
        let matches = index.match_skills("gamma and delta", 5);
        assert_eq!(matches.len(), 1);
        assert_eq!(matches[0].name, "mixed");

        // "alpha beta" should NOT match (description triggers ignored when CSV exists)
        let matches = index.match_skills("alpha and beta", 5);
        assert!(matches.is_empty());
    }
}
