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

fn overlay_heartbeat_context(sessions: &mut [Session], events: &[MonitorEvent]) {
    for event in events {
        let MonitorEvent::HeartbeatReceived {
            stage_id,
            session_id,
            context_tokens: Some(context_tokens),
            ..
        } = event
        else {
            continue;
        };
        if let Some(session) = sessions.iter_mut().find(|session| {
            session.id == *session_id && session.stage_id.as_deref() == Some(stage_id)
        }) {
            // Resident context can decrease after compaction. The fresh
            // heartbeat replaces the persisted snapshot for this poll; the
            // event handler durably applies the same reading later.
            session.context_tokens = *context_tokens;
        }
    }
}

/// Monitor state for tracking changes
pub struct Monitor {
    config: MonitorConfig,
    pub(super) detection: Detection,
    pub(super) handlers: Handlers,
    pub(super) heartbeat_watcher: HeartbeatWatcher,
}

impl Monitor {
    pub fn new(mut config: MonitorConfig) -> Self {
        // Resolve the plan-wide ceilings once, here, rather than re-reading
        // `.loom/work/config.toml` for every session on every tick. Operators edit
        // the section between runs, exactly like `[terminal]`.
        config.context =
            crate::fs::work_dir::read_context_config(&config.work_dir).unwrap_or_default();

        // The staleness threshold lives on the stage, not the watcher —
        // `config.hung_timeout` is only the fallback for a session whose stage
        // cannot be resolved, and detection.rs applies it there.
        let heartbeat_watcher = HeartbeatWatcher::new();
        Self {
            handlers: Handlers::new(config.clone(), None),
            detection: Detection::new(),
            heartbeat_watcher,
            config,
        }
    }

    /// Attach the session liveness service. The orchestrator calls this
    /// once the `NativeBackend` is constructed; until then,
    /// `check_session_alive` falls back to the legacy host-PID probe.
    pub fn set_liveness(&mut self, liveness: LivenessService) {
        self.handlers.set_liveness(liveness);
    }

    /// Poll once and return any events detected
    pub fn poll(&mut self) -> Result<Vec<MonitorEvent>> {
        let mut events = Vec::new();

        let stages = self.load_stages()?;
        let mut sessions = self.load_sessions()?;

        // Poll heartbeat files before judging context. A persisted high-water
        // reading can be older than a fresh post-compaction heartbeat, and
        // killing from that stale snapshot before applying the heartbeat would
        // take down a session that is now safely below its backstop.
        let heartbeat_events = self.detection.detect_heartbeat_events(
            &sessions,
            &stages,
            &mut self.heartbeat_watcher,
            &self.config,
            &self.handlers,
        );
        overlay_heartbeat_context(&mut sessions, &heartbeat_events);

        // Detect sessions before stages so a BudgetExceeded latch established
        // on this fresh snapshot can suppress the generic NeedsHandoff retry.
        // Keep the public event order stable: stage, session, then heartbeat.
        let session_events =
            self.detection
                .detect_session_changes(&sessions, &stages, &self.handlers);
        let stage_events = self.detection.detect_stage_changes(&stages);
        events.extend(stage_events);
        events.extend(session_events);
        events.extend(heartbeat_events);

        // Keep an attached `loom attach` overview in sync with the session
        // reality this poll just observed: ended stages lose their pane, new
        // ones gain one, without the operator detaching and re-attaching.
        // Costs a single `stat` when nobody is attached (the common case).
        // Best-effort — a viewer that cannot be reconciled must never fail
        // the poll (O-4). Logged at `warn`, not `debug`: the executor stops
        // at the first failed step, so one failing step silently blocks
        // every later add/kill in the same pass — a `debug`-level failure
        // would leave that invisible to an operator who never raised the
        // log level.
        if let Err(error) =
            crate::orchestrator::terminal::tmux::refresh_attached_viewer(&self.config.work_dir)
        {
            tracing::warn!(error = %error, "Overview viewer reconcile failed");
        }

        Ok(events)
    }

    /// Get handlers for generating handoffs and crash reports
    pub fn handlers(&self) -> &Handlers {
        &self.handlers
    }

    /// The config this monitor resolved at construction, including the
    /// `[context]` ceilings it read off disk.
    pub fn config(&self) -> &MonitorConfig {
        &self.config
    }

    /// Load all stages from .loom/work/stages/
    pub fn load_stages(&self) -> Result<Vec<Stage>> {
        crate::verify::transitions::list_all_stages(&self.config.work_dir)
    }

    /// Load all sessions from .loom/work/sessions/
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
