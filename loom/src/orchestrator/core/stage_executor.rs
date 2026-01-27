//! Stage execution logic - creating worktrees, spawning sessions

use anyhow::{Context, Result};
use chrono::Utc;

use crate::git;
use crate::git::worktree::setup_worktree_hooks;
use crate::models::failure::{FailureInfo, FailureType};
use crate::models::session::Session;
use crate::models::stage::{Stage, StageStatus, StageType};
use crate::orchestrator::hooks::{find_hooks_dir, setup_hooks_for_worktree, HooksConfig};
use crate::orchestrator::signals::{
    generate_knowledge_signal, generate_signal_with_skills, DependencyStatus,
};

use super::persistence::Persistence;
use super::Orchestrator;

/// Trait for stage execution operations
pub(super) trait StageExecutor: Persistence {
    /// Start ready stages (create worktrees, spawn sessions)
    fn start_ready_stages(&mut self) -> Result<usize>;

    /// Process a single ready stage
    fn start_stage(&mut self, stage_id: &str) -> Result<()>;

    /// Start a knowledge stage (runs in main repo without worktree)
    fn start_knowledge_stage(&mut self, stage: Stage) -> Result<()>;
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
        let mut stage = self.load_stage(stage_id)?;

        // Skip if stage is already executing or completed
        if matches!(
            stage.status,
            StageStatus::Executing | StageStatus::Completed
        ) {
            return Ok(());
        }

        // Skip if stage is held
        if stage.held {
            return Ok(());
        }

        // Transition through Queued if currently WaitingForDeps to reduce race window
        if stage.status == StageStatus::WaitingForDeps {
            stage.try_mark_queued()?;
            self.save_stage(&stage)?;
        }

        // Knowledge stages run in main repo without a worktree - mark executing immediately
        if stage.stage_type == StageType::Knowledge {
            stage.try_mark_executing()?;
            self.save_stage(&stage)?;
            self.graph
                .mark_executing(stage_id)
                .context("Failed to mark stage as executing in graph")?;
            return self.start_knowledge_stage(stage);
        }

        // For worktree stages: attempt worktree creation BEFORE marking as Executing
        // This ensures we don't leave stages in Executing state if worktree creation fails

        // Resolve the base branch for worktree creation
        let resolved = match git::resolve_base_branch(
            stage_id,
            &stage.dependencies,
            &self.graph,
            &self.config.repo_root,
            self.config.base_branch.as_deref(),
        ) {
            Ok(resolved) => resolved,
            Err(e) => {
                let err_msg = e.to_string();

                // Check for merge conflict - mark stage as Blocked
                // Stage is in Queued state here, can transition directly to Blocked
                if err_msg.contains("Merge conflict") {
                    eprintln!("Stage '{stage_id}' blocked due to merge conflict: {err_msg}");
                    if stage.try_mark_blocked().is_ok() {
                        self.save_stage(&stage)?;
                    }
                    return Ok(());
                }

                // Check for scheduling error - skip stage, will retry next cycle
                if err_msg.contains("Scheduling error") {
                    eprintln!(
                        "Stage '{stage_id}' skipped due to scheduling error (will retry): {err_msg}"
                    );
                    return Ok(());
                }

                // Other errors - propagate
                return Err(e).with_context(|| {
                    format!("Failed to resolve base branch for stage: {stage_id}")
                });
            }
        };

        let worktree = match git::get_or_create_worktree(
            stage_id,
            &self.config.repo_root,
            Some(resolved.branch_name()),
        ) {
            Ok(wt) => wt,
            Err(e) => {
                let err_msg = format!("{e:#}");
                eprintln!("Stage '{stage_id}' blocked due to worktree error: {err_msg}");

                // Mark stage as blocked with failure info
                // Stage is in Queued state here, can transition directly to Blocked
                if stage.try_mark_blocked().is_ok() {
                    stage.failure_info = Some(FailureInfo {
                        failure_type: FailureType::InfrastructureError,
                        detected_at: Utc::now(),
                        evidence: vec![err_msg],
                    });
                    self.save_stage(&stage)?;
                }
                return Ok(());
            }
        };

        // Worktree created successfully - NOW mark as Executing
        // This ensures we only reach Executing state after infrastructure is ready
        stage.try_mark_executing()?;
        self.save_stage(&stage)?;
        self.graph
            .mark_executing(stage_id)
            .context("Failed to mark stage as executing in graph")?;

        // Generate and write sandbox settings to worktree
        let mut merged_sandbox = crate::sandbox::merge_config(
            &self.config.sandbox_config,
            &stage.sandbox,
            stage.stage_type,
        );
        crate::sandbox::expand_paths(&mut merged_sandbox);
        if let Err(e) = crate::sandbox::write_settings(&merged_sandbox, &worktree.path) {
            eprintln!("Warning: Failed to write sandbox settings for stage '{stage_id}': {e}");
            // Continue anyway - sandbox is optional enhancement
        }

        let session = Session::new();

