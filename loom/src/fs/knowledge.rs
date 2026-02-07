//! Knowledge map operations for curated codebase knowledge.
//!
//! Design principle: Claude Code already has Glob, Grep, Read, LSP tools.
//! We curate high-level knowledge that helps agents know WHERE to look,
//! not raw indexing.

use anyhow::{Context, Result};
use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};

/// Known knowledge file types
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum KnowledgeFile {
    Architecture,
    EntryPoints,
    Patterns,
    Conventions,
    Mistakes,
    Stack,
    Concerns,
}

impl KnowledgeFile {
    /// Get the filename for this knowledge file type
    pub fn filename(&self) -> &'static str {
        match self {
            KnowledgeFile::Architecture => "architecture.md",
            KnowledgeFile::EntryPoints => "entry-points.md",
            KnowledgeFile::Patterns => "patterns.md",
            KnowledgeFile::Conventions => "conventions.md",
            KnowledgeFile::Mistakes => "mistakes.md",
            KnowledgeFile::Stack => "stack.md",
            KnowledgeFile::Concerns => "concerns.md",
        }
    }

    /// Get a description of what this file contains
    pub fn description(&self) -> &'static str {
        match self {
            KnowledgeFile::Architecture => {
                "High-level component relationships, data flow, module dependencies"
            }
            KnowledgeFile::EntryPoints => "Key files agents should read first",
            KnowledgeFile::Patterns => "Architectural patterns discovered in the codebase",
            KnowledgeFile::Conventions => "Coding conventions discovered in the codebase",
            KnowledgeFile::Mistakes => "Mistakes made and lessons learned - what to avoid",
            KnowledgeFile::Stack => "Dependencies, frameworks, and tooling used in the project",
            KnowledgeFile::Concerns => "Technical debt, warnings, and issues to address",
        }
    }

    /// Parse from filename
    pub fn from_filename(filename: &str) -> Option<Self> {
        match filename {
            "architecture.md" => Some(KnowledgeFile::Architecture),
            "entry-points.md" => Some(KnowledgeFile::EntryPoints),
            "patterns.md" => Some(KnowledgeFile::Patterns),
            "conventions.md" => Some(KnowledgeFile::Conventions),
            "mistakes.md" => Some(KnowledgeFile::Mistakes),
            "stack.md" => Some(KnowledgeFile::Stack),
            "concerns.md" => Some(KnowledgeFile::Concerns),
            _ => None,
        }
    }

    /// All known knowledge file types
    pub fn all() -> &'static [KnowledgeFile] {
        &[
            KnowledgeFile::Architecture,
            KnowledgeFile::EntryPoints,
            KnowledgeFile::Patterns,
            KnowledgeFile::Conventions,
            KnowledgeFile::Mistakes,
            KnowledgeFile::Stack,
            KnowledgeFile::Concerns,
        ]
    }
}

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
        self.analyze_gc_metrics_with_promoted(
            max_file_lines,
            max_total_lines,
            DEFAULT_MAX_PROMOTED_BLOCKS,
        )
    }

    /// Analyze GC metrics with configurable promoted block threshold
    pub fn analyze_gc_metrics_with_promoted(
        &self,
        max_file_lines: usize,
        max_total_lines: usize,
        max_promoted_blocks: usize,
    ) -> Result<GcMetrics> {
        let mut per_file = Vec::new();
        let mut total_lines = 0;

        for file_type in KnowledgeFile::all() {
            let path = self.file_path(*file_type);
            if !path.exists() {
                continue;
            }

            let content = fs::read_to_string(&path)
                .with_context(|| format!("Failed to read {}", file_type.filename()))?;

            let line_count = content.lines().count();
            total_lines += line_count;

            // Find duplicate headers
            let mut header_counts: HashMap<String, usize> = HashMap::new();
            for line in content.lines() {
                if line.starts_with("## ") {
                    let header = line.to_string();
                    *header_counts.entry(header).or_insert(0) += 1;
                }
            }

            let duplicate_headers: Vec<String> = header_counts
                .iter()
                .filter(|(_, &count)| count > 1)
                .map(|(header, _)| header.clone())
                .collect();

            // Count promoted blocks
            let promoted_block_count = content
                .lines()
                .filter(|line| line.starts_with("## Promoted from Memory"))
                .count();

            let has_issues = line_count > max_file_lines
                || !duplicate_headers.is_empty()
                || promoted_block_count > max_promoted_blocks;

            per_file.push(FileGcMetrics {
                file_type: *file_type,
                line_count,
                duplicate_headers,
                promoted_block_count,
                has_issues,
            });
        }

        // Build reasons for GC recommendation
        let mut reasons = Vec::new();

        for file_metrics in &per_file {
            if file_metrics.line_count > max_file_lines {
                reasons.push(format!(
                    "{} has {} lines (max: {})",
                    file_metrics.file_type.filename(),
                    file_metrics.line_count,
                    max_file_lines
                ));
            }

            if !file_metrics.duplicate_headers.is_empty() {
                reasons.push(format!(
                    "{} has duplicate headers: {}",
                    file_metrics.file_type.filename(),
                    file_metrics.duplicate_headers.join(", ")
                ));
            }

            if file_metrics.promoted_block_count > max_promoted_blocks {
                reasons.push(format!(
                    "{} has {} promoted blocks (consider consolidating)",
                    file_metrics.file_type.filename(),
                    file_metrics.promoted_block_count
                ));
            }
        }

        if total_lines > max_total_lines {
            reasons.push(format!(
                "Total lines {} exceeds max {}",
                total_lines, max_total_lines
            ));
        }

        let gc_recommended = !reasons.is_empty();

        Ok(GcMetrics {
            total_lines,
            per_file,
            gc_recommended,
            reasons,
        })
    }
}

