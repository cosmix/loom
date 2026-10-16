//! Advisory wiring evidence from the shared source graph.
use super::duplicate_detection::graph;
use crate::context::extract::dialect::dialect_for_path;
use crate::context::graph_store::ResolvedGraph;
use crate::context::source_graph::{EdgeProvenance, FileCoverage, SourceEdgeKind};
use anyhow::Result;
use std::collections::BTreeSet;
use std::path::Path;

#[derive(Debug)]
pub struct UnwiredFile {
    pub path: String,
    pub importable_name: String,
}

#[derive(Debug)]
pub struct WiringDetectionResult {
    pub unwired_files: Vec<UnwiredFile>,
    pub total_new_files: usize,
    /// Missing parser support or unresolved evidence cannot establish absence.
    pub uncertain_files: Vec<String>,
}

const EXCLUDED_STEMS: &[&str] = &["mod", "lib", "main", "index"];

pub fn detect_unwired_files(directory: &Path, base_branch: &str) -> Result<WiringDetectionResult> {
    let root = graph::repository_root(directory)?;
    let added = graph::changed_files(&root, base_branch, true)?;
    let built = graph::graph(&root)?;
    let result = inspect_wiring(&built.graph, &added);
    for file in &result.uncertain_files {
        eprintln!("Wiring analysis uncertain for {file}: incomplete or unresolved graph evidence");
    }
    Ok(result)
}

fn inspect_wiring(graph: &ResolvedGraph, added: &BTreeSet<String>) -> WiringDetectionResult {
    let mut result = WiringDetectionResult {
        unwired_files: Vec::new(),
        total_new_files: 0,
        uncertain_files: Vec::new(),
    };
    for path in added {
        let name = file_stem(path);
        if EXCLUDED_STEMS.contains(&name.as_str()) || is_test_file(path) {
            continue;
        }
        if dialect_for_path(Path::new(path)).is_none() {
            continue;
        }
        let Some(entry) = graph.files.get(path) else {
            result.total_new_files += 1;
            result.uncertain_files.push(path.clone());
            continue;
        };
        result.total_new_files += 1;
        if is_referenced(graph, path) {
            continue;
        }
        let incomplete = !matches!(entry.coverage, FileCoverage::Full)
            || graph.files.iter().any(|(path, file)| {
                dialect_for_path(Path::new(path)).is_some()
                    && !matches!(file.coverage, FileCoverage::Full)
            })
            || unresolved_reference(graph, path, &name);
        if incomplete {
            result.uncertain_files.push(path.clone());
        } else {
            result.unwired_files.push(UnwiredFile {
                path: path.clone(),
                importable_name: name,
            });
        }
    }
    result
}

/// Only resolved, import/receiver/compiler evidence from another file counts.
/// Unique spelling matches do not establish actual integration.
fn is_referenced(graph: &ResolvedGraph, path: &str) -> bool {
    graph
        .files
        .iter()
        .filter(|(source, _)| source.as_str() != path)
        .any(|(_, file)| {
            file.edges.iter().any(|edge| {
                edge.kind != SourceEdgeKind::Contains
                    && matches!(
                        edge.provenance,
                        EdgeProvenance::Import
                            | EdgeProvenance::Receiver
                            | EdgeProvenance::Compiler
                    )
                    && edge.to.split('#').next() == Some(path)
            })
        })
}

fn unresolved_reference(graph: &ResolvedGraph, path: &str, name: &str) -> bool {
    graph
        .files
        .iter()
        .filter(|(source, _)| source.as_str() != path)
        .any(|(_, file)| {
            file.edges.iter().any(|edge| {
                matches!(
                    edge.provenance,
                    EdgeProvenance::Syntax | EdgeProvenance::UniqueName
                ) && (edge.kind == SourceEdgeKind::Imports
                    || edge.symbol.split([':', '.', '/']).any(|part| part == name)
                    || edge
                        .candidates
                        .iter()
                        .any(|id| id.split('#').next() == Some(path)))
            })
        })
}

fn is_test_file(path: &str) -> bool {
    let lower = path.to_lowercase();
    lower
        .split('/')
        .any(|part| matches!(part, "test" | "tests"))
        || lower.contains("_test.")
        || lower.ends_with("_test")
        || lower.contains(".test.")
        || lower.contains(".spec.")
}

fn file_stem(path: &str) -> String {
    Path::new(path)
        .file_stem()
        .unwrap_or_default()
        .to_string_lossy()
        .into_owned()
}

#[cfg(test)]
#[path = "tests_wiring_detection.rs"]
mod tests;
