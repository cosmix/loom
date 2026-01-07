use anyhow::Result;
use std::fs;
use std::path::Path;

/// Context for shell completion
#[derive(Debug, Clone)]
pub struct CompletionContext {
    pub cwd: String,
    pub shell: String,
    pub cmdline: String,
    pub current_word: String,
    pub prev_word: String,
}

impl CompletionContext {
    /// Parse completion context from shell-provided arguments
    ///
    /// # Arguments
    ///
    /// * `shell` - Shell type (bash, zsh, fish)
    /// * `args` - Arguments passed from shell completion system
    ///
    /// # Returns
    ///
    /// A CompletionContext with parsed fields
    pub fn from_args(shell: &str, args: &[String]) -> Self {
        // Different shells pass arguments differently
        // bash: [cwd, cmdline, current_word, prev_word]
        // zsh: similar format
        // fish: may vary
        let cwd = args.first().cloned().unwrap_or_else(|| ".".to_string());
        let cmdline = args.get(1).cloned().unwrap_or_default();
        let current_word = args.get(2).cloned().unwrap_or_default();
        let prev_word = args.get(3).cloned().unwrap_or_default();

        Self {
            cwd,
            shell: shell.to_string(),
            cmdline,
            current_word,
            prev_word,
        }
    }
}

/// Complete plan file paths from doc/plans/*.md
///
/// # Arguments
///
/// * `cwd` - Current working directory (project root)
/// * `prefix` - Partial filename prefix to filter results
///
/// # Returns
///
/// List of matching plan file paths
pub fn complete_plan_files(cwd: &Path, prefix: &str) -> Result<Vec<String>> {
    let plans_dir = cwd.join("doc/plans");

    if !plans_dir.exists() {
        return Ok(Vec::new());
    }

    let mut results = Vec::new();

    for entry in fs::read_dir(&plans_dir)? {
        let entry = entry?;
        let path = entry.path();

        // Only include .md files
        if path.extension().and_then(|s| s.to_str()) != Some("md") {
            continue;
        }

        if let Some(filename) = path.file_name().and_then(|n| n.to_str()) {
            // Match against prefix
            if prefix.is_empty() || filename.starts_with(prefix) {
                // Return full relative path from cwd
                if let Ok(rel_path) = path.strip_prefix(cwd) {
                    results.push(rel_path.to_string_lossy().to_string());
                }
            }
        }
    }

    results.sort();
    Ok(results)
}

/// Complete stage IDs from .work/stages/*.md
///
/// # Arguments
///
/// * `cwd` - Current working directory (project root)
/// * `prefix` - Partial stage ID prefix to filter results
///
/// # Returns
///
/// List of matching stage IDs
pub fn complete_stage_ids(cwd: &Path, prefix: &str) -> Result<Vec<String>> {
    let stages_dir = cwd.join(".work/stages");

    if !stages_dir.exists() {
        return Ok(Vec::new());
    }

    let mut results = Vec::new();

    for entry in fs::read_dir(&stages_dir)? {
        let entry = entry?;
        let path = entry.path();

        // Only include .md files
        if path.extension().and_then(|s| s.to_str()) != Some("md") {
            continue;
        }

        if let Some(filename) = path.file_name().and_then(|n| n.to_str()) {
            // Extract stage ID using the stage_files module
            if let Some(stage_id) = crate::fs::stage_files::extract_stage_id(filename) {
                // Match against prefix
                if prefix.is_empty() || stage_id.starts_with(prefix) {
                    results.push(stage_id);
                }
            }
        }
    }

    results.sort();
    results.dedup(); // Remove duplicates in case of multiple matches
    Ok(results)
}

