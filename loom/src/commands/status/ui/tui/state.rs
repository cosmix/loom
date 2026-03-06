//! State types for the TUI application.

use std::collections::{HashMap, HashSet, VecDeque};

use ratatui::style::Style;
use ratatui::text::{Line, Span};

use crate::commands::status::ui::theme::Theme;
use crate::daemon::StageInfo;
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
            self.scroll_y = (self.scroll_y + delta as u16).min(max_scroll);
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
    pub executing: Vec<StageInfo>,
    pub pending: Vec<StageInfo>,
    pub completed: Vec<StageInfo>,
    pub blocked: Vec<StageInfo>,
}

impl LiveStatus {
    pub fn total(&self) -> usize {
        self.executing.len() + self.pending.len() + self.completed.len() + self.blocked.len()
    }

    pub fn progress_pct(&self) -> f64 {
        let total = self.total();
        if total == 0 {
            0.0
        } else {
            self.completed.len() as f64 / total as f64
        }
    }

    /// Compute execution levels for all stages based on dependencies.
    pub fn compute_levels(&self) -> HashMap<String, usize> {
        let all_stages: Vec<&StageInfo> = self
            .executing
            .iter()
            .chain(self.pending.iter())
            .chain(self.completed.iter())
            .chain(self.blocked.iter())
            .collect();

        levels::compute_all_levels(&all_stages, |s| s.id.as_str(), |s| &s.dependencies)
    }

    /// Collect all stages into a deduplicated list, sorted by level then id.
    pub fn all_stages(&self) -> Vec<&StageInfo> {
        let levels = self.compute_levels();
        let mut stages = Vec::new();
        let mut seen: HashSet<String> = HashSet::new();

        for stage in self
            .executing
            .iter()
            .chain(self.completed.iter())
            .chain(self.pending.iter())
            .chain(self.blocked.iter())
        {
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
    prev_statuses: HashMap<String, StageStatus>,
}

impl TuiActivityLog {
    const MAX_ENTRIES: usize = 20;

    pub fn new() -> Self {
        Self {
            entries: VecDeque::new(),
            prev_statuses: HashMap::new(),
        }
    }

    /// Update the log by comparing current stage statuses against previous.
    /// Only logs meaningful transitions (started, completed, blocked, ready).
    pub fn update(&mut self, stages: &[&StageInfo]) {
        let now = chrono::Utc::now();

        for stage in stages {
            let prev = self.prev_statuses.get(&stage.id);
            let changed = prev.map(|p| p != &stage.status).unwrap_or(true);

            if !changed {
                continue;
            }

            let entry = match stage.status {
                StageStatus::Executing => Some(TuiActivityEntry {
                    timestamp: now,
                    icon: "●",
                    message: format!("{} started", stage.id),
                    style: Theme::status_executing(),
                }),
                StageStatus::Completed => Some(TuiActivityEntry {
                    timestamp: now,
                    icon: "✓",
                    message: format!("{} completed", stage.id),
                    style: Theme::status_completed(),
                }),
                StageStatus::Blocked => Some(TuiActivityEntry {
                    timestamp: now,
                    icon: "✗",
                    message: format!("{} blocked", stage.id),
                    style: Theme::status_blocked(),
                }),
                StageStatus::Queued => Some(TuiActivityEntry {
                    timestamp: now,
                    icon: "▶",
                    message: format!("{} ready", stage.id),
                    style: Theme::status_queued(),
                }),
                StageStatus::NeedsHandoff => Some(TuiActivityEntry {
                    timestamp: now,
                    icon: "⟳",
                    message: format!("{} needs handoff", stage.id),
                    style: Theme::status_warning(),
                }),
                _ => None,
            };

            if let Some(e) = entry {
                self.entries.push_back(e);
                while self.entries.len() > Self::MAX_ENTRIES {
                    self.entries.pop_front();
                }
            }

            self.prev_statuses
                .insert(stage.id.clone(), stage.status.clone());
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

impl Default for TuiActivityLog {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

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
    fn test_live_status_progress() {
        let mut status = LiveStatus::default();
        assert_eq!(status.total(), 0);
        assert_eq!(status.progress_pct(), 0.0);

        status.pending = vec![StageInfo {
            id: "a".to_string(),
            name: "Stage A".to_string(),
            session_pid: None,
            started_at: chrono::Utc::now(),
            completed_at: None,
            worktree_status: None,
            status: StageStatus::WaitingForDeps,
            merged: false,
            dependencies: vec![],
        }];
        status.completed = vec![StageInfo {
            id: "b".to_string(),
            name: "Stage B".to_string(),
            session_pid: None,
            started_at: chrono::Utc::now(),
            completed_at: Some(chrono::Utc::now()),
            worktree_status: None,
            status: StageStatus::Completed,
            merged: true,
            dependencies: vec![],
        }];

        assert_eq!(status.total(), 2);
        assert_eq!(status.progress_pct(), 0.5);
    }

    #[test]
    fn test_live_status_compute_levels() {
        let status = LiveStatus {
            executing: vec![],
            pending: vec![
                StageInfo {
                    id: "a".to_string(),
                    name: "A".to_string(),
                    session_pid: None,
                    started_at: chrono::Utc::now(),
                    completed_at: None,
                    worktree_status: None,
                    status: StageStatus::WaitingForDeps,
                    merged: false,
                    dependencies: vec![],
                },
                StageInfo {
                    id: "b".to_string(),
                    name: "B".to_string(),
                    session_pid: None,
                    started_at: chrono::Utc::now(),
                    completed_at: None,
                    worktree_status: None,
                    status: StageStatus::WaitingForDeps,
                    merged: false,
                    dependencies: vec!["a".to_string()],
                },
                StageInfo {
                    id: "c".to_string(),
                    name: "C".to_string(),
                    session_pid: None,
                    started_at: chrono::Utc::now(),
                    completed_at: None,
                    worktree_status: None,
                    status: StageStatus::WaitingForDeps,
                    merged: false,
                    dependencies: vec!["a".to_string(), "b".to_string()],
                },
            ],
            completed: vec![],
            blocked: vec![],
        };

        let levels = status.compute_levels();

        assert_eq!(levels.get("a"), Some(&0));
        assert_eq!(levels.get("b"), Some(&1));
        assert_eq!(levels.get("c"), Some(&2));
    }
}
