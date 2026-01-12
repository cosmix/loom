//! Persistent learning store for cross-session knowledge sharing.
//!
//! Learnings are categorized entries that agents can append to but never delete.
//! This provides a robust mechanism for accumulating lessons that survives
//! agent modifications. The store is persisted in .work/learnings/ as separate
//! markdown files per category.
//!
//! Categories:
//! - mistakes.md: Errors and corrections
//! - human-guidance.md: Advice from human operators
//! - patterns.md: Architectural patterns discovered
//! - conventions.md: Coding conventions learned
//!
//! Protection mechanisms:
//! - Each file has a .loom-protected marker
//! - Snapshots are taken before sessions
//! - Deletions are detected and restored

use anyhow::{Context, Result};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};

/// Learning category types
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum LearningCategory {
    /// Errors and corrections
    Mistake,
    /// Advice from human operators (requires --human flag)
    Guidance,
    /// Architectural patterns discovered
    Pattern,
    /// Coding conventions learned
    Convention,
}

impl LearningCategory {
    /// Get the filename for this category
    pub fn filename(&self) -> &'static str {
        match self {
            LearningCategory::Mistake => "mistakes.md",
            LearningCategory::Guidance => "human-guidance.md",
            LearningCategory::Pattern => "patterns.md",
            LearningCategory::Convention => "conventions.md",
        }
    }

    /// Get a human-readable name for this category
    pub fn display_name(&self) -> &'static str {
        match self {
            LearningCategory::Mistake => "Mistakes",
            LearningCategory::Guidance => "Human Guidance",
            LearningCategory::Pattern => "Patterns",
            LearningCategory::Convention => "Conventions",
        }
    }

    /// Get all categories
    pub fn all() -> &'static [LearningCategory] {
        &[
            LearningCategory::Mistake,
            LearningCategory::Guidance,
            LearningCategory::Pattern,
            LearningCategory::Convention,
        ]
    }
}

impl std::fmt::Display for LearningCategory {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            LearningCategory::Mistake => write!(f, "mistake"),
            LearningCategory::Guidance => write!(f, "guidance"),
            LearningCategory::Pattern => write!(f, "pattern"),
            LearningCategory::Convention => write!(f, "convention"),
        }
    }
}

impl std::str::FromStr for LearningCategory {
    type Err = anyhow::Error;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.to_lowercase().as_str() {
            "mistake" | "mistakes" => Ok(LearningCategory::Mistake),
            "guidance" | "human-guidance" => Ok(LearningCategory::Guidance),
            "pattern" | "patterns" => Ok(LearningCategory::Pattern),
            "convention" | "conventions" => Ok(LearningCategory::Convention),
            _ => {
                anyhow::bail!("Invalid category: {s}. Use: mistake, guidance, pattern, convention")
            }
        }
    }
}

/// A single learning entry
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Learning {
    /// When the learning was recorded
    pub timestamp: DateTime<Utc>,
    /// ID of the stage that recorded this learning (or "human" for guidance)
    pub stage_id: String,
    /// The learning description
    pub description: String,
    /// Optional correction (for mistakes)
    pub correction: Option<String>,
    /// Optional source reference (for guidance)
    pub source: Option<String>,
}

/// The protected marker that must be at the start of each learning file
pub const PROTECTED_MARKER: &str = "<!-- .loom-protected -->\n";

/// The header for a learning file
fn category_header(category: LearningCategory) -> String {
    format!("{PROTECTED_MARKER}# {}\n\n", category.display_name())
}

/// Get the learnings directory path
pub fn learnings_dir(work_dir: &Path) -> PathBuf {
    work_dir.join("learnings")
}

/// Get the path to a specific category file
pub fn category_file_path(work_dir: &Path, category: LearningCategory) -> PathBuf {
    learnings_dir(work_dir).join(category.filename())
}

/// Get the snapshots directory path
pub fn snapshots_dir(work_dir: &Path) -> PathBuf {
    learnings_dir(work_dir).join(".snapshots")
}

