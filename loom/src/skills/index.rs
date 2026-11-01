//! Skill index for loading and matching skills from SKILL.md files

use anyhow::{Context, Result};
use std::collections::HashMap;
use std::fs;
use std::path::Path;

use crate::parser::frontmatter::extract_frontmatter_raw;

use super::matcher::{match_skills, normalize_text};
use super::types::{SkillMatch, SkillMetadata};

/// Index of available skills with trigger-to-skill mapping
#[derive(Debug, Clone, Default)]
pub struct SkillIndex {
    /// All loaded skill metadata
    skills: HashMap<String, SkillMetadata>,
    /// Map from normalized trigger to skill names
    trigger_map: HashMap<String, Vec<String>>,
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
        index.load_directory(path)?;
        Ok(index)
    }

    /// Add a root in precedence order; the first valid skill with a name wins.
    pub(super) fn load_directory(&mut self, path: &Path) -> Result<()> {
        if !path.exists() {
            return Ok(());
        }

        let mut entries: Vec<_> = fs::read_dir(path)
            .with_context(|| format!("Failed to read skills directory: {}", path.display()))?
            .flatten()
            .map(|entry| entry.path())
            .collect();
        entries.sort();

        for skill_dir in entries {
            if !skill_dir.is_dir() {
                continue;
            }

            let skill_file = skill_dir.join("SKILL.md");
            if !skill_file.exists() {
                continue;
            }

            match Self::parse_skill_file(&skill_file) {
                Ok(metadata) => {
                    if self.skills.contains_key(&metadata.name) {
                        eprintln!(
                            "Warning: Duplicate skill {} in {}; keeping first",
                            metadata.name,
                            skill_file.display()
                        );
                    } else {
                        self.add_skill(metadata);
                    }
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

        Ok(())
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
        let raw = extract_frontmatter_raw(content)?;
        crate::yaml::from_str(raw).context("Failed to parse SkillMetadata from frontmatter")
    }

    /// Add a skill to the index
    fn add_skill(&mut self, metadata: SkillMetadata) {
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
                let names = self.trigger_map.entry(normalized).or_default();
                if !names.contains(&metadata.name) {
                    names.push(metadata.name.clone());
                }
            }
        }

        self.skills.insert(metadata.name.clone(), metadata);
    }

    /// Get a skill by exact name lookup
    pub fn get_by_name(&self, name: &str) -> Option<&SkillMetadata> {
        self.skills.get(name)
    }

    /// Match skills against input text
    ///
    /// Returns up to `max` skills sorted by relevance score (descending).
    /// Only skills with score >= 2.0 are returned (at least one phrase match
    /// or two word matches).
    pub fn match_skills(&self, text: &str, max: usize) -> Vec<SkillMatch> {
        const SCORE_THRESHOLD: f32 = 2.0;
        let descriptions = self
            .skills
            .iter()
            .map(|(name, metadata)| (name.clone(), metadata.description.clone()))
            .collect();
        match_skills(text, &self.trigger_map, &descriptions, max, SCORE_THRESHOLD)
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

/// Find an ASCII marker without changing byte offsets in the original text.
pub(crate) fn after_ascii_marker<'a>(text: &'a str, marker: &str) -> Option<&'a str> {
    debug_assert!(marker.is_ascii());
    text.to_ascii_lowercase()
        .find(marker)
        .map(|pos| &text[pos + marker.len()..])
}

/// Extract triggers from description text by looking for known patterns:
/// - "TRIGGERS:" followed by comma-separated list
/// - "Trigger keywords:" followed by comma-separated list
fn extract_description_triggers(description: &str) -> Vec<String> {
    // Try "TRIGGERS:" first (case-insensitive search for the marker)
    for line in description.lines() {
        let trimmed = line.trim();

        // Check for "TRIGGERS:" pattern
        if let Some(after) = after_ascii_marker(trimmed, "triggers:") {
            let triggers = parse_csv_triggers(after);
            if !triggers.is_empty() {
                return triggers;
            }
        }

        // Check for "Trigger keywords:" pattern
        if let Some(after) = after_ascii_marker(trimmed, "trigger keywords:") {
            let triggers = parse_csv_triggers(after);
            if !triggers.is_empty() {
                return triggers;
            }
        }
    }

    Vec::new()
}

#[cfg(test)]
#[path = "index_tests.rs"]
mod tests;