/// Default maximum lines per knowledge file before GC is recommended
pub const DEFAULT_MAX_FILE_LINES: usize = 200;

/// Default maximum total lines across all knowledge files before GC is recommended
pub const DEFAULT_MAX_TOTAL_LINES: usize = 800;

/// Default maximum promoted memory blocks per file before GC is recommended
pub const DEFAULT_MAX_PROMOTED_BLOCKS: usize = 3;

/// Metrics for a single knowledge file's GC analysis
#[derive(Debug)]
pub struct FileGcMetrics {
    pub file_type: KnowledgeFile,
    pub line_count: usize,
    pub duplicate_headers: Vec<String>,
    pub promoted_block_count: usize,
    pub has_issues: bool,
}

/// Overall GC metrics across all knowledge files
#[derive(Debug)]
pub struct GcMetrics {
    pub total_lines: usize,
    pub per_file: Vec<FileGcMetrics>,
    pub gc_recommended: bool,
    pub reasons: Vec<String>,
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[test]
    fn test_knowledge_file_types() {
        assert_eq!(KnowledgeFile::Architecture.filename(), "architecture.md");
        assert_eq!(KnowledgeFile::EntryPoints.filename(), "entry-points.md");
        assert_eq!(KnowledgeFile::Patterns.filename(), "patterns.md");
        assert_eq!(KnowledgeFile::Conventions.filename(), "conventions.md");
        assert_eq!(KnowledgeFile::Mistakes.filename(), "mistakes.md");
        assert_eq!(KnowledgeFile::Stack.filename(), "stack.md");
        assert_eq!(KnowledgeFile::Concerns.filename(), "concerns.md");
    }

    #[test]
    fn test_knowledge_file_from_filename() {
        assert_eq!(
            KnowledgeFile::from_filename("architecture.md"),
            Some(KnowledgeFile::Architecture)
        );
        assert_eq!(
            KnowledgeFile::from_filename("entry-points.md"),
            Some(KnowledgeFile::EntryPoints)
        );
        assert_eq!(
            KnowledgeFile::from_filename("patterns.md"),
            Some(KnowledgeFile::Patterns)
        );
        assert_eq!(
            KnowledgeFile::from_filename("mistakes.md"),
            Some(KnowledgeFile::Mistakes)
        );
        assert_eq!(
            KnowledgeFile::from_filename("stack.md"),
            Some(KnowledgeFile::Stack)
        );
        assert_eq!(
            KnowledgeFile::from_filename("concerns.md"),
            Some(KnowledgeFile::Concerns)
        );
        assert_eq!(KnowledgeFile::from_filename("unknown.md"), None);
    }

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
    fn test_gc_metrics_clean() {
        let temp = TempDir::new().unwrap();
        let project_root = temp.path();

        let knowledge = KnowledgeDir::new(project_root);
        knowledge.initialize().unwrap();

        // Add small content to architecture
        knowledge
            .append(
                KnowledgeFile::Architecture,
                "## Small Section\n\n- Line 1\n- Line 2",
            )
            .unwrap();

        let metrics = knowledge.analyze_gc_metrics(200, 800).unwrap();
        assert!(
            !metrics.gc_recommended,
            "Clean metrics should not recommend GC"
        );
        assert!(metrics.reasons.is_empty());
        assert!(metrics.total_lines < 800);
    }