        // Set up Claude Code hooks for this session
        if let Some(hooks_dir) = find_hooks_dir() {
            if let Err(e) = setup_worktree_hooks(
                &worktree.path,
                stage_id,
                &session.id,
                &self.config.work_dir,
                &hooks_dir,
            ) {
                eprintln!("Warning: Failed to set up hooks for stage '{stage_id}': {e}");
                // Continue anyway - hooks are optional enhancement
            }
        }

        let deps = get_dependency_status(&stage, &self.graph);

        let signal_path = generate_signal_with_skills(
            &session,
            &stage,
            &worktree,
            &deps,
            None,
            None, // git_history will be extracted from worktree in future enhancement
            &self.config.work_dir,
            self.skill_index.as_ref(),
        )
        .context("Failed to generate signal file")?;

        // Store original session ID to verify consistency after spawn
        let original_session_id = session.id.clone();

        let spawned_session = if !self.config.manual_mode {
            let spawned = self
                .backend
                .spawn_session(&stage, &worktree, session, &signal_path)
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

        // Update stage with session and worktree info (already marked Executing earlier)
        let mut updated_stage = stage;
        updated_stage.assign_session(spawned_session.id.clone());
        updated_stage.set_worktree(Some(worktree.id.clone()));
        updated_stage.set_resolved_base(Some(resolved.branch_name().to_string()));
        self.save_stage(&updated_stage)?;

        self.active_sessions
            .insert(stage_id.to_string(), spawned_session);
        self.active_worktrees.insert(stage_id.to_string(), worktree);

        Ok(())
    }

    fn start_knowledge_stage(&mut self, stage: Stage) -> Result<()> {
        let stage_id = stage.id.clone();

        // Generate and write sandbox settings to main repo
        let mut merged_sandbox = crate::sandbox::merge_config(
            &self.config.sandbox_config,
            &stage.sandbox,
            stage.stage_type,
        );
        crate::sandbox::expand_paths(&mut merged_sandbox);
        if let Err(e) = crate::sandbox::write_settings(&merged_sandbox, &self.config.repo_root) {
            eprintln!(
                "Warning: Failed to write sandbox settings for knowledge stage '{stage_id}': {e}"
            );
            // Continue anyway - sandbox is optional enhancement
        }

        let session = Session::new();

        // Set up Claude Code hooks for this session in the main repo
        if let Some(hooks_dir) = find_hooks_dir() {
            // Canonicalize work_dir to absolute path
            let absolute_work_dir = self
                .config
                .work_dir
                .canonicalize()
                .unwrap_or_else(|_| self.config.work_dir.clone());

            let config = HooksConfig::new(
                hooks_dir,
                stage_id.to_string(),
                session.id.clone(),
                absolute_work_dir,
            );

            // Set up hooks in the main repo (not a worktree)
            if let Err(e) = setup_hooks_for_worktree(&self.config.repo_root, &config) {
                eprintln!("Warning: Failed to set up hooks for knowledge stage '{stage_id}': {e}");
                // Continue anyway - hooks are optional enhancement
            }
        }

        let deps = get_dependency_status(&stage, &self.graph);

        // Generate knowledge-specific signal (runs in main repo, no commit required)
        let signal_path = generate_knowledge_signal(
            &session,
            &stage,
            &self.config.repo_root,
            &deps,
            &self.config.work_dir,
        )
        .context("Failed to generate knowledge signal file")?;

        // Store original session ID to verify consistency after spawn
        let original_session_id = session.id.clone();

        let spawned_session = if !self.config.manual_mode {
            // Spawn session in the main repo directory (not a worktree)
            let spawned = self
                .backend
                .spawn_knowledge_session(&stage, session, &signal_path, &self.config.repo_root)
                .with_context(|| {
                    format!("Failed to spawn knowledge session for stage: {stage_id}")
                })?;

            // Print confirmation that stage was started
            println!("  Started (knowledge): {stage_id}");

            spawned
        } else {
            println!("Manual mode: Session setup for knowledge stage '{stage_id}'");
            println!("  Directory: {}", self.config.repo_root.display());
            println!("  Signal: {}", signal_path.display());
            println!(
                "  To start: cd {} && claude \"Read the signal file at {} and execute the assigned stage work.\"",
                self.config.repo_root.display(),
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

        // Update stage with session info (already marked Executing earlier)
        let mut updated_stage = stage;
        updated_stage.assign_session(spawned_session.id.clone());
        // Knowledge stages don't have a worktree
        updated_stage.set_worktree(None);
        updated_stage.set_resolved_base(None);
        self.save_stage(&updated_stage)?;

        // Add to active sessions but NOT to active_worktrees (no worktree for knowledge stages)
        self.active_sessions.insert(stage_id, spawned_session);

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
            let (status, outputs) = if let Some(node) = graph.get_node(dep_id) {
                (format!("{:?}", node.status), node.outputs.clone())
            } else {
                ("Unknown".to_string(), Vec::new())
            };

            DependencyStatus {
                stage_id: dep_id.clone(),
                name: dep_id.clone(),
                status,
                outputs,
            }
        })
        .collect()
}
