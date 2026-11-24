//! The bindings a completion from a host-job run requires, shared by the
//! completion of a session's stage (`apply`) and of a verdict-held stage
//! (`verdict_complete`, `verdict_reuse`).

use std::fmt;
use std::path::Path;

use anyhow::Result;

use super::{gates, JobRecord, JobResult};
use crate::handoff::completion::content::VerificationSnapshot;
use crate::models::stage::Stage;
use crate::native_worker::index::stage_disposition;
use crate::verify::review::verdict_records::with_reviews_locked;

/// A binding the passing run no longer satisfies. Raised inside the stage
/// update so the transition is refused and the caller can tell it from an
/// I/O failure.
#[derive(Debug)]
struct BindingFailed(String);

impl fmt::Display for BindingFailed {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.0)
    }
}

impl std::error::Error for BindingFailed {}

pub(super) fn binding_failed(reason: impl Into<String>) -> anyhow::Error {
    anyhow::Error::new(BindingFailed(reason.into()))
}

/// The reason of a binding failure; any other error is a failure to read or
/// write state and stays an error.
pub(super) fn binding_reason(error: anyhow::Error) -> Result<String> {
    match error.downcast_ref::<BindingFailed>() {
        Some(failed) => Ok(failed.0.clone()),
        None => Err(error),
    }
}

/// Run `complete` while the stage's review records cannot change, once the
/// completion gates pass on them. Lock order: the caller's sessions lock or
/// dispute claim, then this reviews lock, then the stage file `complete`
/// takes. The gates take seconds, held under those locks once per completion.
///
/// Why the order cannot deadlock (checked over `verify/review`,
/// `verify/integrity`, `verify/impact_tests` and `commands/hook/review_harvest.rs`):
/// every writer under a `reviews/<stage>` lock (`store::write_round`,
/// `store::record_round` with the harvest's `decide` and its memory journal
/// append, `verdict_records::update` and its `build` closures,
/// `handoffs`, `impact_tests::observed`) only reads and writes files in
/// `reviews/` or the memory journal, so none waits on the sessions lock, a
/// dispute claim or the stages lock while holding the reviews lock. The
/// dispute paths that write review records take the dispute claim first, the
/// order used here. The gates (`daemon::check_completion_gates`) read the
/// stage file through `locked_read` on `stages/` (not held here: `complete`
/// takes it after the gates) and read `reviews/` and `sessions/` without a
/// lock, so they take no second descriptor on a directory this process
/// already holds; `flock` descriptors of one process conflict. The gates must
/// therefore never run inside `update_stage`, which holds `stages/`.
pub(super) fn with_gates_held<T>(
    work_dir: &Path,
    stage_id: &str,
    complete: impl FnOnce() -> Result<T>,
) -> Result<T> {
    with_reviews_locked(work_dir, stage_id, || {
        if let Some(reason) = gates::refusal(work_dir, stage_id) {
            return Err(binding_failed(reason));
        }
        complete()
    })
}

/// The binding every completion from a run requires: the run passed at the
/// requested head, the stage's committed content is still that head, and the
/// check definition is the one the run was bound to.
pub(super) fn verify_bound_run(
    stage: &Stage,
    work_dir: &Path,
    record: &JobRecord,
    result: &JobResult,
) -> Result<()> {
    if let Some(blocker) = stage_disposition(work_dir, &stage.id).blocker(&stage.id) {
        return Err(binding_failed(blocker));
    }
    if !result.passed || result.head != record.head {
        return Err(binding_failed("the run does not verify the requested head"));
    }
    let snapshot = VerificationSnapshot::capture(stage, work_dir).map_err(|error| {
        binding_failed(format!("completion content is not committed: {error:#}"))
    })?;
    if snapshot.commit != result.head {
        return Err(binding_failed(format!(
            "the run verified {} but the stage's committed content is now {}",
            result.head, snapshot.commit
        )));
    }
    if result.policy_hash.as_deref() != Some(snapshot.policy.as_str()) {
        return Err(binding_failed(
            "the stage's check definition changed since the run",
        ));
    }
    Ok(())
}

#[cfg(test)]
pub(crate) mod hooks {
    use std::cell::RefCell;

    thread_local! {
        static BEFORE_COMPLETION_LOCK: RefCell<Option<Box<dyn FnOnce()>>> = RefCell::new(None);
    }

    /// Run `hook` once, on this thread, between a completion's outside gate
    /// check and the locks it completes under.
    pub(crate) fn before_completion_lock(hook: impl FnOnce() + 'static) {
        BEFORE_COMPLETION_LOCK.with(|slot| *slot.borrow_mut() = Some(Box::new(hook)));
    }

    pub(in crate::host_verify) fn run_before_completion_lock() {
        let hook = BEFORE_COMPLETION_LOCK.with(|slot| slot.borrow_mut().take());
        if let Some(hook) = hook {
            hook();
        }
    }
}