/// Initialize the learnings directory with all category files
pub fn init_learnings_dir(work_dir: &Path) -> Result<()> {
    let learnings_path = learnings_dir(work_dir);

    if !learnings_path.exists() {
        fs::create_dir_all(&learnings_path).with_context(|| {
            format!(
                "Failed to create learnings directory: {}",
                learnings_path.display()
            )
        })?;
    }

    // Create snapshots directory
    let snapshots_path = snapshots_dir(work_dir);
    if !snapshots_path.exists() {
        fs::create_dir_all(&snapshots_path).with_context(|| {
            format!(
                "Failed to create snapshots directory: {}",
                snapshots_path.display()
            )
        })?;
    }

    // Initialize category files if they don't exist
    for category in LearningCategory::all() {
        let file_path = category_file_path(work_dir, *category);
        if !file_path.exists() {
            let header = category_header(*category);
            fs::write(&file_path, header).with_context(|| {
                format!("Failed to create category file: {}", file_path.display())
            })?;
        }
    }

    Ok(())
}

/// Append a learning to the appropriate category file
pub fn append_learning(
    work_dir: &Path,
    category: LearningCategory,
    learning: &Learning,
) -> Result<()> {
    let file_path = category_file_path(work_dir, category);

    // Ensure the learnings directory exists
    if !learnings_dir(work_dir).exists() {
        init_learnings_dir(work_dir)?;
    }

    // Format the learning entry
    let entry = format_learning_entry(learning, category);

    // Append to file (create if doesn't exist)
    let mut file = fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(&file_path)
        .with_context(|| format!("Failed to open category file: {}", file_path.display()))?;

    // If file is new/empty, write header first
    let metadata = file.metadata()?;
    if metadata.len() == 0 {
        let header = category_header(category);
        file.write_all(header.as_bytes())?;
    }

    file.write_all(entry.as_bytes())
        .with_context(|| format!("Failed to append learning: {}", file_path.display()))?;

    Ok(())
}

/// Format a learning entry for markdown output
fn format_learning_entry(learning: &Learning, category: LearningCategory) -> String {
    let mut entry = String::new();

    entry.push_str(&format!(
        "## {} ({})\n\n",
        learning.timestamp.format("%Y-%m-%d %H:%M:%S UTC"),
        learning.stage_id
    ));

    match category {
        LearningCategory::Mistake => {
            entry.push_str("**Mistake:**\n");
            entry.push_str(&learning.description);
            entry.push_str("\n\n");

            if let Some(correction) = &learning.correction {
                entry.push_str("**Correction:**\n");
                entry.push_str(correction);
                entry.push_str("\n\n");
            }
        }
        LearningCategory::Guidance => {
            entry.push_str("**Guidance:**\n");
            entry.push_str(&learning.description);
            entry.push_str("\n\n");

            if let Some(source) = &learning.source {
                entry.push_str(&format!("**Source:** {source}\n\n"));
            }
        }
        LearningCategory::Pattern => {
            entry.push_str("**Pattern:**\n");
            entry.push_str(&learning.description);
            entry.push_str("\n\n");
        }
        LearningCategory::Convention => {
            entry.push_str("**Convention:**\n");
            entry.push_str(&learning.description);
            entry.push_str("\n\n");
        }
    }

    entry.push_str("---\n\n");
    entry
}

/// Read all learnings from a category file
pub fn read_learnings(work_dir: &Path, category: LearningCategory) -> Result<Vec<Learning>> {
    let file_path = category_file_path(work_dir, category);

    if !file_path.exists() {
        return Ok(Vec::new());
    }

    let content = fs::read_to_string(&file_path)
        .with_context(|| format!("Failed to read category file: {}", file_path.display()))?;

    parse_learnings(&content, category)
}