/// Complete session IDs from .work/sessions/*.md
///
/// # Arguments
///
/// * `cwd` - Current working directory (project root)
/// * `prefix` - Partial session ID prefix to filter results
///
/// # Returns
///
/// List of matching session IDs
pub fn complete_session_ids(cwd: &Path, prefix: &str) -> Result<Vec<String>> {
    let sessions_dir = cwd.join(".work/sessions");

    if !sessions_dir.exists() {
        return Ok(Vec::new());
    }

    let mut results = Vec::new();

    for entry in fs::read_dir(&sessions_dir)? {
        let entry = entry?;
        let path = entry.path();

        // Only include .md files
        if path.extension().and_then(|s| s.to_str()) != Some("md") {
            continue;
        }

        // Session ID is the filename stem (without .md extension)
        if let Some(stem) = path.file_stem().and_then(|s| s.to_str()) {
            // Match against prefix
            if prefix.is_empty() || stem.starts_with(prefix) {
                results.push(stem.to_string());
            }
        }
    }

    results.sort();
    Ok(results)
}

/// Complete both stage and session IDs
///
/// # Arguments
///
/// * `cwd` - Current working directory (project root)
/// * `prefix` - Partial ID prefix to filter results
///
/// # Returns
///
/// Combined list of matching stage and session IDs
pub fn complete_stage_or_session_ids(cwd: &Path, prefix: &str) -> Result<Vec<String>> {
    let mut results = Vec::new();

    results.extend(complete_stage_ids(cwd, prefix)?);
    results.extend(complete_session_ids(cwd, prefix)?);

    results.sort();
    results.dedup(); // Remove duplicates if a stage and session share an ID
    Ok(results)
}

