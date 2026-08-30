//! Whether an existing `.work/` holds real orchestration state.

use std::path::Path;

/// Whether `.work/` at `work_dir_path` holds real orchestration state — a
/// plan config or any stage/session file — as opposed to only derived
/// caches. Exists for the phantom-`.work/` scenario: a stale `LOOM_WORK_DIR`
/// pin left in `.claude/settings.local.json` after its `.work/` was deleted
/// can cause a hook (`commands/hook/reconcile_graph.rs`) to recreate an empty
/// `.work/context/` + `.work/.loom/` in a repo that was never `loom init`ed.
/// Such a directory carries no plan state at all, so `loom init` should adopt
/// it rather than permanently refuse to run.
pub(super) fn holds_orchestration_state(work_dir_path: &Path) -> bool {
    if work_dir_path.join("config.toml").exists() {
        return true;
    }
    dir_has_markdown_file(&work_dir_path.join("stages"))
        || dir_has_markdown_file(&work_dir_path.join("sessions"))
}

/// Whether `dir` exists and contains at least one `*.md` file (not
/// recursive — stage and session files live directly under their dir).
fn dir_has_markdown_file(dir: &Path) -> bool {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return false;
    };
    entries
        .flatten()
        .any(|entry| entry.path().extension().is_some_and(|ext| ext == "md"))
}

#[cfg(test)]
mod holds_orchestration_state_tests {
    use super::holds_orchestration_state;
    use std::fs;
    use tempfile::TempDir;

    #[test]
    fn false_for_missing_work_dir() {
        let temp = TempDir::new().unwrap();
        assert!(!holds_orchestration_state(&temp.path().join(".work")));
    }

    #[test]
    fn false_for_a_phantom_work_dir_holding_only_derived_caches() {
        let temp = TempDir::new().unwrap();
        let work_dir_path = temp.path().join(".work");
        fs::create_dir_all(work_dir_path.join("context")).unwrap();
        fs::create_dir_all(work_dir_path.join(".loom")).unwrap();

        assert!(!holds_orchestration_state(&work_dir_path));
    }

    #[test]
    fn true_when_config_toml_exists() {
        let temp = TempDir::new().unwrap();
        let work_dir_path = temp.path().join(".work");
        fs::create_dir_all(&work_dir_path).unwrap();
        fs::write(work_dir_path.join("config.toml"), "[plan]\n").unwrap();

        assert!(holds_orchestration_state(&work_dir_path));
    }

    #[test]
    fn true_when_a_stage_file_exists() {
        let temp = TempDir::new().unwrap();
        let work_dir_path = temp.path().join(".work");
        let stages_dir = work_dir_path.join("stages");
        fs::create_dir_all(&stages_dir).unwrap();
        fs::write(stages_dir.join("stage-1.md"), "# Stage").unwrap();

        assert!(holds_orchestration_state(&work_dir_path));
    }

    #[test]
    fn true_when_a_session_file_exists() {
        let temp = TempDir::new().unwrap();
        let work_dir_path = temp.path().join(".work");
        let sessions_dir = work_dir_path.join("sessions");
        fs::create_dir_all(&sessions_dir).unwrap();
        fs::write(sessions_dir.join("session-1.md"), "# Session").unwrap();

        assert!(holds_orchestration_state(&work_dir_path));
    }
}
