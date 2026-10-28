//! Session briefs and the project facts available when authoring starts.

use std::path::{Path, PathBuf};

use anyhow::{Context, Result};

use super::opening;
use super::start::AuthoringStart;
use crate::planning::checklist::ProjectState;
use crate::planning::project::Project;
use crate::planning::slug::PlanState;

pub(super) fn common_brief(start: &AuthoringStart, project: &Project) -> Result<String> {
    Ok(format!(
        "{}\n\n## Starting point\n\n{}\n{}",
        include_str!("text/common_brief.md"),
        starting_point(start, project)?,
        project_state(&project.checklist.state),
    ))
}

pub(super) fn summary_brief() -> &'static str {
    include_str!("text/summary_brief.md")
}

fn starting_point(start: &AuthoringStart, project: &Project) -> Result<String> {
    // Message starts carry paths only; their target is the user's verbatim text.
    if matches!(start, AuthoringStart::Message { .. }) {
        return Ok("Step: any, Refine.\nSkill: loom-plan:refine.\n".into());
    }
    let opening = opening::opening_message(start, project)?;
    let mut lines = opening.lines();
    let label = lines
        .next()
        .and_then(|line| line.strip_prefix("loom plan: "))
        .context("authoring opening is missing its step label")?;
    let step = if matches!(start, AuthoringStart::NoStep) {
        "none chosen."
    } else {
        label.strip_prefix("step ").unwrap_or(label)
    };
    let target = lines
        .next()
        .context("authoring opening is missing its target")?;
    let mut block = format!("Step: {step}\nTarget: {target}\n");
    if !matches!(start, AuthoringStart::NoStep) {
        let skill = lines
            .next()
            .and_then(|line| line.strip_prefix("Load the skill "))
            .and_then(|line| line.split_once(','))
            .map(|(skill, _)| skill)
            .context("authoring opening is missing its skill")?;
        block.push_str(&format!("Skill: {skill}.\n"));
    }
    Ok(block)
}

fn project_state(state: &ProjectState) -> String {
    let count = |kind| state.plans_by_state.get(&kind).copied().unwrap_or(0);
    let locked = state
        .locked_files
        .iter()
        .filter(|path| is_locked_plan(path));
    format!(
        "Project state at launch (it may change; re-read the files before relying on it):\n\
         {}\n\
         - Metaplans: {}.\n\
         - Plans: {} written ({} unlisted), {} running, {} done, {} retired.\n\
         - Locked plan files: {}.\n",
        vision_state(state),
        path_list(state.metaplans.iter()),
        count(PlanState::Written),
        state.plans_unlisted,
        count(PlanState::Running),
        count(PlanState::Done),
        count(PlanState::Retired),
        path_list(locked),
    )
}

fn vision_state(state: &ProjectState) -> String {
    if !state.vision_present {
        return "- Vision: absent.".into();
    }
    let written = state
        .sections_written
        .iter()
        .map(|number| format!("{number:02}"))
        .collect::<Vec<_>>()
        .join(", ");
    format!(
        "- Vision: present, {} sections listed, {} written ({}).",
        state.sections_listed,
        state.sections_written.len(),
        written,
    )
}

fn is_locked_plan(path: &Path) -> bool {
    path.file_name()
        .and_then(|name| name.to_str())
        .is_some_and(|name| {
            ["IN_PROGRESS-PLAN-", "DONE-PLAN-", "RETIRED-PLAN-"]
                .iter()
                .any(|prefix| name.starts_with(prefix))
        })
}

fn path_list<'a>(paths: impl Iterator<Item = &'a PathBuf>) -> String {
    let mut paths: Vec<_> = paths.collect();
    paths.sort_unstable();
    if paths.is_empty() {
        return "none".into();
    }
    paths
        .iter()
        .map(|path| path.display().to_string())
        .collect::<Vec<_>>()
        .join(", ")
}
