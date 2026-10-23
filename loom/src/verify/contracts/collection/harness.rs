//! Harness globs that would freeze production source. A harness is for
//! test-only files (fixtures, helpers); a module declaration a contract
//! needs is derived as scaffold instead, so the implementer can still write
//! the module.

use std::path::Path;

use crate::context::extract::dialect::dialect_for_path;
use crate::context::rank_source::is_test_path;
use crate::verify::contracts::harness_matches;

/// Existing files matched by a harness glob that are production source:
/// a file in a language loom parses that follows no test-file convention.
/// Globs and files are relative to the same directory.
pub fn harness_source_matches(harness: &[String], files: &[String]) -> Vec<String> {
    files
        .iter()
        .filter(|file| is_production_source(file) && harness_matches(file, harness))
        .cloned()
        .collect()
}

fn is_production_source(path: &str) -> bool {
    dialect_for_path(Path::new(path)).is_some() && !is_test_path(path)
}
