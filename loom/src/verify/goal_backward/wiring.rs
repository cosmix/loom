//! Wiring verification - connections between components

use anyhow::Result;
use regex::{Regex, RegexBuilder};
use std::path::Path;

use super::artifacts::MAX_VERIFICATION_FILE_BYTES;
use super::result::{GapType, VerificationGap};
use crate::fs::safe_read::read_to_string_bounded;
use crate::plan::schema::WiringCheck;

/// Compiled-size limit every wiring pattern is built with (1 MiB), which
/// bounds ReDoS. `plan verify` reports a pattern this limit rejects.
pub(crate) const PATTERN_SIZE_LIMIT: usize = 1 << 20;

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
/// * `plan_version` - The plan's `loom.version`; 2 applies the v2 rules
///   (glob `source`, `literal` pattern) in `wiring_v2`
///
/// # Returns
/// A Vec of VerificationGap for any broken wiring connections
pub fn verify_wiring(
    wiring: &[WiringCheck],
    working_dir: &Path,
    plan_version: u32,
) -> Result<Vec<VerificationGap>> {
    if plan_version == 2 {
        return Ok(super::wiring_v2::verify_checks(wiring, working_dir));
    }
    Ok(wiring
        .iter()
        .filter_map(|check| verify_check_v1(check, working_dir).err())
        .collect())
}

/// One v1 check: `source` is a literal path and `pattern` a regex.
fn verify_check_v1(check: &WiringCheck, working_dir: &Path) -> Result<(), VerificationGap> {
    if !working_dir.join(&check.source).exists() {
        return Err(missing_source_gap(check));
    }
    let content = read_source(working_dir, Path::new(&check.source))?;
    let regex = compile_pattern(check, &check.pattern)?;
    if regex.is_match(&content) {
        Ok(())
    } else {
        Err(not_found_gap(check))
    }
}

/// Read a wiring source beneath `root`, bounded in size and refusing a
/// symlink at any path component.
pub(super) fn read_source(root: &Path, relative: &Path) -> Result<String, VerificationGap> {
    read_to_string_bounded(root, relative, MAX_VERIFICATION_FILE_BYTES).map_err(|e| {
        VerificationGap::new(
            GapType::WiringBroken,
            format!("Cannot read wiring source: {} - {e}", relative.display()),
            "Fix file permissions or encoding".to_string(),
        )
    })
}

/// Compile `regex_source` (the check's pattern, escaped when literal) with
/// a size limit that prevents ReDoS.
pub(super) fn compile_pattern(
    check: &WiringCheck,
    regex_source: &str,
) -> Result<Regex, VerificationGap> {
    RegexBuilder::new(regex_source)
        .size_limit(PATTERN_SIZE_LIMIT)
        .build()
        .map_err(|e| {
            VerificationGap::new(
                GapType::WiringBroken,
                format!("Invalid wiring pattern '{}': {}", check.pattern, e),
                "Fix the regex pattern".to_string(),
            )
        })
}

/// Gap for a literal `source` path that does not exist.
pub(super) fn missing_source_gap(check: &WiringCheck) -> VerificationGap {
    VerificationGap::new(
        GapType::WiringBroken,
        format!(
            "Wiring source file missing: {} ({})",
            check.source, check.description
        ),
        format!("Create file: {}", check.source),
    )
}

/// Gap for a pattern found in no source file.
pub(super) fn not_found_gap(check: &WiringCheck) -> VerificationGap {
    VerificationGap::new(
        GapType::WiringBroken,
        format!(
            "Wiring not found: {} (pattern '{}' in {})",
            check.description, check.pattern, check.source
        ),
        format!("Add code matching '{}' to {}", check.pattern, check.source),
    )
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
            literal: false,
        };

        let gaps = verify_wiring(&[check], root.path(), 1).unwrap();

        assert_eq!(gaps.len(), 1);
        assert!(matches!(gaps[0].gap_type, GapType::WiringBroken));
    }
}
