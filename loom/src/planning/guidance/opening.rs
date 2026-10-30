//! Opening instructions and display names for planning sessions.

use std::path::{Path, PathBuf};

use anyhow::{anyhow, bail, Context, Result};

use super::start::AuthoringStart;
use super::{draft_run, PressureCommand, MAX_TEAMMATES};
use crate::planning::checklist::Offer;
use crate::planning::kinds::{plan_path, plans_dir, product_dir, section_path, vision_path};
use crate::planning::project::Project;

/// Render the generated opening with optional feature context.
pub fn opening_message(
    start: &AuthoringStart,
    project: &Project,
    feature: Option<&str>,
) -> Result<String> {
    let feature = start.feature(feature)?;
    let mut opening = authoring_opening(start, project, feature)?;
    if let Some(name) = feature.filter(|_| !opening.is_empty()) {
        opening.push_str(&format!(
            "\nFeature: {name}.\n{}",
            feature_checklist(project, name)
        ));
    }
    Ok(opening)
}

/// Describe the selected feature's six checklist marks at launch.
pub(super) fn feature_checklist(project: &Project, name: &str) -> String {
    let Some(feature) = project.feature(name) else {
        return "Checklist: absent (new feature).".into();
    };
    let steps = feature
        .checklist
        .steps
        .iter()
        .map(|(step, mark)| format!("{step:?}={mark:?}"))
        .collect::<Vec<_>>()
        .join(", ");
    format!("Checklist: {steps}.")
}

fn authoring_opening(
    start: &AuthoringStart,
    project: &Project,
    feature: Option<&str>,
) -> Result<String> {
    match start {
        AuthoringStart::Kickoff { small } => Ok(kickoff_opening(*small)),
        AuthoringStart::Offer(offer) => {
            let feature = feature.context("an authoring offer requires a feature")?;
            let (step, skill) = offer_step_and_skill(offer)?;
            Ok(three_lines(
                step,
                &offer_target(offer, project, feature)?,
                skill,
            ))
        }
        AuthoringStart::Refine { target, passage } => {
            let mut target = format!("Refine {}.", target.display());
            if *passage {
                target.push_str(" The message that follows attaches the passage.");
            }
            Ok(three_lines("any, Refine", &target, "refine"))
        }
        AuthoringStart::Knowledge { open_file } => {
            let mut target = "Change the knowledge base under doc/loom/knowledge/".to_string();
            if let Some(path) = open_file {
                target.push_str(&format!(", starting from {}", path.display()));
            }
            target.push('.');
            Ok(three_lines("any, Knowledge", &target, "knowledge"))
        }
        // The caller supplies the user's verbatim text; this start stores only paths.
        AuthoringStart::Message { .. } => Ok(String::new()),
        AuthoringStart::NoStep => {
            Ok("loom plan: no step chosen.\nAsk me what I want to work on.".into())
        }
    }
}

fn kickoff_opening(small: bool) -> String {
    let step = if small {
        "Kickoff, small work"
    } else {
        "Kickoff"
    };
    three_lines(
        step,
        "Discuss the work and confirm its size and name before writing anything under doc/.",
        "kickoff",
    )
}

fn three_lines(step: &str, target: &str, skill: &str) -> String {
    format!(
        "loom plan: {step}.\n{target}\nLoad the skill loom-plan:{skill}, then begin by \
         telling me what you read and what you plan to ask."
    )
}

