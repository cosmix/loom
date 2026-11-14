//! A draft team's run: once the team settles, the summaries of the documents it wrote build at
//! once, then the product overview over them, with no quiet period and no large-rebuild question.
//!
//! "Draft all with a team" is the user's request for those documents, so the end of the run
//! counts as a press of their summaries. Only a draft run's scope (`TeamRun::Draft`) does this:
//! an address run edits documents that exist, so their summaries take the background path, its
//! quiet period and its large-rebuild question.
//!
//! The run settles on a report of the lead, a turn end or the session going idle or ending, when
//! no teammate is working by then. A teammate going idle settles nothing: that idle wakes the lead
//! for its consistency turn, and the teammate's transcript tail can report it before the lead's
//! turn starts, so the run waits for the end of that turn. The session's end settles a run left
//! working, because a teammate the lead shuts down may never write the record that shows it idle.
//! A run settles once and is armed again only by a teammate working after that, so the refinement
//! turns the reader takes later in the same chat follow the background path and its quiet
//! period. Per section is not used: the lead checks the set for consistency after the teammates
//! finish and may edit any section, and nothing can start during its turn anyway.

#[cfg(test)]
mod tests;

use std::collections::{BTreeSet, HashMap};
use std::path::PathBuf;

use super::Rebuilds;
use crate::plan_session::bridge::team::{TeammateState, TeammateStatus};
use crate::plan_session::kind::SessionDir;
use crate::plan_session::scope::{self, SessionScope, TeamRun};
use crate::planning::kinds::DocKind;
use crate::planning::project::Project;
use crate::planning::summary::cost::Trigger;
use crate::planning::summary::evaluate::{Evaluation, SummaryState};
use crate::planning::summary::store::summary_id_for;

/// The draft-team runs of the authoring sessions that reported teammates.
#[derive(Debug, Default)]
pub(super) struct TeamRuns(HashMap<String, Run>);

#[derive(Debug, Clone, Copy)]
struct Run {
    /// A teammate works, by the last statuses reported.
    working: bool,
    /// The run has not settled since a teammate last worked.
    armed: bool,
}

impl TeamRuns {
    /// The teammates of `session` changed; a working teammate arms its run.
    pub(super) fn teammates(&mut self, session: &str, teammates: &[TeammateStatus]) {
        let working = teammates
            .iter()
            .any(|mate| mate.state == TeammateState::Working);
        let run = self.0.entry(session.to_owned()).or_insert(Run {
            working,
            armed: true,
        });
        run.working = working;
        run.armed |= working;
    }

    /// Whether the run in `session` settles now, which disarms it: armed, with no teammate
    /// working and no lead turn open. An `ended` session settles unless its run already did,
    /// and is forgotten; a session that never reported teammates settles only at its end.
    pub(super) fn settle(&mut self, session: &str, lead_turn_open: bool, ended: bool) -> bool {
        if ended {
            return self.0.remove(session).is_none_or(|run| run.armed);
        }
        match self.0.get_mut(session) {
            Some(run) if run.armed && !run.working && !lead_turn_open => {
                run.armed = false;
                true
            }
            _ => false,
        }
    }
}

/// What a settled run builds: the summary of each of its `documents` that is neither current
/// nor claimed (an id the evaluation does not list yet is a document just written), then the
/// product overview of every feature one of them is a section of. Empty when none is due, so a
/// run that wrote nothing, or whose summaries were already built, rebuilds no overview.
pub(super) fn run_ids(project: &Project, eval: &Evaluation, documents: &[PathBuf]) -> Vec<String> {
    let due = |id: &String| {
        let state = eval.summaries.get(id).map(|summary| summary.state);
        !matches!(
            state,
            Some(SummaryState::Current | SummaryState::Rebuilding)
        )
    };
    let mut ids = Vec::new();
    let mut overviews = BTreeSet::new();
    for path in documents {
        let Some(id) = summary_id_for(project, path).filter(due) else {
            continue;
        };
        if let Some(DocKind::ProductSection {
            feature: Some(feature),
            ..
        }) = project.tree.get(path).map(|doc| &doc.kind)
        {
            overviews.insert(format!("overview/{feature}/product"));
        }
        ids.push(id);
    }
    // After the sections: the session builds the documents first and an overview over them.
    let expected = overviews
        .into_iter()
        .filter(|id| eval.summaries.contains_key(id));
    ids.extend(expected);
    ids
}

/// What a draft team's lead session reported.
#[derive(Debug, Clone, Copy)]
pub(super) enum Lead<'a> {
    /// This turn ended.
    TurnEnded(&'a str),
    /// It went Idle or Ended, closing any open turn.
    Idle,
    /// The session ended.
    Ended,
}

impl Rebuilds {
    /// The teammates of `session` changed. Nothing settles here: the lead's next report does.
    pub(super) fn team_changed(&self, session: &str, teammates: &[TeammateStatus]) {
        self.lock().team.teammates(session, teammates);
    }

    /// When the draft-team run in `session` settles, press the summaries it left out of date
    /// ([`run_ids`]). Idempotent: an id queued or in the running session is not queued twice,
    /// and a current one is not pressed.
    ///
    /// A lead's turn end or idle calls this before the turn's suspension is released, so the
    /// pressed ids are queued when the release tries a start: a start of the background queue
    /// alone could ask first about a batch above the limit that holds them.
    pub(super) fn settle_team(&self, session: &str, lead: Lead<'_>) {
        let due = {
            let mut state = self.lock();
            let open = state.open_turn.as_ref().filter(|(id, _)| id == session);
            let lead_turn_open = open.is_some_and(|(_, turn)| match lead {
                Lead::TurnEnded(ended) => turn != ended,
                Lead::Idle | Lead::Ended => false,
            });
            let ended = matches!(lead, Lead::Ended);
            state.team.settle(session, lead_turn_open, ended)
        };
        let Some(documents) = due.then(|| self.team_documents(session)).flatten() else {
            return;
        };
        let ids = self.read_now(|project, eval| run_ids(project, eval, &documents));
        let ids = ids.unwrap_or_default();
        if !ids.is_empty() {
            self.enqueue(ids, Trigger::Button);
        }
    }

    /// The documents of `session`'s draft-team scope; `None` for any other session, an address
    /// run's included.
    fn team_documents(&self, session: &str) -> Option<Vec<PathBuf>> {
        let env = self.env.as_ref()?;
        let dir = SessionDir::new(env.runtime.sessions.join(session));
        match scope::load(&dir) {
            Ok(SessionScope::Team {
                run: TeamRun::Draft,
                documents,
                ..
            }) => Some(documents),
            Ok(_) => None,
            Err(error) => {
                tracing::debug!(session = %session, %error, "cannot read the session scope");
                None
            }
        }
    }
}
