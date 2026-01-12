//! PID tracking for native terminal sessions
//!
//! Provides reliable PID tracking by using PID files and process discovery
//! instead of relying on the terminal emulator's PID.

use anyhow::{Context, Result};
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::thread;
use std::time::{Duration, Instant};

/// Get the path to the pids directory
pub fn pids_dir(work_dir: &Path) -> PathBuf {
    work_dir.join("pids")
}

/// Get the path to the wrappers directory
pub fn wrappers_dir(work_dir: &Path) -> PathBuf {
    work_dir.join("wrappers")
}

/// Create the pids directory if it doesn't exist
pub fn create_pid_dir(work_dir: &Path) -> Result<()> {
    let dir = pids_dir(work_dir);
    fs::create_dir_all(&dir)
        .with_context(|| format!("Failed to create pids directory: {}", dir.display()))
}

/// Create the wrappers directory if it doesn't exist
pub fn create_wrappers_dir(work_dir: &Path) -> Result<()> {
    let dir = wrappers_dir(work_dir);
    fs::create_dir_all(&dir)
        .with_context(|| format!("Failed to create wrappers directory: {}", dir.display()))
}

/// Get the path to a PID file for a stage
pub fn pid_file_path(work_dir: &Path, stage_id: &str) -> PathBuf {
    pids_dir(work_dir).join(format!("{stage_id}.pid"))
}

/// Get the path to a wrapper script for a stage
pub fn wrapper_script_path(work_dir: &Path, stage_id: &str) -> PathBuf {
    wrappers_dir(work_dir).join(format!("{stage_id}-wrapper.sh"))
}

/// Write a PID to the PID file for a stage
#[allow(dead_code)] // Used by wrapper scripts via shell, not directly called
pub fn write_pid_file(work_dir: &Path, stage_id: &str, pid: u32) -> Result<()> {
    create_pid_dir(work_dir)?;
    let path = pid_file_path(work_dir, stage_id);
    fs::write(&path, pid.to_string())
        .with_context(|| format!("Failed to write PID file: {}", path.display()))
}

/// Read the PID from a PID file for a stage
///
/// Returns None if the file doesn't exist or is invalid
pub fn read_pid_file(work_dir: &Path, stage_id: &str) -> Option<u32> {
    let path = pid_file_path(work_dir, stage_id);
    fs::read_to_string(&path)
        .ok()
        .and_then(|s| s.trim().parse().ok())
}

/// Remove the PID file for a stage
pub fn remove_pid_file(work_dir: &Path, stage_id: &str) {
    let path = pid_file_path(work_dir, stage_id);
    let _ = fs::remove_file(path);
}

/// Remove the wrapper script for a stage
pub fn remove_wrapper_script(work_dir: &Path, stage_id: &str) {
    let path = wrapper_script_path(work_dir, stage_id);
    let _ = fs::remove_file(path);
}

/// Clean up all stage-related files (PID file and wrapper script)
pub fn cleanup_stage_files(work_dir: &Path, stage_id: &str) {
    remove_pid_file(work_dir, stage_id);
    remove_wrapper_script(work_dir, stage_id);
}

/// Check if a PID is alive using kill -0
pub fn check_pid_alive(pid: u32) -> bool {
    Command::new("kill")
        .arg("-0")
        .arg(pid.to_string())
        .output()
        .map(|output| output.status.success())
        .unwrap_or(false)
}

/// Discover the Claude process PID by scanning /proc
///
/// Searches for processes with "claude" in the command line that have
/// the specified working directory. Returns the first matching PID.
///
/// # Arguments
/// * `worktree_path` - The expected working directory of the Claude process
/// * `timeout` - Maximum time to wait for the process to appear
pub fn discover_claude_pid(worktree_path: &Path, timeout: Duration) -> Option<u32> {
    let deadline = Instant::now() + timeout;
    let canonical_worktree = worktree_path.canonicalize().ok()?;

    while Instant::now() < deadline {
        if let Some(pid) = find_claude_process(&canonical_worktree) {
            return Some(pid);
        }
        thread::sleep(Duration::from_millis(100));
    }

    None
}