/// Parse learnings from markdown content
fn parse_learnings(content: &str, category: LearningCategory) -> Result<Vec<Learning>> {
    let mut learnings = Vec::new();
    let mut current_entry: Option<LearningBuilder> = None;

    for line in content.lines() {
        // Skip protected marker and main header
        if line.starts_with("<!--") || line.starts_with("# ") {
            continue;
        }

        // Detect new entry header: ## YYYY-MM-DD HH:MM:SS UTC (stage-id)
        if line.starts_with("## ") {
            // Save previous entry if any
            if let Some(builder) = current_entry.take() {
                if let Some(learning) = builder.build() {
                    learnings.push(learning);
                }
            }

            // Parse new entry header
            let header = line.trim_start_matches("## ");
            if let Some((timestamp_str, stage_part)) = header.split_once(" (") {
                let stage_id = stage_part.trim_end_matches(')').to_string();
                let timestamp =
                    chrono::NaiveDateTime::parse_from_str(timestamp_str, "%Y-%m-%d %H:%M:%S UTC")
                        .ok()
                        .map(|dt| DateTime::from_naive_utc_and_offset(dt, Utc))
                        .unwrap_or_else(Utc::now);

                current_entry = Some(LearningBuilder {
                    timestamp,
                    stage_id,
                    category,
                    current_field: None,
                    description: String::new(),
                    correction: None,
                    source: None,
                });
            }
            continue;
        }

        // Skip separators
        if line == "---" {
            continue;
        }

        // Parse field markers
        if let Some(builder) = &mut current_entry {
            if line.starts_with("**Mistake:**")
                || line.starts_with("**Guidance:**")
                || line.starts_with("**Pattern:**")
                || line.starts_with("**Convention:**")
            {
                builder.current_field = Some("description");
            } else if line.starts_with("**Correction:**") {
                builder.current_field = Some("correction");
            } else if line.starts_with("**Source:**") {
                let source = line.trim_start_matches("**Source:**").trim().to_string();
                builder.source = Some(source);
                builder.current_field = None;
            } else if let Some(field) = builder.current_field {
                // Append to current field
                match field {
                    "description" => {
                        if !builder.description.is_empty() {
                            builder.description.push('\n');
                        }
                        builder.description.push_str(line);
                    }
                    "correction" => {
                        if builder.correction.is_none() {
                            builder.correction = Some(String::new());
                        }
                        if let Some(correction) = &mut builder.correction {
                            if !correction.is_empty() {
                                correction.push('\n');
                            }
                            correction.push_str(line);
                        }
                    }
                    _ => {}
                }
            }
        }
    }

    // Save last entry
    if let Some(builder) = current_entry {
        if let Some(learning) = builder.build() {
            learnings.push(learning);
        }
    }

    Ok(learnings)
}

/// Builder for parsing learnings
struct LearningBuilder {
    timestamp: DateTime<Utc>,
    stage_id: String,
    #[allow(dead_code)]
    category: LearningCategory,
    current_field: Option<&'static str>,
    description: String,
    correction: Option<String>,
    source: Option<String>,
}

impl LearningBuilder {
    fn build(self) -> Option<Learning> {
        if self.description.is_empty() {
            return None;
        }

        Some(Learning {
            timestamp: self.timestamp,
            stage_id: self.stage_id,
            description: self.description.trim().to_string(),
            correction: self.correction.map(|s| s.trim().to_string()),
            source: self.source,
        })
    }
}

/// Get the most recent learnings for embedding in signals
pub fn get_recent_learnings(
    work_dir: &Path,
    max_per_category: usize,
) -> Result<Vec<(LearningCategory, Vec<Learning>)>> {
    let mut result = Vec::new();

    for category in LearningCategory::all() {
        let mut learnings = read_learnings(work_dir, *category)?;

        // Sort by timestamp descending (most recent first)
        learnings.sort_by(|a, b| b.timestamp.cmp(&a.timestamp));

        // Take only the most recent entries
        learnings.truncate(max_per_category);

        if !learnings.is_empty() {
            result.push((*category, learnings));
        }
    }

    Ok(result)
}

/// Format learnings for embedding in a signal
pub fn format_learnings_for_signal(work_dir: &Path, max_per_category: usize) -> Option<String> {
    let recent = get_recent_learnings(work_dir, max_per_category).ok()?;

    if recent.is_empty() {
        return None;
    }

    let mut output = String::new();

    for (category, learnings) in recent {
        output.push_str(&format!("### {}\n\n", category.display_name()));

        for learning in learnings {
            output.push_str(&format!(
                "- **{}** ({}): {}\n",
                learning.timestamp.format("%Y-%m-%d"),
                learning.stage_id,
                truncate_description(&learning.description, 200)
            ));

            if let Some(correction) = &learning.correction {
                output.push_str(&format!(
                    "  - *Correction:* {}\n",
                    truncate_description(correction, 150)
                ));
            }
        }

        output.push('\n');
    }

    Some(output)
}

/// Truncate a description for display
fn truncate_description(s: &str, max_len: usize) -> String {
    // Replace newlines with spaces for single-line display
    let single_line: String = s.lines().collect::<Vec<_>>().join(" ");

    if single_line.len() <= max_len {
        single_line
    } else {
        format!("{}…", &single_line[..max_len - 1])
    }
}

