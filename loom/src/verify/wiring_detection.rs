//! Wiring detection - identify new source files that are not referenced anywhere in the codebase.
//!
//! At stage completion, newly added source files should be imported or referenced by at least
//! one other file. Files that exist but are never referenced are "unwired" and likely indicate
//! a forgotten module declaration or import.

use anyhow::{bail, Context, Result};
use std::path::Path;
use std::process::Command;

/// A new source file that appears to be unwired (not referenced by any other file).
#[derive(Debug)]
pub struct UnwiredFile {
    /// Repository-relative path to the file (e.g. `src/cache/store.rs`)
    pub path: String,
    /// The importable name extracted from the path (e.g. `store`)
    pub importable_name: String,
}

/// Result of wiring detection for all newly added files.
#[derive(Debug)]
pub struct WiringDetectionResult {
    /// Files that were added in the diff but have no references in the codebase
    pub unwired_files: Vec<UnwiredFile>,
    /// Total number of new source files inspected
    pub total_new_files: usize,
}

/// Source file extensions considered when scanning for new files.
const SOURCE_EXTENSIONS: &[&str] = &["rs", "ts", "tsx", "jsx", "js", "py", "go"];

/// Structural files that are excluded from unwired detection because they do not
/// need to be referenced by a sibling (they ARE the entrypoint for their directory).
const EXCLUDED_STEMS: &[&str] = &["mod", "lib", "main", "index"];

/// Common generic names that produce too many false positives when grepped.
const GENERIC_NAMES: &[&str] = &[
    "new", "default", "from", "into", "build", "test", "main", "run", "init", "error", "config",
    "types", "utils", "helpers", "common",
];

/// Detect new source files that are not referenced anywhere in the codebase.
///
/// Steps:
/// 1. Run `git diff --name-status <base_branch>..HEAD` in `worktree_path`.
/// 2. Collect newly added files (status `A`) with a known source extension.
/// 3. Skip structural files (`mod.rs`, `lib.rs`, `main.rs`, `index.*`) and test files.
/// 4. Skip files whose importable name is too generic to search for meaningfully.
/// 5. For each remaining file, search the worktree for language-appropriate import
///    references to its importable name.
/// 6. Return every file for which no reference was found.
pub fn detect_unwired_files(
    worktree_path: &Path,
    base_branch: &str,
) -> Result<WiringDetectionResult> {
    let new_files = collect_added_source_files(worktree_path, base_branch)?;

    let mut unwired_files = Vec::new();

    // Filter generic names first, then count remaining candidates (fix #6).
    let candidate_files: Vec<String> = new_files
        .into_iter()
        .filter(|file_path| {
            let importable_name = extract_importable_name(file_path);
            !GENERIC_NAMES.contains(&importable_name.as_str())
        })
        .collect();

    // total_new_files reflects candidates after filtering generic names (fix #6).
    let total_new_files = candidate_files.len();

    for file_path in &candidate_files {
        let importable_name = extract_importable_name(file_path);

        // Validate that importable_name is safe to interpolate into regex patterns.
        // Names with special characters could inject into grep patterns (fix #2).
        if !is_safe_identifier(&importable_name) {
            continue;
        }

        if !is_referenced(worktree_path, file_path, &importable_name)? {
            unwired_files.push(UnwiredFile {
                path: file_path.clone(),
                importable_name,
            });
        }
    }

    Ok(WiringDetectionResult {
        unwired_files,
        total_new_files,
    })
}

// ---------------------------------------------------------------------------
// Internal helpers
// ---------------------------------------------------------------------------

/// Return `true` if `name` contains only characters safe for regex interpolation.
/// Accepts identifiers matching `^[a-zA-Z0-9_]+$` (fix #2).
fn is_safe_identifier(name: &str) -> bool {
    !name.is_empty() && name.chars().all(|c| c.is_ascii_alphanumeric() || c == '_')
}

