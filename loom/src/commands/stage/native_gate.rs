use std::path::Path;

pub(crate) fn ensure_no_native_workers(work_dir: &Path, stage_id: &str) -> anyhow::Result<()> {
    match crate::native_worker::index::stage_disposition(work_dir, stage_id).blocker(stage_id) {
        Some(blocker) => Err(anyhow::anyhow!(blocker)),
        None => Ok(()),
    }
}

#[cfg(test)]
#[path = "native_gate_tests.rs"]
pub(crate) mod tests;
