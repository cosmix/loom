//! Native terminal backend
//!
//! Spawns sessions in native terminal windows (kitty, alacritty, etc.)
//! using xdg-terminal-exec or fallback detection.

mod detection;
mod pid_tracking;
mod spawner;
mod window_ops;

use anyhow::{bail, Context, Result};
use shell_escape::escape;
use std::borrow::Cow;
use std::path::{Path, PathBuf};
use std::process::Command;

use super::{BackendType, TerminalBackend};
use crate::models::session::{Session, SessionStatus};
use crate::models::stage::Stage;
use crate::models::worktree::Worktree;

pub use detection::detect_terminal;
pub use pid_tracking::{check_pid_alive, cleanup_stage_files, read_pid_file};
pub use spawner::spawn_in_terminal;
pub use window_ops::{close_window_by_title, focus_window_by_pid, window_exists_by_title};

/// Native terminal backend - spawns sessions in native terminal windows
pub struct NativeBackend {
    /// The terminal emulator to use
    terminal: super::emulator::TerminalEmulator,
    /// The .work directory path for PID tracking
    work_dir: PathBuf,
}

impl NativeBackend {
    /// Create a new native backend, detecting the available terminal
    pub fn new(work_dir: PathBuf) -> Result<Self> {
        let terminal = detect_terminal()?;
        Ok(Self { terminal, work_dir })
    }

    /// Get the detected terminal emulator
    pub fn terminal(&self) -> &super::emulator::TerminalEmulator {
        &self.terminal
    }
}

impl TerminalBackend for NativeBackend {
    fn spawn_session(
        &self,
        stage: &Stage,
        worktree: &Worktree,
        session: Session,
        signal_path: &Path,
    ) -> Result<Session> {
        let worktree_path = worktree.path.to_str().ok_or_else(|| {
            anyhow::anyhow!(
                "Worktree path contains invalid UTF-8: {}",
                worktree.path.display()
            )
        })?;

        // Build the title for the terminal window
        let title = format!("loom-{}", stage.id);

        // Build the initial prompt for Claude
        let signal_path_str = signal_path.to_string_lossy();
        let initial_prompt = format!(
            "Read the signal file at {signal_path_str} and execute the assigned stage work. \
             This file contains your assignment, tasks, acceptance criteria, \
             and context files to read."
        );

        // Escape the prompt for shell
        let escaped_prompt = escape(Cow::Borrowed(&initial_prompt));

        // Build the command to run in the terminal
        let claude_cmd = format!("claude {escaped_prompt}");

        // Create wrapper script that writes PID before exec'ing claude
        let wrapper_path =
            pid_tracking::create_wrapper_script(&self.work_dir, &stage.id, &claude_cmd)?;

        // Build the command that runs the wrapper script
        let wrapper_cmd = wrapper_path.to_string_lossy();

        // Spawn the terminal with PID tracking enabled
        let pid = spawn_in_terminal(
            &self.terminal,
            &title,
            Path::new(worktree_path),
            &wrapper_cmd,
            Some(&self.work_dir),
            Some(&stage.id),
        )?;

        // Update the session with spawn info
        let mut session = session;
        session.set_worktree_path(worktree.path.clone());
        session.assign_to_stage(stage.id.clone());
        session.set_pid(pid);
        session.try_mark_running()?;

        Ok(session)
    }

    fn spawn_merge_session(
        &self,
        stage: &Stage,
        session: Session,
        signal_path: &Path,
        repo_root: &Path,
    ) -> Result<Session> {
        let repo_root_str = repo_root.to_str().ok_or_else(|| {
            anyhow::anyhow!(
                "Repository path contains invalid UTF-8: {}",
                repo_root.display()
            )
        })?;

        // Build the title for the merge session window
        let title = format!("loom-merge-{}", stage.id);

        // Build the initial prompt for Claude merge session
        let signal_path_str = signal_path.to_string_lossy();
        let initial_prompt = format!(
            "Read the merge signal file at {signal_path_str} and resolve the merge conflicts. \
             This file contains the conflicting files, merge context, and resolution instructions."
        );

        // Escape the prompt for shell
        let escaped_prompt = escape(Cow::Borrowed(&initial_prompt));

        // Build the command to run in the terminal
        let claude_cmd = format!("claude {escaped_prompt}");

        // Create wrapper script for merge session
        let wrapper_path = pid_tracking::create_wrapper_script(
            &self.work_dir,
            &format!("merge-{}", stage.id),
            &claude_cmd,
        )?;

        // Build the command that runs the wrapper script
        let wrapper_cmd = wrapper_path.to_string_lossy();

        // Spawn the terminal in the main repository (not worktree)
        let pid = spawn_in_terminal(
            &self.terminal,
            &title,
            Path::new(repo_root_str),
            &wrapper_cmd,
            Some(&self.work_dir),
            Some(&format!("merge-{}", stage.id)),
        )?;

        // Update the session with spawn info
        // Note: For merge sessions, we don't set worktree_path since we're in the main repo
        let mut session = session;
        session.assign_to_stage(stage.id.clone());
        session.set_pid(pid);
        session.try_mark_running()?;

        Ok(session)
    }

