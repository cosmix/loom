//! Knowledge GC (garbage collection) analysis.
//!
//! GC maintains the tiered knowledge hierarchy — it does NOT enforce a size
//! budget. There is deliberately no aggregate line cap: total lines are
//! reported, never a reason to compact. The reasons GC fires are structural:
//! a tier-1 file that has outgrown its summary role, an oversized section that
//! should be extracted into a tier-2 topic, duplicate headers, a tier-1 link
//! pointing at a missing topic, a topic nothing links to, or a stale index.

use super::index;
use super::types::{KnowledgeFile, KnowledgeLayout, INDEX_FILENAME};
use anyhow::{Context, Result};
use regex::Regex;
use std::collections::HashMap;
use std::fs;
use std::path::Path;
use std::sync::LazyLock;

/// Default maximum lines for a tier-1 summary file before GC is recommended
pub const DEFAULT_MAX_TIER1_LINES: usize = 250;

/// Default maximum lines for a tier-2 topic file before GC is recommended
pub const DEFAULT_MAX_TOPIC_LINES: usize = 500;

/// Default maximum promoted memory blocks per file before GC is recommended
pub const DEFAULT_MAX_PROMOTED_BLOCKS: usize = 3;

/// A `## ` section in a tier-1 file longer than this many lines should be
/// extracted into a tier-2 topic and replaced with a summary plus a link.
pub const SECTION_EXTRACT_THRESHOLD: usize = 40;

/// GC metrics for a single tier-1 summary file
#[derive(Debug)]
pub struct Tier1Metrics {
    pub file_type: KnowledgeFile,
    pub line_count: usize,
    pub duplicate_headers: Vec<String>,
    pub promoted_block_count: usize,
    /// `(heading, line_count)` for sections over [`SECTION_EXTRACT_THRESHOLD`]
    pub oversized_sections: Vec<(String, usize)>,
    /// Relative topic paths this file links to that do not exist on disk
    pub broken_links: Vec<String>,
    pub has_issues: bool,
}

/// GC metrics for a single tier-2 topic file
#[derive(Debug)]
pub struct TopicMetrics {
    pub category: KnowledgeFile,
    pub slug: String,
    pub line_count: usize,
    pub duplicate_headers: Vec<String>,
    /// No tier-1 file links to this topic
    pub is_orphan: bool,
    pub has_issues: bool,
}

impl TopicMetrics {
    /// Path of this topic relative to the knowledge root
    pub fn relative_path(&self) -> String {
        format!("{}/{}.md", self.category.dir_name(), self.slug)
    }
}

/// Overall GC metrics across the knowledge directory
#[derive(Debug)]
pub struct GcMetrics {
    /// Layout the metrics were gathered from
    pub layout: KnowledgeLayout,
    /// Total lines across every knowledge file — reported, never a GC reason
    pub total_lines: usize,
    pub tier1: Vec<Tier1Metrics>,
    pub topics: Vec<TopicMetrics>,
    /// `INDEX.md` is missing, or does not list every knowledge file
    pub index_stale: bool,
    pub gc_recommended: bool,
    pub reasons: Vec<String>,
}

impl GcMetrics {
    /// Every broken tier-1 link, as `(tier-1 filename, missing topic path)`
    pub fn broken_links(&self) -> Vec<(&'static str, &str)> {
        self.tier1
            .iter()
            .flat_map(|t| {
                t.broken_links
                    .iter()
                    .map(move |link| (t.file_type.filename(), link.as_str()))
            })
            .collect()
    }
}

/// Analyze GC metrics for a knowledge directory, using the default promoted-block
/// threshold ([`DEFAULT_MAX_PROMOTED_BLOCKS`]).
pub fn analyze_gc_metrics(
    root: &Path,
    max_tier1_lines: usize,
    max_topic_lines: usize,
) -> Result<GcMetrics> {
    analyze_gc_metrics_with_promoted(
        root,
        max_tier1_lines,
        max_topic_lines,
        DEFAULT_MAX_PROMOTED_BLOCKS,
    )
}

