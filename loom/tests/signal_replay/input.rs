//! Builds the replay input: the frozen fixture copied into a scratch root and
//! its plan loaded as stage records.

use std::fs;
use std::path::{Path, PathBuf};

use anyhow::{bail, Context, Result};
use loom::models::stage::{PlanIdentity, Stage};
use loom::plan::parser::parse_plan;
use loom::verify::transitions::create_stage;

pub const PLAN_ID: &str = "PLAN-token-flow";

/// Frozen fixture directory.
pub fn fixture_dir() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/signal_replay")
}

/// Repository root, the parent of the crate.
pub fn repo_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("..")
}

fn copy_dir_files(from: &Path, to: &Path, extension: &str) -> Result<()> {
    fs::create_dir_all(to).with_context(|| format!("create {}", to.display()))?;
    for entry in fs::read_dir(from).with_context(|| format!("read {}", from.display()))? {
        let path = entry?.path();
        if path.extension().and_then(|e| e.to_str()) == Some(extension) {
            let name = path.file_name().context("fixture file name")?;
            fs::copy(&path, to.join(name)).with_context(|| format!("copy {}", path.display()))?;
        }
    }
    Ok(())
}

/// Copy the fixture into `root`, write the plan config and create every stage
/// record. Returns the work directory and the stages in render order
/// (dependency level, then id).
pub fn build(root: &Path) -> Result<(PathBuf, Vec<Stage>)> {
    let fixture = fixture_dir();
    let work_dir = root.join(".loom/work");
    copy_dir_files(
        &fixture.join("knowledge"),
        &root.join("doc/loom/knowledge"),
        "md",
    )?;
    copy_dir_files(&fixture.join("memory"), &work_dir.join("memory"), "md")?;

    let plan_path = root.join("doc/plans").join(format!("{PLAN_ID}.md"));
    fs::create_dir_all(plan_path.parent().context("plan directory")?)?;
    fs::copy(fixture.join("plan.md"), &plan_path).context("copy the frozen plan")?;
    fs::write(
        work_dir.join("config.toml"),
        format!(
            "[plan]\nsource_path = \"{}\"\nplan_id = \"{PLAN_ID}\"\nplan_name = \"Live token flow in loom status\"\nbase_branch = \"main\"\n",
            plan_path.display()
        ),
    )
    .context("write the plan config")?;

    // The frozen plan must parse as it is; the fixture is never edited to fit.
    let parsed = parse_plan(&plan_path).context("the frozen plan no longer parses")?;
    if parsed.id != PLAN_ID {
        bail!("frozen plan id is {}, expected {PLAN_ID}", parsed.id);
    }
    let identity = PlanIdentity::from(&parsed);
    let mut stages = Vec::new();
    for definition in &parsed.stages {
        let stage = Stage::from_definition(definition, &identity);
        create_stage(&stage, &work_dir).with_context(|| format!("create stage {}", stage.id))?;
        stages.push(stage);
    }
    order_by_level(&mut stages);
    Ok((work_dir, stages))
}

fn level(stages: &[Stage], id: &str, depth: usize) -> usize {
    // The depth bound turns a dependency cycle into a bounded answer.
    if depth > stages.len() {
        return depth;
    }
    stages
        .iter()
        .find(|s| s.id == id)
        .map(|s| {
            s.dependencies
                .iter()
                .map(|d| level(stages, d, depth + 1) + 1)
                .max()
                .unwrap_or(0)
        })
        .unwrap_or(0)
}

fn order_by_level(stages: &mut Vec<Stage>) {
    let keys: Vec<(usize, String)> = stages
        .iter()
        .map(|s| (level(stages, &s.id, 0), s.id.clone()))
        .collect();
    let mut paired: Vec<_> = keys.into_iter().zip(stages.drain(..)).collect();
    paired.sort_by(|a, b| a.0.cmp(&b.0));
    stages.extend(paired.into_iter().map(|(_, stage)| stage));
}