    fn spawn_base_conflict_session(
        &self,
        stage: &Stage,
        session: Session,
        signal_path: &Path,
        repo_root: &Path,
    ) -> Result<Session> {
        let repo_root_str = repo_root.to_str().ok_or_else(|| {
            anyhow::anyhow!(
                "Repository path contains invalid UTF-8: {}",
                repo_root.display()
            )
        })?;

        // Build the title for the base conflict session window
        let title = format!("loom-base-conflict-{}", stage.id);

        // Build the initial prompt for Claude base conflict resolution session
        let signal_path_str = signal_path.to_string_lossy();
        let initial_prompt = format!(
            "Read the base conflict signal file at {signal_path_str} and resolve the merge conflicts. \
             This file contains the conflicting files from merging dependency branches, \
             and instructions for resolution. After resolving, tell the user to run `loom retry {}`.",
            stage.id
        );

        // Escape the prompt for shell
        let escaped_prompt = escape(Cow::Borrowed(&initial_prompt));

        // Build the command to run in the terminal
        let claude_cmd = format!("claude {escaped_prompt}");

        // Create wrapper script for base conflict session
        let wrapper_path = pid_tracking::create_wrapper_script(
            &self.work_dir,
            &format!("base-conflict-{}", stage.id),
            &claude_cmd,
        )?;

        // Build the command that runs the wrapper script
        let wrapper_cmd = wrapper_path.to_string_lossy();

        // Spawn the terminal in the main repository (not worktree)
        let pid = spawn_in_terminal(
            &self.terminal,
            &title,
            Path::new(repo_root_str),
            &wrapper_cmd,
            Some(&self.work_dir),
            Some(&format!("base-conflict-{}", stage.id)),
        )?;

        // Update the session with spawn info
        // Note: For base conflict sessions, we don't set worktree_path since we're in the main repo
        let mut session = session;
        session.assign_to_stage(stage.id.clone());
        session.set_pid(pid);
        session.try_mark_running()?;

        Ok(session)
    }

    fn spawn_knowledge_session(
        &self,
        stage: &Stage,
        session: Session,
        signal_path: &Path,
        repo_root: &Path,
    ) -> Result<Session> {
        let repo_root_str = repo_root.to_str().ok_or_else(|| {
            anyhow::anyhow!(
                "Repository path contains invalid UTF-8: {}",
                repo_root.display()
            )
        })?;

        // Build the title for the knowledge session window
        let title = format!("loom-knowledge-{}", stage.id);

        // Build the initial prompt for Claude knowledge gathering session
        let signal_path_str = signal_path.to_string_lossy();
        let initial_prompt = format!(
            "Read the signal file at {signal_path_str} and execute the assigned knowledge gathering work. \
             This file contains your assignment, tasks, acceptance criteria, \
             and instructions for populating the knowledge base."
        );

        // Escape the prompt for shell
        let escaped_prompt = escape(Cow::Borrowed(&initial_prompt));

        // Build the command to run in the terminal
        let claude_cmd = format!("claude {escaped_prompt}");

        // Create wrapper script for knowledge session
        let wrapper_path = pid_tracking::create_wrapper_script(
            &self.work_dir,
            &format!("knowledge-{}", stage.id),
            &claude_cmd,
        )?;

        // Build the command that runs the wrapper script
        let wrapper_cmd = wrapper_path.to_string_lossy();

        // Spawn the terminal in the main repository (not worktree)
        let pid = spawn_in_terminal(
            &self.terminal,
            &title,
            Path::new(repo_root_str),
            &wrapper_cmd,
            Some(&self.work_dir),
            Some(&format!("knowledge-{}", stage.id)),
        )?;

        // Update the session with spawn info
        // Note: For knowledge sessions, we don't set worktree_path since we're in the main repo
        let mut session = session;
        session.assign_to_stage(stage.id.clone());
        session.set_pid(pid);
        session.try_mark_running()?;

        Ok(session)
    }