    #[test]
    fn test_gc_metrics_large_file() {
        let temp = TempDir::new().unwrap();
        let project_root = temp.path();

        let knowledge = KnowledgeDir::new(project_root);
        knowledge.initialize().unwrap();

        // Add 250 lines to architecture
        let mut big_content = String::from("## Big Section\n\n");
        for i in 0..250 {
            big_content.push_str(&format!("- Line {}\n", i));
        }
        knowledge
            .append(KnowledgeFile::Architecture, &big_content)
            .unwrap();

        let metrics = knowledge.analyze_gc_metrics(200, 800).unwrap();
        assert!(metrics.gc_recommended, "Large file should recommend GC");
        assert!(!metrics.reasons.is_empty());
        assert!(metrics
            .reasons
            .iter()
            .any(|r| r.contains("architecture.md") && r.contains("lines")));
    }

    #[test]
    fn test_gc_metrics_duplicate_headers() {
        let temp = TempDir::new().unwrap();
        let project_root = temp.path();

        let knowledge = KnowledgeDir::new(project_root);
        knowledge.initialize().unwrap();

        // Add duplicate headers
        knowledge
            .append(
                KnowledgeFile::Patterns,
                "## Same Header\n\n- Content 1\n\n## Same Header\n\n- Content 2",
            )
            .unwrap();

        let metrics = knowledge.analyze_gc_metrics(200, 800).unwrap();
        assert!(
            metrics.gc_recommended,
            "Duplicate headers should recommend GC"
        );

        // Check that duplicate was detected
        let pattern_metrics = metrics
            .per_file
            .iter()
            .find(|m| m.file_type == KnowledgeFile::Patterns)
            .unwrap();
        assert!(!pattern_metrics.duplicate_headers.is_empty());
        assert!(metrics
            .reasons
            .iter()
            .any(|r| r.contains("patterns.md") && r.contains("duplicate headers")));
    }

    #[test]
    fn test_gc_metrics_promoted_blocks() {
        let temp = TempDir::new().unwrap();
        let project_root = temp.path();

        let knowledge = KnowledgeDir::new(project_root);
        knowledge.initialize().unwrap();

        // Add 4 promoted blocks
        let mut content = String::new();
        for i in 0..4 {
            content.push_str(&format!("## Promoted from Memory {}\n\n- Content\n\n", i));
        }
        knowledge.append(KnowledgeFile::Mistakes, &content).unwrap();

        let metrics = knowledge.analyze_gc_metrics(200, 800).unwrap();
        assert!(
            metrics.gc_recommended,
            "Too many promoted blocks should recommend GC"
        );

        let mistakes_metrics = metrics
            .per_file
            .iter()
            .find(|m| m.file_type == KnowledgeFile::Mistakes)
            .unwrap();
        assert_eq!(mistakes_metrics.promoted_block_count, 4);
        assert!(metrics
            .reasons
            .iter()
            .any(|r| r.contains("mistakes.md") && r.contains("promoted blocks")));
    }

    #[test]
    fn test_gc_metrics_total_lines() {
        let temp = TempDir::new().unwrap();
        let project_root = temp.path();

        let knowledge = KnowledgeDir::new(project_root);
        knowledge.initialize().unwrap();

        // Spread content across multiple files to exceed 800 total
        let mut medium_content = String::from("## Medium Section\n\n");
        for i in 0..180 {
            medium_content.push_str(&format!("- Line {}\n", i));
        }

        // Add to multiple files
        knowledge
            .append(KnowledgeFile::Architecture, &medium_content)
            .unwrap();
        knowledge
            .append(KnowledgeFile::Patterns, &medium_content)
            .unwrap();
        knowledge
            .append(KnowledgeFile::Conventions, &medium_content)
            .unwrap();
        knowledge
            .append(KnowledgeFile::Mistakes, &medium_content)
            .unwrap();
        knowledge
            .append(KnowledgeFile::Stack, &medium_content)
            .unwrap();

        let metrics = knowledge.analyze_gc_metrics(200, 800).unwrap();
        assert!(
            metrics.gc_recommended,
            "Total lines exceeding max should recommend GC"
        );
        assert!(metrics.total_lines > 800);
        assert!(metrics
            .reasons
            .iter()
            .any(|r| r.contains("Total lines") && r.contains("exceeds")));
    }
}
