//! Shared facts store for cross-stage knowledge sharing.
//!
//! Facts are key-value pairs with metadata that agents can use to share
//! decisions, patterns, and discoveries across stages. The store is persisted
//! in .work/facts.toml and embedded in signals.

use anyhow::{Context, Result};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::fs;
use std::path::Path;

/// Confidence level for a fact
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Confidence {
    /// Low confidence - tentative or uncertain
    Low,
    /// Medium confidence - reasonable assumption
    #[default]
    Medium,
    /// High confidence - verified or well-established
    High,
}

impl std::fmt::Display for Confidence {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Confidence::Low => write!(f, "low"),
            Confidence::Medium => write!(f, "medium"),
            Confidence::High => write!(f, "high"),
        }
    }
}

impl std::str::FromStr for Confidence {
    type Err = anyhow::Error;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.to_lowercase().as_str() {
            "low" => Ok(Confidence::Low),
            "medium" => Ok(Confidence::Medium),
            "high" => Ok(Confidence::High),
            _ => anyhow::bail!("Invalid confidence level: {s}. Use: low, medium, high"),
        }
    }
}

/// A single fact entry in the store
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Fact {
    /// The value of the fact
    pub value: String,
    /// ID of the stage that created this fact
    pub stage_id: String,
    /// When the fact was created or last updated
    pub timestamp: DateTime<Utc>,
    /// Confidence level
    #[serde(default)]
    pub confidence: Confidence,
}

/// The complete facts store
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct FactsStore {
    /// Facts organized by key
    #[serde(default)]
    pub facts: BTreeMap<String, Fact>,
}

impl FactsStore {
    /// Load the facts store from .work/facts.toml
    pub fn load(work_dir: &Path) -> Result<Self> {
        let facts_path = work_dir.join("facts.toml");
        if !facts_path.exists() {
            return Ok(Self::default());
        }

        let content = fs::read_to_string(&facts_path)
            .with_context(|| format!("Failed to read facts file: {}", facts_path.display()))?;

        let store: FactsStore = toml::from_str(&content)
            .with_context(|| format!("Failed to parse facts file: {}", facts_path.display()))?;

        Ok(store)
    }

    /// Save the facts store to .work/facts.toml
    pub fn save(&self, work_dir: &Path) -> Result<()> {
        let facts_path = work_dir.join("facts.toml");
        let content = toml::to_string_pretty(self).context("Failed to serialize facts to TOML")?;

        fs::write(&facts_path, content)
            .with_context(|| format!("Failed to write facts file: {}", facts_path.display()))?;

        Ok(())
    }

    /// Set a fact with the given key and value
    pub fn set(
        &mut self,
        key: String,
        value: String,
        stage_id: String,
        confidence: Confidence,
    ) -> &Fact {
        let fact = Fact {
            value,
            stage_id,
            timestamp: Utc::now(),
            confidence,
        };
        self.facts.insert(key.clone(), fact);
        self.facts.get(&key).unwrap()
    }

    /// Get a fact by key
    pub fn get(&self, key: &str) -> Option<&Fact> {
        self.facts.get(key)
    }

    /// List all facts, optionally filtered by stage
    pub fn list(&self, stage_id: Option<&str>) -> Vec<(&String, &Fact)> {
        self.facts
            .iter()
            .filter(|(_, fact)| stage_id.is_none_or(|sid| fact.stage_id == sid))
            .collect()
    }

    /// Get facts relevant to a specific stage (its own facts and high-confidence global facts)
    pub fn relevant_for_stage(
        &self,
        stage_id: &str,
        include_all_high: bool,
    ) -> Vec<(&String, &Fact)> {
        self.facts
            .iter()
            .filter(|(_, fact)| {
                fact.stage_id == stage_id
                    || (include_all_high && fact.confidence == Confidence::High)
            })
            .collect()
    }

    /// Format facts for embedding in a signal
    pub fn format_for_signal(&self, stage_id: &str) -> Option<String> {
        let relevant = self.relevant_for_stage(stage_id, true);
        if relevant.is_empty() {
            return None;
        }

        let mut output = String::new();
        output.push_str("| Key | Value | Source | Confidence |\n");
        output.push_str("|-----|-------|--------|------------|\n");

        for (key, fact) in relevant {
            let source = if fact.stage_id == stage_id {
                "(this stage)".to_string()
            } else {
                fact.stage_id.clone()
            };
            // Escape pipe characters in value
            let escaped_value = fact.value.replace('|', "\\|");
            output.push_str(&format!(
                "| {} | {} | {} | {} |\n",
                key, escaped_value, source, fact.confidence
            ));
        }

        Some(output)
    }

    /// Check if the store is empty
    pub fn is_empty(&self) -> bool {
        self.facts.is_empty()
    }
}

