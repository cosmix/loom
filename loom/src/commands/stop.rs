//! Stop command - gracefully shuts down the daemon

use crate::daemon::DaemonServer;
use crate::fs::work_dir::WorkDir;
use anyhow::{Context, Result};
use colored::Colorize;

/// Execute the stop command to gracefully shut down the daemon
pub fn execute() -> Result<()> {
    let work_dir = WorkDir::new(".")?;

    if !DaemonServer::is_running(work_dir.root()) {
        println!("{} Daemon is not running", "─".dimmed());
        return Ok(());
    }

    println!("{} Stopping daemon...", "→".cyan().bold());
    DaemonServer::stop(work_dir.root()).context("Failed to stop daemon")?;

    println!("{} Daemon stopped", "✓".green().bold());
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use serial_test::serial;
    use std::fs;
    use tempfile::TempDir;

    #[test]
    #[serial]
    fn test_stop_when_daemon_not_running() {
        let temp_dir = TempDir::new().expect("Failed to create temp dir");
        let test_dir = temp_dir.path();

        // Create a .work directory structure
        let work_dir_path = test_dir.join(".work");
        fs::create_dir(&work_dir_path).expect("Failed to create .work dir");

        // Change to test directory
        let original_dir = std::env::current_dir().expect("Failed to get current dir");
        std::env::set_current_dir(test_dir).expect("Failed to change dir");

        // Execute stop command when daemon is not running
        let result = execute();

        // Restore original directory
        std::env::set_current_dir(original_dir).expect("Failed to restore dir");

        // Should succeed even when daemon is not running
        assert!(result.is_ok());
    }

    #[test]
    #[serial]
    fn test_stop_succeeds_when_work_dir_missing() {
        let temp_dir = TempDir::new().expect("Failed to create temp dir");
        let test_dir = temp_dir.path();

        // Change to test directory (no .work directory)
        let original_dir = std::env::current_dir().expect("Failed to get current dir");
        std::env::set_current_dir(test_dir).expect("Failed to change dir");

        // Execute stop command when .work dir doesn't exist
        let result = execute();

        // Restore original directory
        std::env::set_current_dir(original_dir).expect("Failed to restore dir");

        // Should succeed - daemon simply reports "not running"
        // WorkDir::new succeeds even without .work, and is_running returns false
        assert!(result.is_ok());
    }
}
