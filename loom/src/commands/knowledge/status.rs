//! `loom knowledge status` — report context-catalog freshness and issues.
//!
//! Read-only: this command never rebuilds the catalog and never touches the
//! knowledge tree. Use `loom knowledge sync` to rebuild.

use super::context::resolve;
use crate::context::refresh::evaluate;
use crate::context::Freshness;
use crate::fs::knowledge::catalog::CatalogIssue;
use crate::fs::knowledge::{KnowledgeLayout, INDEX_FILENAME};
use anyhow::Result;
use colored::Colorize;
use std::collections::BTreeMap;
use std::path::PathBuf;

/// Every kind of [`CatalogIssue`], in the order reported.
const ISSUE_KINDS: [&str; 4] = [
    "duplicate-heading",
    "generic-blurb",
    "broken-link",
    "missing-source-ref",
];

/// Everything `status` reports, gathered once and handed to either printer.
struct StatusReport {
    knowledge_root: PathBuf,
    layout: KnowledgeLayout,
    cache_root: PathBuf,
    chunk_count: Option<usize>,
    revision: Option<String>,
    structural: Freshness,
    semantic: Freshness,
    issue_counts: BTreeMap<&'static str, usize>,
}

fn issue_kind_label(issue: &CatalogIssue) -> &'static str {
    match issue {
        CatalogIssue::DuplicateHeading { .. } => "duplicate-heading",
        CatalogIssue::GenericBlurb { .. } => "generic-blurb",
        CatalogIssue::BrokenLink { .. } => "broken-link",
        CatalogIssue::MissingSourceRef { .. } => "missing-source-ref",
    }
}

/// Count issues by kind, with every kind present (zero-filled) so callers
/// never have to guess whether an absent key means zero or "not counted".
fn count_issues(issues: &[CatalogIssue]) -> BTreeMap<&'static str, usize> {
    let mut counts: BTreeMap<&'static str, usize> =
        ISSUE_KINDS.into_iter().map(|kind| (kind, 0)).collect();
    for issue in issues {
        *counts.entry(issue_kind_label(issue)).or_insert(0) += 1;
    }
    counts
}

fn layout_label(layout: KnowledgeLayout) -> &'static str {
    match layout {
        KnowledgeLayout::Hierarchical => "hierarchical",
        KnowledgeLayout::Legacy => "legacy",
    }
}

/// Show knowledge catalog freshness, size, and reported issues.
pub fn status(json: bool) -> Result<()> {
    let (knowledge_root, store) = resolve()?;

    let layout = if knowledge_root.join(INDEX_FILENAME).exists() {
        KnowledgeLayout::Hierarchical
    } else {
        KnowledgeLayout::Legacy
    };

    let state = evaluate(&store, &knowledge_root)?;
    let catalog = store.load_catalog()?;

    let issue_counts = count_issues(
        catalog
            .as_ref()
            .map(|catalog| catalog.issues.as_slice())
            .unwrap_or_default(),
    );

    let report = StatusReport {
        knowledge_root,
        layout,
        cache_root: store.root().to_path_buf(),
        chunk_count: catalog.as_ref().map(|catalog| catalog.chunks.len()),
        revision: catalog.as_ref().map(|catalog| catalog.revision.clone()),
        structural: state.structural,
        semantic: state.semantic,
        issue_counts,
    };

    if json {
        print_json(&report)
    } else {
        print_human(&report);
        Ok(())
    }
}

fn print_json(report: &StatusReport) -> Result<()> {
    let payload = serde_json::json!({
        "knowledge_root": report.knowledge_root,
        "layout": layout_label(report.layout),
        "cache_root": report.cache_root,
        "chunks": report.chunk_count,
        "revision": report.revision,
        "structural_freshness": report.structural,
        "semantic_freshness": report.semantic,
        "issues": report.issue_counts,
    });
    println!("{}", serde_json::to_string_pretty(&payload)?);
    Ok(())
}

fn print_human(report: &StatusReport) {
    println!("{}", "Knowledge Context Status".bold());
    println!();
    println!(
        "  {} {}",
        "Knowledge root:".cyan(),
        report.knowledge_root.display()
    );
    println!("  {} {}", "Layout:".cyan(), layout_label(report.layout));
    println!("  {} {}", "Cache root:".cyan(), report.cache_root.display());
    match (&report.revision, report.chunk_count) {
        (Some(revision), Some(chunk_count)) => println!(
            "  {} {} ({} chunks)",
            "Catalog:".cyan(),
            revision,
            chunk_count
        ),
        _ => println!("  {} {}", "Catalog:".cyan(), "never built".yellow()),
    }
    println!();
    print_freshness_line("Structural freshness", &report.structural);
    print_freshness_line("Semantic freshness", &report.semantic);
    println!();

    println!("{}", "Issues:".cyan().bold());
    let total: usize = report.issue_counts.values().sum();
    if total == 0 {
        println!("  {} none", "✓".green());
    } else {
        for (kind, count) in &report.issue_counts {
            if *count > 0 {
                println!("  {} {kind}: {count}", "⚠".yellow());
            }
        }
    }
}

fn print_freshness_line(label: &str, freshness: &Freshness) {
    let marker = if freshness.stale {
        "stale".yellow()
    } else {
        "current".green()
    };
    match &freshness.detail {
        Some(detail) => println!("  {label}: {marker} ({detail})"),
        None => println!("  {label}: {marker}"),
    }
}
