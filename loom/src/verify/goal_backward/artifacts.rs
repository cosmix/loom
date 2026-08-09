//! Artifact verification - files that must exist with real implementation

use anyhow::{Context, Result};
use glob::glob;
use std::path::Path;

use super::result::{GapType, VerificationGap};

const MAX_VERIFICATION_FILE_BYTES: usize = 10 * 1024 * 1024;

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

            let relative = path.strip_prefix(working_dir).with_context(|| {
                format!(
                    "Artifact match escaped working directory: {}",
                    path.display()
                )
            })?;
            let bytes = crate::fs::safe_read::read_bounded(
                working_dir,
                relative,
                MAX_VERIFICATION_FILE_BYTES,
            )?;
            let content = String::from_utf8_lossy(&bytes);

            if content.trim().is_empty() {
                gaps.push(VerificationGap::new(
                    GapType::ArtifactEmpty,
                    format!("Artifact is empty: {}", path.display()),
                    "Add implementation to the file".to_string(),
                ));
                continue;
            }

            for stub in STUB_PATTERNS {
                if !is_markdown && content.contains(stub) {
                    gaps.push(VerificationGap::new(
                        GapType::ArtifactStub,
                        format!("Artifact contains stub '{}': {}", stub, path.display()),
                        format!("Replace '{stub}' with actual implementation"),
                    ));
                    break;
                }
            }
        }
    }

    Ok(gaps)
}

/// Verify a regression test file exists and contains required patterns.
///
/// This is used by bug-fix stages to ensure regression tests are in place.
///
/// # Arguments
/// * `regression_test` - The regression test requirement
/// * `working_dir` - Base directory to resolve paths against
///
/// # Returns
/// A Vec of VerificationGap for any missing file or missing patterns
pub fn verify_regression_test(
    regression_test: &crate::plan::schema::RegressionTest,
    working_dir: &Path,
) -> Result<Vec<VerificationGap>> {
    let mut gaps = Vec::new();
    let file_path = working_dir.join(&regression_test.file);

    if !file_path.exists() {
        gaps.push(VerificationGap::new(
            GapType::ArtifactMissing,
            format!("Regression test file missing: {}", regression_test.file),
            format!("Create regression test file: {}", regression_test.file),
        ));
        return Ok(gaps);
    }

    let content = crate::fs::safe_read::read_to_string_bounded(
        working_dir,
        Path::new(&regression_test.file),
        MAX_VERIFICATION_FILE_BYTES,
    )
    .with_context(|| {
        format!(
            "Failed to read regression test file: {}",
            file_path.display()
        )
    })?;

    if content.trim().is_empty() {
        gaps.push(VerificationGap::new(
            GapType::ArtifactEmpty,
            format!("Regression test file is empty: {}", regression_test.file),
            "Add regression test implementation".to_string(),
        ));
        return Ok(gaps);
    }

    for pattern in &regression_test.must_contain {
        if !content.contains(pattern.as_str()) {
            gaps.push(VerificationGap::new(
                GapType::ArtifactStub,
                format!(
                    "Regression test file '{}' missing required pattern: {}",
                    regression_test.file, pattern
                ),
                format!("Add test code containing '{}'", pattern),
            ));
        }
    }

    Ok(gaps)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::plan::schema::RegressionTest;
    use tempfile::TempDir;

    #[test]
    fn test_regression_test_file_missing() {
        let dir = TempDir::new().unwrap();
        let rt = RegressionTest {
            file: "tests/regression_test.rs".to_string(),
            must_contain: vec!["test_bug_fix".to_string()],
        };
        let gaps = verify_regression_test(&rt, dir.path()).unwrap();
        assert_eq!(gaps.len(), 1);
        assert!(matches!(gaps[0].gap_type, GapType::ArtifactMissing));
    }

    #[test]
    fn test_regression_test_file_empty() {
        let dir = TempDir::new().unwrap();
        let test_file = dir.path().join("test.rs");
        std::fs::write(&test_file, "   \n  ").unwrap();
        let rt = RegressionTest {
            file: "test.rs".to_string(),
            must_contain: vec!["test_bug_fix".to_string()],
        };
        let gaps = verify_regression_test(&rt, dir.path()).unwrap();
        assert_eq!(gaps.len(), 1);
        assert!(matches!(gaps[0].gap_type, GapType::ArtifactEmpty));
    }

    #[test]
    fn test_regression_test_missing_pattern() {
        let dir = TempDir::new().unwrap();
        let test_file = dir.path().join("test.rs");
        std::fs::write(&test_file, "fn test_something() { assert!(true); }").unwrap();
        let rt = RegressionTest {
            file: "test.rs".to_string(),
            must_contain: vec!["test_bug_fix".to_string(), "regression".to_string()],
        };
        let gaps = verify_regression_test(&rt, dir.path()).unwrap();
        assert_eq!(gaps.len(), 2);
        assert!(gaps
            .iter()
            .all(|g| matches!(g.gap_type, GapType::ArtifactStub)));
    }

    #[test]
    fn test_regression_test_all_patterns_present() {
        let dir = TempDir::new().unwrap();
        let test_file = dir.path().join("test.rs");
        std::fs::write(
            &test_file,
            "#[test]\nfn test_bug_fix() {\n    // regression test\n    assert!(true);\n}",
        )
        .unwrap();
        let rt = RegressionTest {
            file: "test.rs".to_string(),
            must_contain: vec!["test_bug_fix".to_string(), "regression".to_string()],
        };
        let gaps = verify_regression_test(&rt, dir.path()).unwrap();
        assert!(gaps.is_empty());
    }

    #[test]
    fn test_regression_test_empty_must_contain() {
        let dir = TempDir::new().unwrap();
        let test_file = dir.path().join("test.rs");
        std::fs::write(&test_file, "fn test_something() {}").unwrap();
        let rt = RegressionTest {
            file: "test.rs".to_string(),
            must_contain: vec![],
        };
        let gaps = verify_regression_test(&rt, dir.path()).unwrap();
        assert!(gaps.is_empty()); // No patterns to check = pass
    }

    #[test]
    fn artifact_verification_rejects_outbound_symlink() {
        let root = TempDir::new().unwrap();
        let outside = tempfile::NamedTempFile::new().unwrap();
        std::fs::write(outside.path(), "real implementation").unwrap();
        std::os::unix::fs::symlink(outside.path(), root.path().join("artifact.rs")).unwrap();

        let result = verify_artifacts(&["artifact.rs".to_string()], root.path());

        assert!(result.is_err());
    }

    #[test]
    fn regression_verification_rejects_outbound_symlink() {
        let root = TempDir::new().unwrap();
        let outside = tempfile::NamedTempFile::new().unwrap();
        std::fs::write(outside.path(), "regression").unwrap();
        std::os::unix::fs::symlink(outside.path(), root.path().join("test.rs")).unwrap();
        let requirement = RegressionTest {
            file: "test.rs".to_string(),
            must_contain: vec!["regression".to_string()],
        };

        assert!(verify_regression_test(&requirement, root.path()).is_err());
    }
}
