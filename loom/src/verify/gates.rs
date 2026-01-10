use anyhow::{Context, Result};
use std::io::{stdin, stdout, BufRead, Write};
use std::time::Duration;

use crate::models::stage::Stage;

/// Decision from a human approval gate.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum GateDecision {
    /// Stage approved by human reviewer
    Approved,
    /// Stage rejected with a reason
    Rejected { reason: String },
    /// Gate skipped (auto-approve mode)
    Skipped,
}

/// Configuration for approval gates.
#[derive(Debug, Clone, Default)]
pub struct GateConfig {
    /// If true, automatically approve all gates without prompting
    pub auto_approve: bool,
    /// Optional timeout for interactive prompts (currently not enforced)
    pub timeout: Option<Duration>,
}

impl GateConfig {
    /// Create a new gate configuration
    pub fn new() -> Self {
        Self::default()
    }

    /// Enable auto-approval mode
    pub fn with_auto_approve(mut self, auto_approve: bool) -> Self {
        self.auto_approve = auto_approve;
        self
    }

    /// Set timeout for prompts
    pub fn with_timeout(mut self, timeout: Duration) -> Self {
        self.timeout = Some(timeout);
        self
    }
}

/// Present a human approval gate for a stage.
///
/// If auto_approve is enabled in config, returns GateDecision::Skipped.
/// Otherwise, prompts the user to approve or reject the stage.
pub fn human_gate(stage: &Stage, config: &GateConfig) -> Result<GateDecision> {
    if config.auto_approve {
        return Ok(GateDecision::Skipped);
    }

    loop {
        display_stage_summary(stage)?;

        match prompt_approval()? {
            GateDecision::Approved => return Ok(GateDecision::Approved),
            GateDecision::Rejected { .. } => {
                let reason = prompt_rejection_reason()?;
                return Ok(GateDecision::Rejected { reason });
            }
            GateDecision::Skipped => {
                // This means 'r' was selected - show results again
                continue;
            }
        }
    }
}

/// Display a summary of the stage for approval review.
fn display_stage_summary(stage: &Stage) -> Result<()> {
    let mut out = stdout();

    writeln!(out)?;
    writeln!(out, "Stage '{}' completed successfully.", stage.name)?;
    writeln!(out)?;

    if let Some(ref desc) = stage.description {
        writeln!(out, "Description: {desc}")?;
        writeln!(out)?;
    }

    if !stage.acceptance.is_empty() {
        writeln!(out, "Acceptance criteria passed:")?;
        for criterion in &stage.acceptance {
            writeln!(out, "  - [x] {criterion}")?;
        }
        writeln!(out)?;
    }

    if !stage.files.is_empty() {
        writeln!(out, "Modified file patterns:")?;
        for file_pattern in &stage.files {
            writeln!(out, "  - {file_pattern}")?;
        }
        writeln!(out)?;
    }

    out.flush()?;
    Ok(())
}

/// Prompt user for approval decision.
///
/// Returns:
/// - GateDecision::Approved for 'y'
/// - GateDecision::Rejected for 'n' (caller should prompt for reason)
/// - GateDecision::Skipped for 'r' (show results again)
fn prompt_approval() -> Result<GateDecision> {
    let mut out = stdout();
    write!(out, "Approve this stage? [y/n/r]: ")?;
    out.flush()?;

    let stdin = stdin();
    let mut handle = stdin.lock();
    let mut input = String::new();

    handle
        .read_line(&mut input)
        .context("Failed to read approval response")?;

    let response = input.trim().to_lowercase();

    match response.as_str() {
        "y" | "yes" => Ok(GateDecision::Approved),
        "n" | "no" => Ok(GateDecision::Rejected {
            reason: String::new(),
        }),
        "r" | "results" => Ok(GateDecision::Skipped),
        "" => {
            // Handle EOF gracefully
            writeln!(out)?;
            writeln!(out, "No input received. Treating as rejection.")?;
            Ok(GateDecision::Rejected {
                reason: "No input received (EOF)".to_string(),
            })
        }
        _ => {
            writeln!(
                out,
                "Invalid response. Please enter 'y' (yes), 'n' (no), or 'r' (show results again)."
            )?;
            prompt_approval()
        }
    }
}

/// Prompt user for rejection reason.
fn prompt_rejection_reason() -> Result<String> {
    let mut out = stdout();
    write!(out, "Please provide a reason for rejection: ")?;
    out.flush()?;

    let stdin = stdin();
    let mut handle = stdin.lock();
    let mut input = String::new();

    handle
        .read_line(&mut input)
        .context("Failed to read rejection reason")?;

    let reason = input.trim().to_string();

    if reason.is_empty() {
        Ok("No reason provided".to_string())
    } else {
        Ok(reason)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn create_test_stage() -> Stage {
        let mut stage = Stage::new(
            "Test Stage".to_string(),
            Some("A test stage for verification".to_string()),
        );
        stage.add_acceptance_criterion("Tests pass".to_string());
        stage.add_acceptance_criterion("Code is formatted".to_string());
        stage.add_file_pattern("src/**/*.rs".to_string());
        stage
    }

    #[test]
    fn test_auto_approve_returns_skipped() {
        let stage = create_test_stage();
        let config = GateConfig::new().with_auto_approve(true);

        let decision = human_gate(&stage, &config).expect("Gate should succeed");

        assert_eq!(decision, GateDecision::Skipped);
    }

    #[test]
    fn test_gate_config_builder() {
        let config = GateConfig::new()
            .with_auto_approve(true)
            .with_timeout(Duration::from_secs(30));

        assert!(config.auto_approve);
        assert_eq!(config.timeout, Some(Duration::from_secs(30)));
    }

    #[test]
    fn test_gate_config_default() {
        let config = GateConfig::default();

        assert!(!config.auto_approve);
        assert_eq!(config.timeout, None);
    }

    #[test]
    fn test_gate_decision_equality() {
        assert_eq!(GateDecision::Approved, GateDecision::Approved);
        assert_eq!(GateDecision::Skipped, GateDecision::Skipped);
        assert_eq!(
            GateDecision::Rejected {
                reason: "test".to_string()
            },
            GateDecision::Rejected {
                reason: "test".to_string()
            }
        );

        assert_ne!(GateDecision::Approved, GateDecision::Skipped);
        assert_ne!(
            GateDecision::Rejected {
                reason: "test1".to_string()
            },
            GateDecision::Rejected {
                reason: "test2".to_string()
            }
        );
    }

    #[test]
    fn test_display_stage_summary_no_panic() {
        let stage = create_test_stage();
        // This test ensures display_stage_summary doesn't panic
        // Actual output verification would require capturing stdout
        let result = display_stage_summary(&stage);
        assert!(result.is_ok());
    }

    #[test]
    fn test_stage_with_minimal_info() {
        let stage = Stage::new("Minimal".to_string(), None);
        let result = display_stage_summary(&stage);
        assert!(result.is_ok());
    }
}
