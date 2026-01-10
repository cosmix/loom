//! Context variable support for acceptance criteria.
//!
//! This module provides variable expansion for acceptance criteria commands.
//! Variables use shell-style syntax: `${VARIABLE_NAME}`.
//!
//! # Supported Variables
//!
//! - `${WORKTREE}` - The worktree root directory path
//! - `${PROJECT_ROOT}` - Directory containing the project manifest (Cargo.toml, package.json, etc.)
//! - `${STAGE_ID}` - The current stage identifier
//!
//! # Example
//!
//! ```yaml
//! acceptance:
//!   - "cd ${PROJECT_ROOT} && cargo test"
//!   - "${PROJECT_ROOT}/target/debug/loom --help"
//! ```

use std::collections::HashMap;
use std::path::{Path, PathBuf};

/// Context for expanding variables in acceptance criteria commands.
///
/// Variables are resolved at execution time based on the worktree and stage context.
#[derive(Debug, Clone, Default)]
pub struct CriteriaContext {
    variables: HashMap<String, String>,
}

impl CriteriaContext {
    /// Create a new criteria context for the given worktree path.
    ///
    /// Automatically populates:
    /// - `WORKTREE` - The worktree root path
    /// - `PROJECT_ROOT` - The detected project root (if found)
    pub fn new(worktree_path: &Path) -> Self {
        let mut variables = HashMap::new();

        // ${WORKTREE} - the worktree root
        variables.insert("WORKTREE".into(), worktree_path.display().to_string());

        // ${PROJECT_ROOT} - directory containing Cargo.toml/package.json/etc.
        if let Some(project_root) = find_project_root(worktree_path) {
            variables.insert("PROJECT_ROOT".into(), project_root.display().to_string());
        }

        Self { variables }
    }

    /// Create a new criteria context with a stage ID.
    ///
    /// Includes all variables from `new()` plus:
    /// - `STAGE_ID` - The stage identifier
    pub fn with_stage_id(worktree_path: &Path, stage_id: &str) -> Self {
        let mut ctx = Self::new(worktree_path);
        ctx.variables
            .insert("STAGE_ID".into(), stage_id.to_string());
        ctx
    }

    /// Set a custom variable value.
    pub fn set_variable(&mut self, key: &str, value: &str) {
        self.variables.insert(key.to_string(), value.to_string());
    }

    /// Get a variable value.
    pub fn get_variable(&self, key: &str) -> Option<&str> {
        self.variables.get(key).map(String::as_str)
    }

    /// Expand all variables in a criterion string.
    ///
    /// Variables use the format `${VARIABLE_NAME}`. Unknown variables are left unchanged.
    ///
    /// # Example
    ///
    /// ```ignore
    /// let ctx = CriteriaContext::new(Path::new("/worktrees/my-stage"));
    /// let expanded = ctx.expand("cd ${PROJECT_ROOT} && cargo test");
    /// // Returns: "cd /worktrees/my-stage/loom && cargo test"
    /// ```
    pub fn expand(&self, criterion: &str) -> String {
        let mut result = criterion.to_string();
        for (key, value) in &self.variables {
            result = result.replace(&format!("${{{key}}}"), value);
        }
        result
    }

    /// Check if a criterion contains any unresolved variables.
    ///
    /// Returns a list of variable names that were not resolved.
    pub fn find_unresolved(&self, criterion: &str) -> Vec<String> {
        let mut unresolved = Vec::new();
        let mut chars = criterion.chars().peekable();

        while let Some(c) = chars.next() {
            if c == '$' && chars.peek() == Some(&'{') {
                chars.next(); // consume '{'
                let mut var_name = String::new();
                for c in chars.by_ref() {
                    if c == '}' {
                        break;
                    }
                    var_name.push(c);
                }
                if !var_name.is_empty() && !self.variables.contains_key(&var_name) {
                    unresolved.push(var_name);
                }
            }
        }

        unresolved
    }

    /// List all available variable names.
    pub fn available_variables(&self) -> Vec<&str> {
        self.variables.keys().map(String::as_str).collect()
    }
}

