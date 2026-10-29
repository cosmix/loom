//! Claim attribution and cost lines for summary sessions and refinement subagents.
//!
//! Which subagent rebuilt which summary comes from its own transcript: a Bash `tool_use`
//! running `loom plan summary brief <ids>` binds those ids to it (section 04 "Cost"). When it
//! stops, its claims on them are released and its tokens logged: one line per summary when it
//! briefed one, else one for the set. A summary session's own transcript is one line at its end.

mod scan;
#[cfg(test)]
mod tests;

use std::collections::{BTreeMap, BTreeSet, HashMap};
use std::path::{Path, PathBuf};
use std::sync::Arc;

use anyhow::Result;
use chrono::{SecondsFormat, Utc};

use crate::plan_server::wire::summaries::{SummaryTurnCost, TurnCostRow};
use crate::plan_session::bridge::Bridge;
use crate::planning::runtime::RuntimePaths;
use crate::planning::summary::claims::{clear_session_claims, delete_claim, list_claims};
use crate::planning::summary::cost::estimate::fresh_tokens;
use crate::planning::summary::cost::{self, CostLine, Level, TokenVector, Trigger};
use crate::planning::summary::store::is_overview;

pub use scan::last_seal_error;

/// Where the summaries a chat turn rebuilt are named: the session bridge (a recorder in tests).
pub trait RebuildingSink: Send + Sync {
    fn set_rebuilding(&self, turn: Option<(&str, &str)>, ids: Vec<String>);
}

impl RebuildingSink for Bridge {
    fn set_rebuilding(&self, turn: Option<(&str, &str)>, ids: Vec<String>) {
        Bridge::set_rebuilding(self, turn, ids);
    }
}

/// What a stop recorded.
#[derive(Debug, Default)]
pub struct Attributed {
    /// The cost lines appended to the log.
    pub lines: Vec<CostLine>,
    /// The rows of a chat turn that rebuilt summaries.
    pub turn_cost: Option<SummaryTurnCost>,
}

/// What a session is to attribution.
enum Role {
    /// A background summary session: what queued each of its ids, and its own transcript.
    Summary {
        triggers: BTreeMap<String, Trigger>,
        main: Option<PathBuf>,
    },
    /// An authoring or pressure chat: its latest turn and the ids rebuilt in it.
    Chat {
        turn: Option<String>,
        rebuilt: Vec<String>,
    },
}

/// The level a summary is built at.
pub(crate) fn level_of(id: &str) -> Level {
    if is_overview(id) {
        Level::Overview
    } else {
        Level::Document
    }
}

/// Binds briefed summaries to the subagents that briefed them, releases their claims and logs
/// their cost.
pub struct Attribution {
    paths: RuntimePaths,
    by_agent: HashMap<(String /* session */, String /* agent */), Vec<String /* ids */>>,
    sessions: HashMap<String, Role>,
    /// Source bytes by summary id, recorded on single-summary lines for estimates.
    sizes: BTreeMap<String, u64>,
    sink: Option<Arc<dyn RebuildingSink>>,
}

impl Attribution {
    pub fn new(paths: RuntimePaths) -> Self {
        Attribution {
            paths,
            by_agent: HashMap::new(),
            sessions: HashMap::new(),
            sizes: BTreeMap::new(),
            sink: None,
        }
    }

    /// Name rebuilt summaries on chat turns through `sink`.
    pub fn with_sink(mut self, sink: Arc<dyn RebuildingSink>) -> Self {
        self.sink = Some(sink);
        self
    }

    pub fn set_sizes(&mut self, sizes: BTreeMap<String, u64>) {
        self.sizes = sizes;
    }

    /// A summary session started for the ids of `triggers`.
    pub fn summary_started(&mut self, session: &str, triggers: BTreeMap<String, Trigger>) {
        let role = Role::Summary {
            triggers,
            main: None,
        };
        self.sessions.insert(session.to_owned(), role);
    }

