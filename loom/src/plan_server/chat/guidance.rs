//! The guidance seam for authoring chats: the provider trait, its slot in the extensions and the fixed provider.

use std::os::fd::AsRawFd;
use std::path::Path;
use std::sync::{Arc, PoisonError, RwLock};

use anyhow::{bail, ensure, Result};

use crate::fs::safe_fs::{
    safe_create_dir_all_in_workdir, safe_open_dirfd, safe_write_with_mode_in_workdir,
};
use crate::plan_server::context::PlanContext;
use crate::plan_session::bridge::LaunchGuidance;
use crate::plan_session::kind::SessionDir;
use crate::planning::kinds::{is_feature_name, plans_dir, product_dir, vision_path};
use crate::planning::project::{Feature, Project};

const BRIEF: &str = "You are a planning session started by `loom plan`. Write only under doc/. Ask in plain text, never with a question tool.";
const PLUGIN: &str =
    r#"{"name": "loom-plan", "version": "0.0.0", "description": "loom plan session"}"#;
const CLOSING: &str = "Record the remaining open questions and anything still pending into the documents now, then stop.";

/// Supplies launch files and messages without coupling guidance to the bridge.
pub trait GuidanceProvider: Send + Sync {
    /// Prepare a private brief and opening for an authoring session.
    /// `feature` selects the checklist containing the offer id.
    fn authoring(
        &self,
        ctx: &PlanContext,
        dir: &SessionDir,
        offer: Option<&str>,
        first_message: Option<&str>,
        feature: Option<&str>,
    ) -> Result<LaunchGuidance>;

    /// The instruction sent when the session is asked to finish.
    fn closing_message(&self) -> String;
}

/// The current guidance provider, replaceable through the server's extensions.
pub struct GuidanceSlot(RwLock<Arc<dyn GuidanceProvider>>);

impl GuidanceSlot {
    /// Install the initial provider.
    pub fn new(provider: Arc<dyn GuidanceProvider>) -> Self {
        Self(RwLock::new(provider))
    }

    /// Snapshot the installed provider.
    pub fn get(&self) -> Arc<dyn GuidanceProvider> {
        Arc::clone(&self.0.read().unwrap_or_else(PoisonError::into_inner))
    }

    /// Replace the provider for subsequent sessions.
    pub fn set(&self, provider: Arc<dyn GuidanceProvider>) {
        *self.0.write().unwrap_or_else(PoisonError::into_inner) = provider;
    }
}

/// The installed provider until the guidance engine replaces it: the fixed guidance, for no
/// offer or a step the checklist offers now.
pub struct ChecklistGuidance;

impl GuidanceProvider for ChecklistGuidance {
    fn authoring(
        &self,
        ctx: &PlanContext,
        dir: &SessionDir,
        offer: Option<&str>,
        first_message: Option<&str>,
        feature: Option<&str>,
    ) -> Result<LaunchGuidance> {
        validate_feature(&ctx.project(), feature)?;
        if let Some(offer) = offer.filter(|offer| !offered(ctx, offer, feature)) {
            bail!("the step \"{offer}\" is not offered now");
        }
        FixedGuidance.authoring(ctx, dir, offer, first_message, feature)
    }

    fn closing_message(&self) -> String {
        FixedGuidance.closing_message()
    }
}

/// Whether the checklist offers `offer` now.
fn offered(ctx: &PlanContext, offer: &str, selected: Option<&str>) -> bool {
    let project = ctx.project();
    project
        .moves
        .iter()
        .any(|candidate| candidate.id() == offer)
        || project
            .features
            .iter()
            .filter(|feature| selected.is_none_or(|name| feature.name == name))
            .any(|feature| offers_step(feature, offer))
}

/// Fixed launch files and opening for any offer id; it accepts every id it is given.
pub struct FixedGuidance;

