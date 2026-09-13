//! Result types for acceptance criteria execution

use std::time::Duration;

/// Result of executing a single acceptance criterion (shell command)
#[derive(Debug, Clone)]
pub struct CriterionResult {
    pub command: String,
    pub success: bool,
    pub stdout: String,
    pub stderr: String,
    pub exit_code: Option<i32>,
    pub duration: Duration,
    /// Whether the command was terminated due to timeout
    pub timed_out: bool,
    /// Whether this result came from the acceptance pass cache rather than a
    /// real execution — see `super::cache`. Always `false` for results
    /// built via [`Self::new`]; the internal cache-hit constructor sets it.
    pub cached: bool,
}

impl CriterionResult {
    /// Create a new criterion result
    pub fn new(
        command: String,
        success: bool,
        stdout: String,
        stderr: String,
        exit_code: Option<i32>,
        duration: Duration,
        timed_out: bool,
    ) -> Self {
        Self {
            command,
            success,
            stdout,
            stderr,
            exit_code,
            duration,
            timed_out,
            cached: false,
        }
    }

    pub(super) fn cached_verdict(
        command: String,
        exit_code: Option<i32>,
        stdout: String,
        stderr: String,
        duration: Duration,
    ) -> Self {
        Self {
            command,
            success: true,
            stdout,
            stderr,
            exit_code,
            duration,
            timed_out: false,
            cached: true,
        }
    }

    /// Check if the criterion passed
    pub fn passed(&self) -> bool {
        self.success
    }

    /// Get a summary of the result
    pub fn summary(&self) -> String {
        let status = if self.timed_out {
            "TIMEOUT"
        } else if self.success {
            "PASSED"
        } else {
            "FAILED"
        };
        let duration_ms = self.duration.as_millis();
        format!(
            "{} - {} ({}ms, exit code: {:?})",
            status, self.command, duration_ms, self.exit_code
        )
    }
}

/// Result of running all acceptance criteria for a stage
#[derive(Debug)]
pub enum AcceptanceResult {
    /// All acceptance criteria passed
    AllPassed { results: Vec<CriterionResult> },
    /// One or more acceptance criteria failed
    Failed {
        results: Vec<CriterionResult>,
        failures: Vec<String>,
    },
}

impl AcceptanceResult {
    /// Check if all criteria passed
    pub fn all_passed(&self) -> bool {
        matches!(self, AcceptanceResult::AllPassed { .. })
    }

    /// Get all criterion results
    pub fn results(&self) -> &[CriterionResult] {
        match self {
            AcceptanceResult::AllPassed { results } => results,
            AcceptanceResult::Failed { results, .. } => results,
        }
    }

    /// Get failure messages if any
    pub fn failures(&self) -> Vec<String> {
        match self {
            AcceptanceResult::AllPassed { .. } => Vec::new(),
            AcceptanceResult::Failed { failures, .. } => failures.clone(),
        }
    }

    /// Current wall-clock work across criteria. Cache hits contribute lookup
    /// time, while real runs contribute lookup plus execution time.
    pub fn total_duration(&self) -> Duration {
        self.results().iter().map(|r| r.duration).sum()
    }

    /// Get count of passed criteria
    pub fn passed_count(&self) -> usize {
        self.results().iter().filter(|r| r.passed()).count()
    }

    /// Get count of failed criteria
    pub fn failed_count(&self) -> usize {
        self.results().iter().filter(|r| !r.passed()).count()
    }
}
