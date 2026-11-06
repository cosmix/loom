//! Pressure availability, dialog defaults and start-request validation.

use std::collections::BTreeSet;
use std::path::Path;
use std::sync::atomic::{AtomicBool, Ordering};

use crate::claude::CLAUDE_MODELS;
use crate::cli::types_pressure::PressureModelFlags;
use crate::codex::{CODEX_EFFORTS, CODEX_MODELS};
use crate::models::stage::ALLOWED_REASONING_EFFORTS;
use crate::plan_server::context::PlanContext;
use crate::plan_server::ids::BrowserId;
use crate::plan_server::wire::pressure::{
    disabled_reason, PressureConfigBlock, PressureOptions, PressureSlots, PressureStart,
};
use crate::planning::guidance::StartBlocks;
use crate::planning::kinds::plans_dir;
use crate::planning::project::Project;
use crate::planning::runtime::{server_runtime_paths, RuntimePaths};
use crate::planning::slug::{PlanEntry, PlanState};
use crate::pressure::models::resolve_for;

/// Install once in `PlanContext::extensions`; the driver sets this from acceptance
/// through cleanup, including the gaps between pressure sessions.
#[derive(Default)]
pub struct PressureRunning(pub AtomicBool);

/// Pressure availability and defaults for the requesting browser.
pub fn options(ctx: &PlanContext, browser: &BrowserId) -> PressureOptions {
    options_with_prerequisites(
        ctx,
        browser,
        crate::codex::find_codex_path().is_ok(),
        crate::pressure::codex_run::codex_skill_installed(),
    )
}

fn options_with_prerequisites(
    ctx: &PlanContext,
    browser: &BrowserId,
    codex: bool,
    skill: bool,
) -> PressureOptions {
    let running = ctx
        .extensions()
        .get::<PressureRunning>()
        .is_some_and(|state| state.0.load(Ordering::SeqCst));
    let other_holder = ctx
        .authoring()
        .holder_browser()
        .is_some_and(|holder| &holder != browser);
    let reason = first_disabled_reason(
        codex,
        skill,
        ctx.read_only().is_read_only(),
        other_holder,
        running,
    );
    PressureOptions {
        available: reason.is_none(),
        disabled_reason: reason.map(str::to_owned),
        plans: written_plans(&ctx.project()),
        defaults: default_slots(ctx.root()),
        claude_models: strings(CLAUDE_MODELS),
        claude_efforts: strings(ALLOWED_REASONING_EFFORTS),
        codex_models: strings(CODEX_MODELS),
        codex_efforts: strings(CODEX_EFFORTS),
        running,
        config_block: config_block(ctx.runtime()),
    }
}

/// The persisted block every planning launch waits on; an unreadable block fails closed.
pub fn config_block(runtime: &RuntimePaths) -> Option<PressureConfigBlock> {
    match StartBlocks::current(runtime) {
        Ok(None) => None,
        Ok(Some(block)) => Some(PressureConfigBlock {
            reason: block.reason,
            changed: block.changed,
        }),
        Err(error) => Some(PressureConfigBlock {
            reason: format!("start blocked: config-block unreadable: {error:#}"),
            changed: Vec::new(),
        }),
    }
}

fn strings(values: &[&str]) -> Vec<String> {
    values.iter().map(|value| (*value).to_owned()).collect()
}

fn first_disabled_reason(
    codex: bool,
    skill: bool,
    read_only: bool,
    other_holder: bool,
    running: bool,
) -> Option<&'static str> {
    [
        (!codex, disabled_reason::CODEX_MISSING),
        (!skill, disabled_reason::CODEX_SKILL_MISSING),
        (read_only, disabled_reason::READ_ONLY),
        (other_holder, disabled_reason::NOT_HOLDER),
        (running, disabled_reason::RUNNING),
    ]
    .into_iter()
    .find_map(|(disabled, reason)| disabled.then_some(reason))
}

fn default_slots(root: &Path) -> PressureSlots {
    let models = resolve_for(root, PressureModelFlags::default());
    PressureSlots {
        rounds: 1,
        claude_model: models.claude,
        claude_effort: models.claude_effort,
        codex_model: models.codex,
        codex_effort: models.codex_effort,
        address_model: models.address,
        address_effort: models.address_effort,
    }
}

