//! Monitor module for the loom orchestrator
//!
//! Polls `.work/` state files to detect stage completion, context exhaustion,
//! and session crashes. Enables event-driven orchestration without tight coupling.

use anyhow::{Context, Result};
use std::collections::HashMap;
use std::path::PathBuf;
use std::time::Duration;

use crate::handoff::{generate_handoff, HandoffContent};
use crate::models::constants::{CONTEXT_CRITICAL_THRESHOLD, CONTEXT_WARNING_THRESHOLD};
use crate::models::session::{Session, SessionStatus};
use crate::models::stage::{Stage, StageStatus};
use crate::orchestrator::spawner::{generate_crash_report, session_is_running, CrashReport};
use crate::parser::frontmatter::extract_yaml_frontmatter;

/// Configuration for the monitor
#[derive(Debug, Clone)]
pub struct MonitorConfig {
    pub poll_interval: Duration,
    pub work_dir: PathBuf,
    pub context_warning_threshold: f32,
    pub context_critical_threshold: f32,
}

impl Default for MonitorConfig {
    fn default() -> Self {
        Self {
            poll_interval: Duration::from_secs(5),
            work_dir: PathBuf::from(".work"),
            context_warning_threshold: CONTEXT_WARNING_THRESHOLD,
            context_critical_threshold: CONTEXT_CRITICAL_THRESHOLD,
        }
    }
}

/// Events detected by the monitor
#[derive(Debug, Clone, PartialEq)]
pub enum MonitorEvent {
    StageCompleted {
        stage_id: String,
    },
    StageBlocked {
        stage_id: String,
        reason: String,
    },
    SessionContextWarning {
        session_id: String,
        usage_percent: f32,
    },
    SessionContextCritical {
        session_id: String,
        usage_percent: f32,
    },
    SessionCrashed {
        session_id: String,
        stage_id: Option<String>,
        crash_report_path: Option<PathBuf>,
    },
    SessionNeedsHandoff {
        session_id: String,
        stage_id: String,
    },
    /// Stage is waiting for user input
    StageWaitingForInput {
        stage_id: String,
        session_id: Option<String>,
    },
    /// Stage resumed execution after user input
    StageResumedExecution {
        stage_id: String,
    },
}

/// Monitor state for tracking changes
pub struct Monitor {
    config: MonitorConfig,
    last_stage_states: HashMap<String, StageStatus>,
    last_session_states: HashMap<String, SessionStatus>,
    last_context_levels: HashMap<String, ContextHealth>,
}

impl Monitor {
    pub fn new(config: MonitorConfig) -> Self {
        Self {
            config,
            last_stage_states: HashMap::new(),
            last_session_states: HashMap::new(),
            last_context_levels: HashMap::new(),
        }
    }

    /// Poll once and return any events detected
    pub fn poll(&mut self) -> Result<Vec<MonitorEvent>> {
        let mut events = Vec::new();

        let stages = self.load_stages()?;
        let sessions = self.load_sessions()?;

        events.extend(self.detect_stage_changes(&stages));
        events.extend(self.detect_session_changes(&sessions));

        Ok(events)
    }

    /// Load all stages from .work/stages/
    fn load_stages(&self) -> Result<Vec<Stage>> {
        let stages_dir = self.config.work_dir.join("stages");
        if !stages_dir.exists() {
            return Ok(Vec::new());
        }

        let mut stages = Vec::new();
        let entries = std::fs::read_dir(&stages_dir).with_context(|| {
            format!("Failed to read stages directory: {}", stages_dir.display())
        })?;

        for entry in entries {
            let entry = entry?;
            let path = entry.path();

            if path.extension().and_then(|s| s.to_str()) == Some("md") {
                match self.load_stage_from_file(&path) {
                    Ok(stage) => stages.push(stage),
                    Err(e) => {
                        eprintln!(
                            "Warning: Failed to load stage from {}: {}",
                            path.display(),
                            e
                        );
                    }
                }
            }
        }

        Ok(stages)
    }

    /// Load a single stage from a markdown file
    fn load_stage_from_file(&self, path: &std::path::Path) -> Result<Stage> {
        let content = std::fs::read_to_string(path)
            .with_context(|| format!("Failed to read stage file: {}", path.display()))?;

        parse_stage_from_markdown(&content)
    }

