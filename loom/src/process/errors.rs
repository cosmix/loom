use std::time::Duration;

/// Structured wall-clock timeout returned by bounded command helpers.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProcessTimeoutError {
    operation: String,
    timeout: Duration,
}

impl ProcessTimeoutError {
    pub fn new(operation: impl Into<String>, timeout: Duration) -> Self {
        Self {
            operation: operation.into(),
            timeout,
        }
    }

    pub fn operation(&self) -> &str {
        &self.operation
    }

    pub fn timeout(&self) -> Duration {
        self.timeout
    }
}

impl std::fmt::Display for ProcessTimeoutError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{} timed out after {:?}", self.operation, self.timeout)
    }
}

impl std::error::Error for ProcessTimeoutError {}

/// A captured stream exceeded the per-stream output limit.
#[derive(Debug)]
pub struct ProcessOutputLimitError {
    pub stream: &'static str,
    pub limit: usize,
}

impl std::fmt::Display for ProcessOutputLimitError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "{} exceeded the {} byte output limit",
            self.stream, self.limit
        )
    }
}

impl std::error::Error for ProcessOutputLimitError {}

pub(super) const OUTPUT_LIMIT: usize = 16 * 1024 * 1024;
