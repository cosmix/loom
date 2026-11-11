//! The background tasks of one session (plan A-D2, A-D3): every background agent and shell the
//! session launched, from its launch to the evidence that ended it.
//!
//! A task ends only on evidence: a `Stop` list that omits it, its notification, a `TaskStop`
//! result, and for an agent its hand-back, its `toolEndsTurn` result, meta `finished` or its
//! `SubagentStop`. An agent with none of these turns `Stalled` once its transcript has been
//! quiet past the stall budget. Times come from record timestamps and spool `received_at`; only
//! [`BackgroundTasks::reevaluate`] takes a clock reading. Each field keeps the value of its
//! highest-ranked source and each time its earliest start and newest sighting, so a replay
//! from offset 0 rebuilds the same table in whichever order the spool and the transcripts are
//! read. The table is never persisted. Every id is bounded and every kept string capped.

mod entry;
mod inputs;
#[cfg(test)]
mod tests;

use std::collections::{HashMap, HashSet};
use std::time::Duration;

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use serde_json::Value;

use self::entry::{unended, Entry, Follow, Proof};
use super::registry::AgentMeta;
use crate::agent_state::{stall_reason, AgentProgress, SubagentState, DEFAULT_DONE_DEBOUNCE_SECS};
use crate::plan_session::spool::SpoolEvent;
use crate::plan_session::state::text;
use crate::plan_session::transcript::{bounded_id, RecordMark};

