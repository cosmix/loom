//! One table entry: the published task, the rank of the source behind each field, and how the
//! task ended.

use chrono::{DateTime, Utc};

use super::{AgentFacts, BackgroundTask, TaskKind, TaskState};
use crate::agent_state::SubagentState;
use crate::plan_session::transcript::{cap_chars, LINE_CHARS};

/// The source of a field's value, lowest first: a source ranked above the one that set a field
/// replaces it, one ranked at or below never does, so the read order never changes the result.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, PartialOrd, Ord)]
pub(super) enum Rank {
    #[default]
    None,
    /// A `Stop` list entry or a notification.
    List,
    /// The agent's meta file.
    Meta,
    /// The `SubagentStart` hook.
    Hook,
    /// The launching call and its result, or a fork skill's launch record.
    Launch,
}

/// The field values one piece of evidence carries.
#[derive(Default)]
pub(super) struct Facts<'a> {
    pub(super) label: Option<&'a str>,
    pub(super) tool_use_id: Option<&'a str>,
    pub(super) agent_type: Option<&'a str>,
}

/// What proves an end, weakest first.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum Proof {
    /// A `Stop` list omitted the task: it ended, with no outcome known.
    List,
    /// The task's own end evidence: a hand-back, a `toolEndsTurn` result, meta `finished`, a
    /// `SubagentStop`, an interim notification or a `TaskStop` result.
    Evidence,
    /// A notification's status: the outcome, whatever ended the task first.
    Outcome,
}

/// An agent's fold, read at the time of its newest line.
pub(super) struct Follow {
    steps: u64,
    last_tool: Option<String>,
    seen_at: Option<DateTime<Utc>>,
    pub(super) state: TaskState,
}

impl Follow {
    pub(super) fn of(facts: &AgentFacts) -> Follow {
        let progress = &facts.progress;
        Follow {
            steps: u64::try_from(progress.assistant_entries()).unwrap_or(u64::MAX),
            last_tool: progress.last_tool().map(|tool| cap_chars(tool, LINE_CHARS)),
            seen_at: facts.seen_at,
            state: facts.state(),
        }
    }
}

pub(super) struct Entry {
    pub(super) task: BackgroundTask,
    /// The rank of the label's, the tool use id's and the agent type's source.
    ranks: [Rank; 3],
    /// The newest time of the evidence naming the task; a `Stop` list never counts.
    seen_at: Option<DateTime<Utc>>,
    ended_at: Option<DateTime<Utc>>,
    /// Ended only because a `Stop` list omitted it.
    weak: bool,
}

impl Entry {
    pub(super) fn new(id: &str, kind: TaskKind, at: DateTime<Utc>) -> Entry {
        let task = BackgroundTask {
            id: id.to_owned(),
            kind,
            label: None,
            agent_type: None,
            tool_use_id: None,
            state: TaskState::Running,
            started_at: at,
            last_activity_at: at,
            steps: (kind == TaskKind::Agent).then_some(0),
            last_tool: None,
            report_preview: None,
        };
        Entry {
            task,
            ranks: [Rank::None; 3],
            seen_at: None,
            ended_at: None,
            weak: false,
        }
    }

    /// Launch evidence at `at`: the earliest start is kept.
    pub(super) fn start(&mut self, at: DateTime<Utc>) {
        self.task.started_at = self.task.started_at.min(at);
        self.refresh_activity();
    }

    /// Evidence naming the task at `at`: the newest sighting is kept.
    pub(super) fn touch(&mut self, at: DateTime<Utc>) {
        self.seen_at = self.seen_at.max(Some(at));
        self.refresh_activity();
    }

    /// The newest sighting, else the start.
    pub(super) fn seen(&self) -> DateTime<Utc> {
        self.seen_at.unwrap_or(self.task.started_at)
    }

    fn refresh_activity(&mut self) {
        self.task.last_activity_at = self.seen().max(self.task.started_at);
    }

    /// Each field the evidence carries, when `by` outranks the field's source.
    pub(super) fn fill(&mut self, facts: &Facts<'_>, by: Rank) {
        let [label, tool_use, agent_type] = &mut self.ranks;
        fill(&mut self.task.label, label, facts.label, by);
        fill(&mut self.task.tool_use_id, tool_use, facts.tool_use_id, by);
        fill(&mut self.task.agent_type, agent_type, facts.agent_type, by);
    }

    /// The agent's steps, last tool and newest line time from its fold.
    pub(super) fn follow(&mut self, follow: &Follow) {
        self.task.steps = Some(follow.steps);
        self.task.last_tool.clone_from(&follow.last_tool);
        if let Some(at) = follow.seen_at {
            self.touch(at);
        }
    }

    /// End with `state` at `at` when `proof` is strong enough for the entry: a list ends only an
    /// open task, end evidence also replaces a list's end, and an outcome replaces any end.
    /// True when the task was open.
    pub(super) fn end(&mut self, state: TaskState, at: DateTime<Utc>, proof: Proof) -> bool {
        let open = !self.task.state.is_ended();
        let applies = match proof {
            Proof::List => open,
            Proof::Evidence => open || self.weak,
            Proof::Outcome => true,
        };
        if applies {
            self.task.state = state;
            self.weak = proof == Proof::List;
            self.ended_at = Some(self.ended_at.map_or(at, |ended| ended.min(at)));
        }
        applies && open
    }

    /// An ended agent with new evidence stamped after its end. A shell never reopens.
    pub(super) fn reopens(&self, at: Option<DateTime<Utc>>) -> bool {
        let later = self.ended_at.zip(at).is_some_and(|(ended, at)| at > ended);
        self.task.kind == TaskKind::Agent && later
    }

    pub(super) fn reopen(&mut self, state: TaskState) {
        self.task.state = state;
        self.ended_at = None;
        self.weak = false;
    }
}

/// The state of an unended agent whose fold shows `state`.
pub(super) fn unended(state: SubagentState) -> TaskState {
    match state {
        SubagentState::ToolWait => TaskState::ToolWait,
        _ => TaskState::Running,
    }
}

fn fill(slot: &mut Option<String>, rank: &mut Rank, value: Option<&str>, by: Rank) {
    let Some(value) = value.map(str::trim).filter(|value| !value.is_empty()) else {
        return;
    };
    if by > *rank {
        *slot = Some(cap_chars(value, LINE_CHARS));
        *rank = by;
    }
}