impl GuidanceProvider for FixedGuidance {
    fn authoring(
        &self,
        ctx: &PlanContext,
        dir: &SessionDir,
        offer: Option<&str>,
        first_message: Option<&str>,
        feature: Option<&str>,
    ) -> Result<LaunchGuidance> {
        let project = ctx.project();
        validate_feature(&project, feature)?;
        let feature = match feature {
            Some(name) => project.feature(name),
            None => session_feature(&project, offer),
        };
        let brief = session_brief(&project, feature);
        let opening_message = opening(feature, offer, first_message);
        drop(project);
        let fd = safe_open_dirfd(dir.path())?;
        safe_write_with_mode_in_workdir(
            fd.as_raw_fd(),
            Path::new("brief.md"),
            brief.as_bytes(),
            0o600,
        )?;
        safe_create_dir_all_in_workdir(fd.as_raw_fd(), Path::new("plugin/.claude-plugin"), 0o700)?;
        safe_write_with_mode_in_workdir(
            fd.as_raw_fd(),
            Path::new("plugin/.claude-plugin/plugin.json"),
            PLUGIN.as_bytes(),
            0o600,
        )?;
        Ok(LaunchGuidance {
            brief_path: dir.brief(),
            plugin_dir: dir.plugin(),
            opening_message,
            model: "opus".to_owned(),
            effort: "medium".to_owned(),
            display_name: "loom plan: chat".to_owned(),
            opening_on_argv: false,
        })
    }

    fn closing_message(&self) -> String {
        CLOSING.to_owned()
    }
}

/// Reject malformed or unknown feature selections before rendering guidance.
pub(crate) fn validate_feature(project: &Project, feature: Option<&str>) -> Result<()> {
    if let Some(name) = feature {
        ensure!(is_feature_name(name), "invalid feature name: {name}");
        ensure!(project.feature(name).is_some(), "unknown feature: {name}");
    }
    Ok(())
}

fn offers_step(feature: &Feature, offer: &str) -> bool {
    feature
        .checklist
        .primary
        .iter()
        .chain(&feature.checklist.secondary)
        .any(|candidate| candidate.id() == offer)
}

fn session_feature<'a>(project: &'a Project, offer: Option<&str>) -> Option<&'a Feature> {
    if let Some(offer) = offer {
        let mut features = project
            .features
            .iter()
            .filter(|feature| offers_step(feature, offer));
        let first = features.next();
        if features.next().is_some() {
            return None;
        }
        if first.is_some() {
            return first;
        }
    }
    (project.features.len() == 1).then(|| &project.features[0])
}

fn session_brief(project: &Project, selected: Option<&Feature>) -> String {
    let features: Vec<_> = selected
        .into_iter()
        .chain(project.features.iter().filter(|_| selected.is_none()))
        .collect();
    if features.is_empty() {
        return BRIEF.to_owned();
    }
    let mut brief = BRIEF.to_owned();
    for feature in features {
        brief.push_str(&format!(
            "\nFeature \"{}\": vision {}; product sections under {}; metaplans and plans under {}.",
            feature.name,
            vision_path(&feature.name).display(),
            product_dir(&feature.name).display(),
            plans_dir(Some(&feature.name)).display(),
        ));
    }
    if selected.is_none() {
        brief.push_str("\nAsk which feature to work on before editing feature documents.");
    }
    brief
}

fn opening(feature: Option<&Feature>, offer: Option<&str>, first_message: Option<&str>) -> String {
    // Preserve a delivered message verbatim; blank input still needs a usable opening.
    if let Some(text) = first_message.filter(|text| !text.trim().is_empty()) {
        return text.to_owned();
    }
    let target = feature
        .map(|feature| format!(" for feature \"{}\"", feature.name))
        .unwrap_or_default();
    match offer {
        Some(id) => format!("Start the step \"{id}\"{target}."),
        None => format!("Start a planning session{target}."),
    }
}

#[cfg(test)]
mod tests;
