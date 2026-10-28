//! Opening instructions and display names for planning sessions.

use std::path::Path;

use anyhow::{anyhow, bail, Context, Result};

use super::start::AuthoringStart;
use super::PressureCommand;
use crate::planning::checklist::Offer;
use crate::planning::kinds::VISION_PATH;
use crate::planning::project::Project;

/// Render the generated opening for an authoring start.
pub fn opening_message(start: &AuthoringStart, project: &Project) -> Result<String> {
    match start {
        AuthoringStart::Offer(offer) => {
            let (step, skill) = offer_step_and_skill(offer)?;
            Ok(three_lines(step, &offer_target(offer, project)?, skill))
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

fn three_lines(step: &str, target: &str, skill: &str) -> String {
    format!(
        "loom plan: {step}.\n{target}\nLoad the skill loom-plan:{skill}, then begin by \
         telling me what you read and what you plan to ask."
    )
}

fn offer_step_and_skill(offer: &Offer) -> Result<(&'static str, &'static str)> {
    Ok(match offer {
        Offer::StartVision | Offer::FixVisionTable => ("step 1, Vision", "vision"),
        Offer::AddSectionToVision { .. } => ("step 2, Product spec", "vision"),
        Offer::WriteSection { .. } => ("step 2, Product spec", "section"),
        Offer::StartMetaplan => ("step 3, Metaplan", "metaplan"),
        Offer::WritePlan { .. } => ("step 4, Plans", "plan"),
        Offer::AddPlanToMetaplan { .. } => ("step 4, Plans", "metaplan"),
        Offer::RefineSpec => ("done, Review what was built", "retro"),
        Offer::NewMetaplanRound => ("done, New metaplan round", "metaplan"),
        Offer::PressureTest { .. } | Offer::StartRun => {
            bail!("{} is not an authoring offer", offer.id());
        }
    })
}

fn offer_target(offer: &Offer, project: &Project) -> Result<String> {
    Ok(match offer {
        Offer::StartVision => "Write doc/product/index.md, the product vision.".into(),
        Offer::FixVisionTable => "Fix the \"Specification sections\" table in \
             doc/product/index.md so the app can read it."
            .into(),
        Offer::AddSectionToVision { number } => format!(
            "Add doc/product/section_{number:02}.md to the \"Specification sections\" \
             table in doc/product/index.md."
        ),
        Offer::WriteSection { number } => format!(
            "Write doc/product/section_{number:02}.md, listed in the vision as \"{}\".",
            section_row(*number, offer, project)?
        ),
        Offer::StartMetaplan => "Write doc/plans/index.md, the first metaplan.".into(),
        Offer::WritePlan { metaplan, slug } => format!(
            "Write doc/plans/PLAN-{slug}.md for the entry \"{}\" in {}.",
            plan_heading(metaplan, slug, offer, project)?,
            metaplan.display()
        ),
        Offer::AddPlanToMetaplan { slug } => {
            format!("Add an entry for PLAN-{slug}.md to a metaplan in doc/plans/.")
        }
        Offer::RefineSpec => {
            "Review the done plans and their run reviews against the product spec.".into()
        }
        Offer::NewMetaplanRound => format!(
            "Write doc/plans/metaplan-{:02}.md, the next metaplan round.",
            next_round(project)
        ),
        Offer::PressureTest { .. } | Offer::StartRun => {
            bail!("{} is not an authoring offer", offer.id());
        }
    })
}

fn section_row(number: u32, offer: &Offer, project: &Project) -> Result<String> {
    let missing = || anyhow!("{}: missing vision table row", offer.id());
    let row = project
        .vision_rows
        .as_ref()
        .ok()
        .and_then(|rows| rows.iter().find(|row| row.number == number))
        .ok_or_else(missing)?;
    let line = project
        .text(Path::new(VISION_PATH))
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
) -> Result<&'a str> {
    let entry = project
        .metaplans
        .iter()
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

fn next_round(project: &Project) -> u64 {
    let mut number = 2;
    while project
        .docs
        .contains_key(Path::new(&format!("doc/plans/metaplan-{number:02}.md")))
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
        Some(AuthoringStart::Offer(Offer::WriteSection { number })) => {
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
