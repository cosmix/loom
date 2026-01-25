//! Wiring verification - connections between components

use anyhow::Result;
use regex::Regex;
use std::fs;
use std::path::Path;

use crate::plan::schema::WiringCheck;
use super::result::{GapType, VerificationGap};

/// Verify all wiring checks find their patterns
pub fn verify_wiring(wiring: &[WiringCheck], working_dir: &Path) -> Result<Vec<VerificationGap>> {
    let mut gaps = Vec::new();

    for check in wiring {
        let source_path = working_dir.join(&check.source);

        // Check if source file exists
        if !source_path.exists() {
            gaps.push(VerificationGap::new(
                GapType::WiringBroken,
                format!("Wiring source file missing: {} ({})", check.source, check.description),
                format!("Create file: {}", check.source),
            ));
            continue;
        }

        // Read file content
        let content = match fs::read_to_string(&source_path) {
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

        // Check pattern
        let regex = match Regex::new(&check.pattern) {
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
                format!("Wiring not found: {} (pattern '{}' in {})", check.description, check.pattern, check.source),
                format!("Add code matching '{}' to {}", check.pattern, check.source),
            ));
        }
    }

    Ok(gaps)
}
