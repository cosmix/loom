//! Main codebase analyzer that orchestrates detection functions.

use anyhow::Result;
use std::path::Path;

use super::detectors;

/// Results from codebase analysis
#[derive(Debug, Default)]
pub struct AnalysisResult {
    /// Architecture and structure findings
    pub architecture: String,
    /// Technology stack (languages, frameworks, dependencies)
    pub stack: String,
    /// Detected coding conventions
    pub conventions: String,
    /// Identified concerns (tech debt, TODOs, issues)
    pub concerns: String,
}

/// Analyze a codebase and return structured findings
pub fn analyze_codebase(root: &Path, deep: bool, focus: Option<&str>) -> Result<AnalysisResult> {
    let mut result = AnalysisResult::default();

    // Detect project type and manifest
    let project_info = detectors::detect_project_type(root)?;
    if !project_info.is_empty() {
        result
            .stack
            .push_str(&format!("## Project Type\n\n{project_info}\n\n"));
    }

    // Analyze dependencies
    let deps = detectors::analyze_dependencies(root)?;
    if !deps.is_empty() {
        result
            .stack
            .push_str(&format!("## Key Dependencies\n\n{deps}\n\n"));
    }

    // Find entry points
    let entries = detectors::find_entry_points(root, focus)?;
    if !entries.is_empty() {
        result
            .architecture
            .push_str(&format!("## Entry Points\n\n{entries}\n\n"));
    }

    // Analyze directory structure
    let structure = detectors::analyze_structure(root, deep)?;
    if !structure.is_empty() {
        result
            .architecture
            .push_str(&format!("## Directory Structure\n\n{structure}\n\n"));
    }

    // Detect coding conventions
    let conventions = detectors::detect_conventions(root)?;
    if !conventions.is_empty() {
        result
            .conventions
            .push_str(&format!("## Detected Conventions\n\n{conventions}\n\n"));
    }

    // Find potential concerns (tech debt, issues)
    if deep {
        let concerns = detectors::find_concerns(root)?;
        if !concerns.is_empty() {
            result
                .concerns
                .push_str(&format!("## Potential Concerns\n\n{concerns}\n\n"));
        }
    }

    Ok(result)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use tempfile::TempDir;

    #[test]
    fn test_analyze_empty_directory() {
        let temp = TempDir::new().unwrap();
        let result = analyze_codebase(temp.path(), false, None).unwrap();
        // Empty directory should return empty result, not error
        assert!(
            result.architecture.is_empty() || result.architecture.contains("Directory Structure")
        );
    }

    #[test]
    fn test_analyze_rust_project() {
        let temp = TempDir::new().unwrap();

        // Create a minimal Cargo.toml
        fs::write(
            temp.path().join("Cargo.toml"),
            r#"[package]
name = "test-project"
version = "0.1.0"

[dependencies]
serde = "1.0"
"#,
        )
        .unwrap();

        // Create src/main.rs
        let src_dir = temp.path().join("src");
        fs::create_dir_all(&src_dir).unwrap();
        fs::write(src_dir.join("main.rs"), "fn main() {}").unwrap();

        let result = analyze_codebase(temp.path(), false, None).unwrap();
        assert!(result.stack.contains("Rust"));
    }
}
