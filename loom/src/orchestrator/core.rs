//! Core orchestrator for coordinating stage execution
//!
//! The orchestrator is the heart of `loom run`. It:
//! - Creates worktrees for ready stages
//! - Spawns Claude sessions in tmux
//! - Monitors stage completion and session health
//! - Handles crashes and context exhaustion
//! - Manages the execution graph

use anyhow::{bail, Context, Result};
use std::collections::HashMap;
use std::io::{self, Write};
use std::path::PathBuf;
use std::time::{Duration, Instant};

use crate::fs::stage_files::{find_stage_file, compute_stage_depths, stage_file_path, StageDependencies};
use crate::git;
use crate::models::session::Session;
use crate::models::stage::{Stage, StageStatus};
use crate::models::worktree::Worktree;
use crate::orchestrator::monitor::{Monitor, MonitorConfig, MonitorEvent};
use crate::orchestrator::signals::{generate_signal, remove_signal, DependencyStatus};
use crate::orchestrator::spawner::{
    check_tmux_available, kill_session, session_is_running, spawn_session, SpawnerConfig,
};
use crate::plan::graph::NodeStatus;
use crate::plan::ExecutionGraph;

/// Configuration for the orchestrator
#[derive(Debug, Clone)]
pub struct OrchestratorConfig {
    pub max_parallel_sessions: usize,
    pub poll_interval: Duration,
    pub manual_mode: bool,
    pub tmux_prefix: String,
    pub work_dir: PathBuf,
    pub repo_root: PathBuf,
    /// How often to print status updates during polling (default: 30 seconds)
    pub status_update_interval: Duration,
}

impl Default for OrchestratorConfig {
    fn default() -> Self {
        Self {
            max_parallel_sessions: 4,
            poll_interval: Duration::from_secs(5),
            manual_mode: false,
            tmux_prefix: "loom".to_string(),
            work_dir: PathBuf::from(".work"),
            repo_root: PathBuf::from("."),
            status_update_interval: Duration::from_secs(30),
        }
    }
}

/// Main orchestrator coordinating stage execution
pub struct Orchestrator {
    config: OrchestratorConfig,
    graph: ExecutionGraph,
    active_sessions: HashMap<String, Session>,
    active_worktrees: HashMap<String, Worktree>,
    monitor: Monitor,
}

impl Orchestrator {
    /// Create a new orchestrator from config and execution graph
    pub fn new(config: OrchestratorConfig, graph: ExecutionGraph) -> Self {
        let monitor_config = MonitorConfig {
            poll_interval: config.poll_interval,
            work_dir: config.work_dir.clone(),
            ..Default::default()
        };

        let monitor = Monitor::new(monitor_config);

        Self {
            config,
            graph,
            active_sessions: HashMap::new(),
            active_worktrees: HashMap::new(),
            monitor,
        }
    }