fn offer_step_and_skill(offer: &Offer) -> Result<(&'static str, &'static str)> {
    AuthoringStart::from_offer(offer)?;
    Ok(match offer {
        Offer::StartVision { .. } | Offer::FixVisionTable { .. } => ("step 1, Vision", "vision"),
        Offer::AddSectionToVision { .. } => ("step 2, Product spec", "vision"),
        Offer::WriteSection { .. } => ("step 2, Product spec", "section"),
        Offer::DraftAllSections { .. } => ("step 2, Draft all missing sections", "section"),
        Offer::StartMetaplan { .. } => ("step 3, Metaplan", "metaplan"),
        Offer::WritePlan { .. } => ("step 4, Plans", "plan"),
        Offer::DraftAllPlans { .. } => ("step 4, Draft all missing plans", "plan"),
        Offer::RefineSpec => ("done, Compare spec with build", "retro"),
        Offer::NewMetaplanRound => ("done, New metaplan round", "metaplan"),
        _ => bail!("{} is not an authoring offer", offer.id()),
    })
}

fn offer_target(offer: &Offer, project: &Project, feature: &str) -> Result<String> {
    let product = product_dir(feature);
    let plans = plans_dir(Some(feature));
    let vision = vision_path(feature);
    Ok(match offer {
        Offer::StartVision { .. } => format!("Write {}, the product vision.", vision.display()),
        Offer::FixVisionTable { .. } => format!(
            "Fix the \"Specification sections\" table in {} so the app can read it.",
            vision.display()
        ),
        Offer::AddSectionToVision { number, .. } => format!(
            "Add {}/section_{number:02}.md to the \"Specification sections\" table in {}.",
            product.display(),
            vision.display()
        ),
        Offer::WriteSection { number, .. } => section_target(*number, offer, project, feature)?,
        Offer::StartMetaplan { .. } => {
            format!("Write {}/index.md, the first metaplan.", plans.display())
        }
        Offer::WritePlan { metaplan, slug, .. } => {
            plan_target(metaplan, slug, offer, project, feature)?
        }
        Offer::DraftAllSections { .. } | Offer::DraftAllPlans { .. } => {
            team_target(offer, project, feature)?
        }
        Offer::RefineSpec => format!(
            "Review the done plans and their run reviews in {}/ against the product spec in {}/.",
            plans.display(),
            product.display()
        ),
        Offer::NewMetaplanRound => format!(
            "Write {}/metaplan-{:02}.md, the next metaplan round.",
            plans.display(),
            next_round(project, feature)
        ),
        _ => bail!("{} is not an authoring offer", offer.id()),
    })
}

pub(super) fn plan_bundle(feature: &str, slug: &str) -> PathBuf {
    plans_dir(Some(feature)).join("briefs").join(slug)
}

fn section_target(number: u32, offer: &Offer, project: &Project, feature: &str) -> Result<String> {
    Ok(format!(
        "Write {}, listed in the vision as \"{}\".",
        section_path(feature, number).display(),
        section_row(number, offer, project, feature)?
    ))
}

fn plan_target(
    metaplan: &Path,
    slug: &str,
    offer: &Offer,
    project: &Project,
    feature: &str,
) -> Result<String> {
    Ok(format!(
        "Write {} for the entry \"{}\" in {}. Write its briefs under {}/.",
        plan_path(Some(feature), slug).display(),
        plan_heading(metaplan, slug, offer, project, feature)?,
        metaplan.display(),
        plan_bundle(feature, slug).display()
    ))
}

fn team_target(offer: &Offer, project: &Project, feature: &str) -> Result<String> {
    let run = draft_run(project, &offer.id(), feature)?;
    let documents = run
        .documents
        .iter()
        .map(|path| path.display().to_string())
        .collect::<Vec<_>>()
        .join(", ");
    let mut target = format!(
        "Draft these missing documents in reading order: {documents}. \
         Use the skill's team mode with {} teammates (cap: {MAX_TEAMMATES}).",
        run.teammates
    );
    if run.bundles.is_empty() {
        target.push_str(" Write nothing outside these documents.");
    } else {
        let bundles = run
            .bundles
            .iter()
            .map(|path| format!("{}/", path.display()))
            .collect::<Vec<_>>()
            .join(", ");
        target.push_str(&format!(
            " Brief bundles: {bundles}. Write nothing outside these documents and brief bundles."
        ));
    }
    Ok(target)
}

