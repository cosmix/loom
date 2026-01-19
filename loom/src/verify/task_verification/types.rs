//! Type definitions and constants for task verification

use std::time::Duration;

/// Default timeout for verification commands
pub const DEFAULT_VERIFICATION_TIMEOUT: Duration = Duration::from_secs(30);

/// Timeout for collecting output from child process pipes
pub(super) const OUTPUT_COLLECTION_TIMEOUT: Duration = Duration::from_secs(10);

/// Maximum number of error output lines to show in verification failures
pub(super) const MAX_ERROR_OUTPUT_LINES: usize = 5;
