//! Type definitions for session attachment functionality.

use crate::models::session::SessionStatus;
use crate::orchestrator::terminal::BackendType;

/// Backend information for an attachable session
#[derive(Debug, Clone)]
pub enum SessionBackend {
    /// tmux backend - has a tmux session name
    Tmux { session_name: String },
    /// Native backend - has a process ID
    Native { pid: u32 },
}

/// Information about an attachable session
#[derive(Debug, Clone)]
pub struct AttachableSession {
    pub session_id: String,
    pub stage_id: Option<String>,
    pub stage_name: Option<String>,
    /// Backend-specific information for attaching
    pub backend: SessionBackend,
    pub status: SessionStatus,
    pub context_percent: f64,
}

impl AttachableSession {
    /// Get the tmux session name if this is a tmux session
    pub fn tmux_session(&self) -> Option<&str> {
        match &self.backend {
            SessionBackend::Tmux { session_name } => Some(session_name),
            SessionBackend::Native { .. } => None,
        }
    }

    /// Get the PID if this is a native session
    pub fn pid(&self) -> Option<u32> {
        match &self.backend {
            SessionBackend::Tmux { .. } => None,
            SessionBackend::Native { pid } => Some(*pid),
        }
    }

    /// Get the backend type
    pub fn backend_type(&self) -> BackendType {
        match &self.backend {
            SessionBackend::Tmux { .. } => BackendType::Tmux,
            SessionBackend::Native { .. } => BackendType::Native,
        }
    }

    /// Check if this is a tmux session
    pub fn is_tmux(&self) -> bool {
        matches!(self.backend, SessionBackend::Tmux { .. })
    }

    /// Check if this is a native session
    pub fn is_native(&self) -> bool {
        matches!(self.backend, SessionBackend::Native { .. })
    }
}
