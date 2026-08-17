//! `loom knowledge sync` — rebuild derived context artifacts.
//!
//! Writes only through the context store, never into the knowledge tree
//! itself: `refresh` rebuilds the catalog when stale and persists it to the
//! cache, and is a no-op when the catalog is already current.

use super::context::resolve;
use crate::context::refresh::{
    refresh, RefreshOutcome, SemanticLayer, SemanticOutcome, SOURCE_GRAPH_PREFIX,
};
use anyhow::Result;
use colored::Colorize;

/// Rebuild derived context artifacts when the knowledge tree has changed.
pub fn sync(structural_only: bool, json: bool) -> Result<()> {
    let (knowledge_root, store) = resolve()?;
    let outcome = refresh(&store, &knowledge_root, structural_only)?;

    // Stdout carries the machine-readable result in --json mode, so a refused
    // base publish goes to stderr in BOTH modes: a scripted caller that reads
    // only stdout still learns the tree was dirty and it got an overlay.
    if let SemanticLayer::LocalOverlay { refusal, .. } = &outcome.semantic.layer {
        eprintln!("{SOURCE_GRAPH_PREFIX}base not published - {refusal}");
    }

    if json {
        print_json(&outcome)
    } else {
        print_human(&outcome);
        Ok(())
    }
}

fn print_json(outcome: &RefreshOutcome) -> Result<()> {
    let payload = serde_json::json!({
        "rebuilt": outcome.rebuilt,
        "revision": outcome.structural.revision,
        "files": outcome.report.as_ref().map(|report| report.files),
        "chunks": outcome.report.as_ref().map(|report| report.chunks),
        "issues": outcome.report.as_ref().map(|report| report.issues.len()),
        "semantic": semantic_json(&outcome.semantic),
    });
    println!("{}", serde_json::to_string_pretty(&payload)?);
    Ok(())
}

/// The source-graph half of the `--json` payload. `layer` is the
/// machine-readable discriminator; a caller must never have to substring-match
/// `detail` prose to learn which layer it got.
fn semantic_json(semantic: &SemanticOutcome) -> serde_json::Value {
    let (layer, revision, plan, stage, detail) = match &semantic.layer {
        SemanticLayer::Base { revision } => (
            "base",
            Some(revision.as_str()),
            None,
            None,
            semantic.freshness.detail.as_deref(),
        ),
        SemanticLayer::LocalOverlay {
            plan,
            stage,
            refusal,
        } => (
            "local-overlay",
            Some(semantic.freshness.revision.as_str()),
            Some(plan.as_str()),
            Some(stage.as_str()),
            Some(refusal.as_str()),
        ),
        SemanticLayer::Skipped { reason } => ("skipped", None, None, None, Some(reason.as_str())),
    };
    serde_json::json!({
        "layer": layer,
        "revision": revision,
        "plan": plan,
        "stage": stage,
        "files": semantic.files_extracted,
        "nodes": semantic.nodes,
        "edges": semantic.edges,
        "stale": semantic.freshness.stale,
        "detail": detail,
    })
}

fn print_human(outcome: &RefreshOutcome) {
    if outcome.rebuilt {
        println!("{} Rebuilt the context catalog", "✓".green().bold());
    } else {
        println!("{} Catalog already current", "─".dimmed());
    }
    println!("  {} {}", "Revision:".cyan(), outcome.structural.revision);

    if let Some(report) = &outcome.report {
        println!("  {} {}", "Files:".cyan(), report.files);
        println!("  {} {}", "Chunks:".cyan(), report.chunks);
        println!("  {} {}", "Issues:".cyan(), report.issues.len());
    }
    print_semantic(&outcome.semantic);
}

/// One honest line about the source-graph (semantic) layer. `loom knowledge
/// sync` drives BOTH layers, and printing only the catalog is what made the
/// command look like it did nothing.
fn print_semantic(semantic: &SemanticOutcome) {
    match &semantic.layer {
        SemanticLayer::Base { revision } => println!(
            "{SOURCE_GRAPH_PREFIX}published base for {} ({} files, {} nodes)",
            short_revision(revision),
            semantic.files_extracted,
            semantic.nodes
        ),
        SemanticLayer::LocalOverlay { plan, stage, .. } => println!(
            "{SOURCE_GRAPH_PREFIX}working-tree overlay {plan}/{stage} ({} files, {} nodes)",
            semantic.files_extracted, semantic.nodes
        ),
        SemanticLayer::Skipped { reason } => {
            println!("{SOURCE_GRAPH_PREFIX}skipped ({reason})")
        }
    }
}

/// First 8 characters of a revision, for display only.
fn short_revision(revision: &str) -> &str {
    revision.get(..8).unwrap_or(revision)
}