/// Run `git diff --name-status <base>..HEAD` and return the paths of files
/// whose status is `A` (added) and whose extension is a known source extension.
/// Files identified as test files or structural entrypoints are excluded.
fn collect_added_source_files(worktree_path: &Path, base_branch: &str) -> Result<Vec<String>> {
    let output = Command::new("git")
        .args(["diff", "--name-status", &format!("{}..HEAD", base_branch)])
        .current_dir(worktree_path)
        .output()
        .with_context(|| format!("Failed to run git diff in {}", worktree_path.display()))?;

    // Check that git itself succeeded (fix #1).
    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        bail!(
            "git diff failed in {}: {}",
            worktree_path.display(),
            stderr.trim()
        );
    }

    let stdout = String::from_utf8_lossy(&output.stdout);
    let mut result = Vec::new();

    for line in stdout.lines() {
        // Lines look like "A\tsrc/cache/store.rs" or "M\tsrc/existing.rs"
        let parts: Vec<&str> = line.splitn(2, '\t').collect();
        if parts.len() != 2 {
            continue;
        }

        let status = parts[0].trim();
        let file_path = parts[1].trim();

        // Only newly added files
        if status != "A" {
            continue;
        }

        // Filter by source extension
        if !has_source_extension(file_path) {
            continue;
        }

        // Exclude structural entrypoint files
        let stem = file_stem(file_path);
        if EXCLUDED_STEMS.contains(&stem.as_str()) {
            continue;
        }

        // Exclude test files (path contains "test" or "_test")
        if is_test_file(file_path) {
            continue;
        }

        result.push(file_path.to_string());
    }

    Ok(result)
}

/// Return `true` if the file path has a recognised source extension.
fn has_source_extension(path: &str) -> bool {
    let ext = path.rsplit('.').next().unwrap_or("");
    SOURCE_EXTENSIONS.contains(&ext)
}

/// Return `true` if the file is a test file based on its path segments.
fn is_test_file(path: &str) -> bool {
    let lower = path.to_lowercase();
    lower.contains("/test/")
        || lower.contains("/tests/")
        || lower.contains("_test.")
        || lower.ends_with("_test")
        || lower.contains(".test.")
        || lower.contains(".spec.")
}

/// Extract the importable name from a file path.
///
/// For `src/cache/store.rs` this is `store`.
/// For `src/cache.rs` this is `cache`.
fn extract_importable_name(path: &str) -> String {
    file_stem(path)
}

/// Extract the file stem (filename without extension) from a path string.
fn file_stem(path: &str) -> String {
    let filename = path.rsplit('/').next().unwrap_or(path);
    // Strip extension
    match filename.rfind('.') {
        Some(dot) => filename[..dot].to_string(),
        None => filename.to_string(),
    }
}

/// Search the worktree for references to `importable_name` in files other than
/// the file that was added. Returns `true` if at least one reference is found.
fn is_referenced(worktree_path: &Path, added_file: &str, importable_name: &str) -> Result<bool> {
    // Build language-aware patterns based on the file extension
    let ext = added_file.rsplit('.').next().unwrap_or("").to_lowercase();
    let patterns = build_search_patterns(&ext, importable_name);

    for pattern in &patterns {
        // Use grep -r -l with source-only includes and exclusions for noisy dirs (fix #3).
        // -m 1 stops after the first match to avoid unbounded output (fix #4).
        // --binary-files=without-match skips binary files (fix #4).
        let output = Command::new("grep")
            .args([
                "-r",
                "-l",
                "-m",
                "1",
                "--binary-files=without-match",
                "--include=*.rs",
                "--include=*.ts",
                "--include=*.tsx",
                "--include=*.js",
                "--include=*.jsx",
                "--include=*.py",
                "--include=*.go",
                "--exclude-dir=.git",
                "--exclude-dir=target",
                "--exclude-dir=node_modules",
                "--exclude-dir=.worktrees",
                pattern,
                ".",
            ])
            .current_dir(worktree_path)
            .output();

        let output = match output {
            Ok(o) => o,
            Err(_) => continue, // grep not available or failed; skip this pattern
        };

        // Exit code 2 means grep encountered an error (fix #5).
        if output.status.code() == Some(2) {
            let stderr = String::from_utf8_lossy(&output.stderr);
            eprintln!(
                "wiring_detection: grep error for pattern {:?}: {}",
                pattern,
                stderr.trim()
            );
            continue;
        }

        let stdout = String::from_utf8_lossy(&output.stdout);

        for matched_file in stdout.lines() {
            let matched_file = matched_file.trim();
            // Normalise: strip leading "./"
            let normalised = matched_file.trim_start_matches("./");

            // Exclude the file itself
            if normalised == added_file {
                continue;
            }

            // Found a reference in another file
            return Ok(true);
        }
    }

    Ok(false)
}

