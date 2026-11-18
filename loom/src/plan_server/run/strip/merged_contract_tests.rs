//! Contract for the run strip's merged count: it follows each stage's `merged` flag, as the
//! dashboard does, whatever the commit ancestry says.

use super::*;
use crate::models::stage::{Stage, StageStatus, StageType};
use crate::verify::transitions::serialize_stage_to_markdown;
use std::fs;
use tempfile::TempDir;

const PLAN: &str = "doc/plans/IN_PROGRESS-PLAN-strip.md";

/// A Completed Standard stage with no `completed_commit`, in a work dir with no git history.
fn write_stage(work_dir: &WorkDir, n: usize, merged: bool) {
    let id = format!("stage-{n}");
    let stage = Stage {
        id: id.clone(),
        name: id.clone(),
        status: StageStatus::Completed,
        stage_type: StageType::Standard,
        completed_commit: None,
        merged,
        ..Stage::default()
    };
    let content = serialize_stage_to_markdown(&stage).unwrap();
    let file = work_dir.stages_dir().join(format!("{n:02}-{id}.md"));
    fs::write(file, content).unwrap();
}

#[test]
fn all_merged_stages_count_as_merged() {
    let temp = TempDir::new().unwrap();
    let work_dir = WorkDir::new(temp.path()).unwrap();
    work_dir.initialize().unwrap();
    fs::write(
        work_dir.config_path(),
        format!("[plan]\nsource_path = \"{PLAN}\"\nbase_branch = \"main\"\n"),
    )
    .unwrap();
    for n in 1..=7 {
        write_stage(&work_dir, n, n != 7);
    }

    let strip = build_strip(&work_dir, None).unwrap();

    assert_eq!(strip.total, 7, "{strip:?}");
    assert_eq!(strip.merged, 6, "the merged flag decides: {strip:?}");
}
