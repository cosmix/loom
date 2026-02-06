//! State persistence - loading and saving stages, sessions, and related data
//!
//! File locking operations (lock_exclusive|lock_shared|fs2 crate) prevent
//! data corruption from concurrent orchestrator and agent access.

use anyhow::{Context, Result};
use std::path::Path;

use crate::fs::locking::{locked_read, locked_write};
use crate::fs::stage_files::{
    compute_stage_depths, find_stage_file, stage_file_path, StageDependencies,
};
use crate::models::session::Session;
use crate::models::stage::Stage;
use crate::parser::frontmatter::parse_from_markdown;

use super::Orchestrator;

/// Trait for persistence operations
pub(super) trait Persistence {
    /// Get the work directory path
    fn persistence_work_dir(&self) -> &Path;
    /// Get read access to the execution graph for stage lookups
    fn persistence_graph(&self) -> &crate::plan::ExecutionGraph;

    /// Load stage definition from .work/stages/
    fn load_stage(&self, stage_id: &str) -> Result<Stage> {
        let stages_dir = self.persistence_work_dir().join("stages");

        // Use find_stage_file to handle both prefixed and non-prefixed formats
        let stage_path = find_stage_file(&stages_dir, stage_id)?;

        if stage_path.is_none() {
            // Stage file doesn't exist - create from graph
            let node = self
                .persistence_graph()
                .get_node(stage_id)
                .ok_or_else(|| anyhow::anyhow!("Stage not found in graph: {stage_id}"))?;

            let mut stage = Stage::new(node.name.clone(), node.description.clone());
            stage.id = stage_id.to_string();
            stage.dependencies = node.dependencies.clone();
            stage.parallel_group = node.parallel_group.clone();
            stage.acceptance = node.acceptance.clone();
            stage.setup = node.setup.clone();
            stage.files = node.files.clone();
            stage.auto_merge = node.auto_merge;

            return Ok(stage);
        }

        let stage_path = stage_path.unwrap();
        let content = locked_read(&stage_path)?;

        parse_from_markdown(&content, "Stage")
    }

    /// Save stage state to .work/stages/
    fn save_stage(&self, stage: &Stage) -> Result<()> {
        let stages_dir = self.persistence_work_dir().join("stages");
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

        locked_write(&stage_path, &content)?;

        Ok(())
    }

    /// Compute stage depth using the execution graph
    fn compute_stage_depth(&self, stage_id: &str) -> usize {
        // Build dependency info from the graph
        let stage_deps: Vec<StageDependencies> = self
            .persistence_graph()
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
        let sessions_dir = self.persistence_work_dir().join("sessions");
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

        locked_write(&session_path, &content)?;

        Ok(())
    }
}

impl Persistence for Orchestrator {
    fn persistence_work_dir(&self) -> &Path {
        &self.config.work_dir
    }

    fn persistence_graph(&self) -> &crate::plan::ExecutionGraph {
        &self.graph
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::thread;

    #[test]
    fn test_concurrent_stage_write_safety() {
        let temp = tempfile::tempdir().unwrap();
        let path = temp.path().join("test-stage.md");

        // Write initial content
        locked_write(&path, "initial").unwrap();

        // Spawn threads that write concurrently
        let handles: Vec<_> = (0..10)
            .map(|i| {
                let path = path.clone();
                thread::spawn(move || {
                    let content = format!("content from thread {i}");
                    locked_write(&path, &content).unwrap();
                })
            })
            .collect();

        for handle in handles {
            handle.join().unwrap();
        }

        // Verify final content is valid (not corrupted/interleaved)
        let final_content = locked_read(&path).unwrap();
        assert!(final_content.starts_with("content from thread"));
        // Verify no corruption - should be a complete thread message
        assert!(final_content.len() >= "content from thread 0".len());
        assert!(final_content.len() <= "content from thread 9".len());
    }

    #[test]
    fn test_concurrent_read_write() {
        let temp = tempfile::tempdir().unwrap();
        let path = temp.path().join("test-read-write.md");

        // Initial write
        locked_write(&path, "initial content").unwrap();

        // Spawn reader and writer threads
        let read_path = path.clone();
        let read_handle = thread::spawn(move || {
            for _ in 0..50 {
                let _ = locked_read(&read_path);
            }
        });

        let write_path = path.clone();
        let write_handle = thread::spawn(move || {
            for i in 0..50 {
                locked_write(&write_path, &format!("write {i}")).unwrap();
            }
        });

        read_handle.join().unwrap();
        write_handle.join().unwrap();

        // Should be able to read final state
        let final_content = locked_read(&path).unwrap();
        assert!(final_content.starts_with("write "));
    }
}
