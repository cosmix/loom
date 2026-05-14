//! State persistence - loading and saving stages, sessions, and related data
//!
//! File locking operations (lock_exclusive|lock_shared|fs2 crate) prevent
//! data corruption from concurrent orchestrator and agent access.

use anyhow::{Context, Result};
use std::path::Path;

use crate::fs::locking::locked_write;
use crate::fs::stage_files::{find_stage_file, stage_file_path};
use crate::models::session::Session;
use crate::models::stage::Stage;
use crate::plan::graph::levels::compute_all_levels;

use super::Orchestrator;

/// Trait for persistence operations
pub(super) trait Persistence {
    /// Get the work directory path
    fn persistence_work_dir(&self) -> &Path;
    /// Get read access to the execution graph for stage lookups
    fn persistence_graph(&self) -> &crate::plan::ExecutionGraph;

    /// Load stage definition from .work/stages/
    fn load_stage(&self, stage_id: &str) -> Result<Stage> {
        // Try to load from disk using canonical implementation
        match crate::verify::transitions::load_stage(stage_id, self.persistence_work_dir()) {
            Ok(stage) => Ok(stage),
            Err(e) => {
                // Distinguish between "file missing" and "file corrupt/parse error".
                // Only silently reconstruct from the graph when the file genuinely does
                // not exist; propagate the error when the file is present but broken.
                let stages_dir = self.persistence_work_dir().join("stages");
                match find_stage_file(&stages_dir, stage_id) {
                    Ok(None) => {
                        // File genuinely missing — create from graph
                        let node =
                            self.persistence_graph().get_node(stage_id).ok_or_else(|| {
                                anyhow::anyhow!("Stage not found in graph: {stage_id}")
                            })?;

                        let mut stage = Stage::new(node.name.clone(), node.description.clone());
                        stage.id = stage_id.to_string();
                        stage.dependencies = node.dependencies.clone();
                        stage.parallel_group = node.parallel_group.clone();
                        stage.acceptance = node.acceptance.clone();
                        stage.setup = node.setup.clone();
                        stage.files = node.files.clone();
                        stage.auto_merge = node.auto_merge;

                        Ok(stage)
                    }
                    Ok(Some(path)) => {
                        // File exists but failed to parse — propagate the original error
                        Err(e).with_context(|| {
                            format!("Stage file exists but failed to parse: {}", path.display())
                        })
                    }
                    Err(find_err) => {
                        // Cannot even check whether the file exists — propagate both errors
                        Err(e).with_context(|| {
                            format!(
                                "Failed to load stage '{}' and could not check stage file existence: {}",
                                stage_id, find_err
                            )
                        })
                    }
                }
            }
        }
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

        let content = crate::verify::transitions::serialize_stage_to_markdown(stage)
            .context("Failed to serialize stage to markdown")?;

        locked_write(&stage_path, &content)?;

        Ok(())
    }

    /// Compute stage depth using the execution graph
    fn compute_stage_depth(&self, stage_id: &str) -> usize {
        // Get all nodes from the graph
        let nodes = self.persistence_graph().all_nodes();

        // Compute depths for all stages
        let depths = compute_all_levels(&nodes, |node| node.id.as_str(), |node| &node.dependencies);

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
mod persistence_tests {
    use super::*;
    use crate::plan::graph::ExecutionGraph;
    use crate::plan::schema::StageDefinition;

    struct TestPersistence {
        work_dir: std::path::PathBuf,
        graph: ExecutionGraph,
    }

    impl Persistence for TestPersistence {
        fn persistence_work_dir(&self) -> &Path {
            &self.work_dir
        }
        fn persistence_graph(&self) -> &ExecutionGraph {
            &self.graph
        }
    }

    fn make_stage_def(id: &str) -> StageDefinition {
        StageDefinition {
            id: id.to_string(),
            name: format!("Stage {id}"),
            description: None,
            dependencies: vec![],
            parallel_group: None,
            acceptance: vec![],
            setup: vec![],
            files: vec![],
            auto_merge: None,
            working_dir: ".".to_string(),
            stage_type: Default::default(),
            artifacts: vec![],
            wiring: vec![],
            wiring_tests: vec![],
            dead_code_check: None,
            before_stage: vec![],
            after_stage: vec![],
            context_budget: None,
            sandbox: Default::default(),
            execution_mode: None,
            bug_fix: None,
            regression_test: None,
            model: None,
            reasoning_effort: None,
            code_review: None,
        }
    }

    #[test]
    fn test_load_stage_fails_on_corrupt_file() {
        let temp = tempfile::tempdir().unwrap();
        let work_dir = temp.path();
        let stages_dir = work_dir.join("stages");
        std::fs::create_dir_all(&stages_dir).unwrap();

        // Write a file with invalid YAML frontmatter
        let stage_path = stages_dir.join("test-stage.md");
        std::fs::write(&stage_path, "---\ninvalid: [yaml: broken\n---\n").unwrap();

        let stage_def = make_stage_def("test-stage");
        let graph = ExecutionGraph::build(vec![stage_def]).unwrap();

        let persistence = TestPersistence {
            work_dir: work_dir.to_path_buf(),
            graph,
        };

        let result = persistence.load_stage("test-stage");
        assert!(
            result.is_err(),
            "Expected error for corrupt stage file, but got Ok"
        );
    }

    #[test]
    fn test_load_stage_creates_from_graph_when_missing() {
        let temp = tempfile::tempdir().unwrap();
        let work_dir = temp.path();
        // Do NOT create stages dir or any stage files — file is genuinely absent

        let stage_def = make_stage_def("missing-stage");
        let graph = ExecutionGraph::build(vec![stage_def]).unwrap();

        let persistence = TestPersistence {
            work_dir: work_dir.to_path_buf(),
            graph,
        };

        // Missing file → should reconstruct from graph without error
        let result = persistence.load_stage("missing-stage");
        assert!(
            result.is_ok(),
            "Expected Ok when stage file is genuinely missing, got: {:?}",
            result.err()
        );
        let stage = result.unwrap();
        assert_eq!(stage.id, "missing-stage");
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::fs::locking::locked_read;
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
