//! Knowledge directory manager.

use super::gc::{analyze_gc_metrics, GcMetrics};
use super::types::KnowledgeFile;
use anyhow::{Context, Result};
use std::fs;
use std::path::{Path, PathBuf};

/// Manager for the doc/loom/knowledge/ directory
pub struct KnowledgeDir {
    root: PathBuf,
}

impl KnowledgeDir {
    /// Create a new KnowledgeDir from the project root directory
    pub fn new<P: AsRef<Path>>(project_root: P) -> Self {
        Self {
            root: project_root.as_ref().join("doc/loom/knowledge"),
        }
    }

    /// Get the knowledge directory path
    pub fn root(&self) -> &Path {
        &self.root
    }

    /// Check if the knowledge directory exists
    pub fn exists(&self) -> bool {
        self.root.exists()
    }

    /// Check if the knowledge directory has any meaningful content
    ///
    /// Returns true if at least one knowledge file exists and has content
    /// beyond the default placeholder text.
    pub fn has_content(&self) -> bool {
        if !self.exists() {
            return false;
        }

        for file_type in KnowledgeFile::all() {
            let path = self.file_path(*file_type);
            if path.exists() {
                if let Ok(content) = fs::read_to_string(&path) {
                    // Check if content has more than just the default template
                    // by looking for ## headers added by agents
                    if content.lines().any(|line| {
                        line.starts_with("## ")
                            && !line.contains("(Add ")
                            && !line.contains("append-only")
                    }) {
                        return true;
                    }
                }
            }
        }

        false
    }

    /// Initialize the knowledge directory with default files
    pub fn initialize(&self) -> Result<()> {
        if !self.root.exists() {
            fs::create_dir_all(&self.root).context("Failed to create knowledge directory")?;
        }

        // Create default files if they don't exist
        for file_type in KnowledgeFile::all() {
            let path = self.file_path(*file_type);
            if !path.exists() {
                let content = self.default_content(*file_type);
                fs::write(&path, content)
                    .with_context(|| format!("Failed to create {}", file_type.filename()))?;
            }
        }

        Ok(())
    }

    /// Get the path to a specific knowledge file
    pub fn file_path(&self, file_type: KnowledgeFile) -> PathBuf {
        self.root.join(file_type.filename())
    }

    /// Read a knowledge file
    pub fn read(&self, file_type: KnowledgeFile) -> Result<String> {
        let path = self.file_path(file_type);
        fs::read_to_string(&path)
            .with_context(|| format!("Failed to read {}", file_type.filename()))
    }

    /// Read all knowledge files and return as a map
    pub fn read_all(&self) -> Result<Vec<(KnowledgeFile, String)>> {
        let mut results = Vec::new();
        for file_type in KnowledgeFile::all() {
            let path = self.file_path(*file_type);
            if path.exists() {
                let content = fs::read_to_string(&path)
                    .with_context(|| format!("Failed to read {}", file_type.filename()))?;
                results.push((*file_type, content));
            }
        }
        Ok(results)
    }

    /// Append content to a knowledge file (knowledge files are append-only)
    pub fn append(&self, file_type: KnowledgeFile, content: &str) -> Result<()> {
        let path = self.file_path(file_type);

        // Read existing content
        let existing = if path.exists() {
            fs::read_to_string(&path)
                .with_context(|| format!("Failed to read {}", file_type.filename()))?
        } else {
            self.default_content(file_type)
        };

        // Append new content with proper spacing
        let new_content = if existing.ends_with('\n') {
            format!("{existing}\n{content}\n")
        } else {
            format!("{existing}\n\n{content}\n")
        };

        fs::write(&path, new_content)
            .with_context(|| format!("Failed to write {}", file_type.filename()))?;

        Ok(())
    }

    /// Generate a compact summary of all knowledge for embedding in signals
    pub fn generate_summary(&self) -> Result<String> {
        let mut summary = String::new();
        summary.push_str("## Knowledge Summary\n\n");
        summary.push_str("> Curated knowledge to help you navigate the codebase.\n\n");

        for file_type in KnowledgeFile::all() {
            let path = self.file_path(*file_type);
            if path.exists() {
                let content = fs::read_to_string(&path).ok();
                if let Some(content) = content {
                    // Extract just the headers and first-level items for a compact summary
                    let compact = self.extract_compact_summary(&content);
                    if !compact.is_empty() {
                        summary.push_str(&format!("### {}\n\n", file_type.description()));
                        summary.push_str(&compact);
                        summary.push_str("\n\n");
                    }
                }
            }
        }

        if summary.len()
            <= "## Knowledge Summary\n\n> Curated knowledge to help you navigate the codebase.\n\n"
                .len()
        {
            return Ok(String::new());
        }

        Ok(summary.trim_end().to_string())
    }

