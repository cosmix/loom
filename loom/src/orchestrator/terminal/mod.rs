//! Terminal backend abstraction for session management
//!
//! Provides a unified interface for spawning and managing Claude Code sessions
//! in different terminal environments (native terminal windows or tmux).

pub mod native;
pub mod tmux;

use anyhow::Result;
use std::path::Path;

use crate::models::session::Session;
use crate::models::stage::Stage;
use crate::models::worktree::Worktree;

/// Backend type selection
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum BackendType {
    /// Native terminal windows (default) - each session in its own terminal
    #[default]
    Native,
    /// tmux multiplexer - all sessions in tmux
    Tmux,
}

impl std::fmt::Display for BackendType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            BackendType::Native => write!(f, "native"),
            BackendType::Tmux => write!(f, "tmux"),
        }
    }
}

impl std::str::FromStr for BackendType {
    type Err = anyhow::Error;

    fn from_str(s: &str) -> Result<Self> {
        match s.to_lowercase().as_str() {
            "native" => Ok(BackendType::Native),
            "tmux" => Ok(BackendType::Tmux),
            _ => anyhow::bail!("Unknown backend type: {s}. Expected 'native' or 'tmux'"),
        }
    }
}

/// Trait for terminal backends
///
/// Implementations handle spawning Claude Code sessions in different
/// terminal environments while providing a consistent interface.
pub trait TerminalBackend: Send + Sync {
    /// Spawn a new Claude Code session for the given stage
    ///
    /// Creates a terminal (native window or tmux session) and runs the claude
    /// command with the signal file path as the initial prompt.
    fn spawn_session(
        &self,
        stage: &Stage,
        worktree: &Worktree,
        session: Session,
        signal_path: &Path,
    ) -> Result<Session>;

    /// Kill a running session
    fn kill_session(&self, session: &Session) -> Result<()>;

    /// Check if a session is still alive
    fn is_session_alive(&self, session: &Session) -> Result<bool>;

    /// Attach to a single session (focus/open its terminal)
    fn attach_session(&self, session: &Session) -> Result<()>;

    /// Attach to all active sessions (open all terminal windows)
    fn attach_all(&self, sessions: &[Session]) -> Result<()>;

    /// Get the backend type
    fn backend_type(&self) -> BackendType;
}

/// Create a terminal backend based on the specified type
pub fn create_backend(backend_type: BackendType) -> Result<Box<dyn TerminalBackend>> {
    match backend_type {
        BackendType::Native => {
            let backend = native::NativeBackend::new()?;
            Ok(Box::new(backend))
        }
        BackendType::Tmux => {
            let backend = tmux::TmuxBackend::new()?;
            Ok(Box::new(backend))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_backend_type_display() {
        assert_eq!(BackendType::Native.to_string(), "native");
        assert_eq!(BackendType::Tmux.to_string(), "tmux");
    }

    #[test]
    fn test_backend_type_from_str() {
        assert_eq!(
            "native".parse::<BackendType>().unwrap(),
            BackendType::Native
        );
        assert_eq!("tmux".parse::<BackendType>().unwrap(), BackendType::Tmux);
        assert_eq!(
            "NATIVE".parse::<BackendType>().unwrap(),
            BackendType::Native
        );
        assert!("invalid".parse::<BackendType>().is_err());
    }

    #[test]
    fn test_backend_type_default() {
        assert_eq!(BackendType::default(), BackendType::Native);
    }
}
