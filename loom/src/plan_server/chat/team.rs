//! Scope setup and publishing for kickoff and team authoring sessions.

use super::{draft_cmd, lock, ChatState};
use crate::plan_server::context::PlanContext;
use crate::plan_server::wire::message::PlanMessage;
use crate::plan_server::wire::team::DraftTeamView;
use crate::plan_session::bridge::team::TeammateStatus;
use crate::plan_session::bridge::Bridge;
use crate::plan_session::kind::SessionDir;
use crate::plan_session::model::{ChatDelta, Item};
use crate::plan_session::scope::{self, KickoffGrant, KickoffSize, SessionScope, TeamRun};
use crate::planning::guidance::team_scope;

/// The offers whose session runs as an agent team inside a team scope.
fn is_team(offer: Option<&str>) -> bool {
    matches!(
        offer,
        Some("draft-all-sections" | "draft-all-plans" | "address-findings")
    )
}

pub(super) fn extra_env(
    offer: Option<&str>,
    feature: Option<&str>,
) -> Result<Vec<(String, String)>, String> {
    if !is_team(offer) {
        return Ok(Vec::new());
    }
    if feature.is_none() {
        return Err("a team run needs a feature".to_owned());
    }
    Ok(vec![(
        crate::claude::AGENT_TEAMS_ENV.to_owned(),
        "1".to_owned(),
    )])
}

/// The size a kickoff offer starts with: `kickoff` is a feature, `kickoff-small` a standalone plan.
pub(super) fn offer_size(offer: Option<&str>) -> Option<KickoffSize> {
    match offer {
        Some("kickoff") => Some(KickoffSize::Feature),
        Some("kickoff-small") => Some(KickoffSize::Plan),
        _ => None,
    }
}

/// Write the session's scope after its guidance. A kickoff scope carries the offer's size; a
/// continued kickoff holding a `resume` grant gets the scope with that grant already held. A team
/// scope the guidance wrote stays: it holds the documents of the project snapshot the opening
/// named them from. A provider that wrote none gets one built here.
pub(super) fn prepare_scope(
    ctx: &PlanContext,
    dir: &SessionDir,
    offer: Option<&str>,
    feature: Option<&str>,
    resume: Option<&KickoffGrant>,
) -> anyhow::Result<()> {
    if let Some(size) = offer_size(offer) {
        let scope = match resume {
            Some(grant) => SessionScope::kickoff_resumed(grant.size, &grant.name)?,
            None => SessionScope::kickoff(ctx.root(), size)?,
        };
        return scope::write(dir, &scope);
    }
    let Some(offer) = offer.filter(|offer| is_team(Some(offer))) else {
        return Ok(());
    };
    let feature = feature.ok_or_else(|| anyhow::anyhow!("a team run needs a feature"))?;
    if matches!(scope::load(dir)?, SessionScope::Team { .. }) {
        return Ok(());
    }
    let built = {
        let project = ctx.project();
        team_scope(&project, offer, feature)?
    };
    match built {
        Some(team) => scope::write(dir, &team),
        None => Ok(()),
    }
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
        let kickoff = bridge
            .chat_session_id()
            .and_then(|id| kickoff_size(ctx, state, &id).map(|size| (id, size)));
        if let Some((id, size)) = kickoff {
            let view = lock(&state.kickoff).observe(&id, size, markdown);
            if let Some(view) = view {
                // The draft first: a browser that hears the shape finds it stored.
                draft_cmd::on_shape(ctx, state, &id, view.shape.clone());
                ctx.hub().broadcast(&PlanMessage::Kickoff(Box::new(view)));
            }
        }
    }
}

/// The size of `id`'s kickoff scope, `None` for any other scope; read once per session. A read
/// that fails is not cached, so the next event reads the scope again.
fn kickoff_size(ctx: &PlanContext, state: &ChatState, id: &str) -> Option<KickoffSize> {
    let mut scopes = lock(&state.kickoff_scopes);
    if let Some(size) = scopes.get(id) {
        return *size;
    }
    let dir = SessionDir::new(ctx.runtime().sessions.join(id));
    let size = match scope::load(&dir) {
        Ok(SessionScope::Kickoff(kickoff)) => Some(kickoff.size),
        Ok(_) => None,
        Err(error) => {
            tracing::debug!(session = %id, %error, "cannot read kickoff scope");
            return None;
        }
    };
    scopes.insert(id.to_owned(), size);
    size
}

pub(super) fn publish_draft_team(
    ctx: &PlanContext,
    state: &ChatState,
    id: &str,
    teammates: &[TeammateStatus],
) {
    let dir = SessionDir::new(ctx.runtime().sessions.join(id));
    // An address run's ledger is the lead's alone: no teammate row would ever show it written.
    let documents: Vec<_> = match scope::load(&dir) {
        Ok(SessionScope::Team { run, documents, .. }) => match run {
            TeamRun::Address { ledger } => documents.into_iter().filter(|p| *p != ledger).collect(),
            TeamRun::Draft => documents,
        },
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