    /// Extract a compact summary from a knowledge file
    fn extract_compact_summary(&self, content: &str) -> String {
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

    /// Get default content for a knowledge file type
    fn default_content(&self, file_type: KnowledgeFile) -> String {
        match file_type {
            KnowledgeFile::Architecture => r#"# Architecture

> High-level component relationships, data flow, and module dependencies.
> This file is append-only - agents add discoveries, never delete.

(Add architecture diagrams and component relationships as you discover them)
"#
            .to_string(),
            KnowledgeFile::EntryPoints => r#"# Entry Points

> Key files agents should read first to understand the codebase.
> This file is append-only - agents add discoveries, never delete.

(Add entry points as you discover them)
"#
            .to_string(),
            KnowledgeFile::Patterns => r#"# Architectural Patterns

> Discovered patterns in the codebase that help agents understand how things work.
> This file is append-only - agents add discoveries, never delete.

(Add patterns as you discover them)
"#
            .to_string(),
            KnowledgeFile::Conventions => r#"# Coding Conventions

> Discovered coding conventions in the codebase.
> This file is append-only - agents add discoveries, never delete.

(Add conventions as you discover them)
"#
            .to_string(),
            KnowledgeFile::Mistakes => r#"# Mistakes & Lessons Learned

> Record mistakes made during development and how to avoid them.
> This file is append-only - agents add discoveries, never delete.
>
> Format: Describe what went wrong, why, and how to avoid it next time.

(Add mistakes and lessons as you encounter them)
"#
            .to_string(),
            KnowledgeFile::Stack => r#"# Stack & Dependencies

> Project technology stack, frameworks, and key dependencies.
> This file is append-only - agents add discoveries, never delete.

(Add stack information as you discover it)
"#
            .to_string(),
            KnowledgeFile::Concerns => r#"# Concerns & Technical Debt

> Technical debt, warnings, issues, and improvements needed.
> This file is append-only - agents add discoveries, never delete.

(Add concerns as you discover them)
"#
            .to_string(),
        }
    }

    /// List all knowledge files that exist
    pub fn list_files(&self) -> Result<Vec<(KnowledgeFile, PathBuf)>> {
        let mut files = Vec::new();
        for file_type in KnowledgeFile::all() {
            let path = self.file_path(*file_type);
            if path.exists() {
                files.push((*file_type, path));
            }
        }
        Ok(files)
    }

    /// Analyze GC metrics for knowledge files
    pub fn analyze_gc_metrics(
        &self,
        max_file_lines: usize,
        max_total_lines: usize,
    ) -> Result<GcMetrics> {
        analyze_gc_metrics(&self.root, max_file_lines, max_total_lines)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[test]
    fn test_knowledge_dir_initialize() {
        let temp = TempDir::new().unwrap();
        let project_root = temp.path();

        let knowledge = KnowledgeDir::new(project_root);
        assert!(!knowledge.exists());

        knowledge.initialize().unwrap();
        assert!(knowledge.exists());
        assert!(project_root.join("doc/loom/knowledge").exists());

        // Check all files were created
        for file_type in KnowledgeFile::all() {
            let path = knowledge.file_path(*file_type);
            assert!(path.exists(), "File should exist: {}", file_type.filename());
        }
    }

    #[test]
    fn test_knowledge_append() {
        let temp = TempDir::new().unwrap();
        let project_root = temp.path();

        let knowledge = KnowledgeDir::new(project_root);
        knowledge.initialize().unwrap();

        // Append to entry-points
        knowledge
            .append(
                KnowledgeFile::EntryPoints,
                "## New Section\n\n- New entry point",
            )
            .unwrap();

        let content = knowledge.read(KnowledgeFile::EntryPoints).unwrap();
        assert!(content.contains("## New Section"));
        assert!(content.contains("- New entry point"));
    }

    #[test]
    fn test_generate_summary() {
        let temp = TempDir::new().unwrap();
        let project_root = temp.path();

        let knowledge = KnowledgeDir::new(project_root);
        knowledge.initialize().unwrap();

        // Add some content
        knowledge
            .append(
                KnowledgeFile::EntryPoints,
                "## CLI Entry Point\n\n- main.rs - CLI definition",
            )
            .unwrap();

        let summary = knowledge.generate_summary().unwrap();
        assert!(summary.contains("Knowledge Summary"));
        assert!(summary.contains("CLI Entry Point"));
    }
}
