//! Core Monitor implementation

use anyhow::{Context, Result};

use crate::models::session::Session;
use crate::models::stage::Stage;
use crate::orchestrator::liveness::LivenessService;
use crate::parser::frontmatter::parse_from_markdown;

use super::completion_blockers::{filter_owned_hung_events, BlockerScan, CompletionBlockerWatch};
use super::config::MonitorConfig;
use super::detection::Detection;
use super::events::MonitorEvent;
use super::handlers::Handlers;
use super::heartbeat::HeartbeatWatcher;
use super::input_wait::reconcile_stale_input_waits;

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

fn reconcile_codex_evidence(work_dir: &std::path::Path, sessions: &[Session]) {
    let report = match crate::codex_lifecycle::reconcile_codex_jobs(work_dir, sessions) {
        Ok(report) => report,
        Err(error) => {
            tracing::warn!(error = %error, "Codex lifecycle reconcile failed");
            return;
        }
    };
    for entry in report.entries {
        let unknown = entry.is_unknown();
        tracing::debug!(
            stage_id = %entry.stage_id.as_str(),
            loom_session_id = ?entry.loom_session_id.as_deref(),
            unit_id = ?entry.unit_id.as_deref(),
            invocation_id = ?entry.invocation_id.as_deref(),
            outcome = ?entry.outcome,
            detail = %entry.detail.as_str(),
            "Codex lifecycle reconcile entry"
        );
        if unknown {
            tracing::warn!(
                stage_id = %entry.stage_id.as_str(),
                unit_id = ?entry.unit_id.as_deref(),
                invocation_id = ?entry.invocation_id.as_deref(),
                detail = %entry.detail.as_str(),
                "Codex lifecycle evidence is unknown"
            );
        }
    }
}

/// Monitor state for tracking changes
pub struct Monitor {
    config: MonitorConfig,
    pub(super) detection: Detection,
    pub(super) handlers: Handlers,
    pub(super) heartbeat_watcher: HeartbeatWatcher,
    completion_blocker_watch: CompletionBlockerWatch,
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
            completion_blocker_watch: CompletionBlockerWatch::default(),
            config,
        }
    }

    /// Attach the session liveness service. The orchestrator calls this
    /// once the `NativeBackend` is constructed; until then,
    /// `check_session_alive` falls back to the legacy host-PID probe.
    pub fn set_liveness(&mut self, liveness: LivenessService) {
        self.handlers.set_liveness(liveness);
    }

    fn scan_completion_blockers(&mut self, stages: &[Stage]) -> BlockerScan {
        let repo_root = crate::fs::work_dir::WorkDir::new(&self.config.work_dir)
            .ok()
            .and_then(|work_dir| work_dir.project_root().map(std::path::Path::to_path_buf))
            .unwrap_or_else(|| self.config.work_dir.clone());
        self.completion_blocker_watch.scan(
            stages,
            &self.config.work_dir,
            &repo_root,
            chrono::Utc::now(),
        )
    }

    /// Poll once and return any events detected
    pub fn poll(&mut self) -> Result<Vec<MonitorEvent>> {
        let mut events = Vec::new();

        let stages = self.load_stages()?;
        let mut sessions = self.load_sessions()?;
        reconcile_codex_evidence(&self.config.work_dir, &sessions);
        let blocker_scan = self.scan_completion_blockers(&stages);

        // Poll heartbeat files before judging context. A persisted high-water
        // reading can be older than a fresh post-compaction heartbeat, and
        // killing from that stale snapshot before applying the heartbeat would
        // take down a session that is now safely below its backstop.
        let mut heartbeat_events = self.detection.detect_heartbeat_events(
            &sessions,
            &stages,
            &mut self.heartbeat_watcher,
            &self.config,
            &self.handlers,
        );
        filter_owned_hung_events(&mut heartbeat_events, &blocker_scan.owned_stage_ids);
        overlay_heartbeat_context(&mut sessions, &heartbeat_events);

        // Detect sessions before stages so a BudgetExceeded latch established
        // on this fresh snapshot can suppress the generic NeedsHandoff retry.
        // Keep the public event order stable: stage, completion diagnostics,
        // session, then heartbeat.
        let session_events =
            self.detection
                .detect_session_changes(&sessions, &stages, &self.handlers);
        let stage_events = self.detection.detect_stage_changes(&stages);

        // A stage whose session kept making tool progress after flipping to
        // WaitingForInput was never actually waiting on a person (Claude Code
        // can drive the AskUserQuestion pipeline without a logged tool_use).
        // Resolve it back to Executing on disk now; the NEXT poll observes the
        // WaitingForInput -> Executing transition through the normal detector
        // path and emits StageResumedExecution from it.
        reconcile_stale_input_waits(&self.config.work_dir, &stages, &self.heartbeat_watcher);

        events.extend(stage_events);
        events.extend(blocker_scan.events);
        events.extend(session_events);
        events.extend(heartbeat_events);

        self.refresh_attached_viewer();

        Ok(events)
    }

    /// Keep an attached overview in sync with the session reality just polled.
    /// Best-effort: viewer failure must never fail the monitor poll.
    fn refresh_attached_viewer(&self) {
        if let Err(error) =
            crate::orchestrator::terminal::tmux::refresh_attached_viewer(&self.config.work_dir)
        {
            tracing::warn!(error = %error, "Overview viewer reconcile failed");
        }
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
