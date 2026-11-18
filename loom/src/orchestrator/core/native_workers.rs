//! Native worker supervisors on the orchestrator tick: recovery once at
//! startup, one runner pass per tick, and a bounded cancel when the run ends.

use super::Orchestrator;
use crate::native_worker::runner::recover_after_restart;

impl Orchestrator {
    /// One pass of the native runner; it logs its own failures.
    pub(super) fn tick_native_workers(&mut self) {
        self.native_workers.tick();
    }

    /// Settles every supervisor an earlier daemon left behind that is gone.
    pub(super) fn recover_native_workers(&self) {
        let settled = recover_after_restart(&self.config.work_dir);
        if !settled.is_empty() {
            tracing::warn!(
                invocations = %settled.join(", "),
                "native workers whose supervisor was lost were settled",
            );
        }
    }

    /// Asks every running native worker to cancel and waits a bounded time.
    pub(super) fn stop_native_workers(&mut self) {
        self.native_workers.cancel_all();
    }
}
