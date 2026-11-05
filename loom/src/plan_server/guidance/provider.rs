//! Render authoring guidance and retain the brief shown by the planning server.

use std::collections::BTreeMap;
use std::fs;
use std::os::fd::AsRawFd;
use std::path::{Path, PathBuf};
use std::sync::{Mutex, PoisonError};

use anyhow::{Context, Result};

use super::brief_route::SessionBriefView;
use crate::fs::safe_fs::{safe_open_dirfd, safe_write_with_mode_in_workdir};
use crate::plan_server::chat::guidance::{validate_feature, GuidanceProvider};
use crate::plan_server::context::PlanContext;
use crate::plan_session::attachments::parse_unwrapped;
use crate::plan_session::bridge::LaunchGuidance;
use crate::plan_session::kind::{SessionDir, OPENING_TYPED_FILE};
use crate::planning::guidance::{
    render_session_guidance, AuthoringStart, GuidanceModels, SessionGuidance, SessionGuidanceKind,
    CLOSING_MESSAGE,
};
use crate::planning::project::Project;

/// The bridge's authoring provider and the brief route's shared session store.
pub struct PlanGuidance {
    models: GuidanceModels,
    rendered: Mutex<BTreeMap<String, SessionBriefView>>,
}

impl PlanGuidance {
    pub fn new(models: GuidanceModels) -> PlanGuidance {
        PlanGuidance {
            models,
            rendered: Mutex::new(BTreeMap::new()),
        }
    }

    pub fn view(&self, session_id: &str) -> Option<SessionBriefView> {
        self.rendered
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .get(session_id)
            .cloned()
    }

    fn remember(&self, dir: &SessionDir, label: String, guidance: &SessionGuidance) -> Result<()> {
        let session_id = dir
            .path()
            .file_name()
            .and_then(|name| name.to_str())
            .context("session directory has no UTF-8 session id")?
            .to_owned();
        let view = SessionBriefView {
            session_id: session_id.clone(),
            label,
            brief: fs::read_to_string(&guidance.brief_path).with_context(|| {
                format!("reading session brief {}", guidance.brief_path.display())
            })?,
            model: guidance.model.clone(),
            effort: guidance.effort.clone(),
            display_name: guidance.display_name.clone(),
        };
        self.rendered
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .insert(session_id, view);
        Ok(())
    }
}

impl PlanGuidance {
    /// Render a start in an explicitly requested feature, or without a feature.
    pub fn authoring_in_feature(
        &self,
        ctx: &PlanContext,
        dir: &SessionDir,
        offer: Option<&str>,
        first_message: Option<&str>,
        feature: Option<&str>,
    ) -> Result<LaunchGuidance> {
        let project = ctx.project();
        validate_feature(&project, feature)?;
        let start = authoring_start(&project, offer, first_message, feature)?;
        let label = start.label();
        let mut guidance = render_session_guidance(
            &SessionGuidanceKind::Authoring(start),
            &project,
            ctx.runtime(),
            dir,
            &self.models,
            feature,
        )?;
        drop(project);
        // A typed message is delivered only through the opening, so it must
        // follow the generated offer opening when both are present.
        match (offer, first_message) {
            (None, Some(text)) => guidance.opening_message = text.to_owned(),
            (Some(_), Some(text)) => {
                guidance.opening_message = format!("{}\n\n{text}", guidance.opening_message);
                // The bridge shows the guidance and the typed part as two items; it learns
                // the split here, never by searching the joined text.
                write_opening_typed(dir, text)?;
            }
            _ => {}
        }
        self.remember(dir, label, &guidance)?;
        Ok(guidance.into())
    }
}

impl GuidanceProvider for PlanGuidance {
    fn authoring(
        &self,
        ctx: &PlanContext,
        dir: &SessionDir,
        offer: Option<&str>,
        first_message: Option<&str>,
        feature: Option<&str>,
    ) -> Result<LaunchGuidance> {
        self.authoring_in_feature(ctx, dir, offer, first_message, feature)
    }

    fn closing_message(&self) -> String {
        CLOSING_MESSAGE.to_owned()
    }
}

/// Leaves the typed part of an offer opening where `SessionDir::opening_typed` names it.
fn write_opening_typed(dir: &SessionDir, text: &str) -> Result<()> {
    let dirfd = safe_open_dirfd(dir.path())?;
    safe_write_with_mode_in_workdir(
        dirfd.as_raw_fd(),
        Path::new(OPENING_TYPED_FILE),
        text.as_bytes(),
        0o600,
    )
    .with_context(|| format!("cannot write {}", dir.opening_typed().display()))
}

fn authoring_start(
    project: &Project,
    offer: Option<&str>,
    first_message: Option<&str>,
    feature: Option<&str>,
) -> Result<AuthoringStart> {
    match (offer, first_message) {
        (Some(id), _) => {
            AuthoringStart::from_offer_id(id, project, feature).map_err(anyhow::Error::from)
        }
        (None, Some(text)) => Ok(AuthoringStart::Message {
            paths: parse_unwrapped(text)
                .1
                .into_iter()
                .map(|attachment| PathBuf::from(attachment.path))
                .collect(),
        }),
        (None, None) => anyhow::bail!("a chat starts with a message or an offer"),
    }
}
