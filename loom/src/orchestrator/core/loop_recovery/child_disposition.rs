use std::path::Path;

use crate::native_worker::index::{stage_disposition, NativeDisposition};
use crate::orchestrator::core::Orchestrator;
use crate::subagent_lifecycle::store::{replay, ChildDisposition};

impl Orchestrator {
    pub(crate) fn session_child_disposition_in(
        work_dir: &Path,
        stage_id: &str,
        session_id: &str,
    ) -> ChildDisposition {
        session_child_disposition(work_dir, stage_id, session_id)
    }
}

pub(super) fn session_child_disposition(
    work_dir: &Path,
    stage_id: &str,
    session_id: &str,
) -> ChildDisposition {
    let native = stage_disposition(work_dir, stage_id);
    if let NativeDisposition::Unverified(reason) = native {
        return ChildDisposition::Unknown(reason);
    }
    let journal = replay(work_dir)
        .map(|index| index.session_child_disposition(stage_id, session_id))
        .unwrap_or_else(|error| ChildDisposition::Unknown(error.to_string()));
    match (journal, native) {
        (ChildDisposition::NoChildren, NativeDisposition::Active(_)) => ChildDisposition::Active,
        (journal, _) => journal,
    }
}
