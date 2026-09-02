//! State persistence - loading and saving stages, sessions, and related data
//!
//! File locking operations (lock_exclusive|lock_shared|fs2 crate) prevent
//! data corruption from concurrent orchestrator and agent access.

use anyhow::{Context, Result};
use std::path::Path;

use crate::models::session::Session;
use crate::models::stage::Stage;

use super::Orchestrator;

/// Trait for persistence operations
pub(super) trait Persistence {
    /// Get the work directory path
    fn persistence_work_dir(&self) -> &Path;
    /// Load stage definition from .loom/work/stages/
    fn load_stage(&self, stage_id: &str) -> Result<Stage> {
        crate::verify::transitions::load_stage(stage_id, self.persistence_work_dir())
            .with_context(|| format!("Failed to load canonical stage record: {stage_id}"))
    }

    /// Apply a minimal mutation to a fresh stage record under one lock.
    fn update_stage<F>(&self, stage_id: &str, modify: F) -> Result<Stage>
    where
        F: FnOnce(&mut Stage) -> Result<()>,
    {
        crate::verify::transitions::update_stage(stage_id, self.persistence_work_dir(), modify)
    }

    /// Save session state to .loom/work/sessions/
    ///
    /// Delegates to the single canonical `save_session` in `fs/session_files`
    /// so the daemon, monitor thread, and CLI all share one locked + atomic
    /// persistence routine and one markdown body.
    fn save_session(&self, session: &Session) -> Result<()> {
        crate::fs::session_files::save_session(session, self.persistence_work_dir())
    }
}

impl Persistence for Orchestrator {
    fn persistence_work_dir(&self) -> &Path {
        &self.config.work_dir
    }
}

#[cfg(test)]
mod persistence_tests {
    use super::*;

    struct TestPersistence {
        work_dir: std::path::PathBuf,
    }

    impl Persistence for TestPersistence {
        fn persistence_work_dir(&self) -> &Path {
            &self.work_dir
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

        let persistence = TestPersistence {
            work_dir: work_dir.to_path_buf(),
        };

        let result = persistence.load_stage("test-stage");
        assert!(
            result.is_err(),
            "Expected error for corrupt stage file, but got Ok"
        );
    }

    #[test]
    fn test_load_stage_fails_closed_when_record_is_missing() {
        let temp = tempfile::tempdir().unwrap();
        let work_dir = temp.path();

        let persistence = TestPersistence {
            work_dir: work_dir.to_path_buf(),
        };

        let result = persistence.load_stage("missing-stage");
        assert!(
            result.is_err(),
            "a missing canonical record must not be reconstructed with defaults"
        );
        assert!(
            format!("{:#}", result.unwrap_err()).contains("Stage file not found"),
            "the error should retain the missing-record cause"
        );
    }
}

#[cfg(test)]
mod tests {
    use crate::fs::locking::{locked_read, locked_write};
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
