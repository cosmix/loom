//! Main Orchestrator struct and public interface

use anyhow::{Context, Result};
use chrono::{DateTime, Utc};
use std::collections::{HashMap, HashSet};
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};

use crate::fs::work_integrity::validate_work_dir_state;
use crate::language::{detect_project_languages, DetectedLanguage};
use crate::models::session::Session;
use crate::models::stage::StageStatus;
use crate::models::worktree::Worktree;
use crate::orchestrator::adjudication::AdjudicatorRegistry;
use crate::orchestrator::monitor::{Monitor, MonitorConfig};
use crate::plan::schema::SandboxConfig;
use crate::plan::ExecutionGraph;
use crate::skills::SkillIndex;
use crate::utils::{cleanup_terminal, install_terminal_panic_hook};

use super::event_handler::EventHandler;
use super::persistence::Persistence;
use super::recovery::Recovery;
use super::stage_executor::StageExecutor;
use crate::orchestrator::liveness::LivenessService;
use crate::orchestrator::terminal::dispatcher::{
    resolve_stage_backend, BackendDispatcher, BackendNeeds,
};
use crate::orchestrator::terminal::BackendType;

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
    /// Plan-level sandbox configuration (defaults for all stages)
    pub sandbox_config: SandboxConfig,
    /// Shutdown flag for graceful termination (used by daemon)
    pub shutdown_flag: Option<Arc<AtomicBool>>,
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
            sandbox_config: SandboxConfig::default(),
            shutdown_flag: None,
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
    /// Multi-backend dispatcher — owns the native and/or container
    /// backends actually required by this run, and routes spawn/kill
    /// calls per-stage or per-session.
    pub(super) dispatcher: Arc<BackendDispatcher>,
    /// Backend-aware liveness probe (shared with the monitor thread).
    pub(super) liveness: LivenessService,
    /// Skill index for generating skill recommendations in signals
    pub(super) skill_index: Option<SkillIndex>,
    /// Detected project languages for signal skill injection
    pub(super) detected_languages: Vec<DetectedLanguage>,
    /// Stage IDs that have had a one-shot auto-merge retry attempted during
    /// this daemon session. Prevents the retry from firing on every 5-second
    /// poll for stages that remain stuck as `Completed + !merged`.
    ///
    /// Lifecycle: in-memory only; reset on next `loom run`.
    pub(super) merge_retry_attempted: HashSet<String>,
    /// Stage IDs for which spawn-time dependency verification has already
    /// logged a skip reason. Prevents the 5-second poll loop from flooding
    /// the logs when a dependent stage cannot start because of a phantom
    /// merge. Used by `stage_executor.rs::start_stage`.
    pub(super) spawn_skip_logged: HashSet<String>,
    /// Adjudicator registry — owns worker threads + completion channel.
    /// Disabled (workers never spawn) when `ANTHROPIC_API_KEY` is unset.
    pub(super) adjudicators: AdjudicatorRegistry,
}

