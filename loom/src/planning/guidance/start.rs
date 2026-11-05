//! Authoring entry points, their visible labels and model-selection classes.

use std::path::PathBuf;

use crate::planning::checklist::Offer;
use crate::planning::kinds::is_feature_name;
use crate::planning::paths::check_relpath_syntax;
use crate::planning::project::Project;

mod error;
pub use error::StartError;

/// The action that starts an authoring session.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AuthoringStart {
    /// Shape new work before confirming its size and name.
    Kickoff {
        /// Whether to begin with a standalone plan in mind.
        small: bool,
    },
    /// A feature checklist action.
    Offer(Offer),
    /// Refine a loaded document or an attached passage from it.
    Refine {
        /// The repository-relative document path.
        target: PathBuf,
        /// Whether the next message carries a selected passage.
        passage: bool,
    },
    /// Discuss changes to the knowledge base.
    Knowledge {
        /// An optional document providing context.
        open_file: Option<PathBuf>,
    },
    /// The user's opening message supplies the text and its parsed attachment paths.
    Message {
        /// Repository-relative attachments, in message order.
        paths: Vec<PathBuf>,
    },
    /// Begin a discussion without choosing a step.
    NoStep,
}

impl AuthoringStart {
    /// Convert an offer, refusing file moves and separate execution flows.
    pub fn from_offer(offer: &Offer) -> Result<AuthoringStart, StartError> {
        match offer {
            Offer::MoveIntoFeature { .. } => Err(StartError::FileMove(offer.id())),
            Offer::PressureTest { .. } | Offer::StartRun => {
                Err(StartError::NotAuthoring(offer.id()))
            }
            _ => Ok(Self::Offer(offer.clone())),
        }
    }

    /// Look up an offer in the requested feature or a loaded document relative to `doc/`.
    /// `None` selects no feature checklist; offer ids are not unique across features.
    pub fn from_offer_id(
        id: &str,
        project: &Project,
        feature: Option<&str>,
    ) -> Result<AuthoringStart, StartError> {
        validate_feature(feature)?;
        match id {
            "kickoff" => return Ok(Self::Kickoff { small: false }),
            "kickoff-small" => return Ok(Self::Kickoff { small: true }),
            _ => {}
        }
        let checklist = feature.and_then(|name| project.feature(name));
        let offers = checklist.into_iter().flat_map(|feature| {
            feature
                .checklist
                .primary
                .iter()
                .chain(&feature.checklist.secondary)
        });
        if let Some(offer) = offers.chain(&project.moves).find(|offer| offer.id() == id) {
            return Self::from_offer(offer);
        }
        document_start(id, project).ok_or_else(|| StartError::NotOffered(id.into()))
    }

    /// Validate the requested scope and prevent a start from crossing feature boundaries.
    pub(super) fn feature<'a>(
        &self,
        requested: Option<&'a str>,
    ) -> Result<Option<&'a str>, StartError> {
        validate_feature(requested)?;
        if matches!(self, Self::Kickoff { .. }) {
            return Ok(None);
        }
        if let Self::Offer(offer) = self {
            Self::from_offer(offer)?;
            let selected = requested.ok_or(StartError::FeatureRequired)?;
            if offer.feature().is_some_and(|owner| owner != selected) {
                return Err(StartError::FeatureMismatch(offer.id()));
            }
        }
        Ok(requested)
    }

    /// The action label displayed on the opening message.
    pub fn label(&self) -> String {
        match self {
            Self::Kickoff { small: false } => "New work".into(),
            Self::Kickoff { small: true } => "New standalone plan".into(),
            Self::Offer(offer) => offer.label(),
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

fn validate_feature(feature: Option<&str>) -> Result<(), StartError> {
    if let Some(name) = feature.filter(|name| !is_feature_name(name)) {
        return Err(StartError::InvalidTarget(name.into()));
    }
    Ok(())
}

fn document_start(id: &str, project: &Project) -> Option<AuthoringStart> {
    if id == "knowledge" {
        return Some(AuthoringStart::Knowledge { open_file: None });
    }
    let (kind, raw) = id.split_once(':')?;
    if !matches!(kind, "refine" | "refine-passage" | "knowledge") {
        return None;
    }
    let target = check_relpath_syntax(raw).ok()?;
    if !project.docs.contains_key(&target) {
        return None;
    }
    Some(match kind {
        "knowledge" => AuthoringStart::Knowledge {
            open_file: Some(target),
        },
        _ => AuthoringStart::Refine {
            target,
            passage: kind == "refine-passage",
        },
    })
}

/// Which authoring model and effort pair a start uses.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StepClass {
    /// Product vision, sections and general discussion.
    Spec,
    /// Metaplans and execution plans.
    Plan,
}

/// Classify starts by the step, document, or first message attachment.
pub fn step_class(start: &AuthoringStart) -> StepClass {
    match start {
        AuthoringStart::Offer(offer) => match offer {
            Offer::StartVision { .. }
            | Offer::FixVisionTable { .. }
            | Offer::AddSectionToVision { .. }
            | Offer::WriteSection { .. }
            | Offer::DraftAllSections { .. } => StepClass::Spec,
            _ => StepClass::Plan,
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
        AuthoringStart::Kickoff { .. }
        | AuthoringStart::Knowledge { .. }
        | AuthoringStart::NoStep => StepClass::Spec,
    }
}

#[cfg(test)]
mod tests;