    /// An authoring or pressure session started.
    pub fn chat_started(&mut self, session: &str) {
        let role = Role::Chat {
            turn: None,
            rebuilt: Vec::new(),
        };
        self.sessions.insert(session.to_owned(), role);
    }

    /// A chat turn opened: later briefs of the session's subagents rebuild for it.
    pub fn turn_started(&mut self, session: &str, turn: &str) {
        let role = Role::Chat {
            turn: Some(turn.to_owned()),
            rebuilt: Vec::new(),
        };
        self.sessions.insert(session.to_owned(), role);
    }

    /// A chat turn ended: name what its subagents still running or never stopped briefed.
    pub fn turn_ended(&mut self, session: &str, turn: &str, subagents: &[(String, PathBuf)]) {
        let mut briefed = Vec::new();
        for (agent, path) in subagents {
            let key = (session.to_owned(), agent.clone());
            if self.by_agent.contains_key(&key) {
                continue;
            }
            let ids = scan::scan(path).briefed;
            if !ids.is_empty() {
                briefed.extend(ids.iter().cloned());
                self.by_agent.insert(key, ids);
            }
        }
        let open = match self.sessions.get(session) {
            Some(Role::Chat {
                turn: Some(open), ..
            }) => open.as_str() == turn,
            _ => false,
        };
        if open && !briefed.is_empty() {
            self.name_on_turn(session, &briefed);
        }
    }

    /// The summary session's own transcript, costed when it ends.
    pub fn note_main_transcript(&mut self, session: &str, main: Option<PathBuf>) {
        if let Some(Role::Summary { main: known, .. }) = self.sessions.get_mut(session) {
            if main.is_some() {
                *known = main;
            }
        }
    }

    /// A subagent stopped: bind what its transcript briefed, delete those claims of `session`
    /// (plan 4's `release` reads `LOOM_PLAN_SESSION` and `clear_session_claims` takes every id,
    /// so each is deleted here), and log its tokens.
    pub fn on_subagent_stopped(
        &mut self,
        session: &str,
        agent: &str,
        transcript: &Path,
    ) -> Result<Attributed> {
        let found = scan::scan(transcript);
        let ids = found.briefed;
        let key = (session.to_owned(), agent.to_owned());
        self.by_agent.insert(key, ids.clone());
        for claim in list_claims(&self.paths)? {
            if ids.contains(&claim.summary) && claim.session.as_deref() == Some(session) {
                delete_claim(&self.paths, &claim.summary)?;
            }
        }
        if ids.is_empty() {
            return Ok(Attributed::default());
        }
        let tokens = cost::transcript_tokens(transcript)?;
        let trigger = trigger_of(self.sessions.get(session), &ids);
        let line = self.cost_line(ids.clone(), tokens, found.model, trigger);
        cost::append(&self.paths, &line)?;
        let turn_cost = self.name_on_turn(session, &ids).map(|turn| {
            let tokens = match ids.as_slice() {
                [_] => fresh_tokens(&line.tokens),
                _ => None,
            };
            let rows = ids.iter().map(|id| TurnCostRow {
                id: id.clone(),
                tokens,
            });
            SummaryTurnCost {
                session: session.to_owned(),
                turn,
                rows: rows.collect(),
            }
        });
        Ok(Attributed {
            lines: vec![line],
            turn_cost,
        })
    }

