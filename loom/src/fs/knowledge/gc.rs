//! Knowledge GC (garbage collection) analysis.

use super::types::KnowledgeFile;
use anyhow::{Context, Result};
use std::collections::HashMap;
use std::fs;
use std::path::Path;

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

/// Analyze GC metrics for knowledge files
pub fn analyze_gc_metrics(
    knowledge_root: &Path,
    max_file_lines: usize,
    max_total_lines: usize,
) -> Result<GcMetrics> {
    analyze_gc_metrics_with_promoted(
        knowledge_root,
        max_file_lines,
        max_total_lines,
        DEFAULT_MAX_PROMOTED_BLOCKS,
    )
}

/// Analyze GC metrics with configurable promoted block threshold
pub fn analyze_gc_metrics_with_promoted(
    knowledge_root: &Path,
    max_file_lines: usize,
    max_total_lines: usize,
    max_promoted_blocks: usize,
) -> Result<GcMetrics> {
    let mut per_file = Vec::new();
    let mut total_lines = 0;

    for file_type in KnowledgeFile::all() {
        let path = knowledge_root.join(file_type.filename());
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::fs::knowledge::KnowledgeDir;
    use tempfile::TempDir;

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

        let metrics = analyze_gc_metrics(knowledge.root(), 200, 800).unwrap();
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

        let metrics = analyze_gc_metrics(knowledge.root(), 200, 800).unwrap();
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

        let metrics = analyze_gc_metrics(knowledge.root(), 200, 800).unwrap();
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

        let metrics = analyze_gc_metrics(knowledge.root(), 200, 800).unwrap();
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

        let metrics = analyze_gc_metrics(knowledge.root(), 200, 800).unwrap();
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
