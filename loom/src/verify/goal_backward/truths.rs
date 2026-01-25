//! Truth verification - observable behaviors that must work

use anyhow::Result;
use std::path::Path;
use std::time::Duration;

use crate::verify::criteria::run_single_criterion_with_timeout;
use super::result::{GapType, VerificationGap};

/// Default timeout for truth commands (30 seconds)
const TRUTH_TIMEOUT: Duration = Duration::from_secs(30);

/// Verify all truth commands return exit code 0
pub fn verify_truths(truths: &[String], working_dir: &Path) -> Result<Vec<VerificationGap>> {
    let mut gaps = Vec::new();

    for truth in truths {
        let result = run_single_criterion_with_timeout(truth, Some(working_dir), TRUTH_TIMEOUT)?;

        if !result.success {
            let description = if result.timed_out {
                format!("Truth timed out: {truth}")
            } else {
                format!(
                    "Truth failed (exit {}): {}",
                    result.exit_code.map(|c| c.to_string()).unwrap_or_else(|| "?".to_string()),
                    truth
                )
            };

            let suggestion = if !result.stderr.is_empty() {
                format!("Check error output: {}", result.stderr.lines().next().unwrap_or(""))
            } else {
                "Verify the command works manually".to_string()
            };

            gaps.push(VerificationGap::new(GapType::TruthFailed, description, suggestion));
        }
    }

    Ok(gaps)
}
