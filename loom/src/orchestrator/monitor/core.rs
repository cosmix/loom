//! Core Monitor implementation

use anyhow::{Context, Result};

use crate::models::session::Session;
use crate::models::stage::Stage;
use crate::orchestrator::liveness::LivenessService;
use crate::parser::frontmatter::parse_from_markdown;

use super::config::MonitorConfig;
use super::detection::Detection;
use super::events::MonitorEvent;
use super::handlers::Handlers;
use super::heartbeat::HeartbeatWatcher;

/// Monitor state for tracking changes
pub struct Monitor {
    config: MonitorConfig,
    pub(super) detection: Detection,
    pub(super) handlers: Handlers,
    pub(super) heartbeat_watcher: HeartbeatWatcher,
}

impl Monitor {
    pub fn new(config: MonitorConfig) -> Self {
        let heartbeat_watcher = HeartbeatWatcher::with_timeout(config.hung_timeout);
        Self {
            handlers: Handlers::new(config.clone(), None),
            detection: Detection::new(),
            heartbeat_watcher,
            config,
        }
    }

    /// Attach a backend-aware liveness service. The orchestrator calls
    /// this once the dispatcher is constructed; until then,
    /// `check_session_alive` falls back to the legacy host-PID probe.
    pub fn set_liveness(&mut self, liveness: LivenessService) {
        self.handlers.set_liveness(liveness);
    }

    /// Poll once and return any events detected
    pub fn poll(&mut self) -> Result<Vec<MonitorEvent>> {
        let mut events = Vec::new();

        let stages = self.load_stages()?;
        let sessions = self.load_sessions()?;

        events.extend(self.detection.detect_stage_changes(&stages));
        events.extend(
            self.detection
                .detect_session_changes(&sessions, &stages, &self.handlers),
        );

        // Poll for heartbeat updates and detect hung sessions
        events.extend(self.detection.detect_heartbeat_events(
            &sessions,
            &mut self.heartbeat_watcher,
            &self.config,
            &self.handlers,
        ));

        Ok(events)
    }

    /// Get handlers for generating handoffs and crash reports
    pub fn handlers(&self) -> &Handlers {
        &self.handlers
    }

    /// Load all stages from .work/stages/
    pub fn load_stages(&self) -> Result<Vec<Stage>> {
        crate::verify::transitions::list_all_stages(&self.config.work_dir)
    }

    /// Load all sessions from .work/sessions/
    pub fn load_sessions(&self) -> Result<Vec<Session>> {
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
                match load_session_from_file(&path) {
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
}

/// Load a single session from a markdown file
fn load_session_from_file(path: &std::path::Path) -> Result<Session> {
    let content = std::fs::read_to_string(path)
        .with_context(|| format!("Failed to read session file: {}", path.display()))?;

    parse_session_from_markdown(&content)
}

/// Parse a Session from markdown with YAML frontmatter
pub fn parse_session_from_markdown(content: &str) -> Result<Session> {
    parse_from_markdown(content, "Session")
}