fn section_row(number: u32, offer: &Offer, project: &Project, feature: &str) -> Result<String> {
    let missing = || anyhow!("{}: missing vision table row", offer.id());
    let row = project
        .feature(feature)
        .and_then(|feature| feature.vision_rows.as_ref().ok())
        .and_then(|rows| rows.iter().find(|row| row.number == number))
        .ok_or_else(missing)?;
    let line = project
        .text(&vision_path(feature))
        .and_then(|text| text.lines().nth(row.line))
        .ok_or_else(missing)?;
    let trimmed = line.trim();
    let trimmed = trimmed.strip_prefix('|').unwrap_or(trimmed);
    let trimmed = trimmed.strip_suffix('|').unwrap_or(trimmed);
    let text = trimmed
        .split('|')
        .map(str::trim)
        .collect::<Vec<_>>()
        .join(" | ");
    let mut chars = text.chars();
    let mut snippet: String = chars.by_ref().take(120).collect();
    if chars.next().is_some() {
        snippet.pop();
        snippet.push('…');
    }
    Ok(snippet)
}

fn plan_heading<'a>(
    metaplan: &Path,
    slug: &str,
    offer: &Offer,
    project: &'a Project,
    feature: &str,
) -> Result<&'a str> {
    let entry = project
        .feature(feature)
        .into_iter()
        .flat_map(|feature| &feature.metaplans)
        .find(|(path, _)| path == metaplan)
        .and_then(|(_, entries)| {
            entries
                .iter()
                .find(|entry| entry.slug.as_deref() == Some(slug))
        })
        .with_context(|| format!("{}: missing entry in {}", offer.id(), metaplan.display()))?;
    // Parsed headings already omit Markdown markers; tolerate the prefix if present.
    let heading = entry
        .heading
        .trim()
        .strip_prefix("## ")
        .unwrap_or(entry.heading.trim());
    let suffix = entry.plan_file.as_ref().map(|file| format!(" (`{file}`)"));
    Ok(suffix
        .as_deref()
        .and_then(|suffix| heading.strip_suffix(suffix))
        .unwrap_or(heading))
}

fn next_round(project: &Project, feature: &str) -> u64 {
    let mut number = 2;
    while project
        .docs
        .contains_key(&plans_dir(Some(feature)).join(format!("metaplan-{number:02}.md")))
    {
        number += 1;
    }
    number
}

/// Render the summary rebuilding instruction, preserving the requested id order.
pub fn summary_opening_message(ids: &[String]) -> String {
    format!(
        "loom plan: rebuild summaries.\nRebuild: {}.\nLoad the skill loom-plan:summaries.",
        ids.join(", ")
    )
}

/// Invoke the pressure plugin command for the selected plan.
pub fn pressure_opening_message(cmd: PressureCommand, plan_rel: &str) -> String {
    let command = match cmd {
        PressureCommand::Pressure => "pressure",
        PressureCommand::Address => "address",
    };
    format!("/loom-plan:{command} {plan_rel}")
}

/// Name a session by its authoring skill or non-authoring task label.
pub fn display_name(kind_label: &str, start: Option<&AuthoringStart>) -> String {
    let skill = match start {
        Some(AuthoringStart::Kickoff { .. }) => "kickoff",
        Some(AuthoringStart::Offer(Offer::WriteSection { number, .. })) => {
            return format!("loom plan: section {number:02}");
        }
        Some(AuthoringStart::Offer(offer)) => offer_step_and_skill(offer)
            .map(|(_, skill)| skill)
            .unwrap_or("chat"),
        Some(AuthoringStart::Refine { .. } | AuthoringStart::Message { .. }) => "refine",
        Some(AuthoringStart::Knowledge { .. }) => "knowledge",
        Some(AuthoringStart::NoStep) => "chat",
        None => kind_label,
    };
    format!("loom plan: {skill}")
}

#[cfg(test)]
mod tests;