    /// Load all sessions from .work/sessions/
    fn load_sessions(&self) -> Result<Vec<Session>> {
        let sessions_dir = self.config.work_dir.join("sessions");
        if !sessions_dir.exists() {
            return Ok(Vec::new());
        }

        let mut sessions = Vec::new();
        let entries = std::fs::read_dir(&sessions_dir).with_context(|| {
            format!(
                "Failed to read sessions directory: {}",
                sessions_dir.display()
            )
        })?;

        for entry in entries {
            let entry = entry?;
            let path = entry.path();

            if path.extension().and_then(|s| s.to_str()) == Some("md") {
                match self.load_session_from_file(&path) {
                    Ok(session) => sessions.push(session),
                    Err(e) => {
                        eprintln!(
                            "Warning: Failed to load session from {}: {}",
                            path.display(),
                            e
                        );
                    }
                }
            }
        }

        Ok(sessions)
    }

    /// Load a single session from a markdown file
    fn load_session_from_file(&self, path: &std::path::Path) -> Result<Session> {
        let content = std::fs::read_to_string(path)
            .with_context(|| format!("Failed to read session file: {}", path.display()))?;

        parse_session_from_markdown(&content)
    }

    /// Detect stage status changes
    fn detect_stage_changes(&mut self, stages: &[Stage]) -> Vec<MonitorEvent> {
        let mut events = Vec::new();

        for stage in stages {
            let previous_status = self.last_stage_states.get(&stage.id);
            let current_status = &stage.status;

            if previous_status != Some(current_status) {
                match current_status {
                    StageStatus::Completed => {
                        events.push(MonitorEvent::StageCompleted {
                            stage_id: stage.id.clone(),
                        });
                    }
                    StageStatus::Blocked => {
                        events.push(MonitorEvent::StageBlocked {
                            stage_id: stage.id.clone(),
                            reason: stage
                                .close_reason
                                .clone()
                                .unwrap_or_else(|| "Unknown reason".to_string()),
                        });
                    }
                    StageStatus::NeedsHandoff => {
                        if let Some(session_id) = &stage.session {
                            events.push(MonitorEvent::SessionNeedsHandoff {
                                session_id: session_id.clone(),
                                stage_id: stage.id.clone(),
                            });
                        }
                    }
                    StageStatus::WaitingForInput => {
                        events.push(MonitorEvent::StageWaitingForInput {
                            stage_id: stage.id.clone(),
                            session_id: stage.session.clone(),
                        });
                    }
                    _ => {}
                }

                // Check for transition FROM WaitingForInput TO Executing
                if previous_status == Some(&StageStatus::WaitingForInput)
                    && current_status == &StageStatus::Executing
                {
                    events.push(MonitorEvent::StageResumedExecution {
                        stage_id: stage.id.clone(),
                    });
                }

                self.last_stage_states
                    .insert(stage.id.clone(), current_status.clone());
            }
        }

        events
    }

    /// Detect session status changes and context levels
    fn detect_session_changes(&mut self, sessions: &[Session]) -> Vec<MonitorEvent> {
        let mut events = Vec::new();

        // Load stages for handoff generation
        let stages = self.load_stages().unwrap_or_default();

        for session in sessions {
            let previous_status = self.last_session_states.get(&session.id);
            let current_status = &session.status;

            let current_context_health =
                context_health(session.context_tokens, session.context_limit);
            let previous_context_health = self.last_context_levels.get(&session.id);

            if previous_status == Some(&SessionStatus::Running)
                && current_status == &SessionStatus::Running
            {
                if let Some(tmux_name) = &session.tmux_session {
                    if let Ok(is_alive) = self.check_tmux_session_alive(tmux_name) {
                        if !is_alive {
                            // Generate crash report
                            let crash_report_path = self
                                .handle_session_crash(session, "Tmux session no longer running");

                            events.push(MonitorEvent::SessionCrashed {
                                session_id: session.id.clone(),
                                stage_id: session.stage_id.clone(),
                                crash_report_path,
                            });

                            self.last_session_states
                                .insert(session.id.clone(), SessionStatus::Crashed);
                            continue;
                        }
                    }
                }
            }

            if previous_status != Some(current_status) {
                if current_status == &SessionStatus::Crashed {
                    // Generate crash report
                    let crash_report_path =
                        self.handle_session_crash(session, "Session marked as crashed");

                    events.push(MonitorEvent::SessionCrashed {
                        session_id: session.id.clone(),
                        stage_id: session.stage_id.clone(),
                        crash_report_path,
                    });
                }

                self.last_session_states
                    .insert(session.id.clone(), current_status.clone());
            }

            if previous_context_health != Some(&current_context_health) {
                match current_context_health {
                    ContextHealth::Yellow => {
                        events.push(MonitorEvent::SessionContextWarning {
                            session_id: session.id.clone(),
                            usage_percent: context_usage_percent(
                                session.context_tokens,
                                session.context_limit,
                            ),
                        });
                    }
                    ContextHealth::Red => {
                        let usage_percent =
                            context_usage_percent(session.context_tokens, session.context_limit);

                        events.push(MonitorEvent::SessionContextCritical {
                            session_id: session.id.clone(),
                            usage_percent,
                        });

                        // Generate handoff file if session has an associated stage
                        if let Some(stage_id) = &session.stage_id {
                            if let Some(stage) = stages.iter().find(|s| &s.id == stage_id) {
                                if let Ok(handoff_path) =
                                    self.handle_context_critical(session, stage)
                                {
                                    eprintln!(
                                        "Generated handoff for session {} at {}",
                                        session.id,
                                        handoff_path.display()
                                    );
                                }
                            }
                        }
                    }
                    _ => {}
                }

                self.last_context_levels
                    .insert(session.id.clone(), current_context_health);
            }
        }

        events
    }

