//! State types for the TUI application.

use std::collections::{HashMap, HashSet, VecDeque};

use ratatui::style::Style;
use ratatui::text::{Line, Span};

use crate::commands::status::data::{
    CompletionBlockerState, CompletionBlockerSummary, StageSummary, StatusData,
};
use crate::commands::status::ui::theme::Theme;
use crate::models::stage::StageStatus;
use crate::plan::graph::levels;

/// Graph state tracking for scroll position.
#[derive(Default)]
pub struct GraphState {
    /// Vertical scroll offset for the tree.
    pub scroll_y: u16,
    /// Total number of lines in the tree.
    pub total_lines: u16,
    /// Viewport height for scrolling bounds.
    pub viewport_height: u16,
}

impl GraphState {
    /// Scroll by a delta, clamping to bounds.
    pub fn scroll_by(&mut self, delta: i16) {
        if delta < 0 {
            self.scroll_y = self.scroll_y.saturating_sub((-delta) as u16);
        } else {
            let max_scroll = self.total_lines.saturating_sub(self.viewport_height);
            self.scroll_y = self.scroll_y.saturating_add(delta as u16).min(max_scroll);
        }
    }

    /// Jump to start.
    pub fn scroll_to_start(&mut self) {
        self.scroll_y = 0;
    }

    /// Jump to end.
    pub fn scroll_to_end(&mut self) {
        self.scroll_y = self.total_lines.saturating_sub(self.viewport_height);
    }
}

/// Live status data received from daemon.
#[derive(Default)]
pub struct LiveStatus {
    pub data: StatusData,
}

impl LiveStatus {
    /// Compute execution levels for all stages based on dependencies.
    pub fn compute_levels(&self) -> HashMap<String, usize> {
        levels::compute_all_levels(&self.data.stages, |s| s.id.as_str(), |s| &s.dependencies)
    }

    /// Collect all stages into a deduplicated list, sorted by level then id.
    pub fn all_stages(&self) -> Vec<&StageSummary> {
        self.all_stages_with_levels(&self.compute_levels())
    }

    /// Same as `all_stages`, but reuses an already-computed level map instead
    /// of recomputing it - the caller already needs both.
    pub fn all_stages_with_levels(&self, levels: &HashMap<String, usize>) -> Vec<&StageSummary> {
        let mut stages = Vec::new();
        let mut seen: HashSet<String> = HashSet::new();

        for stage in &self.data.stages {
            if seen.insert(stage.id.clone()) {
                stages.push(stage);
            }
        }

        stages.sort_by(|a, b| {
            let la = levels.get(&a.id).copied().unwrap_or(0);
            let lb = levels.get(&b.id).copied().unwrap_or(0);
            la.cmp(&lb).then_with(|| a.id.cmp(&b.id))
        });

        stages
    }
}

/// A single activity log entry for the TUI.
pub struct TuiActivityEntry {
    pub timestamp: chrono::DateTime<chrono::Utc>,
    pub icon: &'static str,
    pub message: String,
    pub style: Style,
}

/// Activity log that tracks stage state transitions for the TUI.
pub struct TuiActivityLog {
    entries: VecDeque<TuiActivityEntry>,
    previous: HashMap<String, StageActivityState>,
}

struct StageActivityState {
    status: StageStatus,
    blocker_state: Option<CompletionBlockerState>,
    blocker_fingerprint: Option<String>,
}

impl TuiActivityLog {
    const MAX_ENTRIES: usize = 20;

    pub fn new() -> Self {
        Self {
            entries: VecDeque::new(),
            previous: HashMap::new(),
        }
    }