fn written_plans(project: &Project) -> Vec<String> {
    let features = project.features.iter().flat_map(|feature| {
        let listed = feature.metaplans.iter().flat_map(move |(_, entries)| {
            entries.iter().filter_map(move |entry| {
                project
                    .tree
                    .plan(entry.slug.as_deref()?)
                    .filter(|plan| plan.feature.as_deref() == Some(feature.name.as_str()))
            })
        });
        listed.chain(by_filename(project.tree.plans.values().filter(
            move |plan| plan.feature.as_deref() == Some(feature.name.as_str()),
        )))
    });
    let standalone = by_filename(
        project
            .standalone
            .iter()
            .filter_map(|slug| project.tree.plan(slug)),
    );
    let mut seen = BTreeSet::new();
    features
        .chain(standalone)
        .filter(|plan| plan.state == PlanState::Written)
        .filter(|plan| plan.path.parent() == Some(plans_dir(plan.feature.as_deref()).as_path()))
        .filter(|plan| {
            project
                .docs
                .get(&plan.path)
                .is_some_and(|doc| doc.lock.is_none())
        })
        .filter(|plan| seen.insert(plan.slug.clone()))
        .map(|plan| plan.path.to_string_lossy().into_owned())
        .collect()
}

fn by_filename<'a>(plans: impl Iterator<Item = &'a PlanEntry>) -> Vec<&'a PlanEntry> {
    let mut plans: Vec<_> = plans.collect();
    plans.sort_by(|a, b| {
        a.path
            .file_name()
            .cmp(&b.path.file_name())
            .then(a.path.cmp(&b.path))
    });
    plans
}

/// Return a stable route error code. Host prerequisites and authoring belong to
/// the route gate; this validates the selected plan, slots and persistent block.
pub fn validate_start(project: &Project, start: &PressureStart) -> Result<(), String> {
    if !written_plans(project).contains(&start.plan) {
        return Err("plan_not_written".into());
    }
    validate_slots(&start.slots)?;
    let (runtime, _) =
        server_runtime_paths(&project.root).map_err(|_| "config_blocked".to_owned())?;
    validate_config(&runtime)
}

fn validate_slots(slots: &PressureSlots) -> Result<(), String> {
    let valid = [
        (slots.claude_model.as_str(), CLAUDE_MODELS),
        (slots.claude_effort.as_str(), ALLOWED_REASONING_EFFORTS),
        (slots.codex_model.as_str(), CODEX_MODELS),
        (slots.codex_effort.as_str(), CODEX_EFFORTS),
        (slots.address_model.as_str(), CLAUDE_MODELS),
        (slots.address_effort.as_str(), ALLOWED_REASONING_EFFORTS),
    ]
    .into_iter()
    .all(|(value, allowed)| allowed.contains(&value));
    if !valid {
        return Err("invalid_model".into());
    }
    if !(1..=10).contains(&slots.rounds) {
        return Err("invalid_rounds".into());
    }
    Ok(())
}

fn validate_config(runtime: &RuntimePaths) -> Result<(), String> {
    match StartBlocks::current(runtime) {
        Ok(None) => Ok(()),
        Ok(Some(_)) | Err(_) => Err("config_blocked".into()),
    }
}

#[cfg(test)]
mod tests;

#[cfg(test)]
mod feature_tests {
    use super::*;

    #[test]
    fn pressure_lists_feature_and_standalone_plans_with_scoped_metaplan_order() {
        let root = tempfile::tempdir().unwrap();
        for (relative, contents) in [
            (
                "doc/plans/beta/index.md",
                "# Beta\n\n## First `PLAN-b.md`\n",
            ),
            (
                "doc/plans/alpha/index.md",
                "# Alpha\n\n## Foreign `PLAN-b.md`\n\n## First `PLAN-z.md`\n\n## Standalone `PLAN-00-loose.md`\n",
            ),
            ("doc/plans/alpha/PLAN-z.md", "# Z\n"),
            ("doc/plans/alpha/PLAN-a.md", "# A\n"),
            ("doc/plans/beta/PLAN-b.md", "# B\n"),
            ("doc/plans/beta/PLAN-0-beta.md", "# Unlisted beta\n"),
            ("doc/plans/PLAN-00-loose.md", "# Later standalone\n"),
            ("doc/plans/PLAN-00-loose-extra.md", "# First standalone\n"),
            ("doc/plans/alpha/IN_PROGRESS-PLAN-running.md", "# Running\n"),
            ("doc/plans/alpha/PLAN-running.md", "# Stray\n"),
            ("doc/plans/alpha/archive/PLAN-archived.md", "# Archived\n"),
        ] {
            let path = root.path().join(relative);
            std::fs::create_dir_all(path.parent().unwrap()).unwrap();
            std::fs::write(path, contents).unwrap();
        }
        let project = Project::load(root.path()).unwrap();

        assert_eq!(
            written_plans(&project),
            [
                "doc/plans/alpha/PLAN-z.md",
                "doc/plans/alpha/PLAN-a.md",
                "doc/plans/beta/PLAN-b.md",
                "doc/plans/beta/PLAN-0-beta.md",
                "doc/plans/PLAN-00-loose-extra.md",
                "doc/plans/PLAN-00-loose.md",
            ]
        );
    }
}
