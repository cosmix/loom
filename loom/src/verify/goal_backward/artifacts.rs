//! Artifact verification - files that must exist with real implementation

use anyhow::Result;
use glob::glob;
use std::fs;
use std::path::Path;

use super::result::{GapType, VerificationGap};

/// Patterns that indicate a file is a stub
const STUB_PATTERNS: &[&str] = &[
    "TODO",
    "FIXME",
    "unimplemented!",
    "todo!",
    "panic!(\"not implemented",
    "pass  # TODO",
    "raise NotImplementedError",
    "throw new Error(\"Not implemented",
];

/// Verify all artifact patterns match existing, non-stub files.
///
/// Checks that files matching artifact glob patterns:
/// 1. Exist (at least one file matches each pattern)
/// 2. Are not empty
/// 3. Do not contain stub patterns (TODO, FIXME, unimplemented!, etc.)
///
/// # Arguments
/// * `artifacts` - Glob patterns for required artifact files
/// * `working_dir` - Base directory to resolve patterns against
///
/// # Returns
/// A Vec of VerificationGap for any missing, empty, or stub artifacts
pub fn verify_artifacts(artifacts: &[String], working_dir: &Path) -> Result<Vec<VerificationGap>> {
    let mut gaps = Vec::new();

    for pattern in artifacts {
        let full_pattern = working_dir.join(pattern);
        let pattern_str = full_pattern.to_string_lossy();

        let matches: Vec<_> = glob(&pattern_str)
            .map_err(|e| anyhow::anyhow!("Invalid glob pattern '{pattern}': {e}"))?
            .filter_map(|r| r.ok())
            .collect();

        if matches.is_empty() {
            gaps.push(VerificationGap::new(
                GapType::ArtifactMissing,
                format!("No files match artifact pattern: {pattern}"),
                format!("Create file(s) matching: {pattern}"),
            ));
            continue;
        }

        // Check each matched file for stubs
        for path in matches {
            // Skip stub detection for markdown files - they naturally reference
            // TODO/FIXME in rule text and templates
            let is_markdown = path.extension().is_some_and(|ext| {
                let ext_lower = ext.to_ascii_lowercase();
                ext_lower == "md" || ext_lower == "mdx" || ext_lower == "markdown"
            });

            if let Ok(content) = fs::read_to_string(&path) {
                // Check for empty files
                if content.trim().is_empty() {
                    gaps.push(VerificationGap::new(
                        GapType::ArtifactEmpty,
                        format!("Artifact is empty: {}", path.display()),
                        "Add implementation to the file".to_string(),
                    ));
                    continue;
                }

                // Check for stub patterns (skip for markdown files)
                for stub in STUB_PATTERNS {
                    if !is_markdown && content.contains(stub) {
                        gaps.push(VerificationGap::new(
                            GapType::ArtifactStub,
                            format!("Artifact contains stub '{}': {}", stub, path.display()),
                            format!("Replace '{stub}' with actual implementation"),
                        ));
                        break; // One gap per file
                    }
                }
            }
        }
    }

    Ok(gaps)
}