impl Orchestrator {
    /// Create a new orchestrator from config and execution graph
    pub fn new(config: OrchestratorConfig, graph: ExecutionGraph) -> Result<Self> {
        let monitor_config = MonitorConfig {
            poll_interval: config.poll_interval,
            work_dir: config.work_dir.clone(),
            ..Default::default()
        };

        let mut monitor = Monitor::new(monitor_config);

        // Compute the per-stage backend overrides declared in the plan
        // so the dispatcher only constructs the backends actually needed.
        let stage_overrides = graph
            .all_nodes()
            .iter()
            .filter_map(|node| {
                crate::verify::transitions::load_stage(&node.id, &config.work_dir).ok()
            })
            .filter_map(|stage| {
                stage.execution_backend().and_then(|backend| {
                    // Validate at construction time so misconfigured
                    // plans fail before the first poll.
                    resolve_stage_backend(config.backend_type, Some(backend)).ok()
                })
            })
            .collect::<Vec<_>>();

        let needs = BackendNeeds::from_project_and_overrides(config.backend_type, &stage_overrides);
        let dispatcher = Arc::new(
            BackendDispatcher::for_plan(config.backend_type, needs, &config.work_dir)
                .context("Failed to construct backend dispatcher")?,
        );
        let liveness = LivenessService::new(Arc::clone(&dispatcher));
        monitor.set_liveness(liveness.clone());

        // Load skill index if skill routing is enabled
        let skill_index = if config.enable_skill_routing {
            Self::load_skill_index(&config)
        } else {
            None
        };

        // Detect project languages for skill recommendations
        let detected_languages = detect_project_languages(&config.repo_root);

        // Adjudicator: read ANTHROPIC_API_KEY at daemon startup. When the
        // env var is absent, the registry stays in disabled mode for the
        // entire daemon run and disputes route directly to NeedsHumanReview.
        let api_key = std::env::var("ANTHROPIC_API_KEY").ok().and_then(|k| {
            let trimmed = k.trim().to_string();
            if trimmed.is_empty() {
                None
            } else {
                Some(trimmed)
            }
        });
        if api_key.is_none() {
            tracing::warn!(
                target: "loom::adjudication",
                "ANTHROPIC_API_KEY not set; adjudicator is disabled for this daemon run",
            );
        }
        let adjudicators = AdjudicatorRegistry::new(api_key, &config.work_dir);

        // Reconcile any orphaned plan-amendment snapshots from a prior
        // crash. This is cheap (no I/O when no snapshots exist) and must
        // happen BEFORE the first poll tick reads stage acceptance arrays.
        if let Err(e) = crate::plan::amendment::verify_plan_versions_consistency(
            &resolve_plan_path_for_startup(&config.work_dir).unwrap_or_default(),
            &config.work_dir,
        ) {
            tracing::warn!(
                target: "loom::adjudication",
                error = %e,
                "plan-amendment consistency check failed at startup",
            );
        }

        Ok(Self {
            config,
            graph,
            active_sessions: HashMap::new(),
            active_worktrees: HashMap::new(),
            monitor,
            reported_crashes: HashSet::new(),
            dispatcher,
            liveness,
            skill_index,
            detected_languages,
            merge_retry_attempted: HashSet::new(),
            spawn_skip_logged: HashSet::new(),
            adjudicators,
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

        // Record start time
        let started_at = Utc::now();

        // Validate .work directory integrity before starting
        validate_work_dir_state(&self.config.repo_root)
            .context("Work directory integrity check failed")?;

        // Reconcile any active main-repo merge BEFORE syncing graph and
        // BEFORE recovering orphaned sessions. Recovery deletes orphaned
        // session files; attribution depends on their metadata. Sync reads
        // stage files into the graph; if reconcile flips the disk state
        // AFTER sync, the graph keeps the stale view and would queue
        // dependents based on a phantom merge.
        self.reconcile_and_update_graph()
            .context("Failed to reconcile active main-repo merge")?;

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

        // Adjudicator hooks: poll for pending disputes/verdicts and drain
        // completed worker threads. Idempotent + cheap when there are no
        // disputes on disk.
        self.check_pending_disputes()
            .context("Failed to check pending disputes")?;
        self.apply_pending_verdicts()
            .context("Failed to apply pending verdicts")?;
        self.drain_completed_adjudicator_workers()
            .context("Failed to drain completed adjudicator workers")?;

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
            // Check shutdown flag at start of each iteration
            if let Some(ref flag) = self.config.shutdown_flag {
                if flag.load(Ordering::Relaxed) {
                    println!("Orchestrator shutdown requested");
                    break;
                }
            }

            // Reconcile main-repo active merge BEFORE sync each iteration.
            // This catches `--no-verify --force-unsafe` produced phantom
            // merges and any state-divergence that a manual git operation
            // introduced between polls.
            self.reconcile_and_update_graph()
                .context("Failed to reconcile active main-repo merge")?;

            // Re-sync with stage files to pick up external changes
            // (e.g., stages verified via `loom verify` command)
            self.sync_graph_with_stage_files()
                .context("Failed to sync graph with stage files")?;

            // Sync queued status back to files so status display is accurate
            self.sync_queued_status_to_files()
                .context("Failed to sync queued status to files")?;

            // Adjudicator hooks (every tick): scan for new disputes,
            // apply ready verdicts, drain completed workers. The calls
            // are no-ops when there are no pending disputes on disk.
            self.check_pending_disputes()
                .context("Failed to check pending disputes")?;
            self.apply_pending_verdicts()
                .context("Failed to apply pending verdicts")?;
            self.drain_completed_adjudicator_workers()
                .context("Failed to drain completed adjudicator workers")?;

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
                println!("Sessions are now running. To view progress:");
                println!("  loom status               View overall progress");
                println!();
            }

            if !self.config.manual_mode {
                // Collect stage IDs BEFORE handle_events() to avoid missing completed stages
                // that get removed from active_sessions during event handling
                let stage_ids: Vec<String> = self.active_sessions.keys().cloned().collect();

                let events = self
                    .monitor
                    .poll()
                    .context("Failed to poll monitor for events")?;

                self.handle_events(events)
                    .context("Failed to handle monitor events")?;

                for stage_id in &stage_ids {
                    if let Ok(stage) = self.load_stage(stage_id) {
                        match stage.status {
                            StageStatus::Completed if !completed_stages.contains(stage_id) => {
                                completed_stages.push(stage_id.clone());
                            }
                            StageStatus::Blocked if !failed_stages.contains(stage_id) => {
                                failed_stages.push(stage_id.clone());
                            }
                            StageStatus::NeedsHandoff if !needs_handoff.contains(stage_id) => {
                                needs_handoff.push(stage_id.clone());
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
                // Normal mode: exit only when all stages are Completed or Skipped
                // Stages with failures do NOT trigger exit - orchestrator keeps running
                // for user intervention via `loom stage retry` or `loom status`
                if self.graph.is_complete() {
                    break;
                }
            }

            // Use shorter sleep intervals to check shutdown flag more frequently
            let poll_interval = self.config.poll_interval;
            let check_interval = Duration::from_millis(100);
            let mut elapsed = Duration::ZERO;

            while elapsed < poll_interval {
                if let Some(ref flag) = self.config.shutdown_flag {
                    if flag.load(Ordering::Relaxed) {
                        break;
                    }
                }
                std::thread::sleep(check_interval);
                elapsed += check_interval;
            }
        }

        // Restore terminal state before returning (clears \r-based status line)
        cleanup_terminal();

        Ok(OrchestratorResult {
            completed_stages,
            failed_stages,
            needs_handoff,
            total_sessions_spawned,
            started_at,
            completed_at: Utc::now(),
        })
    }

    /// Count currently running sessions
    pub fn running_session_count(&self) -> usize {
        self.active_sessions.len()
    }

    /// Poll `.work/disputes/` for new dispute requests and spawn worker
    /// threads as needed. See [`AdjudicatorRegistry::check_pending_disputes`].
    pub(crate) fn check_pending_disputes(&mut self) -> Result<()> {
        let work_dir = self.config.work_dir.clone();
        self.adjudicators.check_pending_disputes(&work_dir)
    }

    /// Apply verdict files written by adjudicator workers to the
    /// stage state. See [`AdjudicatorRegistry::apply_pending_verdicts`].
    pub(crate) fn apply_pending_verdicts(&mut self) -> Result<()> {
        let work_dir = self.config.work_dir.clone();
        self.adjudicators.apply_pending_verdicts(&work_dir)
    }

    /// Drain the worker→orchestrator completion channel and join any
    /// handles whose workers have reported done.
    pub(crate) fn drain_completed_adjudicator_workers(&mut self) -> Result<()> {
        let work_dir = self.config.work_dir.clone();
        self.adjudicators.drain_completed_workers(&work_dir)
    }

    /// Cooperative shutdown for the adjudicator registry. Called from
    /// the daemon shutdown path; signals workers to cancel and joins
    /// their handles until `deadline`.
    pub fn shutdown_adjudicators(&mut self, deadline: std::time::Instant) {
        self.adjudicators.shutdown(deadline);
    }
}

/// Resolve the plan source_path from `.work/config.toml` for daemon-
/// startup recovery. Returns `None` when there is no config yet (e.g.
/// during a test that doesn't initialise one).
fn resolve_plan_path_for_startup(work_dir: &std::path::Path) -> Option<PathBuf> {
    let cfg = crate::fs::work_dir::load_config(work_dir).ok().flatten()?;
    let path = cfg.source_path()?;
    if path.is_absolute() {
        Some(path)
    } else {
        let root = work_dir
            .canonicalize()
            .ok()
            .and_then(|wd| wd.parent().map(|p| p.to_path_buf()))?;
        Some(root.join(path))
    }
}

/// Result of orchestrator run
#[derive(Debug)]
pub struct OrchestratorResult {
    pub completed_stages: Vec<String>,
    pub failed_stages: Vec<String>,
    pub needs_handoff: Vec<String>,
    pub total_sessions_spawned: usize,
    /// When the orchestrator started running
    pub started_at: DateTime<Utc>,
    /// When the orchestrator finished running
    pub completed_at: DateTime<Utc>,
}

impl OrchestratorResult {
    pub fn is_success(&self) -> bool {
        self.failed_stages.is_empty() && self.needs_handoff.is_empty()
    }
}
