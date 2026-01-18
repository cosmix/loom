//! Main Orchestrator struct and public interface

use anyhow::{Context, Result};
use std::collections::{HashMap, HashSet};
use std::path::PathBuf;
use std::time::{Duration, Instant};

use crate::models::session::Session;
use crate::models::stage::StageStatus;
use crate::models::worktree::Worktree;
use crate::orchestrator::monitor::{Monitor, MonitorConfig};
use crate::plan::ExecutionGraph;
use crate::skills::SkillIndex;
use crate::utils::{cleanup_terminal, install_terminal_panic_hook};

use super::event_handler::EventHandler;
use super::persistence::Persistence;
use super::recovery::Recovery;
use super::stage_executor::StageExecutor;
use crate::orchestrator::terminal::{create_backend, BackendType, TerminalBackend};

/// Configuration for the orchestrator
#[derive(Debug, Clone)]
pub struct OrchestratorConfig {
    pub max_parallel_sessions: usize,
    pub poll_interval: Duration,
    pub manual_mode: bool,
    /// Watch mode: continuously spawn ready stages until all are terminal
    pub watch_mode: bool,
    pub work_dir: PathBuf,
    pub repo_root: PathBuf,
    /// How often to print status updates during polling (default: 30 seconds)
    pub status_update_interval: Duration,
    /// Terminal backend to use for spawning sessions
    pub backend_type: BackendType,
    /// Enable automatic merge when stages complete (default: true)
    pub auto_merge: bool,
    /// Base branch to use for stages with no dependencies (from config.toml)
    pub base_branch: Option<String>,
    /// Directory containing skill files (default: ~/.claude/skills/)
    pub skills_dir: Option<PathBuf>,
    /// Enable skill routing recommendations in signals (default: true)
    pub enable_skill_routing: bool,
    /// Maximum number of skill recommendations per signal (default: 5)
    pub max_skill_recommendations: usize,
}

impl Default for OrchestratorConfig {
    fn default() -> Self {
        Self {
            max_parallel_sessions: 4,
            poll_interval: Duration::from_secs(5),
            manual_mode: false,
            watch_mode: false,
            work_dir: PathBuf::from(".work"),
            repo_root: PathBuf::from("."),
            status_update_interval: Duration::from_secs(30),
            backend_type: BackendType::Native,
            auto_merge: true,
            base_branch: None,
            skills_dir: None, // Will default to ~/.claude/skills/ when loading
            enable_skill_routing: true,
            max_skill_recommendations: 5,
        }
    }
}

/// Main orchestrator coordinating stage execution
pub struct Orchestrator {
    pub(super) config: OrchestratorConfig,
    pub(super) graph: ExecutionGraph,
    pub(super) active_sessions: HashMap<String, Session>,
    pub(super) active_worktrees: HashMap<String, Worktree>,
    pub(super) monitor: Monitor,
    /// Track reported crashes to avoid duplicate messages
    pub(super) reported_crashes: HashSet<String>,
    /// Terminal backend for spawning sessions
    pub(super) backend: Box<dyn TerminalBackend>,
    /// Skill index for generating skill recommendations in signals
    pub(super) skill_index: Option<SkillIndex>,
}

impl Orchestrator {
    /// Create a new orchestrator from config and execution graph
    pub fn new(config: OrchestratorConfig, graph: ExecutionGraph) -> Result<Self> {
        let monitor_config = MonitorConfig {
            poll_interval: config.poll_interval,
            work_dir: config.work_dir.clone(),
            ..Default::default()
        };

        let monitor = Monitor::new(monitor_config);

        // Create the terminal backend based on config
        let backend = create_backend(config.backend_type, &config.work_dir)
            .context("Failed to create terminal backend")?;

        // Load skill index if skill routing is enabled
        let skill_index = if config.enable_skill_routing {
            Self::load_skill_index(&config)
        } else {
            None
        };

        Ok(Self {
            config,
            graph,
            active_sessions: HashMap::new(),
            active_worktrees: HashMap::new(),
            monitor,
            reported_crashes: HashSet::new(),
            backend,
            skill_index,
        })
    }

