//! State types for the TUI application.

use std::collections::{HashMap, HashSet};

use crate::daemon::StageInfo;
use crate::models::stage::StageStatus;

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

/// Unified stage entry for the table display.
#[derive(Clone)]
pub struct UnifiedStage {
    pub id: String,
    pub status: StageStatus,
    pub merged: bool,
    pub started_at: Option<chrono::DateTime<chrono::Utc>>,
    pub completed_at: Option<chrono::DateTime<chrono::Utc>>,
    pub level: usize,
    pub dependencies: Vec<String>,
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

        let stage_map: HashMap<&str, &StageInfo> =
            all_stages.iter().map(|s| (s.id.as_str(), *s)).collect();

        let mut levels: HashMap<String, usize> = HashMap::new();

        fn get_level(
            stage_id: &str,
            stage_map: &HashMap<&str, &StageInfo>,
            levels: &mut HashMap<String, usize>,
            visiting: &mut HashSet<String>,
        ) -> usize {
            if let Some(&level) = levels.get(stage_id) {
                return level;
            }

            if visiting.contains(stage_id) {
                return 0;
            }
            visiting.insert(stage_id.to_string());

            let stage = match stage_map.get(stage_id) {
                Some(s) => s,
                None => return 0,
            };

            let level = if stage.dependencies.is_empty() {
                0
            } else {
                stage
                    .dependencies
                    .iter()
                    .map(|dep| get_level(dep, stage_map, levels, visiting))
                    .max()
                    .unwrap_or(0)
                    + 1
            };

            visiting.remove(stage_id);
            levels.insert(stage_id.to_string(), level);
            level
        }

        for stage in &all_stages {
            let mut visiting = HashSet::new();
            get_level(&stage.id, &stage_map, &mut levels, &mut visiting);
        }

        levels
    }

    /// Build unified list of all stages for table display, sorted by execution order.
    pub fn unified_stages(&self) -> Vec<UnifiedStage> {
        let levels = self.compute_levels();
        let mut stages = Vec::new();
        let mut seen: HashSet<String> = HashSet::new();

        let to_unified =
            |stage: &StageInfo, levels: &HashMap<String, usize>| UnifiedStage {
                id: stage.id.clone(),
                status: stage.status.clone(),
                merged: stage.merged,
                started_at: Some(stage.started_at),
                completed_at: stage.completed_at,
                level: levels.get(&stage.id).copied().unwrap_or(0),
                dependencies: stage.dependencies.clone(),
            };

        for stage in &self.executing {
            if seen.insert(stage.id.clone()) {
                stages.push(to_unified(stage, &levels));
            }
        }

        for stage in &self.completed {
            if seen.insert(stage.id.clone()) {
                stages.push(to_unified(stage, &levels));
            }
        }

        for stage in &self.pending {
            if seen.insert(stage.id.clone()) {
                stages.push(to_unified(stage, &levels));
            }
        }

        for stage in &self.blocked {
            if seen.insert(stage.id.clone()) {
                stages.push(to_unified(stage, &levels));
            }
        }

        stages.sort_by(|a, b| a.level.cmp(&b.level).then_with(|| a.id.cmp(&b.id)));

        stages
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