    /// Main run loop - executes until all stages complete or error
    pub fn run(&mut self) -> Result<OrchestratorResult> {
        if !self.config.manual_mode {
            check_tmux_available()
                .context("tmux is required for automatic session spawning. Use --manual to set up sessions yourself.")?;
        }

        // Sync graph with existing stage states and recover orphaned sessions
        self.sync_graph_with_stage_files()
            .context("Failed to sync graph with existing stage files")?;

        let recovered = self.recover_orphaned_sessions()
            .context("Failed to recover orphaned sessions")?;

        if recovered > 0 {
            println!("Recovered {} orphaned session(s) - stages reset to Ready", recovered);
        }

        let mut total_sessions_spawned = 0;
        let mut completed_stages = Vec::new();
        let mut failed_stages = Vec::new();
        let mut needs_handoff = Vec::new();
        let mut last_status_update = Instant::now();
        let mut printed_view_instructions = false;

        loop {
            let started = self
                .start_ready_stages()
                .context("Failed to start ready stages")?;
            total_sessions_spawned += started;

            // Print instructions on how to view sessions (once, after first batch starts)
            if started > 0 && !printed_view_instructions && !self.config.manual_mode {
                printed_view_instructions = true;
                println!();
                println!("Sessions are running in tmux. To view them:");
                println!("  loom attach <stage-id>    Attach to a stage's session");
                println!("  loom sessions list        List all active sessions");
                println!("  loom status               View overall progress");
                println!();
                println!("Tip: Run orchestrator in tmux for detach/reattach:");
                println!("  tmux new -s loom-orchestrator 'loom run'");
                println!();
            }

            if !self.config.manual_mode {
                let events = self
                    .monitor
                    .poll()
                    .context("Failed to poll monitor for events")?;

                self.handle_events(events)
                    .context("Failed to handle monitor events")?;

                for stage_id in self.active_sessions.keys() {
                    if let Ok(stage) = self.load_stage(stage_id) {
                        match stage.status {
                            StageStatus::Completed => {
                                if !completed_stages.contains(&stage_id.clone()) {
                                    completed_stages.push(stage_id.clone());
                                }
                            }
                            StageStatus::Blocked => {
                                if !failed_stages.contains(&stage_id.clone()) {
                                    failed_stages.push(stage_id.clone());
                                }
                            }
                            StageStatus::NeedsHandoff => {
                                if !needs_handoff.contains(&stage_id.clone()) {
                                    needs_handoff.push(stage_id.clone());
                                }
                            }
                            _ => {}
                        }
                    }
                }

                // Print periodic status updates to show progress
                if last_status_update.elapsed() >= self.config.status_update_interval {
                    self.print_status_update();
                    last_status_update = Instant::now();
                }
            }

            if self.graph.is_complete() {
                break;
            }

            if !failed_stages.is_empty() && self.running_session_count() == 0 {
                break;
            }

            if self.config.manual_mode {
                break;
            }

            std::thread::sleep(self.config.poll_interval);
        }

        Ok(OrchestratorResult {
            completed_stages,
            failed_stages,
            needs_handoff,
            total_sessions_spawned,
        })
    }

    /// Run a single stage by ID (for `loom run --stage <id>`)
    pub fn run_single(&mut self, stage_id: &str) -> Result<OrchestratorResult> {
        let node = self
            .graph
            .get_node(stage_id)
            .ok_or_else(|| anyhow::anyhow!("Stage not found: {stage_id}"))?;

        if node.status != crate::plan::NodeStatus::Ready {
            bail!(
                "Stage '{}' is not ready for execution. Current status: {:?}",
                stage_id,
                node.status
            );
        }

        self.start_stage(stage_id)
            .context("Failed to start stage")?;

        if self.config.manual_mode {
            return Ok(OrchestratorResult {
                completed_stages: Vec::new(),
                failed_stages: Vec::new(),
                needs_handoff: Vec::new(),
                total_sessions_spawned: 1,
            });
        }

        check_tmux_available().context("tmux is required for single stage execution")?;

        let mut completed = false;
        let mut failed = false;
        let mut needs_handoff = false;

        loop {
            let events = self.monitor.poll().context("Failed to poll monitor")?;

            for event in events {
                match event {
                    MonitorEvent::StageCompleted { stage_id: sid } if sid == stage_id => {
                        completed = true;
                    }
                    MonitorEvent::StageBlocked { stage_id: sid, .. } if sid == stage_id => {
                        failed = true;
                    }
                    MonitorEvent::SessionNeedsHandoff { stage_id: sid, .. } if sid == stage_id => {
                        needs_handoff = true;
                    }
                    MonitorEvent::SessionCrashed {
                        stage_id: Some(sid),
                        ..
                    } if sid == stage_id => {
                        failed = true;
                    }
                    _ => {}
                }
            }

            if completed || failed || needs_handoff {
                break;
            }

            std::thread::sleep(self.config.poll_interval);
        }

        Ok(OrchestratorResult {
            completed_stages: if completed {
                vec![stage_id.to_string()]
            } else {
                Vec::new()
            },
            failed_stages: if failed {
                vec![stage_id.to_string()]
            } else {
                Vec::new()
            },
            needs_handoff: if needs_handoff {
                vec![stage_id.to_string()]
            } else {
                Vec::new()
            },
            total_sessions_spawned: 1,
        })
    }