/// One background task as the bridge publishes it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct BackgroundTask {
    /// The agent id, or the shell task id.
    pub id: String,
    pub kind: TaskKind,
    /// The launching call's `description`, else the meta file's, else the `Stop` list's.
    pub label: Option<String>,
    /// The `SubagentStart` `agent_type`, else the meta file's `agentType`.
    pub agent_type: Option<String>,
    /// The launching call; none for a fork skill or a shell adopted from a `Stop` list.
    pub tool_use_id: Option<String>,
    pub state: TaskState,
    pub started_at: DateTime<Utc>,
    pub last_activity_at: DateTime<Utc>,
    /// An agent's assistant entries (Claude Code writes one per content block); none for a shell.
    pub steps: Option<u64>,
    pub last_tool: Option<String>,
    /// The start of an agent's hand-back report.
    pub report_preview: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum TaskKind {
    Agent,
    Shell,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum TaskState {
    Running,
    ToolWait,
    Stalled,
    Done,
    Failed,
    Stopped,
}

impl TaskState {
    pub fn is_ended(self) -> bool {
        matches!(
            self,
            TaskState::Done | TaskState::Failed | TaskState::Stopped
        )
    }
}

/// What one input did to the table.
#[derive(Debug, Default, PartialEq, Eq)]
pub struct Change {
    /// A task was added, removed, or one of its published fields changed.
    pub changed: bool,
    /// The agent ids that ended for the first time; each is reported once, ever.
    pub stopped: Vec<String>,
}

/// The newest `Stop` list: the ids it named and when it was received.
struct StopList {
    at: DateTime<Utc>,
    ids: HashSet<String>,
}

/// What an agent's own files said, kept whether or not its entry exists yet.
#[derive(Default)]
struct AgentFacts {
    progress: AgentProgress,
    /// The fold's state at the time of the newest line, where no debounce has passed.
    fold: Option<SubagentState>,
    /// The newest timestamp among the agent's transcript lines.
    seen_at: Option<DateTime<Utc>>,
    meta: Option<AgentMeta>,
}

impl AgentFacts {
    /// The state of the agent while it runs, as its newest line shows it.
    fn state(&self) -> TaskState {
        self.fold.map_or(TaskState::Running, unended)
    }

    /// Quiet past the stall budget at `now`, by [`stall_reason`] over the newest line's fold.
    /// The done debounce never moves a visible state (done and generating both read running
    /// and both stall past the budget), so the line's own fold answers for any later `now`.
    fn stalls(&self, now: DateTime<Utc>, budget_secs: i64) -> bool {
        let (Some(fold), Some(idle)) = (self.fold, self.progress.idle_secs(now)) else {
            return false;
        };
        stall_reason(fold, idle, self.progress.last_tool(), budget_secs).is_some()
    }
}

/// One session's background tasks.
#[derive(Default)]
pub struct BackgroundTasks {
    entries: HashMap<String, Entry>,
    agents: HashMap<String, AgentFacts>,
    /// Agent ids already reported ended; a clear keeps them.
    stopped: HashSet<String>,
    /// Agent ids whose meta names a teammate: never entries.
    teammates: HashSet<String>,
    last_stop: Option<StopList>,
}

impl BackgroundTasks {
    /// A spool event: `SubagentStart`, `SubagentStop`, `Stop`, `SessionStart` and `SessionEnd`
    /// count; a `SubagentStop`'s `background_tasks` lags (the stopping agent lists itself), so
    /// only a `Stop` list is read.
    pub fn event(&mut self, event: &SpoolEvent) -> Change {
        let (payload, at) = (&event.payload, event.received_at);
        let agent = || task_id(payload.get("agent_id"));
        let mut change = Change::default();
        match event.event.as_str() {
            "SubagentStart" => {
                if let Some(id) = agent() {
                    self.started(&id, text(payload, "agent_type"), at, &mut change);
                }
            }
            // An unknown id is an internal fork (`agent_type: ""`, no `SubagentStart`).
            "SubagentStop" => {
                if let Some(id) = agent() {
                    let only = Some(TaskKind::Agent);
                    self.update(&id, only, &mut change, |entry| {
                        entry.touch(at);
                        entry.end(TaskState::Done, at, Proof::Evidence)
                    });
                }
            }
            "Stop" => {
                let list = payload.get("background_tasks").and_then(Value::as_array);
                if let Some(list) = list {
                    self.stop_list(at, list, &mut change);
                }
            }
            "SessionStart" => {
                if matches!(
                    text(payload, "source"),
                    Some("startup" | "resume" | "clear")
                ) {
                    self.clear(&mut change);
                }
            }
            "SessionEnd" => self.clear(&mut change),
            _ => {}
        }
        change
    }

    /// A mapper mark: its background evidence, or `agents_killed`, which clears the table. An
    /// interrupt ends no background task.
    pub fn mark(&mut self, mark: &RecordMark) -> Change {
        let mut change = Change::default();
        match mark {
            RecordMark::Background { at, evidence } => self.evidence(*at, evidence, &mut change),
            RecordMark::AgentsKilled { .. } => self.clear(&mut change),
            _ => {}
        }
        change
    }

    /// One raw line of agent `agent_id`'s transcript. Its `toolEndsTurn` result ends the agent;
    /// an assistant record stamped after the agent ended reopens it.
    pub fn agent_line(&mut self, agent_id: &str, line: &Value) -> Change {
        let mut change = Change::default();
        let Some(id) = bounded_id(agent_id) else {
            return change;
        };
        let facts = self.agents.entry(id.clone()).or_default();
        facts.progress.observe(line);
        let at = facts.progress.last_at();
        // At the line's own time: idle 0 (a line without a time has no idle time at all).
        let now = at.unwrap_or(DateTime::<Utc>::MIN_UTC);
        facts.fold = Some(facts.progress.state(now, DEFAULT_DONE_DEBOUNCE_SECS));
        facts.seen_at = facts.seen_at.max(at);
        let follow = Follow::of(facts);
        let kind = line.get("type").and_then(Value::as_str);
        let ends = line.get("toolEndsTurn").and_then(Value::as_bool) == Some(true);
        let ends_at = at.filter(|_| kind == Some("user") && ends);
        let assistant = kind == Some("assistant");
        self.update(&id, Some(TaskKind::Agent), &mut change, |entry| {
            entry.follow(&follow);
            if let Some(at) = ends_at {
                return entry.end(TaskState::Done, at, Proof::Evidence);
            }
            if assistant && entry.reopens(at) {
                entry.reopen(follow.state);
            } else if !entry.task.state.is_ended() {
                entry.task.state = follow.state;
            }
            false
        });
        change
    }

    /// An agent's meta file, read once: its fields, and `finished: true` ends the agent. A
    /// teammate's meta removes the id from the table for good.
    pub fn meta(&mut self, agent_id: &str, meta: &AgentMeta) -> Change {
        let mut change = Change::default();
        let Some(id) = bounded_id(agent_id) else {
            return change;
        };
        if meta.teammate.is_some() {
            change.changed = self.entries.remove(&id).is_some();
            self.teammates.insert(id);
            return change;
        }
        self.agents.entry(id.clone()).or_default().meta = Some(meta.clone());
        self.apply_meta(&id, &mut change);
        change
    }

    /// Turn each running agent whose transcript has been quiet past `stall_budget` at `now`
    /// stalled: the one state that changes with no new bytes. A stalled agent stays stalled
    /// until new evidence arrives (its idle time only grows), so a tick allocates nothing
    /// unless an agent stalls.
    pub fn reevaluate(&mut self, now: DateTime<Utc>, stall_budget: Duration) -> Change {
        let budget = i64::try_from(stall_budget.as_secs()).unwrap_or(i64::MAX);
        let mut changed = false;
        for entry in self.entries.values_mut() {
            let task = &mut entry.task;
            let running = matches!(task.state, TaskState::Running | TaskState::ToolWait);
            if task.kind != TaskKind::Agent || !running {
                continue;
            }
            if self
                .agents
                .get(&task.id)
                .is_some_and(|facts| facts.stalls(now, budget))
            {
                task.state = TaskState::Stalled;
                changed = true;
            }
        }
        let stopped = Vec::new();
        Change { changed, stopped }
    }

    /// Agents running or waiting on a tool: they hold a session's close. Shells, stalled agents
    /// and ended tasks do not.
    pub fn blocking(&self) -> usize {
        let blocks = |entry: &&Entry| {
            entry.task.kind == TaskKind::Agent
                && matches!(entry.task.state, TaskState::Running | TaskState::ToolWait)
        };
        self.entries.values().filter(blocks).count()
    }

    /// Every task, in `started_at` order, then by id.
    pub fn tasks(&self) -> Vec<BackgroundTask> {
        let mut tasks: Vec<BackgroundTask> =
            self.entries.values().map(|e| e.task.clone()).collect();
        tasks.sort_by(|a, b| (a.started_at, &a.id).cmp(&(b.started_at, &b.id)));
        tasks
    }

    fn clear(&mut self, change: &mut Change) {
        change.changed |= !self.entries.is_empty();
        self.entries.clear();
        self.last_stop = None;
    }

    /// Run `f` on the entry `id` (of kind `only`, when given) and note what it changed; `f`
    /// returns true when it ended the task for the first time.
    fn update(
        &mut self,
        id: &str,
        only: Option<TaskKind>,
        change: &mut Change,
        f: impl FnOnce(&mut Entry) -> bool,
    ) {
        let entry = self.entries.get_mut(id);
        let Some(entry) = entry.filter(|entry| only.is_none_or(|kind| entry.task.kind == kind))
        else {
            return;
        };
        let before = entry.task.clone();
        let ended = f(entry);
        change.changed |= entry.task != before;
        let agent = entry.task.kind == TaskKind::Agent;
        if ended && agent && self.stopped.insert(id.to_owned()) {
            change.stopped.push(id.to_owned());
        }
    }
}

/// An untrusted payload or list id: a bounded string.
fn task_id(value: Option<&Value>) -> Option<String> {
    bounded_id(value?.as_str()?)
}
