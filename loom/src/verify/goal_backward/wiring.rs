//! Wiring verification - connections between components

use anyhow::Result;
use regex::RegexBuilder;
use std::path::Path;

use super::result::{GapType, VerificationGap};
use crate::plan::schema::WiringCheck;

/// Verify all wiring checks find their patterns in source files.
///
/// For each wiring check, verifies that:
/// 1. The source file exists
/// 2. The file is readable
/// 3. The regex pattern matches somewhere in the file content
///
/// # Arguments
/// * `wiring` - Wiring check definitions with source file and pattern
/// * `working_dir` - Base directory to resolve source paths against
///
/// # Returns
/// A Vec of VerificationGap for any broken wiring connections
pub fn verify_wiring(wiring: &[WiringCheck], working_dir: &Path) -> Result<Vec<VerificationGap>> {
    let mut gaps = Vec::new();

    for check in wiring {
        let source_path = working_dir.join(&check.source);

        // Check if source file exists
        if !source_path.exists() {
            gaps.push(VerificationGap::new(
                GapType::WiringBroken,
                format!(
                    "Wiring source file missing: {} ({})",
                    check.source, check.description
                ),
                format!("Create file: {}", check.source),
            ));
            continue;
        }

        let content = match crate::fs::safe_read::read_to_string_bounded(
            working_dir,
            Path::new(&check.source),
            10 * 1024 * 1024,
        ) {
            Ok(c) => c,
            Err(e) => {
                gaps.push(VerificationGap::new(
                    GapType::WiringBroken,
                    format!("Cannot read wiring source: {} - {}", check.source, e),
                    "Fix file permissions or encoding".to_string(),
                ));
                continue;
            }
        };

        // Check pattern with size limits to prevent ReDoS
        let regex = match RegexBuilder::new(&check.pattern)
            .size_limit(1 << 20) // 1MB compiled size limit
            .build()
        {
            Ok(r) => r,
            Err(e) => {
                gaps.push(VerificationGap::new(
                    GapType::WiringBroken,
                    format!("Invalid wiring pattern '{}': {}", check.pattern, e),
                    "Fix the regex pattern".to_string(),
                ));
                continue;
            }
        };

        if !regex.is_match(&content) {
            gaps.push(VerificationGap::new(
                GapType::WiringBroken,
                format!(
                    "Wiring not found: {} (pattern '{}' in {})",
                    check.description, check.pattern, check.source
                ),
                format!("Add code matching '{}' to {}", check.pattern, check.source),
            ));
        }
    }

    Ok(gaps)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn wiring_verification_rejects_outbound_symlink() {
        let root = tempfile::tempdir().unwrap();
        let outside = tempfile::NamedTempFile::new().unwrap();
        std::fs::write(outside.path(), "fn registered() {}").unwrap();
        std::os::unix::fs::symlink(outside.path(), root.path().join("source.rs")).unwrap();
        let check = WiringCheck {
            source: "source.rs".to_string(),
            pattern: "registered".to_string(),
            description: "registration".to_string(),
        };

        let gaps = verify_wiring(&[check], root.path()).unwrap();

        assert_eq!(gaps.len(), 1);
        assert!(matches!(gaps[0].gap_type, GapType::WiringBroken));
    }
}