    /// Start ready stages (create worktrees, spawn sessions)
    fn start_ready_stages(&mut self) -> Result<usize> {
        let ready_stages = self.graph.ready_stages();
        let available_slots = self
            .config
            .max_parallel_sessions
            .saturating_sub(self.running_session_count());
        let mut started = 0;

        // Collect stage IDs first to avoid borrow checker issues
        let stage_ids: Vec<String> = ready_stages
            .iter()
            .take(available_slots)
            .map(|node| node.id.clone())
            .collect();

        for stage_id in stage_ids {
            self.start_stage(&stage_id)
                .with_context(|| format!("Failed to start stage: {stage_id}"))?;
            started += 1;
        }

        Ok(started)
    }

    /// Process a single ready stage
    fn start_stage(&mut self, stage_id: &str) -> Result<()> {
        let stage = self.load_stage(stage_id)?;

        // Skip if stage is already executing or completed
        if matches!(
            stage.status,
            StageStatus::Executing | StageStatus::Completed | StageStatus::Verified
        ) {
            return Ok(());
        }

        let worktree = git::get_or_create_worktree(stage_id, &self.config.repo_root)
            .with_context(|| format!("Failed to get or create worktree for stage: {stage_id}"))?;

        let session = Session::new();

        let deps = self.get_dependency_status(&stage);

        let signal_path = generate_signal(
            &session,
            &stage,
            &worktree,
            &deps,
            None,
            &self.config.work_dir,
        )
        .context("Failed to generate signal file")?;

        // Store original session ID to verify consistency after spawn
        let original_session_id = session.id.clone();

        let spawned_session = if !self.config.manual_mode {
            let spawner_config = SpawnerConfig {
                max_parallel_sessions: self.config.max_parallel_sessions,
                tmux_prefix: self.config.tmux_prefix.clone(),
            };

            let spawned = spawn_session(&stage, &worktree, &spawner_config, session, &signal_path)
                .with_context(|| format!("Failed to spawn session for stage: {stage_id}"))?;

            // Print confirmation that stage was started
            println!(
                "  Started: {} (tmux: {}-{})",
                stage_id, self.config.tmux_prefix, stage_id
            );

            spawned
        } else {
            println!("Manual mode: Session setup for stage '{stage_id}'");
            println!("  Worktree: {}", worktree.path.display());
            println!("  Signal: {}", signal_path.display());
            println!("  To start: cd {} && claude \"Read the signal file at {} and execute the assigned stage work.\"",
                     worktree.path.display(), signal_path.display());
            session
        };

        // Verify session ID consistency (signal file uses this ID)
        debug_assert_eq!(
            original_session_id, spawned_session.id,
            "Session ID mismatch: signal file created with '{}' but saving session with '{}'",
            original_session_id, spawned_session.id
        );

        self.save_session(&spawned_session)?;

        let mut updated_stage = stage;
        updated_stage.assign_session(spawned_session.id.clone());
        updated_stage.set_worktree(Some(worktree.id.clone()));
        updated_stage.mark_executing();
        self.save_stage(&updated_stage)?;

        self.graph
            .mark_executing(stage_id)
            .context("Failed to mark stage as executing in graph")?;

        self.active_sessions
            .insert(stage_id.to_string(), spawned_session);
        self.active_worktrees.insert(stage_id.to_string(), worktree);

        Ok(())
    }