    /// Update the log by comparing current stage statuses against previous.
    /// Only logs meaningful transitions (started, completed, blocked, ready).
    pub fn update(&mut self, stages: &[&StageSummary]) {
        let now = chrono::Utc::now();

        for stage in stages {
            let (status_changed, completion_changed) = {
                let previous = self.previous.get(&stage.id);
                (
                    previous
                        .map(|state| state.status != stage.status)
                        .unwrap_or(true),
                    blocker_changed(previous, stage.completion_blocker.as_ref()),
                )
            };
            if status_changed {
                if let Some(entry) = status_entry(stage, now) {
                    self.push(entry);
                }
            }
            if completion_changed {
                if let Some(entry) = blocker_entry(stage, now) {
                    self.push(entry);
                }
            }
            self.previous
                .insert(stage.id.clone(), StageActivityState::from(*stage));
        }
    }

    fn push(&mut self, entry: TuiActivityEntry) {
        self.entries.push_back(entry);
        while self.entries.len() > Self::MAX_ENTRIES {
            self.entries.pop_front();
        }
    }

    /// Render the most recent entries as TUI Lines, oldest first.
    pub fn render_lines(&self, count: usize) -> Vec<Line<'static>> {
        self.entries
            .iter()
            .rev()
            .take(count)
            .collect::<Vec<_>>()
            .into_iter()
            .rev()
            .map(|entry| {
                let time_str = entry.timestamp.format("%H:%M:%S").to_string();
                Line::from(vec![
                    Span::styled(time_str, Theme::dimmed()),
                    Span::raw("  "),
                    Span::styled(entry.icon.to_string(), entry.style),
                    Span::raw(" "),
                    Span::styled(entry.message.clone(), entry.style),
                ])
            })
            .collect()
    }

    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    pub fn len(&self) -> usize {
        self.entries.len()
    }
}

impl From<&StageSummary> for StageActivityState {
    fn from(stage: &StageSummary) -> Self {
        Self {
            status: stage.status.clone(),
            blocker_state: stage
                .completion_blocker
                .as_ref()
                .map(|blocker| blocker.state),
            blocker_fingerprint: stage
                .completion_blocker
                .as_ref()
                .map(|blocker| blocker.fingerprint.clone()),
        }
    }
}

fn blocker_changed(
    previous: Option<&StageActivityState>,
    blocker: Option<&CompletionBlockerSummary>,
) -> bool {
    blocker.is_some_and(|blocker| {
        previous.is_none_or(|previous| {
            previous.blocker_state != Some(blocker.state)
                || previous.blocker_fingerprint.as_deref() != Some(blocker.fingerprint.as_str())
        })
    })
}

fn blocker_entry(
    stage: &StageSummary,
    timestamp: chrono::DateTime<chrono::Utc>,
) -> Option<TuiActivityEntry> {
    let blocker = stage.completion_blocker.as_ref()?;
    let (icon, style) = match blocker.state {
        CompletionBlockerState::Pending => (StageStatus::Executing.icon(), Theme::status_warning()),
        CompletionBlockerState::Blocked | CompletionBlockerState::OwnershipUnknown => {
            (StageStatus::Blocked.icon(), Theme::status_blocked())
        }
    };
    Some(TuiActivityEntry {
        timestamp,
        icon,
        message: format!("{} {}", stage.id, blocker.activity_text()),
        style,
    })
}

fn status_entry(
    stage: &StageSummary,
    timestamp: chrono::DateTime<chrono::Utc>,
) -> Option<TuiActivityEntry> {
    let message = match &stage.status {
        StageStatus::Executing => "started",
        StageStatus::Completed => "completed",
        StageStatus::Blocked => "blocked",
        StageStatus::Queued => "ready",
        StageStatus::NeedsHandoff => "needs handoff",
        _ => return None,
    };
    Some(TuiActivityEntry {
        timestamp,
        icon: stage.status.icon(),
        message: format!("{} {message}", stage.id),
        style: stage.status.tui_style(),
    })
}

