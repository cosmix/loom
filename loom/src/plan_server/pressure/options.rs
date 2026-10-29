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
    disabled_reason, PressureOptions, PressureSlots, PressureStart,
};
use crate::planning::guidance::StartBlocks;
use crate::planning::project::Project;
use crate::planning::runtime::{server_runtime_paths, RuntimePaths};
use crate::planning::slug::PlanState;
use crate::pressure::models::resolve_for;

/// Install once in `PlanContext::extensions`; the driver sets this from acceptance
/// through cleanup, including the gaps between pressure sessions.
#[derive(Default)]
pub struct PressureRunning(pub AtomicBool);

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
        rounds: 2,
        claude_model: models.claude,
        claude_effort: models.claude_effort,
        codex_model: models.codex,
        codex_effort: models.codex_effort,
        address_model: models.address,
        address_effort: models.address_effort,
    }
}

fn written_plans(project: &Project) -> Vec<String> {
    let listed = project.metaplans.iter().flat_map(|(_, entries)| {
        entries
            .iter()
            .filter_map(|entry| project.tree.plans.get(entry.slug.as_deref()?))
    });
    let mut seen = BTreeSet::new();
    // Written winners all have the same PLAN- prefix, so slug order is filename order.
    listed
        .chain(project.tree.plans.values())
        .filter(|plan| plan.state == PlanState::Written)
        .filter(|plan| plan.path.parent() == Some(Path::new("doc/plans")))
        .filter(|plan| {
            project
                .docs
                .get(&plan.path)
                .is_some_and(|doc| doc.lock.is_none())
        })
        .filter(|plan| seen.insert(plan.path.clone()))
        .map(|plan| plan.path.to_string_lossy().into_owned())
        .collect()
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