    /// Handle monitor events
    fn handle_events(&mut self, events: Vec<MonitorEvent>) -> Result<()> {
        for event in events {
            match event {
                MonitorEvent::StageCompleted { stage_id } => {
                    self.on_stage_completed(&stage_id)?;
                }
                MonitorEvent::StageBlocked { stage_id, reason } => {
                    eprintln!("Stage '{stage_id}' blocked: {reason}");
                    self.graph.mark_blocked(&stage_id)?;
                }
                MonitorEvent::SessionContextWarning {
                    session_id,
                    usage_percent,
                } => {
                    eprintln!(
                        "Warning: Session '{session_id}' context at {usage_percent:.1}%"
                    );
                }
                MonitorEvent::SessionContextCritical {
                    session_id,
                    usage_percent,
                } => {
                    eprintln!(
                        "Critical: Session '{session_id}' context at {usage_percent:.1}%"
                    );
                }
                MonitorEvent::SessionCrashed {
                    session_id,
                    stage_id,
                } => {
                    self.on_session_crashed(&session_id, stage_id)?;
                }
                MonitorEvent::SessionNeedsHandoff {
                    session_id,
                    stage_id,
                } => {
                    self.on_needs_handoff(&session_id, &stage_id)?;
                }
                MonitorEvent::StageWaitingForInput {
                    stage_id,
                    session_id,
                } => {
                    if let Some(sid) = session_id {
                        eprintln!("Stage '{stage_id}' (session '{sid}') is waiting for user input");
                    } else {
                        eprintln!("Stage '{stage_id}' is waiting for user input");
                    }
                }
                MonitorEvent::StageResumedExecution { stage_id } => {
                    eprintln!("Stage '{stage_id}' resumed execution after user input");
                }
            }
        }
        Ok(())
    }

    /// Handle stage completion
    fn on_stage_completed(&mut self, stage_id: &str) -> Result<()> {
        self.graph.mark_completed(stage_id)?;

        if let Some(session) = self.active_sessions.remove(stage_id) {
            remove_signal(&session.id, &self.config.work_dir)?;
            let _ = kill_session(&session);
        }

        self.active_worktrees.remove(stage_id);

        Ok(())
    }

    /// Handle session crash
    fn on_session_crashed(&mut self, session_id: &str, stage_id: Option<String>) -> Result<()> {
        eprintln!("Session '{session_id}' crashed");

        if let Some(sid) = stage_id {
            self.active_sessions.remove(&sid);

            let mut stage = self.load_stage(&sid)?;
            stage.status = StageStatus::Blocked;
            stage.close_reason = Some("Session crashed".to_string());
            self.save_stage(&stage)?;

            self.graph.mark_blocked(&sid)?;
        }

        Ok(())
    }

    /// Handle context exhaustion (needs handoff)
    fn on_needs_handoff(&mut self, session_id: &str, stage_id: &str) -> Result<()> {
        eprintln!("Session '{session_id}' needs handoff for stage '{stage_id}'");

        let mut stage = self.load_stage(stage_id)?;
        stage.mark_needs_handoff();
        self.save_stage(&stage)?;

        Ok(())
    }

    /// Load stage definition from .work/stages/
    fn load_stage(&self, stage_id: &str) -> Result<Stage> {
        let stages_dir = self.config.work_dir.join("stages");

        // Use find_stage_file to handle both prefixed and non-prefixed formats
        let stage_path = find_stage_file(&stages_dir, stage_id)?;

        if stage_path.is_none() {
            // Stage file doesn't exist - create from graph
            let node = self
                .graph
                .get_node(stage_id)
                .ok_or_else(|| anyhow::anyhow!("Stage not found in graph: {stage_id}"))?;

            let mut stage = Stage::new(node.name.clone(), None);
            stage.id = stage_id.to_string();
            stage.dependencies = node.dependencies.clone();
            stage.parallel_group = node.parallel_group.clone();

            return Ok(stage);
        }

        let stage_path = stage_path.unwrap();
        let content = std::fs::read_to_string(&stage_path)
            .with_context(|| format!("Failed to read stage file: {}", stage_path.display()))?;

        let frontmatter = extract_yaml_frontmatter(&content)?;
        let stage: Stage = serde_yaml::from_value(frontmatter)
            .context("Failed to deserialize Stage from frontmatter")?;

        Ok(stage)
    }