    fn kill_session(&self, session: &Session) -> Result<()> {
        // First, try to close the window by title (more reliable for all terminals).
        // The title is set to "loom-{stage_id}" when spawning.
        // This approach works correctly even for terminal emulators like gnome-terminal
        // that use a server process, where killing by PID would kill all windows.
        if let Some(stage_id) = &session.stage_id {
            let title = format!("loom-{stage_id}");
            if close_window_by_title(&title) {
                // Clean up tracking files after closing the window
                cleanup_stage_files(&self.work_dir, stage_id);
                return Ok(());
            }
        }

        // Fallback to PID-based killing for terminals where window title closing
        // didn't work (e.g., no wmctrl/xdotool installed, or window already closed).
        // This works correctly for terminals like kitty/alacritty where each window
        // has its own process.
        if let Some(pid) = session.pid {
            // Send SIGTERM to the process
            let output = Command::new("kill")
                .arg("-TERM")
                .arg(pid.to_string())
                .output()
                .context("Failed to kill session process")?;

            if !output.status.success() {
                // Process might already be dead, which is fine
                let stderr = String::from_utf8_lossy(&output.stderr);
                if !stderr.contains("No such process") {
                    bail!("Failed to kill process {pid}: {stderr}");
                }
            }

            // Clean up tracking files
            if let Some(stage_id) = &session.stage_id {
                cleanup_stage_files(&self.work_dir, stage_id);
            }
        }
        Ok(())
    }

    fn is_session_alive(&self, session: &Session) -> Result<bool> {
        // Layered approach to checking if session is alive:
        // 1. Try reading from PID file (most current)
        // 2. Check if that PID is alive
        // 3. Fallback to stored session.pid
        // 4. Fallback to window existence check

        // First, try to get the most current PID from the PID file
        if let Some(stage_id) = &session.stage_id {
            if let Some(current_pid) = read_pid_file(&self.work_dir, stage_id) {
                // We have a PID from the tracking file, check if it's alive
                if check_pid_alive(current_pid) {
                    return Ok(true);
                }
                // PID file exists but process is dead - clean up and continue checking
                cleanup_stage_files(&self.work_dir, stage_id);
            }
        }

        // Fallback to the stored PID from the session
        if let Some(pid) = session.pid {
            // Check if process exists using kill -0
            let output = Command::new("kill")
                .arg("-0")
                .arg(pid.to_string())
                .output()
                .context("Failed to check process status")?;

            if output.status.success() {
                return Ok(true);
            }
        }

        // Final fallback: check if window still exists
        if let Some(stage_id) = &session.stage_id {
            let title = format!("loom-{stage_id}");
            if window_exists_by_title(&title) {
                return Ok(true);
            }
        }

        Ok(false)
    }

    fn attach_session(&self, session: &Session) -> Result<()> {
        if session.status != SessionStatus::Running {
            bail!("Session {} is not running", session.id);
        }

        if let Some(pid) = session.pid {
            // Try to focus the window using wmctrl or xdotool
            // This is best-effort - we don't fail if it doesn't work
            let _ = focus_window_by_pid(pid);
        }

        Ok(())
    }

    fn attach_all(&self, sessions: &[Session]) -> Result<()> {
        for session in sessions {
            if session.status == SessionStatus::Running {
                if let Some(pid) = session.pid {
                    // Try to focus each window, but don't fail on errors
                    let _ = focus_window_by_pid(pid);
                }
            }
        }
        Ok(())
    }

    fn backend_type(&self) -> BackendType {
        BackendType::Native
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[test]
    fn test_native_backend_creation() {
        // May fail if no terminal is available
        let temp_dir = TempDir::new().unwrap();
        let result = NativeBackend::new(temp_dir.path().to_path_buf());
        if result.is_ok() {
            let backend = result.unwrap();
            assert_eq!(backend.backend_type(), BackendType::Native);
        }
    }
}
