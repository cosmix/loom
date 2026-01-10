use anyhow::{Context, Result};
use std::fs;
use std::path::Path;

use crate::commands::runner::serialization::{runner_from_markdown, runner_to_markdown};
use crate::commands::track::serialization::{track_from_markdown, track_to_markdown};
use crate::models::runner::Runner;
use crate::models::track::Track;

/// Trait for types that can be serialized to and from Markdown format.
///
/// This trait provides a unified interface for loading and saving domain objects
/// as Markdown files with YAML frontmatter. It enables consistent file I/O
/// patterns across different model types (Runner, Track, Signal, etc.).
pub trait MarkdownSerializable: Sized {
    /// Parse an instance from markdown content.
    ///
    /// # Arguments
    /// * `content` - The markdown string to parse, including frontmatter
    ///
    /// # Returns
    /// * `Ok(Self)` - Successfully parsed instance
    /// * `Err` - If parsing fails due to missing required fields or invalid format
    fn from_markdown(content: &str) -> Result<Self>;

    /// Serialize the instance to markdown format.
    ///
    /// The output includes YAML frontmatter delimited by `---` and
    /// structured markdown sections for human readability.
    ///
    /// # Returns
    /// * `Ok(String)` - The serialized markdown content
    /// * `Err` - If serialization fails
    fn to_markdown(&self) -> Result<String>;

    /// Load an instance from a file path.
    ///
    /// # Arguments
    /// * `path` - The path to the markdown file
    ///
    /// # Returns
    /// * `Ok(Self)` - Successfully loaded and parsed instance
    /// * `Err` - If file reading or parsing fails
    fn load(path: &Path) -> Result<Self> {
        let content = fs::read_to_string(path)
            .with_context(|| format!("Failed to read {}", path.display()))?;
        Self::from_markdown(&content)
    }

    /// Save the instance to a file path.
    ///
    /// Creates parent directories if they don't exist.
    ///
    /// # Arguments
    /// * `path` - The destination path for the markdown file
    ///
    /// # Returns
    /// * `Ok(())` - Successfully written
    /// * `Err` - If writing fails
    fn save(&self, path: &Path) -> Result<()> {
        let content = self.to_markdown()?;

        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)
                .with_context(|| format!("Failed to create directory {}", parent.display()))?;
        }

        fs::write(path, content).with_context(|| format!("Failed to write {}", path.display()))?;
        Ok(())
    }
}

impl MarkdownSerializable for Runner {
    fn from_markdown(content: &str) -> Result<Self> {
        runner_from_markdown(content)
    }

    fn to_markdown(&self) -> Result<String> {
        runner_to_markdown(self)
    }
}

impl MarkdownSerializable for Track {
    fn from_markdown(content: &str) -> Result<Self> {
        track_from_markdown(content)
    }

    fn to_markdown(&self) -> Result<String> {
        track_to_markdown(self)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::runner::RunnerStatus;
    use crate::models::track::TrackStatus;
    use tempfile::TempDir;

    #[test]
    fn test_runner_roundtrip() {
        let runner = Runner::new("test-runner".to_string(), "developer".to_string());

        let markdown = runner.to_markdown().expect("Should serialize runner");
        let parsed = Runner::from_markdown(&markdown).expect("Should parse runner markdown");

        assert_eq!(parsed.name, runner.name);
        assert_eq!(parsed.runner_type, runner.runner_type);
        assert_eq!(parsed.status, RunnerStatus::Idle);
    }

    #[test]
    fn test_track_roundtrip() {
        let track = Track::new(
            "test-track".to_string(),
            Some("A test track description".to_string()),
        );

        let markdown = track.to_markdown().expect("Should serialize track");
        let parsed = Track::from_markdown(&markdown).expect("Should parse track markdown");

        assert_eq!(parsed.name, track.name);
        assert_eq!(parsed.description, track.description);
        assert_eq!(parsed.status, TrackStatus::Active);
    }

    #[test]
    fn test_runner_save_and_load() {
        let temp_dir = TempDir::new().expect("Should create temp dir");
        let file_path = temp_dir.path().join("runners").join("test-runner.md");

        let runner = Runner::new("test-runner".to_string(), "developer".to_string());
        runner.save(&file_path).expect("Should save runner");

        let loaded = Runner::load(&file_path).expect("Should load runner");
        assert_eq!(loaded.name, runner.name);
        assert_eq!(loaded.runner_type, runner.runner_type);
    }

    #[test]
    fn test_track_save_and_load() {
        let temp_dir = TempDir::new().expect("Should create temp dir");
        let file_path = temp_dir.path().join("tracks").join("test-track.md");

        let track = Track::new("test-track".to_string(), Some("Description".to_string()));
        track.save(&file_path).expect("Should save track");

        let loaded = Track::load(&file_path).expect("Should load track");
        assert_eq!(loaded.name, track.name);
        assert_eq!(loaded.description, track.description);
    }
}
