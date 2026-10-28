//! Authoring entry points, their visible labels and model-selection classes.

use std::path::PathBuf;

use crate::planning::checklist::Offer;
use crate::planning::paths::check_relpath_syntax;
use crate::planning::project::Project;

/// The action that starts an authoring session.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AuthoringStart {
    Offer(Offer),
    Refine {
        target: PathBuf,
        passage: bool,
    },
    Knowledge {
        open_file: Option<PathBuf>,
    },
    /// The user's opening message supplies the text and its parsed attachment paths.
    Message {
        paths: Vec<PathBuf>,
    },
    NoStep,
}

impl AuthoringStart {
    /// Pressure testing and running plans have their own session flows.
    pub fn from_offer(offer: &Offer) -> Option<AuthoringStart> {
        match offer {
            Offer::PressureTest { .. } | Offer::StartRun => None,
            _ => Some(Self::Offer(offer.clone())),
        }
    }

    /// Look up a current checklist offer, or a document start relative to `doc/`.
    /// Document starts must name a document in the loaded project.
    pub fn from_offer_id(id: &str, project: &Project) -> Option<AuthoringStart> {
        if let Some(offer) = project
            .checklist
            .primary
            .iter()
            .chain(project.checklist.secondary.iter())
            .find(|offer| offer.id() == id)
        {
            return Self::from_offer(offer);
        }
        if id == "knowledge" {
            return Some(Self::Knowledge { open_file: None });
        }
        let (kind, raw) = id.split_once(':')?;
        if !matches!(kind, "refine" | "refine-passage" | "knowledge") {
            return None;
        }
        let target = check_relpath_syntax(raw).ok()?;
        if !project.docs.contains_key(&target) {
            return None;
        }
        match kind {
            "knowledge" => Some(Self::Knowledge {
                open_file: Some(target),
            }),
            _ => Some(Self::Refine {
                target,
                passage: kind == "refine-passage",
            }),
        }
    }

    /// The action label displayed on the opening message.
    pub fn label(&self, project: &Project) -> String {
        match self {
            Self::Offer(offer) => offer_label(offer, project),
            Self::Refine { target, .. } => format!("Refine {}", target.display()),
            Self::Knowledge {
                open_file: Some(path),
            } => {
                format!(
                    "Change the knowledge base, starting from {}",
                    path.display()
                )
            }
            Self::Knowledge { open_file: None } => "Change the knowledge base".into(),
            Self::Message { .. } | Self::NoStep => "New chat".into(),
        }
    }
}

fn offer_label(offer: &Offer, project: &Project) -> String {
    match offer {
        Offer::StartVision => "Start a vision discussion".into(),
        Offer::FixVisionTable => "Fix the vision's section list".into(),
        Offer::AddSectionToVision { number } => format!("Add section {number:02} to vision"),
        Offer::WriteSection { number } => {
            let label = format!("Write section {number:02}");
            project
                .vision_rows
                .as_ref()
                .ok()
                .and_then(|rows| rows.iter().find(|row| row.number == *number))
                .map_or_else(|| label.clone(), |row| format!("{label} — {}", row.name))
        }
        Offer::StartMetaplan => "Start a metaplan".into(),
        Offer::WritePlan { slug, .. } => format!("Write PLAN-{slug}.md"),
        Offer::AddPlanToMetaplan { slug } => format!("Add PLAN-{slug}.md to a metaplan"),
        Offer::RefineSpec => "Review what was built".into(),
        Offer::NewMetaplanRound => "Start a new metaplan round".into(),
        Offer::PressureTest { slug } => format!("Pressure-test PLAN-{slug}.md"),
        Offer::StartRun => "Start run".into(),
    }
}

/// Which authoring model and effort pair a start uses.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StepClass {
    Spec,
    Plan,
}

/// Classify starts by the step, document, or first message attachment.
pub fn step_class(start: &AuthoringStart) -> StepClass {
    match start {
        AuthoringStart::Offer(offer) => match offer {
            Offer::StartVision
            | Offer::FixVisionTable
            | Offer::AddSectionToVision { .. }
            | Offer::WriteSection { .. }
            | Offer::RefineSpec => StepClass::Spec,
            Offer::StartMetaplan
            | Offer::WritePlan { .. }
            | Offer::AddPlanToMetaplan { .. }
            | Offer::NewMetaplanRound
            | Offer::PressureTest { .. }
            | Offer::StartRun => StepClass::Plan,
        },
        AuthoringStart::Refine { target, .. } => {
            if target.starts_with("doc/product") {
                StepClass::Spec
            } else {
                StepClass::Plan
            }
        }
        AuthoringStart::Message { paths } => {
            if paths
                .first()
                .is_some_and(|path| path.starts_with("doc/plans"))
            {
                StepClass::Plan
            } else {
                StepClass::Spec
            }
        }
        AuthoringStart::Knowledge { .. } | AuthoringStart::NoStep => StepClass::Spec,
    }
}

#[cfg(test)]
mod tests;