/// Build a list of grep patterns for detecting references to `name` in files
/// of the given `extension`.
fn build_search_patterns(ext: &str, name: &str) -> Vec<String> {
    match ext {
        "rs" => vec![format!(r"mod {}", name), format!(r"use .*{}", name)],
        "ts" | "tsx" | "js" | "jsx" => vec![
            format!(r#"from.*['"'].*{}['"']"#, name),
            format!(r"require.*{}", name),
        ],
        "py" => vec![format!(r"import {}", name), format!(r"from {} ", name)],
        "go" => vec![format!(r#"import.*".*{}"#, name)],
        _ => vec![
            // Generic fallback: just look for the name
            name.to_string(),
        ],
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_extract_importable_name_nested() {
        assert_eq!(extract_importable_name("src/cache/store.rs"), "store");
    }

    #[test]
    fn test_extract_importable_name_top_level() {
        assert_eq!(extract_importable_name("src/cache.rs"), "cache");
    }

    #[test]
    fn test_extract_importable_name_no_extension() {
        assert_eq!(extract_importable_name("src/cache"), "cache");
    }

    #[test]
    fn test_has_source_extension_rs() {
        assert!(has_source_extension("src/foo.rs"));
    }

    #[test]
    fn test_has_source_extension_ts() {
        assert!(has_source_extension("src/foo.ts"));
    }

    #[test]
    fn test_has_source_extension_txt() {
        assert!(!has_source_extension("src/foo.txt"));
    }

    #[test]
    fn test_is_test_file_tests_dir() {
        assert!(is_test_file("src/tests/foo.rs"));
    }

    #[test]
    fn test_is_test_file_suffix() {
        assert!(is_test_file("src/foo_test.rs"));
    }

    #[test]
    fn test_is_test_file_spec() {
        assert!(is_test_file("src/foo.spec.ts"));
    }

    #[test]
    fn test_is_not_test_file() {
        assert!(!is_test_file("src/cache/store.rs"));
    }

    #[test]
    fn test_excluded_stem_mod() {
        let stem = file_stem("src/cache/mod.rs");
        assert!(EXCLUDED_STEMS.contains(&stem.as_str()));
    }

    #[test]
    fn test_build_search_patterns_rust() {
        let patterns = build_search_patterns("rs", "store");
        assert!(patterns.iter().any(|p| p.contains("mod store")));
        assert!(patterns.iter().any(|p| p.contains("use .*store")));
    }

    #[test]
    fn test_build_search_patterns_typescript() {
        let patterns = build_search_patterns("ts", "client");
        assert!(patterns.iter().any(|p| p.contains("client")));
    }

    #[test]
    fn test_build_search_patterns_python() {
        let patterns = build_search_patterns("py", "utils");
        assert!(patterns.iter().any(|p| p.contains("import utils")));
    }

    #[test]
    fn test_build_search_patterns_go() {
        let patterns = build_search_patterns("go", "cache");
        assert!(patterns.iter().any(|p| p.contains("cache")));
    }

    #[test]
    fn test_is_safe_identifier_valid() {
        assert!(is_safe_identifier("store"));
        assert!(is_safe_identifier("my_module"));
        assert!(is_safe_identifier("Module123"));
    }

    #[test]
    fn test_is_safe_identifier_invalid() {
        assert!(!is_safe_identifier(""));
        assert!(!is_safe_identifier("foo-bar"));
        assert!(!is_safe_identifier("foo.bar"));
        assert!(!is_safe_identifier("foo bar"));
        assert!(!is_safe_identifier("foo*"));
        assert!(!is_safe_identifier("../etc/passwd"));
    }
}
