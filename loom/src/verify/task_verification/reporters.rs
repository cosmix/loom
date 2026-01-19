//! Verification result reporting and summarization

use crate::checkpoints::CheckpointVerificationResult;

/// Get a summary of verification results
pub fn summarize_verifications(
    results: &[CheckpointVerificationResult],
) -> (usize, usize, Vec<String>) {
    let passed = results.iter().filter(|r| r.passed).count();
    let failed = results.iter().filter(|r| !r.passed).count();
    let warnings: Vec<String> = results
        .iter()
        .filter(|r| !r.passed)
        .map(|r| r.message.clone())
        .collect();

    (passed, failed, warnings)
}