    /// A session ended: clear every claim it holds; a summary session's own transcript is one
    /// line for the ids no subagent briefed (the overviews it built itself), else for all.
    pub fn on_session_ended(&mut self, session: &str) -> Result<Vec<CostLine>> {
        clear_session_claims(&self.paths, session)?;
        let bound: BTreeSet<String> = self
            .by_agent
            .iter()
            .filter(|((owner, _), _)| owner == session)
            .flat_map(|(_, ids)| ids.iter().cloned())
            .collect();
        self.by_agent.retain(|(owner, _), _| owner != session);
        let Some(Role::Summary {
            triggers,
            main: Some(main),
        }) = self.sessions.remove(session)
        else {
            return Ok(Vec::new());
        };
        let unbound = triggers.keys().filter(|id| !bound.contains(*id));
        let mut ids: Vec<String> = unbound.cloned().collect();
        if ids.is_empty() {
            ids = triggers.keys().cloned().collect();
        }
        let tokens = cost::transcript_tokens(&main)?;
        let trigger = pressed_or_background(&triggers, &ids);
        let line = self.cost_line(ids, tokens, scan::scan(&main).model, trigger);
        cost::append(&self.paths, &line)?;
        Ok(vec![line])
    }

    /// After the session bridge's recovery: delete every claim of a session that is neither
    /// live nor left alone; return the terminal (session-less) claims, which stay.
    pub fn sweep_after_recovery(
        &mut self,
        live: &[&str],
        left_alone: &[&str],
    ) -> Result<Vec<String>> {
        let mut terminal = Vec::new();
        for claim in list_claims(&self.paths)? {
            match claim.session.as_deref() {
                None => terminal.push(claim.summary),
                Some(session) if live.contains(&session) || left_alone.contains(&session) => {}
                Some(_) => delete_claim(&self.paths, &claim.summary)?,
            }
        }
        Ok(terminal)
    }

    /// Delete the terminal claims on `ids`; a claim a session took meanwhile is kept. Returns
    /// the ids released.
    pub fn release_terminal(&mut self, ids: &[String]) -> Result<Vec<String>> {
        let mut released = Vec::new();
        for claim in list_claims(&self.paths)? {
            if claim.session.is_none() && ids.contains(&claim.summary) {
                delete_claim(&self.paths, &claim.summary)?;
                released.push(claim.summary);
            }
        }
        Ok(released)
    }

    /// Add `ids` to the chat's open turn and name the turn's rebuilt summaries on it; returns
    /// the turn, `None` for a session that is no chat with a turn.
    fn name_on_turn(&mut self, session: &str, ids: &[String]) -> Option<String> {
        let Some(Role::Chat {
            turn: Some(turn),
            rebuilt,
        }) = self.sessions.get_mut(session)
        else {
            return None;
        };
        for id in ids {
            if !rebuilt.contains(id) {
                rebuilt.push(id.clone());
            }
        }
        if let Some(sink) = &self.sink {
            sink.set_rebuilding(Some((session, turn.as_str())), rebuilt.clone());
        }
        Some(turn.clone())
    }

    fn cost_line(
        &self,
        ids: Vec<String>,
        tokens: TokenVector,
        model: Option<String>,
        trigger: Trigger,
    ) -> CostLine {
        let level = if ids.iter().all(|id| is_overview(id)) {
            Level::Overview
        } else {
            Level::Document
        };
        let source_bytes = match ids.as_slice() {
            [id] => self.sizes.get(id).copied(),
            _ => None,
        };
        CostLine {
            time: Utc::now().to_rfc3339_opts(SecondsFormat::Secs, true),
            trigger,
            summaries: ids,
            level,
            model,
            source_bytes,
            tokens,
        }
    }
}

/// A chat rebuilds in refinement; a summary session's ids are a Button's when all were
/// pressed; an unknown session counts as background.
fn trigger_of(role: Option<&Role>, ids: &[String]) -> Trigger {
    match role {
        Some(Role::Chat { .. }) => Trigger::Refinement,
        Some(Role::Summary { triggers, .. }) => pressed_or_background(triggers, ids),
        None => Trigger::Background,
    }
}

fn pressed_or_background(triggers: &BTreeMap<String, Trigger>, ids: &[String]) -> Trigger {
    let pressed = |id: &String| triggers.get(id) == Some(&Trigger::Button);
    if ids.iter().all(pressed) {
        Trigger::Button
    } else {
        Trigger::Background
    }
}
