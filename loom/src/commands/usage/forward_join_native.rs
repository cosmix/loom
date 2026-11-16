use super::{ProviderDiagnostics, ReceiptThread};
use std::path::Path;

/// Threads of every native worker record. An unreadable record cannot name its thread, so it
/// counts as an unreadable file: the thread set is then incomplete and `scope_available` is false.
pub(super) fn native_threads(
    root: &Path,
    diagnostics: &mut ProviderDiagnostics,
) -> Vec<ReceiptThread> {
    let mut rows = Vec::new();
    for stage_id in crate::native_worker::index::stages(root) {
        diagnostics.unreadable_files += crate::native_worker::index::stage_units(root, &stage_id)
            .iter()
            .filter(|unit| unit.is_err())
            .count();
        for thread in crate::native_worker::index::stage_threads(root, &stage_id) {
            rows.push(ReceiptThread {
                thread_id: Some(thread.thread_id),
                observed_at: thread.started_at,
                stage_id: stage_id.clone(),
            });
        }
    }
    rows
}
