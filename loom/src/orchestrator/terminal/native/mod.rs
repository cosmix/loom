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

use crate::claude::find_claude_path;
use crate::models::session::Session;
use crate::models::stage::Stage;
use crate::models::worktree::Worktree;

pub use detection::detect_terminal;
pub use pid_tracking::{cleanup_stage_files, create_wrapper_script, read_pid_file};
pub use spawner::spawn_in_terminal;
pub use window_ops::{close_window_by_title, window_exists_by_title};
#[cfg(target_os = "macos")]
pub use window_ops::{close_window_by_title_for_terminal, window_exists_by_title_for_terminal};

fn close_window_for_terminal(title: &str, terminal: &super::emulator::TerminalEmulator) -> bool {
    #[cfg(target_os = "macos")]
    {
        close_window_by_title_for_terminal(title, terminal)
    }

    #[cfg(not(target_os = "macos"))]
    {
        let _ = terminal;
        close_window_by_title(title)
    }
}

fn window_exists_for_terminal(title: &str, terminal: &super::emulator::TerminalEmulator) -> bool {
    #[cfg(target_os = "macos")]
    {
        window_exists_by_title_for_terminal(title, terminal)
    }

    #[cfg(not(target_os = "macos"))]
    {
        let _ = terminal;
        window_exists_by_title(title)
    }
}

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
        // Log the detected terminal for debugging terminal selection issues
        eprintln!("Detected terminal: {}", terminal.display_name());
        Ok(Self { terminal, work_dir })
    }

    /// Get the detected terminal emulator
    pub fn terminal(&self) -> &super::emulator::TerminalEmulator {
        &self.terminal
    }

    /// Returns (window_title, pid_file_key) for a session — preferring the
    /// session's tracking_key, with a legacy fallback to the bare stage id.
    fn window_title_and_pid_key(session: &Session) -> Option<(String, String)> {
        if !session.tracking_key.is_empty() {
            let title = session.tracking_key.clone();
            let pid_key = title.strip_prefix("loom-").unwrap_or(&title).to_string();
            return Some((title, pid_key));
        }
        session
            .stage_id
            .as_ref()
            .map(|sid| (format!("loom-{sid}"), sid.clone()))
    }

    pub fn spawn_session(
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

        // Find claude's absolute path (needed for macOS where terminals don't inherit PATH)
        let claude_path = find_claude_path()?;
        let model_flag = format!(
            " --model {}",
            escape(Cow::Borrowed(stage.effective_model()))
        );
        let effort_flag = format!(" --effort {}", stage.effective_reasoning_effort());
        let claude_cmd = format!(
            "{}{model_flag}{effort_flag} {escaped_prompt}",
            claude_path.display()
        );

        // Create wrapper script that writes PID before exec'ing claude
        // Pass the worktree path so the script can cd there (important for macOS)
        // Pass the session.id so LOOM_SESSION_ID env var is set for hooks and memory commands
        let wrapper_path = pid_tracking::create_wrapper_script(
            &self.work_dir,
            &stage.id,
            &session.id,
            &claude_cmd,
            Some(Path::new(worktree_path)),
        )?;

        // Build the command that runs the wrapper script
        // IMPORTANT: Use absolute path because macOS terminals open in home directory
        let wrapper_path_abs = wrapper_path.canonicalize().unwrap_or(wrapper_path);
        let wrapper_cmd = wrapper_path_abs.to_string_lossy();

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

    pub fn spawn_merge_session(
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

        // Find claude's absolute path (needed for macOS where terminals don't inherit PATH)
        let claude_path = find_claude_path()?;
        // Merge conflict resolution always runs on opus[1m] with xhigh reasoning,
        // regardless of the originating stage's model/effort — conflicts benefit
        // from the strongest model and maximum deliberation.
        let model_flag = " --model opus[1m]".to_string();
        let effort_flag = " --effort xhigh".to_string();
        let claude_cmd = format!(
            "{}{model_flag}{effort_flag} {escaped_prompt}",
            claude_path.display()
        );

        // Create wrapper script for merge session
        // Pass repo root so the script can cd there (important for macOS)
        let wrapper_path = pid_tracking::create_wrapper_script(
            &self.work_dir,
            &format!("merge-{}", stage.id),
            &session.id,
            &claude_cmd,
            Some(Path::new(repo_root_str)),
        )?;

        // Build the command that runs the wrapper script
        // IMPORTANT: Use absolute path because macOS terminals open in home directory
        let wrapper_path_abs = wrapper_path.canonicalize().unwrap_or(wrapper_path);
        let wrapper_cmd = wrapper_path_abs.to_string_lossy();

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

    pub fn spawn_base_conflict_session(
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

        // Find claude's absolute path (needed for macOS where terminals don't inherit PATH)
        let claude_path = find_claude_path()?;
        // Base-branch conflict resolution always runs on opus[1m] with xhigh
        // reasoning — same rationale as merge conflict sessions.
        let model_flag = " --model opus[1m]".to_string();
        let effort_flag = " --effort xhigh".to_string();
        let claude_cmd = format!(
            "{}{model_flag}{effort_flag} {escaped_prompt}",
            claude_path.display()
        );

        // Create wrapper script for base conflict session
        // Pass repo root so the script can cd there (important for macOS)
        let wrapper_path = pid_tracking::create_wrapper_script(
            &self.work_dir,
            &format!("base-conflict-{}", stage.id),
            &session.id,
            &claude_cmd,
            Some(Path::new(repo_root_str)),
        )?;

        // Build the command that runs the wrapper script
        // IMPORTANT: Use absolute path because macOS terminals open in home directory
        let wrapper_path_abs = wrapper_path.canonicalize().unwrap_or(wrapper_path);
        let wrapper_cmd = wrapper_path_abs.to_string_lossy();

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

    pub fn spawn_knowledge_session(
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

        // Find claude's absolute path (needed for macOS where terminals don't inherit PATH)
        let claude_path = find_claude_path()?;
        let model_flag = format!(
            " --model {}",
            escape(Cow::Borrowed(stage.effective_model()))
        );
        let effort_flag = format!(" --effort {}", stage.effective_reasoning_effort());
        let claude_cmd = format!(
            "{}{model_flag}{effort_flag} {escaped_prompt}",
            claude_path.display()
        );

        // Create wrapper script for knowledge session
        // Pass repo root so the script can cd there (important for macOS)
        let wrapper_path = pid_tracking::create_wrapper_script(
            &self.work_dir,
            &format!("knowledge-{}", stage.id),
            &session.id,
            &claude_cmd,
            Some(Path::new(repo_root_str)),
        )?;

        // Build the command that runs the wrapper script
        // IMPORTANT: Use absolute path because macOS terminals open in home directory
        let wrapper_path_abs = wrapper_path.canonicalize().unwrap_or(wrapper_path);
        let wrapper_cmd = wrapper_path_abs.to_string_lossy();

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

    pub fn kill_session(&self, session: &Session) -> Result<()> {
        // Resolve the window title and PID-file key for this session,
        // preferring the session's tracking_key so that merge/knowledge/
        // base-conflict sessions (which use prefixed titles and keys) are
        // killed correctly, not just bare stage sessions.
        let resolved = Self::window_title_and_pid_key(session);

        // First, try to close the window by title (more reliable for all terminals).
        // This approach works correctly even for terminal emulators like gnome-terminal
        // that use a server process, where killing by PID would kill all windows.
        if let Some((title, pid_key)) = &resolved {
            if close_window_for_terminal(title, &self.terminal) {
                // Clean up tracking files after closing the window
                cleanup_stage_files(&self.work_dir, pid_key);
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
            if let Some((_, pid_key)) = &resolved {
                cleanup_stage_files(&self.work_dir, pid_key);
            }
        }
        Ok(())
    }

    pub fn is_session_alive(&self, session: &Session) -> Result<bool> {
        // Layered approach to checking if session is alive:
        // 1. Try reading from PID file (most current)
        // 2. Check if that PID is alive
        // 3. Fallback to stored session.pid
        // 4. Fallback to window existence check

        // Resolve the window title and PID-file key, preferring the
        // session's tracking_key so prefixed sessions resolve correctly.
        let resolved = Self::window_title_and_pid_key(session);

        // First, try to get the most current PID from the PID file
        if let Some((_, pid_key)) = &resolved {
            if let Some(current_pid) = read_pid_file(&self.work_dir, pid_key) {
                // We have a PID from the tracking file, check if it's alive
                if crate::process::is_process_alive(current_pid) {
                    return Ok(true);
                }
                // PID file exists but process is dead - clean up and continue checking
                cleanup_stage_files(&self.work_dir, pid_key);
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
        if let Some((title, _)) = &resolved {
            if window_exists_for_terminal(title, &self.terminal) {
                return Ok(true);
            }
        }

        Ok(false)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[test]
    fn test_native_backend_creation() {
        // May fail if no terminal is available; we only assert that when a
        // terminal *is* available, construction succeeds.
        let temp_dir = TempDir::new().unwrap();
        let result = NativeBackend::new(temp_dir.path().to_path_buf());
        if let Ok(backend) = result {
            // Sanity: the constructed backend exposes its terminal emulator.
            let _ = backend.terminal();
        }
    }

    #[test]
    fn window_title_and_pid_key_for_stage_session() {
        let mut session = Session::new();
        session.assign_to_stage("worker-pool".to_string());
        let (title, pid_key) = NativeBackend::window_title_and_pid_key(&session).unwrap();
        assert_eq!(title, "loom-worker-pool");
        assert_eq!(pid_key, "worker-pool");
    }

    #[test]
    fn window_title_and_pid_key_for_merge_session() {
        let mut session = Session::new_merge("loom/feature".to_string(), "main".to_string());
        session.assign_to_stage("feature".to_string());
        let (title, pid_key) = NativeBackend::window_title_and_pid_key(&session).unwrap();
        assert_eq!(title, "loom-merge-feature");
        assert_eq!(pid_key, "merge-feature");
    }

    #[test]
    fn window_title_and_pid_key_for_knowledge_session() {
        let session = Session::new_knowledge("kb");
        let (title, pid_key) = NativeBackend::window_title_and_pid_key(&session).unwrap();
        assert_eq!(title, "loom-knowledge-kb");
        assert_eq!(pid_key, "knowledge-kb");
    }

    #[test]
    fn window_title_and_pid_key_for_base_conflict_session() {
        let mut session = Session::new_base_conflict("loom/_base/feature".to_string());
        session.assign_to_stage("feature".to_string());
        let (title, pid_key) = NativeBackend::window_title_and_pid_key(&session).unwrap();
        assert_eq!(title, "loom-base-conflict-feature");
        assert_eq!(pid_key, "base-conflict-feature");
    }

    #[test]
    fn window_title_and_pid_key_legacy_fallback() {
        // Legacy session: empty tracking_key, falls back to the bare stage id.
        let mut session = Session::new();
        session.stage_id = Some("legacy".to_string());
        session.tracking_key = String::new();
        let (title, pid_key) = NativeBackend::window_title_and_pid_key(&session).unwrap();
        assert_eq!(title, "loom-legacy");
        assert_eq!(pid_key, "legacy");
    }

    #[test]
    fn window_title_and_pid_key_none_without_stage() {
        // No tracking_key and no stage_id → nothing to resolve.
        let session = Session::new();
        assert!(NativeBackend::window_title_and_pid_key(&session).is_none());
    }
}