/// Find a Claude process with the given working directory
fn find_claude_process(worktree_path: &Path) -> Option<u32> {
    let proc_dir = Path::new("/proc");

    let entries = fs::read_dir(proc_dir).ok()?;

    for entry in entries.flatten() {
        let file_name = entry.file_name();
        let pid_str = file_name.to_string_lossy();

        // Skip non-numeric entries
        let pid: u32 = match pid_str.parse() {
            Ok(p) => p,
            Err(_) => continue,
        };

        // Check if this is a claude process
        let cmdline_path = entry.path().join("cmdline");
        if let Ok(cmdline) = fs::read_to_string(&cmdline_path) {
            // cmdline uses null bytes as separators
            if !cmdline.contains("claude") {
                continue;
            }
        } else {
            continue;
        }

        // Check working directory matches worktree
        let cwd_link = entry.path().join("cwd");
        if let Ok(cwd) = fs::read_link(&cwd_link) {
            // Canonicalize cwd for comparison
            if let Ok(canonical_cwd) = cwd.canonicalize() {
                if canonical_cwd == worktree_path {
                    return Some(pid);
                }
            } else if cwd == worktree_path {
                return Some(pid);
            }
        }
    }

    None
}

/// Create a wrapper script that writes its PID before exec'ing claude
///
/// The wrapper script:
/// 1. Creates the pids directory if needed
/// 2. Writes its own PID ($$) to the PID file
/// 3. exec's the claude command (replacing the shell process)
///
/// # Arguments
/// * `work_dir` - The .work directory path
/// * `stage_id` - The stage identifier
/// * `claude_cmd` - The claude command to execute (e.g., "claude 'prompt here'")
///
/// # Returns
/// The path to the created wrapper script
pub fn create_wrapper_script(work_dir: &Path, stage_id: &str, claude_cmd: &str) -> Result<PathBuf> {
    create_wrappers_dir(work_dir)?;
    create_pid_dir(work_dir)?;

    let wrapper_path = wrapper_script_path(work_dir, stage_id);
    let pid_file = pid_file_path(work_dir, stage_id);

    let script = format!(
        r#"#!/bin/bash
# Loom wrapper script for stage: {stage_id}
# Writes PID to file before exec'ing claude

# Write our PID to the tracking file
echo $$ > "{pid_file}"

# Replace this process with claude
exec {claude_cmd}
"#,
        stage_id = stage_id,
        pid_file = pid_file.display(),
        claude_cmd = claude_cmd
    );

    fs::write(&wrapper_path, &script)
        .with_context(|| format!("Failed to write wrapper script: {}", wrapper_path.display()))?;

    // Make the script executable
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let mut perms = fs::metadata(&wrapper_path)?.permissions();
        perms.set_mode(0o755);
        fs::set_permissions(&wrapper_path, perms)?;
    }

    Ok(wrapper_path)
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[test]
    fn test_pid_file_operations() {
        let temp_dir = TempDir::new().unwrap();
        let work_dir = temp_dir.path();
        let stage_id = "test-stage";

        // Initially no PID file
        assert!(read_pid_file(work_dir, stage_id).is_none());

        // Write PID
        write_pid_file(work_dir, stage_id, 12345).unwrap();

        // Read PID back
        assert_eq!(read_pid_file(work_dir, stage_id), Some(12345));

        // Remove PID file
        remove_pid_file(work_dir, stage_id);
        assert!(read_pid_file(work_dir, stage_id).is_none());
    }

    #[test]
    fn test_wrapper_script_creation() {
        let temp_dir = TempDir::new().unwrap();
        let work_dir = temp_dir.path();
        let stage_id = "test-stage";
        let claude_cmd = "claude 'test prompt'";

        let wrapper_path = create_wrapper_script(work_dir, stage_id, claude_cmd).unwrap();

        // Check file exists
        assert!(wrapper_path.exists());

        // Check content
        let content = fs::read_to_string(&wrapper_path).unwrap();
        assert!(content.contains("#!/bin/bash"));
        assert!(content.contains("echo $$"));
        assert!(content.contains(claude_cmd));

        // Check executable
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let perms = fs::metadata(&wrapper_path).unwrap().permissions();
            assert!(perms.mode() & 0o111 != 0); // Has some execute bit
        }
    }

    #[test]
    fn test_check_pid_alive() {
        // Current process should be alive
        let our_pid = std::process::id();
        assert!(check_pid_alive(our_pid));

        // Non-existent PID should not be alive (using a very high PID)
        assert!(!check_pid_alive(999999999));
    }

    #[test]
    fn test_cleanup_stage_files() {
        let temp_dir = TempDir::new().unwrap();
        let work_dir = temp_dir.path();
        let stage_id = "test-stage";

        // Create files
        write_pid_file(work_dir, stage_id, 12345).unwrap();
        create_wrapper_script(work_dir, stage_id, "claude 'test'").unwrap();

        // Verify they exist
        assert!(pid_file_path(work_dir, stage_id).exists());
        assert!(wrapper_script_path(work_dir, stage_id).exists());

        // Cleanup
        cleanup_stage_files(work_dir, stage_id);

        // Verify they're gone
        assert!(!pid_file_path(work_dir, stage_id).exists());
        assert!(!wrapper_script_path(work_dir, stage_id).exists());
    }
}