/// Analyze GC metrics for a knowledge directory with a configurable promoted-block
/// threshold. See the module docs for what each reason means.
pub fn analyze_gc_metrics_with_promoted(
    root: &Path,
    max_tier1_lines: usize,
    max_topic_lines: usize,
    max_promoted_blocks: usize,
) -> Result<GcMetrics> {
    let layout = if root.join(INDEX_FILENAME).exists() {
        KnowledgeLayout::Hierarchical
    } else {
        KnowledgeLayout::Legacy
    };

    let topics = index::scan_topics(root)?;
    let mut reasons = Vec::new();
    let mut total_lines = 0;

    // Read every tier-1 file once; the content is reused below both for this
    // file's own metrics and for orphan-topic detection (does ANY tier-1 file
    // link to a given topic?).
    let mut tier1_raw = Vec::new();
    for file_type in KnowledgeFile::all() {
        let path = root.join(file_type.filename());
        if !path.exists() {
            continue;
        }
        let content = fs::read_to_string(&path)
            .with_context(|| format!("Failed to read {}", file_type.filename()))?;
        tier1_raw.push((*file_type, content));
    }

    let mut tier1 = Vec::new();
    for (file_type, content) in &tier1_raw {
        let line_count = content.lines().count();
        total_lines += line_count;

        let duplicate_headers = find_duplicate_headers(content);
        let promoted_block_count = content
            .lines()
            .filter(|line| line.starts_with("## Promoted from Memory"))
            .count();
        let oversized_sections = find_oversized_sections(content);
        let broken_links = find_broken_links(content, root);

        let mut has_issues = false;

        if line_count > max_tier1_lines {
            has_issues = true;
            reasons.push(format!(
                "{} has {line_count} lines (max: {max_tier1_lines})",
                file_type.filename(),
            ));
        }
        for (heading, section_lines) in &oversized_sections {
            has_issues = true;
            reasons.push(format!(
                "{} section '{heading}' has {section_lines} lines — consider extracting into a {}/<topic>.md tier-2 topic",
                file_type.filename(),
                file_type.dir_name(),
            ));
        }
        if !duplicate_headers.is_empty() {
            has_issues = true;
            reasons.push(format!(
                "{} has duplicate headers: {}",
                file_type.filename(),
                duplicate_headers.join(", ")
            ));
        }
        if promoted_block_count > max_promoted_blocks {
            has_issues = true;
            reasons.push(format!(
                "{} has {promoted_block_count} promoted blocks (consider consolidating)",
                file_type.filename(),
            ));
        }
        for link in &broken_links {
            has_issues = true;
            reasons.push(format!(
                "{} links to missing topic {link}",
                file_type.filename(),
            ));
        }

        tier1.push(Tier1Metrics {
            file_type: *file_type,
            line_count,
            duplicate_headers,
            promoted_block_count,
            oversized_sections,
            broken_links,
            has_issues,
        });
    }

    let mut topic_metrics = Vec::new();
    for topic in &topics {
        let content = fs::read_to_string(&topic.path)
            .with_context(|| format!("Failed to read topic file: {}", topic.path.display()))?;
        let line_count = content.lines().count();
        total_lines += line_count;

        let duplicate_headers = find_duplicate_headers(&content);
        let rel_path = topic.relative_path().display().to_string();
        let is_orphan = !tier1_raw.iter().any(|(_, c)| c.contains(&rel_path));

        let mut has_issues = false;
        if line_count > max_topic_lines {
            has_issues = true;
            reasons.push(format!(
                "{rel_path} has {line_count} lines (max: {max_topic_lines})"
            ));
        }
        if !duplicate_headers.is_empty() {
            has_issues = true;
            reasons.push(format!(
                "{rel_path} has duplicate headers: {}",
                duplicate_headers.join(", ")
            ));
        }
        if is_orphan {
            has_issues = true;
            reasons.push(format!(
                "{rel_path} is an orphan topic — no tier-1 file links to it"
            ));
        }

        topic_metrics.push(TopicMetrics {
            category: topic.category,
            slug: topic.slug.clone(),
            line_count,
            duplicate_headers,
            is_orphan,
            has_issues,
        });
    }

    let index_stale = if layout == KnowledgeLayout::Hierarchical {
        let index_content = fs::read_to_string(root.join(INDEX_FILENAME)).unwrap_or_default();
        let missing_tier1 = tier1_raw
            .iter()
            .any(|(ft, _)| !index_content.contains(ft.filename()));
        let missing_topic = topics.iter().any(|t| {
            let rel = t.relative_path().display().to_string();
            !index_content.contains(&rel)
        });
        missing_tier1 || missing_topic
    } else {
        false
    };
    if index_stale {
        reasons.push("INDEX.md is stale — run `loom knowledge index` to regenerate it".to_string());
    }

    // Legacy dirs get exactly one advisory reason nudging toward the tiered
    // hierarchy. Deliberately unconditional (added even when nothing else is
    // wrong) and never an aggregate-lines reason — see module docs.
    if layout == KnowledgeLayout::Legacy {
        reasons.push(
            "flat layout, run loom knowledge gc to migrate to the tiered hierarchy".to_string(),
        );
    }

    let gc_recommended = !reasons.is_empty();

    Ok(GcMetrics {
        layout,
        total_lines,
        tier1,
        topics: topic_metrics,
        index_stale,
        gc_recommended,
        reasons,
    })
}

