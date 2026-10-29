//! Session guidance, launch blocks and rendered planning artifacts.

use std::path::PathBuf;

use anyhow::{bail, Context};

use crate::plan_session::kind::SessionDir;
use crate::planning::project::Project;
use crate::planning::runtime::RuntimePaths;

mod blocks;
mod brief;
pub mod models;
pub mod opening;
mod plugin;
pub mod start;
pub mod team;

pub use blocks::{ConfigBlock, StartBlocks};
pub use models::{GuidanceModels, ModelEffort};
pub use opening::opening_message;
pub use start::{step_class, AuthoringStart, StartError, StepClass};
pub use team::{draft_run, DraftRun, MAX_TEAMMATES};

/// The pressure plugin command that starts the session.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PressureCommand {
    /// Review a plan for gaps and contradictions.
    Pressure,
    /// Address findings from a plan review.
    Address,
}

/// The work and inputs used to render a planning session.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SessionGuidanceKind {
    /// Discuss or write planning documents.
    Authoring(AuthoringStart),
    /// Rebuild only the requested summaries.
    Summary {
        /// Feature-scoped section or overview ids, or plan summary ids.
        ids: Vec<String>,
    },
    /// Run a pressure plugin command with a prepared brief.
    Pressure {
        /// The plugin command to invoke.
        command: PressureCommand,
        /// The repository-relative plan path.
        plan_rel: String,
        /// The prepared review instructions.
        brief: String,
        /// The session model.
        model: String,
        /// The session reasoning effort.
        effort: String,
    },
}

/// Rendered artifacts and launch settings for a planning session.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SessionGuidance {
    /// The rendered brief's absolute path.
    pub brief_path: PathBuf,
    /// The session's private plugin directory.
    pub plugin_dir: PathBuf,
    /// The generated first message, including the requested feature context.
    pub opening_message: String,
    /// The resolved session model.
    pub model: String,
    /// The resolved reasoning effort.
    pub effort: String,
    /// The session name displayed by the app.
    pub display_name: String,
}

/// The final instruction delivered when a planning session ends.
pub const CLOSING_MESSAGE: &str = concat!(
    "loom plan is ending this session; nobody will answer further questions.\n",
    "1. Write into the documents every decision this conversation settled that is not in them yet.\n",
    "2. Add every point still open as an item under \"## Open questions\" in the document it concerns,\n",
    "   with the options and your recommendation.\n",
    "3. Finish the turn-end duty for what you changed.\n",
    "Then reply \"Closing write done.\" and stop."
);

impl From<SessionGuidance> for crate::plan_session::bridge::LaunchGuidance {
    fn from(guidance: SessionGuidance) -> Self {
        Self {
            brief_path: guidance.brief_path,
            plugin_dir: guidance.plugin_dir,
            opening_message: guidance.opening_message,
            model: guidance.model,
            effort: guidance.effort,
            display_name: guidance.display_name,
            opening_on_argv: false,
        }
    }
}

/// Render guidance after checking the server's persistent launch block.
/// `feature` comes from the session request; `None` scopes standalone plans or general chats.
pub fn render_session_guidance(
    kind: &SessionGuidanceKind,
    project: &Project,
    runtime: &RuntimePaths,
    session_dir: &SessionDir,
    models: &GuidanceModels,
    feature: Option<&str>,
) -> anyhow::Result<SessionGuidance> {
    match StartBlocks::current(runtime) {
        Ok(None) => {}
        Ok(Some(block)) => bail!("start blocked: {}", block.reason),
        Err(error) => return Err(error).context("start blocked: config-block unreadable"),
    }
    let (opening_message, display_name) = opening_and_name(kind, project, feature)?;
    let (model, effort) = session_model(kind, models);
    let brief = match kind {
        SessionGuidanceKind::Authoring(start) => brief::common_brief(start, project, feature)?,
        SessionGuidanceKind::Summary { .. } => brief::summary_brief().to_owned(),
        SessionGuidanceKind::Pressure { brief, .. } => brief.clone(),
    };
    plugin::preflight(session_dir)?;
    plugin::write_plugin(kind, session_dir, models)?;
    plugin::write_brief(session_dir, &brief)?;
    Ok(SessionGuidance {
        brief_path: session_dir.brief(),
        plugin_dir: session_dir.plugin(),
        opening_message,
        model: model.to_owned(),
        effort: effort.to_owned(),
        display_name,
    })
}

fn opening_and_name(
    kind: &SessionGuidanceKind,
    project: &Project,
    feature: Option<&str>,
) -> anyhow::Result<(String, String)> {
    Ok(match kind {
        SessionGuidanceKind::Authoring(start) => (
            opening::opening_message(start, project, feature)?,
            opening::display_name("authoring", Some(start)),
        ),
        SessionGuidanceKind::Summary { ids } => (
            opening::summary_opening_message(ids),
            opening::display_name("summaries", None),
        ),
        SessionGuidanceKind::Pressure {
            command, plan_rel, ..
        } => (
            opening::pressure_opening_message(*command, plan_rel),
            opening::display_name(
                match command {
                    PressureCommand::Pressure => "pressure",
                    PressureCommand::Address => "address",
                },
                None,
            ),
        ),
    })
}

fn session_model<'a>(
    kind: &'a SessionGuidanceKind,
    models: &'a GuidanceModels,
) -> (&'a str, &'a str) {
    let resolved = match kind {
        SessionGuidanceKind::Authoring(start) => match step_class(start) {
            StepClass::Spec => &models.spec,
            StepClass::Plan => &models.plan,
        },
        SessionGuidanceKind::Summary { .. } => &models.summary_session,
        SessionGuidanceKind::Pressure { model, effort, .. } => return (model, effort),
    };
    (&resolved.model, &resolved.effort)
}

#[cfg(test)]
mod tests;
