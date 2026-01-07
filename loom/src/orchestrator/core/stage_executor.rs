//! Stage execution logic - creating worktrees, spawning sessions

use anyhow::{Context, Result};

use crate::git;
use crate::models::session::Session;
use crate::models::stage::{Stage, StageStatus};
use crate::orchestrator::signals::{generate_signal, DependencyStatus};
use crate::orchestrator::spawner::{spawn_session, SpawnerConfig};

use super::persistence::Persistence;
use super::Orchestrator;

/// Trait for stage execution operations
pub(super) trait StageExecutor: Persistence {
    /// Start ready stages (create worktrees, spawn sessions)
    fn start_ready_stages(&mut self) -> Result<usize>;

    /// Process a single ready stage
    fn start_stage(&mut self, stage_id: &str) -> Result<()>;
}

impl StageExecutor for Orchestrator {
    fn start_ready_stages(&mut self) -> Result<usize> {
        let ready_stages = self.graph.ready_stages();
        let available_slots = self
            .config
            .max_parallel_sessions
            .saturating_sub(self.active_sessions.len());

        // Collect stage IDs first to avoid borrow checker issues
        let stage_ids: Vec<String> = ready_stages
            .iter()
            .take(available_slots)
            .map(|node| node.id.clone())
            .collect();

        let mut started = 0;
        for stage_id in stage_ids {
            self.start_stage(&stage_id)
                .with_context(|| format!("Failed to start stage: {stage_id}"))?;
            started += 1;
        }

        Ok(started)
    }

    fn start_stage(&mut self, stage_id: &str) -> Result<()> {
        let stage = self.load_stage(stage_id)?;

        // Skip if stage is already executing or completed
        if matches!(
            stage.status,
            StageStatus::Executing | StageStatus::Completed | StageStatus::Verified
        ) {
            return Ok(());
        }

        // Skip if stage is held
        if stage.held {
            return Ok(());
        }

        let worktree = git::get_or_create_worktree(stage_id, &self.config.repo_root)
            .with_context(|| format!("Failed to get or create worktree for stage: {stage_id}"))?;

        let session = Session::new();

        let deps = get_dependency_status(&stage, &self.graph);

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
            println!("  Started: {stage_id}");

            spawned
        } else {
            println!("Manual mode: Session setup for stage '{stage_id}'");
            println!("  Worktree: {}", worktree.path.display());
            println!("  Signal: {}", signal_path.display());
            println!(
                "  To start: cd {} && claude \"Read the signal file at {} and execute the assigned stage work.\"",
                worktree.path.display(),
                signal_path.display()
            );
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
        updated_stage.try_mark_executing()?;
        self.save_stage(&updated_stage)?;

        self.graph
            .mark_executing(stage_id)
            .context("Failed to mark stage as executing in graph")?;

        self.active_sessions
            .insert(stage_id.to_string(), spawned_session);
        self.active_worktrees.insert(stage_id.to_string(), worktree);

        Ok(())
    }
}

/// Get dependency status for signal generation
fn get_dependency_status(
    stage: &Stage,
    graph: &crate::plan::ExecutionGraph,
) -> Vec<DependencyStatus> {
    stage
        .dependencies
        .iter()
        .map(|dep_id| {
            let status = if let Some(node) = graph.get_node(dep_id) {
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