/// Repeated `## ` heading lines within a single file's content.
fn find_duplicate_headers(content: &str) -> Vec<String> {
    let mut header_counts: HashMap<String, usize> = HashMap::new();
    for line in content.lines() {
        if line.starts_with("## ") {
            *header_counts.entry(line.to_string()).or_insert(0) += 1;
        }
    }
    header_counts
        .into_iter()
        .filter(|(_, count)| *count > 1)
        .map(|(header, _)| header)
        .collect()
}

/// `(heading, body_line_count)` for every `## ` section whose body (the lines
/// up to but excluding the next `## ` heading, or EOF) exceeds
/// [`SECTION_EXTRACT_THRESHOLD`] lines.
fn find_oversized_sections(content: &str) -> Vec<(String, usize)> {
    let lines: Vec<&str> = content.lines().collect();
    let mut sections = Vec::new();
    let mut i = 0;
    while i < lines.len() {
        if let Some(heading) = lines[i].strip_prefix("## ") {
            let start = i + 1;
            let end = lines[start..]
                .iter()
                .position(|line| line.starts_with("## "))
                .map(|offset| start + offset)
                .unwrap_or(lines.len());
            let body_len = end - start;
            if body_len > SECTION_EXTRACT_THRESHOLD {
                sections.push((heading.trim_end().to_string(), body_len));
            }
            i = end;
        } else {
            i += 1;
        }
    }
    sections
}

/// Markdown links of the form `](<category>/<slug>.md)` whose target does not
/// exist under `root`.
fn find_broken_links(content: &str, root: &Path) -> Vec<String> {
    static LINK_RE: LazyLock<Regex> = LazyLock::new(|| {
        Regex::new(r"\]\(([A-Za-z0-9_.-]+/[A-Za-z0-9_.-]+\.md)\)")
            .expect("static broken-link regex is valid")
    });
    let mut links = Vec::new();
    for cap in LINK_RE.captures_iter(content) {
        let target = cap[1].to_string();
        // Only links into a real category directory are topic pointers. A link
        // to any other two-segment path (`](loom/README.md)`, `](docs/x.md)`)
        // is an ordinary repo link, not a broken topic.
        let is_topic_link = target
            .split_once('/')
            .is_some_and(|(dir, _)| KnowledgeFile::all().iter().any(|f| f.dir_name() == dir));
        if is_topic_link && !root.join(&target).exists() && !links.contains(&target) {
            links.push(target);
        }
    }
    links
}

#[cfg(test)]
#[path = "tests_gc.rs"]
mod tests;