    /// Save stage state to .work/stages/
    fn save_stage(&self, stage: &Stage) -> Result<()> {
        let stages_dir = self.config.work_dir.join("stages");
        if !stages_dir.exists() {
            std::fs::create_dir_all(&stages_dir).context("Failed to create stages directory")?;
        }

        // Check if a file already exists for this stage (with any prefix)
        let stage_path = if let Some(existing_path) = find_stage_file(&stages_dir, &stage.id)? {
            // Update existing file in place
            existing_path
        } else {
            // New stage - compute depth using the execution graph
            let depth = self.compute_stage_depth(&stage.id);
            stage_file_path(&stages_dir, depth, &stage.id)
        };

        let yaml = serde_yaml::to_string(stage).context("Failed to serialize stage to YAML")?;

        let content = format!(
            "---\n{}---\n\n# Stage: {}\n\n{}\n",
            yaml,
            stage.name,
            stage
                .description
                .as_deref()
                .unwrap_or("No description provided.")
        );

        std::fs::write(&stage_path, content)
            .with_context(|| format!("Failed to write stage file: {}", stage_path.display()))?;

        Ok(())
    }

    /// Compute stage depth using the execution graph
    fn compute_stage_depth(&self, stage_id: &str) -> usize {
        // Build dependency info from the graph
        let stage_deps: Vec<StageDependencies> = self
            .graph
            .all_nodes()
            .iter()
            .map(|node| StageDependencies {
                id: node.id.clone(),
                dependencies: node.dependencies.clone(),
            })
            .collect();

        // Compute depths for all stages
        let depths = compute_stage_depths(&stage_deps).unwrap_or_default();

        // Return depth for this stage
        depths.get(stage_id).copied().unwrap_or(0)
    }

    /// Save session state to .work/sessions/
    fn save_session(&self, session: &Session) -> Result<()> {
        let sessions_dir = self.config.work_dir.join("sessions");
        if !sessions_dir.exists() {
            std::fs::create_dir_all(&sessions_dir)
                .context("Failed to create sessions directory")?;
        }

        let session_path = sessions_dir.join(format!("{}.md", session.id));

        let yaml = serde_yaml::to_string(session).context("Failed to serialize session to YAML")?;

        let content = format!(
            "---\n{}---\n\n# Session: {}\n\nStatus: {:?}\n",
            yaml, session.id, session.status
        );

        std::fs::write(&session_path, content)
            .with_context(|| format!("Failed to write session file: {}", session_path.display()))?;

        Ok(())
    }

    /// Get dependency status for signal generation
    fn get_dependency_status(&self, stage: &Stage) -> Vec<DependencyStatus> {
        stage
            .dependencies
            .iter()
            .map(|dep_id| {
                let status = if let Some(node) = self.graph.get_node(dep_id) {
                    format!("{:?}", node.status)
                } else {
                    "Unknown".to_string()
                };

                DependencyStatus {
                    stage_id: dep_id.clone(),
                    name: dep_id.clone(),
                    status,
                }
            })
            .collect()
    }

    /// Sync the execution graph with existing stage file statuses.
    ///
    /// This is called on startup to ensure the in-memory graph reflects
    /// the actual state persisted in `.work/stages/` files.
    fn sync_graph_with_stage_files(&mut self) -> Result<()> {
        let stages_dir = self.config.work_dir.join("stages");
        if !stages_dir.exists() {
            return Ok(());
        }

        // Read all stage files and sync their status to the graph
        for entry in std::fs::read_dir(&stages_dir)? {
            let entry = entry?;
            let path = entry.path();

            if path.extension().and_then(|s| s.to_str()) != Some("md") {
                continue;
            }

            // Extract stage ID from filename (handles prefixed format like 01-stage-id.md)
            let filename = path.file_stem().and_then(|s| s.to_str()).unwrap_or("");
            let stage_id = if let Some(rest) = filename.strip_prefix(|c: char| c.is_ascii_digit()) {
                // Remove leading digits and dash
                rest.trim_start_matches(|c: char| c.is_ascii_digit() || c == '-')
            } else {
                filename
            };

            if stage_id.is_empty() {
                continue;
            }

            // Load the stage and sync status
            if let Ok(stage) = self.load_stage(stage_id) {
                match stage.status {
                    StageStatus::Completed | StageStatus::Verified => {
                        // Mark as completed in graph (ignore errors for stages not in graph)
                        let _ = self.graph.mark_completed(stage_id);
                    }
                    StageStatus::Executing => {
                        // Will be handled by orphan detection
                    }
                    StageStatus::Blocked => {
                        // Will be handled by orphan recovery
                    }
                    _ => {}
                }
            }
        }

        Ok(())
    }