/// Find the project root directory containing a manifest file.
///
/// Checks for common project manifest files:
/// - Cargo.toml (Rust)
/// - package.json (Node.js)
/// - go.mod (Go)
/// - pyproject.toml (Python)
/// - pom.xml (Java/Maven)
/// - build.gradle (Java/Gradle)
///
/// First checks the worktree root, then immediate subdirectories.
fn find_project_root(worktree: &Path) -> Option<PathBuf> {
    const MARKERS: &[&str] = &[
        "Cargo.toml",
        "package.json",
        "go.mod",
        "pyproject.toml",
        "pom.xml",
        "build.gradle",
    ];

    // Check worktree root first
    for marker in MARKERS {
        if worktree.join(marker).exists() {
            return Some(worktree.to_path_buf());
        }
    }

    // Check immediate subdirectories
    if let Ok(entries) = std::fs::read_dir(worktree) {
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_dir() {
                for marker in MARKERS {
                    if path.join(marker).exists() {
                        return Some(path);
                    }
                }
            }
        }
    }

    None
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use tempfile::tempdir;

    #[test]
    fn test_expand_worktree_variable() {
        let ctx = CriteriaContext::new(Path::new("/worktrees/my-stage"));
        let expanded = ctx.expand("cd ${WORKTREE} && ls");
        assert_eq!(expanded, "cd /worktrees/my-stage && ls");
    }

    #[test]
    fn test_expand_multiple_variables() {
        let mut ctx = CriteriaContext::new(Path::new("/worktrees/my-stage"));
        ctx.set_variable("PROJECT_ROOT", "/worktrees/my-stage/loom");

        let expanded = ctx.expand("cd ${PROJECT_ROOT} && ${WORKTREE}/script.sh");
        assert_eq!(
            expanded,
            "cd /worktrees/my-stage/loom && /worktrees/my-stage/script.sh"
        );
    }

    #[test]
    fn test_expand_with_stage_id() {
        let ctx = CriteriaContext::with_stage_id(Path::new("/worktrees/my-stage"), "stage-123");
        let expanded = ctx.expand("echo ${STAGE_ID}");
        assert_eq!(expanded, "echo stage-123");
    }

    #[test]
    fn test_expand_unknown_variable_unchanged() {
        let ctx = CriteriaContext::new(Path::new("/worktrees/my-stage"));
        let expanded = ctx.expand("echo ${UNKNOWN_VAR}");
        assert_eq!(expanded, "echo ${UNKNOWN_VAR}");
    }

    #[test]
    fn test_expand_no_variables() {
        let ctx = CriteriaContext::new(Path::new("/worktrees/my-stage"));
        let expanded = ctx.expand("cargo test --lib");
        assert_eq!(expanded, "cargo test --lib");
    }

    #[test]
    fn test_find_unresolved_empty() {
        let mut ctx = CriteriaContext::default();
        ctx.set_variable("FOO", "bar");
        let unresolved = ctx.find_unresolved("echo ${FOO}");
        assert!(unresolved.is_empty());
    }

    #[test]
    fn test_find_unresolved_found() {
        let ctx = CriteriaContext::default();
        let unresolved = ctx.find_unresolved("cd ${PROJECT_ROOT} && ${WORKTREE}/run.sh");
        assert_eq!(unresolved.len(), 2);
        assert!(unresolved.contains(&"PROJECT_ROOT".to_string()));
        assert!(unresolved.contains(&"WORKTREE".to_string()));
    }

    #[test]
    fn test_find_project_root_at_worktree() {
        let dir = tempdir().expect("failed to create temp dir");
        fs::write(dir.path().join("Cargo.toml"), "[package]").expect("failed to write file");

        let root = find_project_root(dir.path());
        assert_eq!(root, Some(dir.path().to_path_buf()));
    }

    #[test]
    fn test_find_project_root_in_subdirectory() {
        let dir = tempdir().expect("failed to create temp dir");
        let subdir = dir.path().join("loom");
        fs::create_dir(&subdir).expect("failed to create subdir");
        fs::write(subdir.join("Cargo.toml"), "[package]").expect("failed to write file");

        let root = find_project_root(dir.path());
        assert_eq!(root, Some(subdir));
    }

    #[test]
    fn test_find_project_root_not_found() {
        let dir = tempdir().expect("failed to create temp dir");
        let root = find_project_root(dir.path());
        assert!(root.is_none());
    }

    #[test]
    fn test_find_project_root_package_json() {
        let dir = tempdir().expect("failed to create temp dir");
        fs::write(dir.path().join("package.json"), "{}").expect("failed to write file");

        let root = find_project_root(dir.path());
        assert_eq!(root, Some(dir.path().to_path_buf()));
    }

    #[test]
    fn test_criteria_context_new_with_project_root() {
        let dir = tempdir().expect("failed to create temp dir");
        fs::write(dir.path().join("Cargo.toml"), "[package]").expect("failed to write file");

        let ctx = CriteriaContext::new(dir.path());
        assert!(ctx.get_variable("WORKTREE").is_some());
        assert!(ctx.get_variable("PROJECT_ROOT").is_some());
    }

    #[test]
    fn test_criteria_context_new_without_project_root() {
        let dir = tempdir().expect("failed to create temp dir");

        let ctx = CriteriaContext::new(dir.path());
        assert!(ctx.get_variable("WORKTREE").is_some());
        assert!(ctx.get_variable("PROJECT_ROOT").is_none());
    }

    #[test]
    fn test_available_variables() {
        let mut ctx = CriteriaContext::default();
        ctx.set_variable("FOO", "1");
        ctx.set_variable("BAR", "2");

        let vars = ctx.available_variables();
        assert_eq!(vars.len(), 2);
        assert!(vars.contains(&"FOO"));
        assert!(vars.contains(&"BAR"));
    }

    #[test]
    fn test_set_and_get_variable() {
        let mut ctx = CriteriaContext::default();
        assert!(ctx.get_variable("CUSTOM").is_none());

        ctx.set_variable("CUSTOM", "my-value");
        assert_eq!(ctx.get_variable("CUSTOM"), Some("my-value"));
    }
}
