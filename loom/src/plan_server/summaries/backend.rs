//! The summary session spawner the server installs: Claude through the session bridge, or a
//! Codex job, chosen by `plan.summary_backend` at each start.

use std::path::PathBuf;
use std::sync::Arc;

use super::codex_spawner::{is_codex_session, CodexSummarySpawner};
use super::scheduler::SessionSpawner;
use super::spawner::RealSpawner;
use crate::plan_server::context::PlanContext;
use crate::plan_server::lifecycle::{line, safe};
use crate::planning::summary::config::SummaryModels;

/// Picks [`RealSpawner`] or [`CodexSummarySpawner`] per start.
pub struct BackendSpawner {
    claude: RealSpawner,
    codex: CodexSummarySpawner,
    root: PathBuf,
}

impl BackendSpawner {
    pub fn new(ctx: &Arc<PlanContext>) -> Self {
        BackendSpawner {
            claude: RealSpawner::new(ctx),
            codex: CodexSummarySpawner::new(ctx),
            root: ctx.root().to_path_buf(),
        }
    }

    /// True when the configuration, re-read now so a changed setting applies to the next
    /// start, selects Codex. A configuration that cannot be read selects Claude, said once per
    /// start.
    fn use_codex(&self) -> bool {
        match SummaryModels::load(&self.root) {
            Ok(models) => models.backend == "codex",
            Err(error) => {
                line(&format!(
                    "summaries: cannot read the summary backend, using claude: {}",
                    safe(&format!("{error:#}"))
                ));
                false
            }
        }
    }
}

impl SessionSpawner for BackendSpawner {
    fn start_summary(&self, ids: &[String]) -> anyhow::Result<String> {
        if self.use_codex() {
            self.codex.start_summary(ids)
        } else {
            self.claude.start_summary(ids)
        }
    }

    /// A session goes to the spawner that started it, which its id says even after a Codex job
    /// left the Codex spawner's registry (its claims may still need releasing).
    fn stop(&self, session: &str, release_claims: bool) {
        if is_codex_session(session) {
            self.codex.stop(session, release_claims);
        } else {
            self.claude.stop(session, release_claims);
        }
    }

    fn runs_outside_bridge(&self, session: &str) -> bool {
        self.codex.runs_outside_bridge(session)
    }

    fn stop_all(&self) {
        self.codex.stop_all();
    }
}
