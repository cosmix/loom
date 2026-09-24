//! `loom stage review integrity`: a stage's current test-integrity events with
//! their detail, and which of them `integrity.json` accepts (DESIGN D13).

use anyhow::Result;

use crate::verify::integrity;
use crate::verify::review::report::single_line;
use crate::verify::transitions::load_stage;

use super::review_status::stage_worktree_and_target;

/// `loom stage review integrity <stage-id>`.
pub fn review_integrity(stage_id: String) -> Result<()> {
    let work_dir = crate::commands::common::work_dir_path()?;
    let stage = load_stage(&stage_id, &work_dir)?;
    let (worktree, target) = stage_worktree_and_target(&work_dir, &stage)?;
    let scan = integrity::scan(&worktree, &target, &stage.ratchet_files)?;
    let accepted = integrity::load_accepted(&work_dir, &stage_id)?;

    if scan.events.is_empty() {
        println!("No test-integrity event for stage '{stage_id}' against {target}.");
    } else {
        println!("Test-integrity events of stage '{stage_id}' against {target}:");
    }
    for event in &scan.events {
        let status = integrity::shortfall(event, &accepted).unwrap_or_else(|| "accepted".into());
        println!("  {}: {status}", integrity::describe(event));
        for line in &event.detail {
            println!("      - {}", single_line(line));
        }
    }
    for path in &scan.unprofiled {
        println!(
            "Not counted, no language profile covers it: {}",
            single_line(path)
        );
    }
    Ok(())
}
