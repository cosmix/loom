use crate::fs::work_dir::WorkDir;
use crate::handoff::{find_continuation_handoff, load_handoff_content};
use crate::models::stage::StageStatus;
use crate::verify::transitions::{load_stage, update_stage};
use anyhow::{bail, Result};
use std::io::{stdin, stdout, Write};
use std::path::Path;

/// Queue continuation for the orchestrator's write-ahead spawn path. Preserve
/// the predecessor id so it can select the exact handoff and refuse a second
/// writer while the predecessor remains alive.
fn queue_for_continuation(stage_id: &str, work_dir: &Path) -> Result<()> {
    update_stage(stage_id, work_dir, |current| current.try_mark_queued())?;
    Ok(())
}

/// Resume failed/blocked stages with handoff context
/// Usage: loom resume <stage_id>
pub fn execute(stage_id: String) -> Result<()> {
    let work_dir = WorkDir::new(".")?;
    work_dir.load()?;
    let stage = load_stage(&stage_id, work_dir.root())?;

    if !matches!(
        stage.status,
        StageStatus::Blocked | StageStatus::NeedsHandoff
    ) {
        bail!(
            "Stage '{}' has status {:?}. Can only resume stages with status Blocked or NeedsHandoff.",
            stage_id,
            stage.status
        );
    }

    println!("Stage: {} (status: {:?})", stage.name, stage.status);

    let handoff_path =
        find_continuation_handoff(&stage_id, stage.session.as_deref(), work_dir.root())?;

    if let Some(ref path) = handoff_path {
        println!("\nLatest handoff: {}", path.display());
        let content = load_handoff_content(path)?;
        let lines: Vec<&str> = content.lines().take(20).collect();
        println!("\nHandoff summary (first 20 lines):");
        println!("---");
        for line in lines {
            println!("{line}");
        }
        println!("---");
    } else {
        println!("\nNo handoff found for this stage.");
    }

    print!("\nResume this stage? (y/n): ");
    stdout().flush()?;

    let mut response = String::new();
    stdin().read_line(&mut response)?;

    if !response.trim().eq_ignore_ascii_case("y") {
        println!("Resume cancelled.");
        return Ok(());
    }

    queue_for_continuation(&stage_id, work_dir.root())?;
    println!("\n✓ Stage queued for safe continuation.");
    println!(
        "Run `loom run` to let the orchestrator verify the predecessor and spawn its successor."
    );
    if let Some(ref path) = handoff_path {
        println!("Handoff context: {}", path.display());
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::stage::Stage;
    use crate::verify::transitions::create_stage;

    #[test]
    fn queuing_preserves_the_predecessor_and_spawns_nothing() {
        let temp = tempfile::TempDir::new().unwrap();
        let mut stage = Stage::new("resume".to_string(), None);
        stage.id = "resume-stage".to_string();
        stage.status = StageStatus::NeedsHandoff;
        stage.session = Some("session-old".to_string());
        create_stage(&stage, temp.path()).unwrap();

        queue_for_continuation(&stage.id, temp.path()).unwrap();

        let queued = load_stage(&stage.id, temp.path()).unwrap();
        assert_eq!(queued.status, StageStatus::Queued);
        assert_eq!(queued.session.as_deref(), Some("session-old"));
        assert!(!temp.path().join("sessions").exists());
    }
}
