//! Type definitions for session attachment functionality.

use crate::models::session::SessionStatus;
use crate::orchestrator::terminal::BackendType;

/// Backend information for an attachable session
#[derive(Debug, Clone)]
pub enum SessionBackend {
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
    /// Get the PID for this native session
    pub fn pid(&self) -> u32 {
        match &self.backend {
            SessionBackend::Native { pid } => *pid,
        }
    }

    /// Get the backend type
    pub fn backend_type(&self) -> BackendType {
        // All sessions are now native
        BackendType::Native
    }

    /// Check if this is a native session (always true now)
    pub fn is_native(&self) -> bool {
        true
    }
}
