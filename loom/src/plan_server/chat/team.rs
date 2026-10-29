//! Scope setup and publishing for kickoff and team authoring sessions.

use super::{lock, ChatState};
use crate::plan_server::context::PlanContext;
use crate::plan_server::wire::message::PlanMessage;
use crate::plan_server::wire::team::DraftTeamView;
use crate::plan_session::bridge::team::TeammateStatus;
use crate::plan_session::bridge::Bridge;
use crate::plan_session::kind::SessionDir;
use crate::plan_session::model::{ChatDelta, Item};
use crate::plan_session::scope::{self, SessionScope};
use crate::planning::guidance::draft_run;

fn is_team(offer: Option<&str>) -> bool {
    matches!(offer, Some("draft-all-sections" | "draft-all-plans"))
}

pub(super) fn extra_env(
    offer: Option<&str>,
    feature: Option<&str>,
) -> Result<Vec<(String, String)>, String> {
    if !is_team(offer) {
        return Ok(Vec::new());
    }
    if feature.is_none() {
        return Err("a draft-all run needs a feature".to_owned());
    }
    Ok(vec![(
        crate::claude::AGENT_TEAMS_ENV.to_owned(),
        "1".to_owned(),
    )])
}

pub(super) fn prepare_scope(
    ctx: &PlanContext,
    dir: &SessionDir,
    offer: Option<&str>,
    feature: Option<&str>,
) -> anyhow::Result<()> {
    if matches!(offer, Some("kickoff" | "kickoff-small")) {
        scope::write(dir, &SessionScope::kickoff(ctx.root())?)?;
    } else if let Some(offer @ ("draft-all-sections" | "draft-all-plans")) = offer {
        let feature = feature.ok_or_else(|| anyhow::anyhow!("a draft-all run needs a feature"))?;
        let run = {
            let project = ctx.project();
            draft_run(&project, offer, feature)?
        };
        scope::write(dir, &SessionScope::team(run.documents, run.bundles)?)?;
    }
    Ok(())
}

pub(super) fn observe_kickoff(
    ctx: &PlanContext,
    state: &ChatState,
    bridge: &Bridge,
    delta: &ChatDelta,
) {
    if let ChatDelta::ItemAdded {
        item: Item::Text { markdown, .. },
        ..
    }
    | ChatDelta::ItemReplaced {
        item: Item::Text { markdown, .. },
        ..
    } = delta
    {
        if let Some(id) = bridge
            .chat_session_id()
            .filter(|id| is_kickoff(ctx, state, id))
        {
            let view = lock(&state.kickoff).observe(&id, markdown);
            if let Some(view) = view {
                ctx.hub().broadcast(&PlanMessage::Kickoff(Box::new(view)));
            }
        }
    }
}

fn is_kickoff(ctx: &PlanContext, state: &ChatState, id: &str) -> bool {
    let mut scopes = lock(&state.kickoff_scopes);
    *scopes.entry(id.to_owned()).or_insert_with(|| {
        let dir = SessionDir::new(ctx.runtime().sessions.join(id));
        match scope::load(&dir) {
            Ok(scope) => matches!(scope, SessionScope::Kickoff(_)),
            Err(error) => {
                tracing::debug!(session = %id, %error, "cannot read kickoff scope");
                false
            }
        }
    })
}

pub(super) fn publish_draft_team(
    ctx: &PlanContext,
    state: &ChatState,
    id: &str,
    teammates: &[TeammateStatus],
) {
    let dir = SessionDir::new(ctx.runtime().sessions.join(id));
    let documents = match scope::load(&dir) {
        Ok(SessionScope::Team { documents, .. }) => documents,
        Ok(_) => {
            tracing::debug!(session = %id, "ignoring teammates outside a draft team scope");
            return;
        }
        Err(error) => {
            tracing::debug!(session = %id, %error, "cannot read draft team scope");
            return;
        }
    };
    let view = DraftTeamView {
        session: id.to_owned(),
        documents: documents
            .iter()
            .map(|path| path.to_string_lossy().into_owned())
            .collect(),
        teammates: teammates.to_vec(),
    };
    *lock(&state.team) = Some(view.clone());
    ctx.hub().broadcast(&PlanMessage::DraftTeam(Box::new(view)));
}

pub(super) fn clear(state: &ChatState, id: &str) {
    lock(&state.kickoff).clear(id);
    lock(&state.kickoff_scopes).remove(id);
    let mut team = lock(&state.team);
    if team.as_ref().is_some_and(|view| view.session == id) {
        *team = None;
    }
}

pub(super) fn snapshots(state: &ChatState) -> Vec<PlanMessage> {
    let mut messages = Vec::new();
    if let Some(view) = lock(&state.kickoff).view() {
        messages.push(PlanMessage::Kickoff(Box::new(view)));
    }
    if let Some(view) = lock(&state.team).clone() {
        messages.push(PlanMessage::DraftTeam(Box::new(view)));
    }
    messages
}
