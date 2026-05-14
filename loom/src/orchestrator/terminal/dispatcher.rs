//! Backend dispatcher — owns the `TerminalBackend` instance and routes
//! every spawn/kill/liveness call to it, keyed on either a per-stage
//! backend selection or a session's persisted `backend` metadata.
//!
//! Why a dispatcher?
//!
//! Backend selection is per-stage (with a project default). Sessions
//! persist their resolved backend on disk so that even after daemon
//! restart we route a `kill` through a consistent path. The dispatcher
//! is the single source of truth for which backends are actually
//! constructed; `BackendNeeds` lets the caller declare up-front.

use anyhow::{bail, Context, Result};
use std::path::Path;

use super::{create_backend, BackendType, TerminalBackend};
use crate::models::session::Session;

/// Declares which backends a plan actually needs constructed.
#[derive(Debug, Clone, Copy, Default)]
pub struct BackendNeeds {
    pub native: bool,
}

impl BackendNeeds {
    /// Convenience constructor for a project where the default backend is
    /// `project` and each per-stage override is also accounted for.
    pub fn from_project_and_overrides(
        project: BackendType,
        per_stage_overrides: &[BackendType],
    ) -> Self {
        let mut needs = Self::default();
        let mark = |needs: &mut Self, b: BackendType| match b {
            BackendType::Native => needs.native = true,
        };
        mark(&mut needs, project);
        for b in per_stage_overrides {
            mark(&mut needs, *b);
        }
        needs
    }
}

/// Owns the constructed backends. Each is optional so we never pay the
/// cost (terminal probe) for a backend that no stage will use.
pub struct BackendDispatcher {
    native: Option<Box<dyn TerminalBackend>>,
}

impl BackendDispatcher {
    /// Construct the dispatcher for a plan. `project` is the
    /// project-level default backend; `needs` tells the dispatcher which
    /// concrete backends to instantiate.
    pub fn for_plan(_project: BackendType, needs: BackendNeeds, work_dir: &Path) -> Result<Self> {
        let native =
            if needs.native {
                Some(create_backend(BackendType::Native, work_dir).context(
                    "Failed to construct the native terminal backend required by this plan",
                )?)
            } else {
                None
            };

        if native.is_none() {
            bail!("BackendDispatcher constructed with no backends — nothing to spawn");
        }

        Ok(Self { native })
    }

    /// Construct a dispatcher around an already-built backend. Lets call
    /// sites that compute backend selection themselves (foreground run,
    /// continuation) reuse a constructed backend without duplicating the
    /// factory step.
    pub fn from_single(backend_type: BackendType, backend: Box<dyn TerminalBackend>) -> Self {
        match backend_type {
            BackendType::Native => Self {
                native: Some(backend),
            },
        }
    }

    /// Backend selected for a stage (already resolved via
    /// [`resolve_stage_backend`]).
    pub fn for_stage(&self, backend: BackendType) -> &dyn TerminalBackend {
        match backend {
            BackendType::Native => self
                .native
                .as_deref()
                .expect("for_stage(Native) requested but dispatcher was built without Native"),
        }
    }

    /// Try to look up a backend by type, returning `None` if this
    /// dispatcher wasn't built with it. Useful for cleanup paths that
    /// run after a partial restart.
    pub fn try_for(&self, backend: BackendType) -> Option<&dyn TerminalBackend> {
        match backend {
            BackendType::Native => self.native.as_deref(),
        }
    }

    /// Route by the session's persisted `backend` field. This is the
    /// canonical lookup for monitor liveness checks, `loom sessions kill`,
    /// and `loom stop`.
    pub fn for_session(&self, session: &Session) -> &dyn TerminalBackend {
        self.for_stage(session.backend)
    }

    /// Iterate over every constructed backend (useful for global cleanup).
    pub fn all(&self) -> impl Iterator<Item = (BackendType, &dyn TerminalBackend)> {
        self.native
            .as_deref()
            .map(|b| (BackendType::Native, b))
            .into_iter()
    }
}

/// Resolve which backend a stage should run on.
///
/// With only the native backend available, a stage override (if present)
/// must agree with the project default — which it trivially does, since
/// `Native` is the only variant.
pub fn resolve_stage_backend(
    project: BackendType,
    stage_override: Option<BackendType>,
) -> Result<BackendType> {
    match stage_override {
        None => Ok(project),
        Some(BackendType::Native) => Ok(BackendType::Native),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn project_default_used_when_no_override() {
        assert_eq!(
            resolve_stage_backend(BackendType::Native, None).unwrap(),
            BackendType::Native
        );
    }

    #[test]
    fn native_override_resolves_to_native() {
        let resolved =
            resolve_stage_backend(BackendType::Native, Some(BackendType::Native)).unwrap();
        assert_eq!(resolved, BackendType::Native);
    }

    #[test]
    fn needs_collects_overrides() {
        let needs =
            BackendNeeds::from_project_and_overrides(BackendType::Native, &[BackendType::Native]);
        assert!(needs.native);
    }
}
