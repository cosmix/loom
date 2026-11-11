//! How launch evidence, `Stop` lists, notifications and end evidence change the table.

use std::collections::HashSet;

use chrono::{DateTime, Utc};
use serde_json::Value;

use super::entry::{Entry, Facts, Follow, Proof, Rank};
use super::{task_id, AgentFacts, BackgroundTasks, Change, StopList, TaskKind, TaskState};
use crate::plan_session::transcript::notification::{
    BackgroundEvidence, NotificationStatus, TaskNotification,
};

impl BackgroundTasks {
    pub(super) fn evidence(
        &mut self,
        at: DateTime<Utc>,
        evidence: &BackgroundEvidence,
        change: &mut Change,
    ) {
        match evidence {
            BackgroundEvidence::AgentLaunch {
                agent_id,
                tool_use_id,
                label,
            } => {
                let facts = launched(label, tool_use_id);
                self.launch(agent_id, TaskKind::Agent, at, &facts, Rank::Launch, change);
            }
            BackgroundEvidence::ShellLaunch {
                task_id,
                tool_use_id,
                label,
            } => {
                let facts = launched(label, tool_use_id);
                self.launch(task_id, TaskKind::Shell, at, &facts, Rank::Launch, change);
            }
            BackgroundEvidence::ForkLaunch { agent_id, label } => {
                let facts = Facts {
                    label: label.as_deref(),
                    ..Facts::default()
                };
                self.launch(agent_id, TaskKind::Agent, at, &facts, Rank::Launch, change);
            }
            BackgroundEvidence::TaskStopped { task_id } => {
                self.update(task_id, None, change, |entry| {
                    entry.touch(at);
                    entry.end(TaskState::Stopped, at, Proof::Evidence)
                });
            }
            BackgroundEvidence::Notification(notification) => {
                self.notified(notification, at, change);
            }
            BackgroundEvidence::Handback { agent_id, report } => {
                self.update(agent_id, Some(TaskKind::Agent), change, |entry| {
                    entry.touch(at);
                    entry.task.report_preview.clone_from(report);
                    entry.end(TaskState::Done, at, Proof::Evidence)
                });
            }
        }
    }

    /// Launch evidence at `at`: adds the entry, or fills the fields it lacks and keeps the
    /// earliest start. A `Stop` list's adoption only sets the start: a list is no sighting.
    pub(super) fn launch(
        &mut self,
        id: &str,
        kind: TaskKind,
        at: DateTime<Utc>,
        facts: &Facts<'_>,
        by: Rank,
        change: &mut Change,
    ) {
        if self.teammates.contains(id) {
            return;
        }
        let created = !self.entries.contains_key(id);
        if created {
            self.entries.insert(id.to_owned(), Entry::new(id, kind, at));
            change.changed = true;
        }
        self.update(id, None, change, |entry| {
            entry.start(at);
            if by > Rank::List {
                entry.touch(at);
            }
            entry.fill(facts, by);
            false
        });
        if created {
            self.created(id, change);
        }
    }

    /// A `SubagentStart`: launch evidence, and a new run of an agent that ended before it.
    pub(super) fn started(
        &mut self,
        id: &str,
        agent_type: Option<&str>,
        at: DateTime<Utc>,
        change: &mut Change,
    ) {
        let facts = Facts {
            agent_type,
            ..Facts::default()
        };
        self.launch(id, TaskKind::Agent, at, &facts, Rank::Hook, change);
        let facts = self.agents.get(id);
        let state = facts.map_or(TaskState::Running, AgentFacts::state);
        self.update(id, Some(TaskKind::Agent), change, |entry| {
            if entry.reopens(Some(at)) {
                entry.reopen(state);
            }
            false
        });
    }

    /// A new entry takes what was read before it: the agent's fold and meta file, and the end
    /// the newest `Stop` list already proved when it omits an entry started before it.
    fn created(&mut self, id: &str, change: &mut Change) {
        if let Some(facts) = self.agents.get(id) {
            let follow = Follow::of(facts);
            self.update(id, Some(TaskKind::Agent), change, |entry| {
                entry.follow(&follow);
                entry.task.state = follow.state;
                false
            });
        }
        self.apply_meta(id, change);
        let stop = self.last_stop.as_ref();
        let omitted = stop
            .filter(|stop| !stop.ids.contains(id))
            .map(|stop| stop.at);
        if let Some(at) = omitted {
            self.update(id, None, change, |entry| {
                entry.task.started_at < at && entry.end(TaskState::Done, at, Proof::List)
            });
        }
    }

