//! Backend dispatcher — owns one or both `TerminalBackend` instances and
//! routes every spawn/kill/liveness call to the correct one based on
//! either a per-stage backend selection or a session's persisted
//! `backend` metadata.
//!
//! Why a dispatcher?
//!
//! Stage 2 made backend selection per-stage (with project default).
//! Sessions persist their resolved backend on disk so that even after
//! daemon restart we route a `kill` to the right runtime. Hard-coding
//! `Box<dyn TerminalBackend>` on the orchestrator forces an early commit
//! to one backend per run; we need both in flight when a plan has mixed
//! stages.
//!
//! The dispatcher is the single source of truth for which backends are
//! actually constructed (no need to detect Docker when no stage uses
//! containers). `BackendNeeds` lets the caller declare up-front.

use anyhow::{anyhow, bail, Context, Result};
use std::path::Path;

use super::{create_backend, BackendType, TerminalBackend};
use crate::models::session::Session;

/// Declares which backends a plan actually needs constructed.
#[derive(Debug, Clone, Copy, Default)]
pub struct BackendNeeds {
    pub native: bool,
    pub container: bool,
}

impl BackendNeeds {
    /// Convenience constructor for a project where the default backend is
    /// `default` and at least one stage uses each of `extras`.
    pub fn from_project_and_overrides(
        project: BackendType,
        per_stage_overrides: &[BackendType],
    ) -> Self {
        let mut needs = Self::default();
        let mark = |needs: &mut Self, b: BackendType| match b {
            BackendType::Native => needs.native = true,
            BackendType::Container => needs.container = true,
        };
        mark(&mut needs, project);
        for b in per_stage_overrides {
            mark(&mut needs, *b);
        }
        needs
    }
}

/// Owns the constructed backends. Each is optional so we never pay the
/// cost (Docker detection, terminal probe) for a backend that no stage
/// will use.
pub struct BackendDispatcher {
    native: Option<Box<dyn TerminalBackend>>,
    container: Option<Box<dyn TerminalBackend>>,
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
        let container = if needs.container {
            Some(create_backend(BackendType::Container, work_dir).context(
                "Failed to construct the container terminal backend required by this plan",
            )?)
        } else {
            None
        };

        if native.is_none() && container.is_none() {
            bail!("BackendDispatcher constructed with no backends — nothing to spawn");
        }

        Ok(Self { native, container })
    }

    /// Construct a dispatcher around an already-built backend. Lets call
    /// sites that compute backend selection themselves (foreground run,
    /// continuation) reuse a constructed backend without duplicating the
    /// factory step.
    pub fn from_single(backend_type: BackendType, backend: Box<dyn TerminalBackend>) -> Self {
        match backend_type {
            BackendType::Native => Self {
                native: Some(backend),
                container: None,
            },
            BackendType::Container => Self {
                native: None,
                container: Some(backend),
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
            BackendType::Container => self.container.as_deref().expect(
                "for_stage(Container) requested but dispatcher was built without Container",
            ),
        }
    }

    /// Try to look up a backend by type, returning `None` if this
    /// dispatcher wasn't built with it. Useful for cleanup paths that
    /// run after a partial restart.
    pub fn try_for(&self, backend: BackendType) -> Option<&dyn TerminalBackend> {
        match backend {
            BackendType::Native => self.native.as_deref(),
            BackendType::Container => self.container.as_deref(),
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
        let n = self.native.as_deref().map(|b| (BackendType::Native, b));
        let c = self
            .container
            .as_deref()
            .map(|b| (BackendType::Container, b));
        n.into_iter().chain(c)
    }
}

/// Resolve which backend a stage should run on.
///
/// Narrowing-only: a container-backed project may *opt out* of containers
/// for an individual stage, but a native-backed project must reject any
/// per-stage opt-in to containers. The container backend requires
/// project-wide image provisioning (`loom init --backend container`)
/// before it can spawn anything; allowing a single stage to override
/// silently would surface as a confusing image-digest error at spawn
/// time. Instead, refuse early with an actionable message.
pub fn resolve_stage_backend(
    project: BackendType,
    stage_override: Option<BackendType>,
) -> Result<BackendType> {
    match (project, stage_override) {
        (_, None) => Ok(project),
        (BackendType::Container, Some(BackendType::Native)) => Ok(BackendType::Native),
        (BackendType::Container, Some(BackendType::Container)) => Ok(BackendType::Container),
        (BackendType::Native, Some(BackendType::Native)) => Ok(BackendType::Native),
        (BackendType::Native, Some(BackendType::Container)) => Err(anyhow!(
            "Stage requests container backend but the project default is `native`. \
             Run `loom init --backend container` to provision the container image first, \
             then re-run."
        )),
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
        assert_eq!(
            resolve_stage_backend(BackendType::Container, None).unwrap(),
            BackendType::Container
        );
    }

    #[test]
    fn container_project_can_narrow_to_native() {
        let resolved =
            resolve_stage_backend(BackendType::Container, Some(BackendType::Native)).unwrap();
        assert_eq!(resolved, BackendType::Native);
    }

    #[test]
    fn native_project_rejects_container_override() {
        let err =
            resolve_stage_backend(BackendType::Native, Some(BackendType::Container)).unwrap_err();
        let msg = format!("{err}");
        assert!(msg.contains("loom init --backend container"), "{msg}");
    }

    #[test]
    fn needs_collects_overrides() {
        let needs = BackendNeeds::from_project_and_overrides(
            BackendType::Native,
            &[BackendType::Container],
        );
        assert!(needs.native);
        assert!(needs.container);
    }
}