    /// Load the skill index from the configured or default directory
    fn load_skill_index(config: &OrchestratorConfig) -> Option<SkillIndex> {
        // Determine skills directory: use config or default to ~/.claude/skills/
        let skills_dir = config.skills_dir.clone().unwrap_or_else(|| {
            dirs::home_dir()
                .map(|h| h.join(".claude").join("skills"))
                .unwrap_or_else(|| PathBuf::from(".claude/skills"))
        });

        if !skills_dir.exists() {
            return None;
        }

        match SkillIndex::load_from_directory(&skills_dir) {
            Ok(index) => {
                if index.is_empty() {
                    None
                } else {
                    Some(index)
                }
            }
            Err(e) => {
                eprintln!("Warning: Failed to load skill index: {e}");
                None
            }
        }
    }

    /// Main run loop - executes until all stages complete or error
    pub fn run(&mut self) -> Result<OrchestratorResult> {
        // Install panic hook to restore terminal on panic
        install_terminal_panic_hook();

        // Sync graph with existing stage states and recover orphaned sessions
        self.sync_graph_with_stage_files()
            .context("Failed to sync graph with existing stage files")?;

        let recovered = self
            .recover_orphaned_sessions()
            .context("Failed to recover orphaned sessions")?;

        if recovered > 0 {
            println!("Recovered {recovered} orphaned session(s) - stages reset to Ready");
        }

        // After recovery, ensure ready status is updated for all stages
        self.graph.refresh_ready_status();

        // Sync queued status from graph back to files so status display is accurate
        self.sync_queued_status_to_files()
            .context("Failed to sync queued status to files")?;

        // Spawn merge resolution sessions for stages stuck in MergeConflict/MergeBlocked
        let initial_merge_sessions = self
            .spawn_merge_resolution_sessions()
            .context("Failed to spawn merge resolution sessions")?;

        let mut total_sessions_spawned = initial_merge_sessions;
        let mut completed_stages = Vec::new();
        let mut failed_stages = Vec::new();
        let mut needs_handoff = Vec::new();
        let mut last_status_update = Instant::now();
        let mut printed_view_instructions = false;

        loop {
            // Re-sync with stage files to pick up external changes
            // (e.g., stages verified via `loom verify` command)
            self.sync_graph_with_stage_files()
                .context("Failed to sync graph with stage files")?;

            // Sync queued status back to files so status display is accurate
            self.sync_queued_status_to_files()
                .context("Failed to sync queued status to files")?;

            // Spawn merge resolution sessions for stages stuck in MergeConflict/MergeBlocked
            let merge_sessions_spawned = self
                .spawn_merge_resolution_sessions()
                .context("Failed to spawn merge resolution sessions")?;
            total_sessions_spawned += merge_sessions_spawned;

            let started = self
                .start_ready_stages()
                .context("Failed to start ready stages")?;
            total_sessions_spawned += started;

            // Print instructions on how to view sessions (once, after first batch starts)
            if started > 0 && !printed_view_instructions && !self.config.manual_mode {
                printed_view_instructions = true;
                println!();
                println!("Sessions are now running. To view them:");
                println!("  loom attach <stage-id>    Attach to a stage's session");
                println!("  loom attach all           View all sessions at once");
                println!("  loom status               View overall progress");
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

            // Exit conditions depend on mode
            if self.config.manual_mode {
                // Manual mode: exit after first batch
                break;
            }

            if self.config.watch_mode {
                // Watch mode: only exit when all stages are terminal
                if self.all_stages_terminal() {
                    println!();
                    println!("All stages are in terminal state (verified/blocked/held).");
                    break;
                }
            } else {
                // Normal mode: exit on completion or when failed with no running sessions
                if self.graph.is_complete() {
                    break;
                }

                // Check if there are ready stages waiting - don't exit if there are
                let ready_stages = self.graph.ready_stages();
                let has_ready_stages = !ready_stages.is_empty();

                // Only exit on failure if no sessions are running AND no stages are ready to start
                if !failed_stages.is_empty()
                    && self.running_session_count() == 0
                    && !has_ready_stages
                {
                    break;
                }
            }

            std::thread::sleep(self.config.poll_interval);
        }

        // Restore terminal state before returning (clears \r-based status line)
        cleanup_terminal();

        Ok(OrchestratorResult {
            completed_stages,
            failed_stages,
            needs_handoff,
            total_sessions_spawned,
        })
    }

    /// Count currently running sessions
    pub fn running_session_count(&self) -> usize {
        self.active_sessions.len()
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
