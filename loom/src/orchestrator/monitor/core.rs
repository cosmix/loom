//! Core Monitor implementation

use anyhow::{Context, Result};

use crate::models::session::Session;
use crate::models::stage::Stage;
use crate::parser::frontmatter::extract_yaml_frontmatter;

use super::config::MonitorConfig;
use super::detection::Detection;
use super::events::MonitorEvent;
use super::handlers::Handlers;

/// Monitor state for tracking changes
pub struct Monitor {
    config: MonitorConfig,
    pub(super) detection: Detection,
    pub(super) handlers: Handlers,
}

impl Monitor {
    pub fn new(config: MonitorConfig) -> Self {
        Self {
            handlers: Handlers::new(config.clone()),
            detection: Detection::new(),
            config,
        }
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

        Ok(events)
    }

    /// Load all stages from .work/stages/
    pub fn load_stages(&self) -> Result<Vec<Stage>> {
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
                match load_stage_from_file(&path) {
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

/// Load a single stage from a markdown file
fn load_stage_from_file(path: &std::path::Path) -> Result<Stage> {
    let content = std::fs::read_to_string(path)
        .with_context(|| format!("Failed to read stage file: {}", path.display()))?;

    parse_stage_from_markdown(&content)
}

/// Load a single session from a markdown file
fn load_session_from_file(path: &std::path::Path) -> Result<Session> {
    let content = std::fs::read_to_string(path)
        .with_context(|| format!("Failed to read session file: {}", path.display()))?;

    parse_session_from_markdown(&content)
}

/// Parse a Stage from markdown with YAML frontmatter
pub fn parse_stage_from_markdown(content: &str) -> Result<Stage> {
    let frontmatter = extract_yaml_frontmatter(content)?;

    let stage: Stage = serde_yaml::from_value(frontmatter)
        .context("Failed to deserialize Stage from frontmatter")?;

    Ok(stage)
}

/// Parse a Session from markdown with YAML frontmatter
pub fn parse_session_from_markdown(content: &str) -> Result<Session> {
    let frontmatter = extract_yaml_frontmatter(content)?;

    let session: Session = serde_yaml::from_value(frontmatter)
        .context("Failed to deserialize Session from frontmatter")?;

    Ok(session)
}