    /// Recover orphaned sessions (tmux died but session/stage files exist).
    ///
    /// For each orphaned session, resets the stage to Ready so it can be restarted.
    /// Returns the number of recovered sessions.
    fn recover_orphaned_sessions(&mut self) -> Result<usize> {
        let sessions_dir = self.config.work_dir.join("sessions");
        if !sessions_dir.exists() {
            return Ok(0);
        }

        let mut recovered = 0;

        for entry in std::fs::read_dir(&sessions_dir)? {
            let entry = entry?;
            let path = entry.path();

            if path.extension().and_then(|s| s.to_str()) != Some("md") {
                continue;
            }

            // Load session from file
            let content = std::fs::read_to_string(&path)?;
            let session: Session = match extract_yaml_frontmatter(&content) {
                Ok(yaml) => match serde_yaml::from_value(yaml) {
                    Ok(s) => s,
                    Err(_) => continue,
                },
                Err(_) => continue,
            };

            // Check if session has a tmux session name
            let tmux_name = match &session.tmux_session {
                Some(name) => name.clone(),
                None => continue,
            };

            // Check if tmux session is still running
            let is_running = session_is_running(&tmux_name).unwrap_or(false);

            if !is_running {
                // Orphaned session - get stage ID and reset it
                if let Some(stage_id) = &session.stage_id {
                    // Load the stage
                    if let Ok(mut stage) = self.load_stage(stage_id) {
                        // Only recover if stage was Executing or Blocked due to crash
                        if matches!(stage.status, StageStatus::Executing | StageStatus::Blocked) {
                            println!("  Recovering orphaned stage: {} (was {:?})", stage_id, stage.status);

                            // Reset stage to Ready
                            stage.status = StageStatus::Ready;
                            stage.session = None;
                            stage.close_reason = None;
                            stage.updated_at = chrono::Utc::now();
                            self.save_stage(&stage)?;

                            // Update graph - first ensure it's not in a terminal state
                            if let Some(node) = self.graph.get_node(stage_id) {
                                if node.status != NodeStatus::Completed {
                                    // Mark as ready in graph
                                    self.graph.mark_ready(stage_id)?;
                                }
                            }

                            recovered += 1;
                        }
                    }
                }

                // Remove the orphaned session file
                let _ = std::fs::remove_file(&path);

                // Remove the orphaned signal file
                let signal_path = self.config.work_dir.join("signals").join(format!("{}.md", session.id));
                let _ = std::fs::remove_file(&signal_path);
            }
        }

        Ok(recovered)
    }

    /// Count currently running sessions
    fn running_session_count(&self) -> usize {
        self.active_sessions.len()
    }

    /// Print a status update showing current stage counts
    fn print_status_update(&self) {
        let nodes = self.graph.all_nodes();
        let mut running = 0;
        let mut pending = 0;
        let mut completed = 0;
        let mut blocked = 0;

        for node in nodes {
            match node.status {
                NodeStatus::Executing => running += 1,
                NodeStatus::Pending | NodeStatus::Ready => pending += 1,
                NodeStatus::Completed => completed += 1,
                NodeStatus::Blocked => blocked += 1,
            }
        }

        let mut status_parts = vec![
            format!("{running} running"),
            format!("{pending} pending"),
            format!("{completed} completed"),
        ];

        if blocked > 0 {
            status_parts.push(format!("{blocked} blocked"));
        }

        print!(
            "\r[Polling... {}] (Ctrl+C to detach, 'loom status' for details)    ",
            status_parts.join(", ")
        );
        // Flush stdout to ensure the status line appears immediately
        let _ = io::stdout().flush();
    }
}

/// Result of orchestrator run
#[derive(Debug)]
pub struct OrchestratorResult {
    pub completed_stages: Vec<String>,
    pub failed_stages: Vec<String>,
    pub needs_handoff: Vec<String>,
    pub total_sessions_spawned: usize,
}