impl Default for TuiActivityLog {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn summary(id: &str, status: StageStatus, deps: &[&str]) -> StageSummary {
        StageSummary {
            id: id.to_string(),
            name: id.to_string(),
            status,
            stage_type: Default::default(),
            dependencies: deps.iter().map(|dep| (*dep).to_string()).collect(),
            context_tokens: None,
            elapsed_secs: None,
            execution_secs: None,
            base_branch: None,
            base_merged_from: vec![],
            failure_info: None,
            activity_status: Default::default(),
            last_tool: None,
            last_activity: None,
            staleness_secs: None,
            context_ceiling_tokens: None,
            review_reason: None,
            merged: false,
            merge_assumed: false,
            cleanup_warning: None,
            held: false,
            retry_count: 0,
            max_retries: None,
            pid: None,
            session_alive: false,
            model: String::new(),
            session_type: None,
            incoherence: None,
            execution_models: vec![],
            dispute_count: 0,
            judge_heartbeat_secs: None,
            session_backend: None,
            outgoing_session_exit_reason: None,
            completion_blocker: None,
        }
    }

    fn blocker(state: CompletionBlockerState, fingerprint: &str) -> CompletionBlockerSummary {
        CompletionBlockerSummary {
            state,
            fingerprint: fingerprint.to_owned(),
            failure_code: "acceptance_failed".to_owned(),
            summary: Some("verification failed".to_owned()),
            commit: "abc1234".to_owned(),
            repeat_count: 1,
            first_observed_at: None,
            last_observed_at: None,
            next_action: "inspect output".to_owned(),
        }
    }

    #[test]
    fn test_graph_state_default() {
        let state = GraphState::default();
        assert_eq!(state.scroll_y, 0);
        assert_eq!(state.total_lines, 0);
        assert_eq!(state.viewport_height, 0);
    }

    #[test]
    fn test_graph_state_scroll_by() {
        let mut state = GraphState {
            scroll_y: 5,
            total_lines: 20,
            viewport_height: 10,
        };

        state.scroll_by(3);
        assert_eq!(state.scroll_y, 8);

        state.scroll_by(-3);
        assert_eq!(state.scroll_y, 5);

        state.scroll_by(100);
        assert_eq!(state.scroll_y, 10);

        state.scroll_by(-100);
        assert_eq!(state.scroll_y, 0);
    }

    #[test]
    fn test_graph_state_scroll_to_start_end() {
        let mut state = GraphState {
            scroll_y: 5,
            total_lines: 20,
            viewport_height: 10,
        };

        state.scroll_to_end();
        assert_eq!(state.scroll_y, 10);

        state.scroll_to_start();
        assert_eq!(state.scroll_y, 0);
    }

    #[test]
    fn test_live_status_compute_levels() {
        let status = LiveStatus {
            data: StatusData {
                stages: vec![
                    summary("a", StageStatus::WaitingForDeps, &[]),
                    summary("b", StageStatus::WaitingForDeps, &["a"]),
                    summary("c", StageStatus::WaitingForDeps, &["a", "b"]),
                ],
                ..Default::default()
            },
        };

        let levels = status.compute_levels();

        assert_eq!(levels.get("a"), Some(&0));
        assert_eq!(levels.get("b"), Some(&1));
        assert_eq!(levels.get("c"), Some(&2));
    }

    #[test]
    fn blocker_transitions_deduplicate_and_track_fingerprint() {
        let mut log = TuiActivityLog::new();
        let mut stage = summary("stage", StageStatus::Executing, &[]);
        log.update(&[&stage]);
        let baseline = log.len();
        stage.completion_blocker = Some(blocker(CompletionBlockerState::Pending, "first"));
        log.update(&[&stage]);
        let after_pending = log.len();
        assert_eq!(after_pending, baseline + 1);
        log.update(&[&stage]);
        assert_eq!(log.len(), after_pending);
        stage.completion_blocker.as_mut().unwrap().state = CompletionBlockerState::Blocked;
        log.update(&[&stage]);
        assert_eq!(log.len(), baseline + 2);
        stage.completion_blocker.as_mut().unwrap().fingerprint = "second".to_owned();
        log.update(&[&stage]);
        assert_eq!(log.len(), baseline + 3);
    }
}
