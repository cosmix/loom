//! Knowledge directory manager.

use super::gc::{analyze_gc_metrics, GcMetrics};
use super::types::KnowledgeFile;
use anyhow::{Context, Result};
use std::fs::{self, OpenOptions};
use std::io::Write;
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

        // create_new atomically fails if file exists, preventing TOCTOU race
        for file_type in KnowledgeFile::all() {
            let path = self.file_path(*file_type);
            let content = self.default_content(*file_type);
            match OpenOptions::new().write(true).create_new(true).open(&path) {
                Ok(mut file) => {
                    file.write_all(content.as_bytes())
                        .with_context(|| format!("Failed to write {}", file_type.filename()))?;
                }
                Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => {
                    // File already exists, skip (idempotent)
                }
                Err(e) => {
                    return Err(anyhow::anyhow!(e))
                        .with_context(|| format!("Failed to create {}", file_type.filename()));
                }
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
        let default = self.default_content(file_type);
        let content_owned = content.to_string();

        crate::fs::locking::locked_read_modify_write(&path, |existing| {
            let base = if existing.is_empty() {
                default
            } else {
                existing
            };
            if base.ends_with('\n') {
                format!("{base}\n{content_owned}\n")
            } else {
                format!("{base}\n\n{content_owned}\n")
            }
        })
        .with_context(|| format!("Failed to append to {}", file_type.filename()))
    }

    /// Replace a section in a knowledge file identified by its ## heading.
    ///
    /// Finds the first `## <heading>` line and replaces everything between it and
    /// the next `## ` heading (or EOF) with the new content. If the heading is not
    /// found, appends a new section.
    pub fn replace_section(
        &self,
        file_type: KnowledgeFile,
        heading: &str,
        content: &str,
    ) -> Result<()> {
        let path = self.file_path(file_type);
        let default = self.default_content(file_type);
        let heading_owned = heading.to_string();
        let content_owned = content.to_string();

        crate::fs::locking::locked_read_modify_write(&path, |existing| {
            let base = if existing.is_empty() {
                default
            } else {
                existing
            };

            let target_line = format!("## {heading_owned}");
            let lines: Vec<&str> = base.lines().collect();

            // Find the heading
            let heading_idx = lines.iter().position(|line| line.trim_end() == target_line);

            match heading_idx {
                Some(start) => {
                    // Find the next ## heading after this one (or EOF)
                    let end = lines
                        .iter()
                        .enumerate()
                        .skip(start + 1)
                        .find(|(_, line)| line.starts_with("## "))
                        .map(|(i, _)| i)
                        .unwrap_or(lines.len());

                    let mut result = String::new();
                    // Lines before the heading
                    for line in &lines[..start] {
                        result.push_str(line);
                        result.push('\n');
                    }
                    // Replacement section
                    result.push_str(&format!("## {heading_owned}\n\n{content_owned}\n"));
                    // Lines after the replaced section
                    if end < lines.len() {
                        result.push('\n');
                        for (i, line) in lines[end..].iter().enumerate() {
                            result.push_str(line);
                            if i < lines.len() - end - 1 {
                                result.push('\n');
                            }
                        }
                        // Preserve trailing newline
                        if base.ends_with('\n') {
                            result.push('\n');
                        }
                    }
                    result
                }
                None => {
                    // Heading not found, append
                    let mut result = base;
                    if !result.ends_with('\n') {
                        result.push('\n');
                    }
                    result.push_str(&format!("\n## {heading_owned}\n\n{content_owned}\n"));
                    result
                }
            }
        })
        .with_context(|| {
            format!(
                "Failed to replace section '{}' in {}",
                heading,
                file_type.filename()
            )
        })
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

    #[test]
    fn test_initialize_idempotent() {
        let temp = TempDir::new().unwrap();
        let project_root = temp.path();

        let knowledge = KnowledgeDir::new(project_root);
        knowledge.initialize().unwrap();

        // Append extra content to a file
        knowledge
            .append(KnowledgeFile::Mistakes, "## A Mistake\n\nDon't do this")
            .unwrap();
        let content_before = knowledge.read(KnowledgeFile::Mistakes).unwrap();

        // Re-initialize should NOT overwrite existing files
        knowledge.initialize().unwrap();
        let content_after = knowledge.read(KnowledgeFile::Mistakes).unwrap();
        assert_eq!(
            content_before, content_after,
            "initialize() must not overwrite existing files"
        );
    }

    #[test]
    fn test_replace_section_existing() {
        let temp = TempDir::new().unwrap();
        let project_root = temp.path();

        let knowledge = KnowledgeDir::new(project_root);
        knowledge.initialize().unwrap();

        // Add two sections
        knowledge
            .append(
                KnowledgeFile::Patterns,
                "## Section A\n\nOriginal A content",
            )
            .unwrap();
        knowledge
            .append(
                KnowledgeFile::Patterns,
                "## Section B\n\nOriginal B content",
            )
            .unwrap();

        // Replace Section A
        knowledge
            .replace_section(KnowledgeFile::Patterns, "Section A", "Updated A content")
            .unwrap();

        let content = knowledge.read(KnowledgeFile::Patterns).unwrap();
        assert!(
            content.contains("Updated A content"),
            "Should contain updated content"
        );
        assert!(
            !content.contains("Original A content"),
            "Should not contain old content"
        );
        // Section B should be untouched
        assert!(
            content.contains("Original B content"),
            "Section B should be preserved"
        );
    }

    #[test]
    fn test_replace_section_not_found_appends() {
        let temp = TempDir::new().unwrap();
        let project_root = temp.path();

        let knowledge = KnowledgeDir::new(project_root);
        knowledge.initialize().unwrap();

        knowledge
            .replace_section(KnowledgeFile::Patterns, "New Heading", "Brand new content")
            .unwrap();

        let content = knowledge.read(KnowledgeFile::Patterns).unwrap();
        assert!(content.contains("## New Heading"));
        assert!(content.contains("Brand new content"));
    }

    #[test]
    fn test_replace_section_at_eof() {
        let temp = TempDir::new().unwrap();
        let project_root = temp.path();

        let knowledge = KnowledgeDir::new(project_root);
        knowledge.initialize().unwrap();

        // Add a section that is at the end of the file (no following ## heading)
        knowledge
            .append(
                KnowledgeFile::Patterns,
                "## Last Section\n\nOld last content",
            )
            .unwrap();

        knowledge
            .replace_section(KnowledgeFile::Patterns, "Last Section", "New last content")
            .unwrap();

        let content = knowledge.read(KnowledgeFile::Patterns).unwrap();
        assert!(content.contains("New last content"));
        assert!(!content.contains("Old last content"));
    }

    #[test]
    fn test_replace_section_exact_heading_match() {
        let temp = TempDir::new().unwrap();
        let project_root = temp.path();

        let knowledge = KnowledgeDir::new(project_root);
        knowledge.initialize().unwrap();

        // Add sections with similar names
        knowledge
            .append(KnowledgeFile::Patterns, "## Merge Flow\n\nMerge content")
            .unwrap();
        knowledge
            .append(
                KnowledgeFile::Patterns,
                "## Merge Flow Extended\n\nExtended content",
            )
            .unwrap();

        // Replace only the exact match
        knowledge
            .replace_section(KnowledgeFile::Patterns, "Merge Flow", "Updated merge")
            .unwrap();

        let content = knowledge.read(KnowledgeFile::Patterns).unwrap();
        assert!(content.contains("Updated merge"));
        // The "Extended" section should be preserved
        assert!(
            content.contains("Extended content"),
            "Exact match should not affect similar-named sections"
        );
    }
}