/// Create a snapshot of all learning files before a session
pub fn create_snapshot(work_dir: &Path, session_id: &str) -> Result<PathBuf> {
    let snapshots_path = snapshots_dir(work_dir);
    let snapshot_path = snapshots_path.join(session_id);

    if !snapshots_path.exists() {
        fs::create_dir_all(&snapshots_path)?;
    }

    if !snapshot_path.exists() {
        fs::create_dir_all(&snapshot_path)?;
    }

    for category in LearningCategory::all() {
        let source = category_file_path(work_dir, *category);
        if source.exists() {
            let dest = snapshot_path.join(category.filename());
            fs::copy(&source, &dest)
                .with_context(|| format!("Failed to snapshot {}", source.display()))?;
        }
    }

    Ok(snapshot_path)
}

/// Verify that learning files have not been truncated/deleted
pub fn verify_learnings(work_dir: &Path, session_id: &str) -> Result<VerificationResult> {
    let snapshot_path = snapshots_dir(work_dir).join(session_id);

    if !snapshot_path.exists() {
        return Ok(VerificationResult::NoSnapshot);
    }

    let mut issues = Vec::new();

    for category in LearningCategory::all() {
        let current_path = category_file_path(work_dir, *category);
        let snapshot_file = snapshot_path.join(category.filename());

        if !snapshot_file.exists() {
            continue;
        }

        let snapshot_content = fs::read_to_string(&snapshot_file)?;
        let snapshot_len = snapshot_content.len();

        if !current_path.exists() {
            issues.push(VerificationIssue::Deleted(*category));
            continue;
        }

        let current_content = fs::read_to_string(&current_path)?;
        let current_len = current_content.len();

        // Check if file was truncated (length decreased)
        if current_len < snapshot_len {
            issues.push(VerificationIssue::Truncated {
                category: *category,
                snapshot_len,
                current_len,
            });
        }

        // Check if protected marker was removed
        if !current_content.starts_with(PROTECTED_MARKER) {
            issues.push(VerificationIssue::MarkerRemoved(*category));
        }
    }

    if issues.is_empty() {
        Ok(VerificationResult::Ok)
    } else {
        Ok(VerificationResult::Issues(issues))
    }
}

/// Result of learning file verification
#[derive(Debug)]
pub enum VerificationResult {
    /// All files are intact
    Ok,
    /// No snapshot exists for this session
    NoSnapshot,
    /// Issues were found
    Issues(Vec<VerificationIssue>),
}

/// A verification issue found during post-session check
#[derive(Debug)]
pub enum VerificationIssue {
    /// A category file was deleted
    Deleted(LearningCategory),
    /// A category file was truncated
    Truncated {
        category: LearningCategory,
        snapshot_len: usize,
        current_len: usize,
    },
    /// The protected marker was removed
    MarkerRemoved(LearningCategory),
}

/// Restore learning files from a snapshot
pub fn restore_from_snapshot(work_dir: &Path, session_id: &str) -> Result<Vec<LearningCategory>> {
    let snapshot_path = snapshots_dir(work_dir).join(session_id);

    if !snapshot_path.exists() {
        anyhow::bail!("No snapshot found for session: {session_id}");
    }

    let mut restored = Vec::new();

    for category in LearningCategory::all() {
        let snapshot_file = snapshot_path.join(category.filename());
        if !snapshot_file.exists() {
            continue;
        }

        let current_path = category_file_path(work_dir, *category);
        fs::copy(&snapshot_file, &current_path)
            .with_context(|| format!("Failed to restore {}", current_path.display()))?;

        restored.push(*category);
    }

    Ok(restored)
}

/// Clean up a snapshot after successful verification
pub fn cleanup_snapshot(work_dir: &Path, session_id: &str) -> Result<()> {
    let snapshot_path = snapshots_dir(work_dir).join(session_id);

    if snapshot_path.exists() {
        fs::remove_dir_all(&snapshot_path)
            .with_context(|| format!("Failed to cleanup snapshot: {}", snapshot_path.display()))?;
    }

    Ok(())
}