impl OrchestratorResult {
    pub fn is_success(&self) -> bool {
        self.failed_stages.is_empty() && self.needs_handoff.is_empty()
    }
}

/// Extract YAML frontmatter from markdown content
fn extract_yaml_frontmatter(content: &str) -> Result<serde_yaml::Value> {
    let lines: Vec<&str> = content.lines().collect();

    if lines.is_empty() || !lines[0].trim().starts_with("---") {
        bail!("No frontmatter delimiter found");
    }

    let mut end_idx = None;
    for (idx, line) in lines.iter().enumerate().skip(1) {
        if line.trim().starts_with("---") {
            end_idx = Some(idx);
            break;
        }
    }

    let end_idx = end_idx.ok_or_else(|| anyhow::anyhow!("Frontmatter not properly closed"))?;

    let yaml_content = lines[1..end_idx].join("\n");

    serde_yaml::from_str(&yaml_content).context("Failed to parse YAML frontmatter")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::plan::schema::StageDefinition;

    fn create_test_config() -> OrchestratorConfig {
        OrchestratorConfig {
            max_parallel_sessions: 2,
            poll_interval: Duration::from_millis(100),
            manual_mode: true,
            tmux_prefix: "test".to_string(),
            work_dir: PathBuf::from("/tmp/test-work"),
            repo_root: PathBuf::from("/tmp/test-repo"),
            status_update_interval: Duration::from_secs(30),
        }
    }

    fn create_simple_graph() -> ExecutionGraph {
        let stages = vec![StageDefinition {
            id: "stage-1".to_string(),
            name: "Stage 1".to_string(),
            description: None,
            dependencies: vec![],
            parallel_group: None,
            acceptance: vec![],
            files: vec![],
        }];

        ExecutionGraph::build(stages).unwrap()
    }

    #[test]
    fn test_orchestrator_config_default() {
        let config = OrchestratorConfig::default();
        assert_eq!(config.max_parallel_sessions, 4);
        assert_eq!(config.poll_interval, Duration::from_secs(5));
        assert!(!config.manual_mode);
        assert_eq!(config.tmux_prefix, "loom");
    }

    #[test]
    fn test_orchestrator_result_success() {
        let result = OrchestratorResult {
            completed_stages: vec!["stage-1".to_string()],
            failed_stages: vec![],
            needs_handoff: vec![],
            total_sessions_spawned: 1,
        };

        assert!(result.is_success());
    }

    #[test]
    fn test_orchestrator_result_failure() {
        let result = OrchestratorResult {
            completed_stages: vec![],
            failed_stages: vec!["stage-1".to_string()],
            needs_handoff: vec![],
            total_sessions_spawned: 1,
        };

        assert!(!result.is_success());
    }

    #[test]
    fn test_orchestrator_result_needs_handoff() {
        let result = OrchestratorResult {
            completed_stages: vec![],
            failed_stages: vec![],
            needs_handoff: vec!["stage-1".to_string()],
            total_sessions_spawned: 1,
        };

        assert!(!result.is_success());
    }

    #[test]
    fn test_running_session_count() {
        let config = create_test_config();
        let graph = create_simple_graph();
        let orchestrator = Orchestrator::new(config, graph);

        assert_eq!(orchestrator.running_session_count(), 0);
    }

    #[test]
    fn test_extract_yaml_frontmatter() {
        let content = r#"---
id: stage-1
name: Test Stage
status: Pending
---

# Stage Details
Test content
"#;

        let result = extract_yaml_frontmatter(content);
        assert!(result.is_ok());

        let value = result.unwrap();
        assert!(value.get("id").is_some());
        assert!(value.get("name").is_some());
    }

    #[test]
    fn test_extract_yaml_frontmatter_no_delimiter() {
        let content = "No frontmatter here";
        let result = extract_yaml_frontmatter(content);
        assert!(result.is_err());
    }

    #[test]
    fn test_extract_yaml_frontmatter_not_closed() {
        let content = r#"---
id: stage-1
name: Test Stage
"#;
        let result = extract_yaml_frontmatter(content);
        assert!(result.is_err());
    }
}