/// Validate a fact key
pub fn validate_fact_key(key: &str) -> Result<()> {
    if key.is_empty() {
        anyhow::bail!("Fact key cannot be empty");
    }

    if key.len() > 64 {
        anyhow::bail!("Fact key too long: {} characters (max 64)", key.len());
    }

    // Allow alphanumeric, underscores, and dashes
    let valid = key
        .chars()
        .all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '-');
    if !valid {
        anyhow::bail!(
            "Fact key '{key}' contains invalid characters. Use alphanumeric, underscore, or dash."
        );
    }

    Ok(())
}

/// Validate a fact value
pub fn validate_fact_value(value: &str) -> Result<()> {
    if value.is_empty() {
        anyhow::bail!("Fact value cannot be empty");
    }

    if value.len() > 500 {
        anyhow::bail!("Fact value too long: {} characters (max 500)", value.len());
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[test]
    fn test_confidence_display() {
        assert_eq!(Confidence::Low.to_string(), "low");
        assert_eq!(Confidence::Medium.to_string(), "medium");
        assert_eq!(Confidence::High.to_string(), "high");
    }

    #[test]
    fn test_confidence_from_str() {
        assert_eq!("low".parse::<Confidence>().unwrap(), Confidence::Low);
        assert_eq!("MEDIUM".parse::<Confidence>().unwrap(), Confidence::Medium);
        assert_eq!("High".parse::<Confidence>().unwrap(), Confidence::High);
        assert!("invalid".parse::<Confidence>().is_err());
    }

    #[test]
    fn test_facts_store_set_get() {
        let mut store = FactsStore::default();

        store.set(
            "auth_pattern".to_string(),
            "JWT with refresh tokens".to_string(),
            "implement-auth".to_string(),
            Confidence::High,
        );

        let fact = store.get("auth_pattern").unwrap();
        assert_eq!(fact.value, "JWT with refresh tokens");
        assert_eq!(fact.stage_id, "implement-auth");
        assert_eq!(fact.confidence, Confidence::High);
    }

    #[test]
    fn test_facts_store_list_by_stage() {
        let mut store = FactsStore::default();

        store.set(
            "fact1".to_string(),
            "value1".to_string(),
            "stage-a".to_string(),
            Confidence::Medium,
        );
        store.set(
            "fact2".to_string(),
            "value2".to_string(),
            "stage-b".to_string(),
            Confidence::High,
        );
        store.set(
            "fact3".to_string(),
            "value3".to_string(),
            "stage-a".to_string(),
            Confidence::Low,
        );

        let all_facts = store.list(None);
        assert_eq!(all_facts.len(), 3);

        let stage_a_facts = store.list(Some("stage-a"));
        assert_eq!(stage_a_facts.len(), 2);
    }

    #[test]
    fn test_facts_store_save_load() {
        let temp_dir = TempDir::new().unwrap();
        let work_dir = temp_dir.path();

        let mut store = FactsStore::default();
        store.set(
            "test_key".to_string(),
            "test_value".to_string(),
            "test-stage".to_string(),
            Confidence::High,
        );

        store.save(work_dir).unwrap();

        let loaded = FactsStore::load(work_dir).unwrap();
        let fact = loaded.get("test_key").unwrap();
        assert_eq!(fact.value, "test_value");
        assert_eq!(fact.stage_id, "test-stage");
    }

    #[test]
    fn test_facts_format_for_signal() {
        let mut store = FactsStore::default();

        store.set(
            "auth_pattern".to_string(),
            "JWT tokens".to_string(),
            "implement-auth".to_string(),
            Confidence::High,
        );
        store.set(
            "db_choice".to_string(),
            "PostgreSQL".to_string(),
            "setup-db".to_string(),
            Confidence::High,
        );
        store.set(
            "local_fact".to_string(),
            "some value".to_string(),
            "other-stage".to_string(),
            Confidence::Low,
        );

        let signal_content = store.format_for_signal("implement-auth").unwrap();
        assert!(signal_content.contains("auth_pattern"));
        assert!(signal_content.contains("db_choice")); // High confidence from other stage
        assert!(!signal_content.contains("local_fact")); // Low confidence from other stage
    }

    #[test]
    fn test_validate_fact_key() {
        assert!(validate_fact_key("valid_key").is_ok());
        assert!(validate_fact_key("valid-key-123").is_ok());
        assert!(validate_fact_key("").is_err());
        assert!(validate_fact_key("invalid key").is_err());
        assert!(validate_fact_key("a".repeat(65).as_str()).is_err());
    }

    #[test]
    fn test_validate_fact_value() {
        assert!(validate_fact_value("valid value").is_ok());
        assert!(validate_fact_value("").is_err());
        assert!(validate_fact_value(&"a".repeat(501)).is_err());
    }
}