    /// Check if a tmux session is still running
    fn check_tmux_session_alive(&self, tmux_name: &str) -> Result<bool> {
        session_is_running(tmux_name)
    }

    /// Handle critical context by generating a handoff file
    ///
    /// Called when a session reaches critical context threshold.
    /// Loads session and stage data, creates handoff content, and generates the handoff file.
    fn handle_context_critical(&self, session: &Session, stage: &Stage) -> Result<PathBuf> {
        let context_percent = context_usage_percent(session.context_tokens, session.context_limit);

        let goals = stage
            .description
            .clone()
            .unwrap_or_else(|| format!("Work on stage: {}", stage.name));

        let content = HandoffContent::new(session.id.clone(), stage.id.clone())
            .with_context_percent(context_percent)
            .with_goals(goals)
            .with_plan_id(stage.plan_id.clone())
            .with_next_steps(vec![
                "Review handoff and continue from current state".to_string()
            ]);

        generate_handoff(session, stage, content, &self.config.work_dir)
    }

    /// Handle session crash by generating a crash report
    ///
    /// Called when a session crash is detected.
    /// Creates a CrashReport and generates the crash report file.
    fn handle_session_crash(&self, session: &Session, reason: &str) -> Option<PathBuf> {
        let mut report = CrashReport::new(
            session.id.clone(),
            session.stage_id.clone(),
            reason.to_string(),
        );

        // Add tmux session info if available
        if let Some(tmux_session) = &session.tmux_session {
            report = report.with_tmux_session(tmux_session.clone());
        }

        // Generate the crash report
        let crashes_dir = self.config.work_dir.join("crashes");
        let logs_dir = self.config.work_dir.join("logs");

        match generate_crash_report(&report, &crashes_dir, &logs_dir) {
            Ok(path) => {
                eprintln!("Generated crash report: {}", path.display());
                Some(path)
            }
            Err(e) => {
                eprintln!(
                    "Failed to generate crash report for session '{}': {}",
                    session.id, e
                );
                None
            }
        }
    }
}

/// Context health level for a session
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ContextHealth {
    Green,
    Yellow,
    Red,
}

/// Calculate context health from tokens
pub fn context_health(tokens: u32, limit: u32) -> ContextHealth {
    if limit == 0 {
        return ContextHealth::Green;
    }

    let usage = tokens as f32 / limit as f32;

    if usage >= CONTEXT_WARNING_THRESHOLD {
        ContextHealth::Red
    } else if usage >= 0.60 {
        ContextHealth::Yellow
    } else {
        ContextHealth::Green
    }
}

/// Calculate context usage percentage
pub fn context_usage_percent(tokens: u32, limit: u32) -> f32 {
    if limit == 0 {
        return 0.0;
    }

    (tokens as f32 / limit as f32) * 100.0
}

/// Parse a Stage from markdown with YAML frontmatter
fn parse_stage_from_markdown(content: &str) -> Result<Stage> {
    let frontmatter = extract_yaml_frontmatter(content)?;

    let stage: Stage = serde_yaml::from_value(frontmatter)
        .context("Failed to deserialize Stage from frontmatter")?;

    Ok(stage)
}