/// Main entry point for dynamic completions
///
/// Determines what to complete based on context and prints results to stdout
///
/// # Arguments
///
/// * `ctx` - Completion context from shell
///
/// # Returns
///
/// Ok(()) on success, error if completion fails
pub fn complete_dynamic(ctx: &CompletionContext) -> Result<()> {
    let cwd = Path::new(&ctx.cwd);
    let prefix = &ctx.current_word;

    // Determine what to complete based on previous word and command line
    let completions = match ctx.prev_word.as_str() {
        "init" => complete_plan_files(cwd, prefix)?,

        "verify" | "merge" | "resume" => complete_stage_ids(cwd, prefix)?,

        "attach" => complete_stage_or_session_ids(cwd, prefix)?,

        "kill" if ctx.cmdline.contains("sessions") => complete_session_ids(cwd, prefix)?,

        "complete" | "block" | "reset" | "waiting" if ctx.cmdline.contains("stage") => {
            complete_stage_ids(cwd, prefix)?
        }

        "--stage" | "-s" if ctx.cmdline.contains("run") => complete_stage_ids(cwd, prefix)?,

        _ => Vec::new(),
    };

    // Print completions, one per line
    for completion in completions {
        println!("{completion}");
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use tempfile::TempDir;

    fn setup_test_workspace() -> TempDir {
        let temp_dir = TempDir::new().unwrap();
        let root = temp_dir.path();

        // Create doc/plans with sample files
        let plans_dir = root.join("doc/plans");
        fs::create_dir_all(&plans_dir).unwrap();
        fs::write(plans_dir.join("PLAN-0001-feature-a.md"), "content").unwrap();
        fs::write(plans_dir.join("PLAN-0002-feature-b.md"), "content").unwrap();
        fs::write(plans_dir.join("PLAN-0010-bugfix.md"), "content").unwrap();

        // Create .work/stages with sample files
        let stages_dir = root.join(".work/stages");
        fs::create_dir_all(&stages_dir).unwrap();
        fs::write(stages_dir.join("01-core-architecture.md"), "content").unwrap();
        fs::write(stages_dir.join("02-math-core.md"), "content").unwrap();
        fs::write(stages_dir.join("02-ui-framework.md"), "content").unwrap();
        fs::write(stages_dir.join("03-integration.md"), "content").unwrap();

        // Create .work/sessions with sample files
        let sessions_dir = root.join(".work/sessions");
        fs::create_dir_all(&sessions_dir).unwrap();
        fs::write(sessions_dir.join("session-001.md"), "content").unwrap();
        fs::write(sessions_dir.join("session-002.md"), "content").unwrap();
        fs::write(sessions_dir.join("session-abc.md"), "content").unwrap();

        temp_dir
    }

    #[test]
    fn test_complete_plan_files_all() {
        let temp_dir = setup_test_workspace();
        let root = temp_dir.path();

        let results = complete_plan_files(root, "").unwrap();
        assert_eq!(results.len(), 3);
        assert!(results.contains(&"doc/plans/PLAN-0001-feature-a.md".to_string()));
        assert!(results.contains(&"doc/plans/PLAN-0002-feature-b.md".to_string()));
        assert!(results.contains(&"doc/plans/PLAN-0010-bugfix.md".to_string()));
    }

    #[test]
    fn test_complete_plan_files_with_prefix() {
        let temp_dir = setup_test_workspace();
        let root = temp_dir.path();

        let results = complete_plan_files(root, "PLAN-000").unwrap();
        assert_eq!(results.len(), 2);
        assert!(results.contains(&"doc/plans/PLAN-0001-feature-a.md".to_string()));
        assert!(results.contains(&"doc/plans/PLAN-0002-feature-b.md".to_string()));
    }

    #[test]
    fn test_complete_plan_files_no_match() {
        let temp_dir = setup_test_workspace();
        let root = temp_dir.path();

        let results = complete_plan_files(root, "PLAN-9999").unwrap();
        assert_eq!(results.len(), 0);
    }

    #[test]
    fn test_complete_plan_files_missing_dir() {
        let temp_dir = TempDir::new().unwrap();
        let root = temp_dir.path();

        let results = complete_plan_files(root, "").unwrap();
        assert_eq!(results.len(), 0);
    }

    #[test]
    fn test_complete_stage_ids_all() {
        let temp_dir = setup_test_workspace();
        let root = temp_dir.path();

        let results = complete_stage_ids(root, "").unwrap();
        assert_eq!(results.len(), 4);
        assert!(results.contains(&"core-architecture".to_string()));
        assert!(results.contains(&"math-core".to_string()));
        assert!(results.contains(&"ui-framework".to_string()));
        assert!(results.contains(&"integration".to_string()));
    }

    #[test]
    fn test_complete_stage_ids_with_prefix() {
        let temp_dir = setup_test_workspace();
        let root = temp_dir.path();

        let results = complete_stage_ids(root, "core").unwrap();
        assert_eq!(results.len(), 1);
        assert!(results.contains(&"core-architecture".to_string()));
    }

    #[test]
    fn test_complete_stage_ids_missing_dir() {
        let temp_dir = TempDir::new().unwrap();
        let root = temp_dir.path();

        let results = complete_stage_ids(root, "").unwrap();
        assert_eq!(results.len(), 0);
    }

    #[test]
    fn test_complete_session_ids_all() {
        let temp_dir = setup_test_workspace();
        let root = temp_dir.path();

        let results = complete_session_ids(root, "").unwrap();
        assert_eq!(results.len(), 3);
        assert!(results.contains(&"session-001".to_string()));
        assert!(results.contains(&"session-002".to_string()));
        assert!(results.contains(&"session-abc".to_string()));
    }

    #[test]
    fn test_complete_session_ids_with_prefix() {
        let temp_dir = setup_test_workspace();
        let root = temp_dir.path();

        let results = complete_session_ids(root, "session-00").unwrap();
        assert_eq!(results.len(), 2);
        assert!(results.contains(&"session-001".to_string()));
        assert!(results.contains(&"session-002".to_string()));
    }

    #[test]
    fn test_complete_session_ids_missing_dir() {
        let temp_dir = TempDir::new().unwrap();
        let root = temp_dir.path();

        let results = complete_session_ids(root, "").unwrap();
        assert_eq!(results.len(), 0);
    }

    #[test]
    fn test_complete_stage_or_session_ids() {
        let temp_dir = setup_test_workspace();
        let root = temp_dir.path();

        let results = complete_stage_or_session_ids(root, "").unwrap();
        // 4 stages + 3 sessions = 7 total
        assert_eq!(results.len(), 7);
        assert!(results.contains(&"core-architecture".to_string()));
        assert!(results.contains(&"session-001".to_string()));
    }

    #[test]
    fn test_completion_context_from_args() {
        let args = vec![
            "/home/user/project".to_string(),
            "loom init PLAN".to_string(),
            "PLAN-001".to_string(),
            "init".to_string(),
        ];

        let ctx = CompletionContext::from_args("bash", &args);

        assert_eq!(ctx.cwd, "/home/user/project");
        assert_eq!(ctx.shell, "bash");
        assert_eq!(ctx.cmdline, "loom init PLAN");
        assert_eq!(ctx.current_word, "PLAN-001");
        assert_eq!(ctx.prev_word, "init");
    }

    #[test]
    fn test_completion_context_from_args_empty() {
        let args: Vec<String> = Vec::new();
        let ctx = CompletionContext::from_args("zsh", &args);

        assert_eq!(ctx.cwd, ".");
        assert_eq!(ctx.shell, "zsh");
        assert_eq!(ctx.cmdline, "");
        assert_eq!(ctx.current_word, "");
        assert_eq!(ctx.prev_word, "");
    }

    #[test]
    fn test_complete_dynamic_init() {
        let temp_dir = setup_test_workspace();
        let root = temp_dir.path();

        let ctx = CompletionContext {
            cwd: root.to_string_lossy().to_string(),
            shell: "bash".to_string(),
            cmdline: "loom init".to_string(),
            current_word: "PLAN".to_string(),
            prev_word: "init".to_string(),
        };

        // complete_dynamic prints to stdout, so we can't easily test the output
        // but we can verify it doesn't error
        assert!(complete_dynamic(&ctx).is_ok());
    }

    #[test]
    fn test_complete_dynamic_verify() {
        let temp_dir = setup_test_workspace();
        let root = temp_dir.path();

        let ctx = CompletionContext {
            cwd: root.to_string_lossy().to_string(),
            shell: "bash".to_string(),
            cmdline: "loom verify".to_string(),
            current_word: "core".to_string(),
            prev_word: "verify".to_string(),
        };

        assert!(complete_dynamic(&ctx).is_ok());
    }

    #[test]
    fn test_complete_dynamic_attach() {
        let temp_dir = setup_test_workspace();
        let root = temp_dir.path();

        let ctx = CompletionContext {
            cwd: root.to_string_lossy().to_string(),
            shell: "bash".to_string(),
            cmdline: "loom attach".to_string(),
            current_word: "".to_string(),
            prev_word: "attach".to_string(),
        };

        assert!(complete_dynamic(&ctx).is_ok());
    }

    #[test]
    fn test_complete_dynamic_stage_flag() {
        let temp_dir = setup_test_workspace();
        let root = temp_dir.path();

        let ctx = CompletionContext {
            cwd: root.to_string_lossy().to_string(),
            shell: "bash".to_string(),
            cmdline: "loom run --stage".to_string(),
            current_word: "".to_string(),
            prev_word: "--stage".to_string(),
        };

        assert!(complete_dynamic(&ctx).is_ok());
    }

    #[test]
    fn test_complete_dynamic_sessions_kill() {
        let temp_dir = setup_test_workspace();
        let root = temp_dir.path();

        let ctx = CompletionContext {
            cwd: root.to_string_lossy().to_string(),
            shell: "bash".to_string(),
            cmdline: "loom sessions kill".to_string(),
            current_word: "".to_string(),
            prev_word: "kill".to_string(),
        };

        assert!(complete_dynamic(&ctx).is_ok());
    }

    #[test]
    fn test_complete_dynamic_stage_complete() {
        let temp_dir = setup_test_workspace();
        let root = temp_dir.path();

        let ctx = CompletionContext {
            cwd: root.to_string_lossy().to_string(),
            shell: "bash".to_string(),
            cmdline: "loom stage complete".to_string(),
            current_word: "".to_string(),
            prev_word: "complete".to_string(),
        };

        assert!(complete_dynamic(&ctx).is_ok());
    }

    #[test]
    fn test_complete_dynamic_no_match() {
        let temp_dir = setup_test_workspace();
        let root = temp_dir.path();

        let ctx = CompletionContext {
            cwd: root.to_string_lossy().to_string(),
            shell: "bash".to_string(),
            cmdline: "loom status".to_string(),
            current_word: "".to_string(),
            prev_word: "status".to_string(),
        };

        // Should not error, just return empty results
        assert!(complete_dynamic(&ctx).is_ok());
    }
}