    /// The agent's meta fields, ranked below its launch and `SubagentStart`; `finished: true`
    /// ends it at its newest sighting.
    pub(super) fn apply_meta(&mut self, id: &str, change: &mut Change) {
        let meta = self.agents.get(id).and_then(|facts| facts.meta.clone());
        let Some(meta) = meta else {
            return;
        };
        self.update(id, Some(TaskKind::Agent), change, |entry| {
            let facts = Facts {
                label: meta.description.as_deref(),
                tool_use_id: meta.tool_use_id.as_deref(),
                agent_type: meta.agent_type.as_deref(),
            };
            entry.fill(&facts, Rank::Meta);
            let at = entry.seen();
            meta.finished && entry.end(TaskState::Done, at, Proof::Evidence)
        });
    }

    /// A notification gives the outcome and fills the `tool_use_id`, also for a task already
    /// ended; one naming an unknown id adds an ended entry. The interim variant ends an agent
    /// like its hand-back does, and later records reopen it.
    fn notified(
        &mut self,
        notification: &TaskNotification,
        at: DateTime<Utc>,
        change: &mut Change,
    ) {
        let id = notification.task_id.as_str();
        if !self.entries.contains_key(id) {
            let summary = notification.summary.as_deref().unwrap_or_default();
            let agent = notification.interim || summary.starts_with("Agent ");
            let kind = if agent {
                TaskKind::Agent
            } else {
                TaskKind::Shell
            };
            self.launch(id, kind, at, &Facts::default(), Rank::List, change);
        }
        let (state, proof) = if notification.interim {
            (TaskState::Done, Proof::Evidence)
        } else {
            (outcome(&notification.status), Proof::Outcome)
        };
        let facts = Facts {
            tool_use_id: notification.tool_use_id.as_deref(),
            ..Facts::default()
        };
        self.update(id, None, change, |entry| {
            entry.touch(at);
            entry.fill(&facts, Rank::List);
            entry.end(state, at, proof)
        });
    }

    /// A `Stop` list names every task still running: it adopts an unknown shell (one launched
    /// inside a subagent) and ends each open entry started before it that it omits. It never
    /// adopts an unknown agent, and never reopens or re-creates an ended entry.
    pub(super) fn stop_list(&mut self, at: DateTime<Utc>, list: &[Value], change: &mut Change) {
        let mut ids = HashSet::new();
        for item in list {
            let Some(id) = task_id(item.get("id")) else {
                continue;
            };
            let shell = item.get("type").and_then(Value::as_str) == Some("shell");
            if shell && !self.entries.contains_key(&id) {
                let label = item.get("description").and_then(Value::as_str);
                let facts = Facts {
                    label,
                    ..Facts::default()
                };
                self.launch(&id, TaskKind::Shell, at, &facts, Rank::List, change);
            }
            ids.insert(id);
        }
        let omitted: Vec<String> = self
            .entries
            .iter()
            .filter(|(id, entry)| {
                let open = !entry.task.state.is_ended();
                open && entry.task.started_at < at && !ids.contains(*id)
            })
            .map(|(id, _)| id.clone())
            .collect();
        for id in omitted {
            self.update(&id, None, change, |entry| {
                entry.end(TaskState::Done, at, Proof::List)
            });
        }
        if self.last_stop.as_ref().is_none_or(|stop| stop.at <= at) {
            self.last_stop = Some(StopList { at, ids });
        }
    }
}

/// The fields a launch result carries.
fn launched<'a>(label: &'a Option<String>, tool_use_id: &'a Option<String>) -> Facts<'a> {
    Facts {
        label: label.as_deref(),
        tool_use_id: tool_use_id.as_deref(),
        agent_type: None,
    }
}

/// `completed` is done, `failed` failed, and `killed` or any other status stopped.
fn outcome(status: &NotificationStatus) -> TaskState {
    match status {
        NotificationStatus::Completed => TaskState::Done,
        NotificationStatus::Failed => TaskState::Failed,
        NotificationStatus::Killed | NotificationStatus::Other(_) => TaskState::Stopped,
    }
}