/// Validate a learning description
pub fn validate_description(description: &str) -> Result<()> {
    if description.is_empty() {
        anyhow::bail!("Learning description cannot be empty");
    }

    if description.len() > 2000 {
        anyhow::bail!(
            "Learning description too long: {} characters (max 2000)",
            description.len()
        );
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[test]
    fn test_category_display() {
        assert_eq!(LearningCategory::Mistake.to_string(), "mistake");
        assert_eq!(LearningCategory::Guidance.to_string(), "guidance");
        assert_eq!(LearningCategory::Pattern.to_string(), "pattern");
        assert_eq!(LearningCategory::Convention.to_string(), "convention");
    }

    #[test]
    fn test_category_from_str() {
        assert_eq!(
            "mistake".parse::<LearningCategory>().unwrap(),
            LearningCategory::Mistake
        );
        assert_eq!(
            "GUIDANCE".parse::<LearningCategory>().unwrap(),
            LearningCategory::Guidance
        );
        assert_eq!(
            "patterns".parse::<LearningCategory>().unwrap(),
            LearningCategory::Pattern
        );
        assert!("invalid".parse::<LearningCategory>().is_err());
    }

    #[test]
    fn test_init_learnings_dir() {
        let temp_dir = TempDir::new().unwrap();
        let work_dir = temp_dir.path();

        init_learnings_dir(work_dir).unwrap();

        assert!(learnings_dir(work_dir).exists());
        assert!(snapshots_dir(work_dir).exists());

        for category in LearningCategory::all() {
            let file_path = category_file_path(work_dir, *category);
            assert!(file_path.exists());

            let content = fs::read_to_string(&file_path).unwrap();
            assert!(content.starts_with(PROTECTED_MARKER));
        }
    }

    #[test]
    fn test_append_and_read_learning() {
        let temp_dir = TempDir::new().unwrap();
        let work_dir = temp_dir.path();

        init_learnings_dir(work_dir).unwrap();

        let learning = Learning {
            timestamp: Utc::now(),
            stage_id: "test-stage".to_string(),
            description: "Test learning description".to_string(),
            correction: Some("Test correction".to_string()),
            source: None,
        };

        append_learning(work_dir, LearningCategory::Mistake, &learning).unwrap();

        let learnings = read_learnings(work_dir, LearningCategory::Mistake).unwrap();
        assert_eq!(learnings.len(), 1);
        assert_eq!(learnings[0].description, "Test learning description");
        assert_eq!(learnings[0].correction.as_deref(), Some("Test correction"));
    }

    #[test]
    fn test_snapshot_and_verify() {
        let temp_dir = TempDir::new().unwrap();
        let work_dir = temp_dir.path();

        init_learnings_dir(work_dir).unwrap();

        let learning = Learning {
            timestamp: Utc::now(),
            stage_id: "test-stage".to_string(),
            description: "Important learning".to_string(),
            correction: None,
            source: None,
        };

        append_learning(work_dir, LearningCategory::Pattern, &learning).unwrap();

        // Create snapshot
        let snapshot_path = create_snapshot(work_dir, "session-123").unwrap();
        assert!(snapshot_path.exists());

        // Verify - should be OK
        let result = verify_learnings(work_dir, "session-123").unwrap();
        assert!(matches!(result, VerificationResult::Ok));

        // Truncate file
        let pattern_file = category_file_path(work_dir, LearningCategory::Pattern);
        fs::write(&pattern_file, "truncated").unwrap();

        // Verify - should detect issue
        let result = verify_learnings(work_dir, "session-123").unwrap();
        assert!(matches!(result, VerificationResult::Issues(_)));

        // Restore from snapshot
        let restored = restore_from_snapshot(work_dir, "session-123").unwrap();
        assert!(restored.contains(&LearningCategory::Pattern));

        // Content should be back
        let content = fs::read_to_string(&pattern_file).unwrap();
        assert!(content.contains("Important learning"));
    }

    #[test]
    fn test_format_learnings_for_signal() {
        let temp_dir = TempDir::new().unwrap();
        let work_dir = temp_dir.path();

        init_learnings_dir(work_dir).unwrap();

        let learning1 = Learning {
            timestamp: Utc::now(),
            stage_id: "stage-a".to_string(),
            description: "First mistake".to_string(),
            correction: Some("Fix it this way".to_string()),
            source: None,
        };

        let learning2 = Learning {
            timestamp: Utc::now(),
            stage_id: "stage-b".to_string(),
            description: "A useful pattern".to_string(),
            correction: None,
            source: None,
        };

        append_learning(work_dir, LearningCategory::Mistake, &learning1).unwrap();
        append_learning(work_dir, LearningCategory::Pattern, &learning2).unwrap();

        let signal_content = format_learnings_for_signal(work_dir, 5).unwrap();
        assert!(signal_content.contains("Mistakes"));
        assert!(signal_content.contains("First mistake"));
        assert!(signal_content.contains("Patterns"));
        assert!(signal_content.contains("A useful pattern"));
    }

    #[test]
    fn test_validate_description() {
        assert!(validate_description("Valid description").is_ok());
        assert!(validate_description("").is_err());
        assert!(validate_description(&"a".repeat(2001)).is_err());
    }
}