/// Parse a Session from markdown with YAML frontmatter
fn parse_session_from_markdown(content: &str) -> Result<Session> {
    let frontmatter = extract_yaml_frontmatter(content)?;

    let session: Session = serde_yaml::from_value(frontmatter)
        .context("Failed to deserialize Session from frontmatter")?;

    Ok(session)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::constants::DEFAULT_CONTEXT_LIMIT;

    #[test]
    fn test_monitor_config_default() {
        let config = MonitorConfig::default();
        assert_eq!(config.poll_interval, Duration::from_secs(5));
        assert_eq!(config.work_dir, PathBuf::from(".work"));
        assert_eq!(config.context_warning_threshold, CONTEXT_WARNING_THRESHOLD);
        assert_eq!(
            config.context_critical_threshold,
            CONTEXT_CRITICAL_THRESHOLD
        );
    }

    #[test]
    fn test_context_health_green() {
        let tokens = 50_000;
        let limit = DEFAULT_CONTEXT_LIMIT;
        let health = context_health(tokens, limit);
        assert_eq!(health, ContextHealth::Green);
    }

    #[test]
    fn test_context_health_yellow() {
        let tokens = 130_000;
        let limit = DEFAULT_CONTEXT_LIMIT;
        let health = context_health(tokens, limit);
        assert_eq!(health, ContextHealth::Yellow);
    }

    #[test]
    fn test_context_health_red() {
        let tokens = 160_000;
        let limit = DEFAULT_CONTEXT_LIMIT;
        let health = context_health(tokens, limit);
        assert_eq!(health, ContextHealth::Red);
    }

    #[test]
    fn test_context_health_zero_limit() {
        let health = context_health(100, 0);
        assert_eq!(health, ContextHealth::Green);
    }

    #[test]
    fn test_context_usage_percent() {
        let tokens = 100_000;
        let limit = DEFAULT_CONTEXT_LIMIT;
        let percent = context_usage_percent(tokens, limit);
        assert_eq!(percent, 50.0);
    }

    #[test]
    fn test_context_usage_percent_zero_limit() {
        let percent = context_usage_percent(100, 0);
        assert_eq!(percent, 0.0);
    }

    #[test]
    fn test_detect_stage_completion() {
        let mut monitor = Monitor::new(MonitorConfig::default());

        let mut stage = Stage::new("test".to_string(), Some("Test stage".to_string()));
        stage.id = "stage-1".to_string();
        stage.status = StageStatus::Executing;

        // First poll - stage appears as Executing (no previous state, no event)
        let events = monitor.detect_stage_changes(&[stage.clone()]);
        assert_eq!(events.len(), 0);

        // Stage completes - should generate StageCompleted event
        stage.status = StageStatus::Completed;
        let events = monitor.detect_stage_changes(&[stage]);
        assert_eq!(events.len(), 1);

        if let MonitorEvent::StageCompleted { stage_id } = &events[0] {
            assert_eq!(stage_id, "stage-1");
        } else {
            panic!("Expected StageCompleted event");
        }
    }

    #[test]
    fn test_detect_session_crash() {
        let mut monitor = Monitor::new(MonitorConfig::default());

        let mut session = Session::new();
        session.id = "session-1".to_string();
        session.status = SessionStatus::Spawning;

        let events = monitor.detect_session_changes(&[session.clone()]);
        assert_eq!(events.len(), 0);

        session.status = SessionStatus::Crashed;
        let events = monitor.detect_session_changes(&[session]);
        assert_eq!(events.len(), 1);

        if let MonitorEvent::SessionCrashed {
            session_id,
            stage_id,
            crash_report_path: _,
        } = &events[0]
        {
            assert_eq!(session_id, "session-1");
            assert_eq!(stage_id, &None);
        } else {
            panic!("Expected SessionCrashed event");
        }
    }

    #[test]
    fn test_detect_context_warning() {
        let mut monitor = Monitor::new(MonitorConfig::default());

        let mut session = Session::new();
        session.id = "session-1".to_string();
        session.status = SessionStatus::Running;
        session.context_tokens = 50_000;

        monitor.detect_session_changes(&[session.clone()]);

        session.context_tokens = 130_000;
        let events = monitor.detect_session_changes(&[session]);
        assert_eq!(events.len(), 1);

        if let MonitorEvent::SessionContextWarning {
            session_id,
            usage_percent,
        } = &events[0]
        {
            assert_eq!(session_id, "session-1");
            assert!(usage_percent > &60.0 && usage_percent < &75.0);
        } else {
            panic!("Expected SessionContextWarning event");
        }
    }

    #[test]
    fn test_detect_context_critical() {
        let mut monitor = Monitor::new(MonitorConfig::default());

        let mut session = Session::new();
        session.id = "session-1".to_string();
        session.status = SessionStatus::Running;
        session.context_tokens = 50_000;

        monitor.detect_session_changes(&[session.clone()]);

        session.context_tokens = 160_000;
        let events = monitor.detect_session_changes(&[session]);
        assert_eq!(events.len(), 1);

        if let MonitorEvent::SessionContextCritical {
            session_id,
            usage_percent,
        } = &events[0]
        {
            assert_eq!(session_id, "session-1");
            assert!(usage_percent >= &75.0);
        } else {
            panic!("Expected SessionContextCritical event");
        }
    }

    #[test]
    fn test_parse_session_frontmatter() {
        let content = r#"---
id: session-abc-123
stage_id: stage-1
tmux_session: loom-session-abc
worktree_path: null
pid: 12345
status: running
context_tokens: 100000
context_limit: 200000
created_at: "2024-01-01T00:00:00Z"
last_active: "2024-01-01T01:00:00Z"
---

# Session Details
Test content
"#;

        let session = parse_session_from_markdown(content).expect("Should parse session");
        assert_eq!(session.id, "session-abc-123");
        assert_eq!(session.stage_id, Some("stage-1".to_string()));
        assert_eq!(session.status, SessionStatus::Running);
        assert_eq!(session.context_tokens, 100_000);
        assert_eq!(session.context_limit, 200_000);
    }

    #[test]
    fn test_parse_stage_frontmatter() {
        let content = r#"---
id: stage-1
name: Test Stage
description: A test stage
status: executing
dependencies: []
parallel_group: null
acceptance: []
files: []
plan_id: null
worktree: null
session: session-1
parent_stage: null
child_stages: []
created_at: "2024-01-01T00:00:00Z"
updated_at: "2024-01-01T01:00:00Z"
completed_at: null
close_reason: null
---

# Stage Details
Test content
"#;

        let stage = parse_stage_from_markdown(content).expect("Should parse stage");
        assert_eq!(stage.id, "stage-1");
        assert_eq!(stage.name, "Test Stage");
        assert_eq!(stage.status, StageStatus::Executing);
        assert_eq!(stage.session, Some("session-1".to_string()));
    }

    #[test]
    fn test_stage_blocked_event() {
        let mut monitor = Monitor::new(MonitorConfig::default());

        let mut stage = Stage::new("test".to_string(), Some("Test stage".to_string()));
        stage.id = "stage-1".to_string();
        stage.status = StageStatus::Executing;

        monitor.detect_stage_changes(&[stage.clone()]);

        stage.status = StageStatus::Blocked;
        stage.close_reason = Some("Dependency failed".to_string());

        let events = monitor.detect_stage_changes(&[stage]);
        assert_eq!(events.len(), 1);

        if let MonitorEvent::StageBlocked { stage_id, reason } = &events[0] {
            assert_eq!(stage_id, "stage-1");
            assert_eq!(reason, "Dependency failed");
        } else {
            panic!("Expected StageBlocked event");
        }
    }

    #[test]
    fn test_session_needs_handoff_event() {
        let mut monitor = Monitor::new(MonitorConfig::default());

        let mut stage = Stage::new("test".to_string(), Some("Test stage".to_string()));
        stage.id = "stage-1".to_string();
        stage.status = StageStatus::Executing;
        stage.session = Some("session-1".to_string());

        monitor.detect_stage_changes(&[stage.clone()]);

        stage.status = StageStatus::NeedsHandoff;

        let events = monitor.detect_stage_changes(&[stage]);
        assert_eq!(events.len(), 1);

        if let MonitorEvent::SessionNeedsHandoff {
            session_id,
            stage_id,
        } = &events[0]
        {
            assert_eq!(session_id, "session-1");
            assert_eq!(stage_id, "stage-1");
        } else {
            panic!("Expected SessionNeedsHandoff event");
        }
    }
}
