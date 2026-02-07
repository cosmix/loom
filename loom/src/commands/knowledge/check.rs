//! Knowledge check command - validate knowledge completeness and coverage.

use crate::fs::knowledge::{
    KnowledgeDir, KnowledgeFile, DEFAULT_MAX_FILE_LINES, DEFAULT_MAX_TOTAL_LINES,
};
use crate::fs::work_dir::WorkDir;
use anyhow::{bail, Context, Result};
use colored::Colorize;

#[derive(Debug)]
pub struct FileCheckResult {
    pub file_type: KnowledgeFile,
    pub exists: bool,
    pub has_content: bool,
    pub section_count: usize,
}

#[derive(Debug)]
pub struct SrcCoverageResult {
    pub src_directories: Vec<String>,
    pub mentioned_directories: Vec<String>,
    pub coverage_percent: f64,
}

#[derive(Debug)]
pub struct KnowledgeCheckResult {
    pub directory_exists: bool,
    pub file_results: Vec<FileCheckResult>,
    pub src_coverage: Option<SrcCoverageResult>,
    pub overall_pass: bool,
}

pub fn check(min_coverage: u8, src_path: Option<String>, quiet: bool) -> Result<()> {
    let work_dir = WorkDir::new(".")?;
    let main_project_root = work_dir
        .main_project_root()
        .context("Could not determine main project root")?;
    let knowledge = KnowledgeDir::new(&main_project_root);

    if !knowledge.exists() {
        if !quiet {
            println!(
                "{} Knowledge directory not found at {}",
                "✗".red().bold(),
                knowledge.root().display()
            );
        }
        bail!("Knowledge directory does not exist. Run 'loom knowledge init' first.");
    }

    let result = analyze_knowledge_completeness(&knowledge, &main_project_root, src_path)?;

    let arch_result = result
        .file_results
        .iter()
        .find(|r| r.file_type == KnowledgeFile::Architecture);

    if let Some(arch) = arch_result {
        if !arch.has_content {
            if !quiet {
                println!(
                    "{} architecture.md has no content sections",
                    "✗".red().bold()
                );
            }
            bail!("architecture.md is empty. Architecture documentation is required.");
        }
    }

    if let Some(ref src_coverage) = result.src_coverage {
        if (src_coverage.coverage_percent as u8) < min_coverage {
            if !quiet {
                print_check_results(&result);
                println!(
                    "\n{} Coverage {:.0}% is below minimum {}%",
                    "✗".red().bold(),
                    src_coverage.coverage_percent,
                    min_coverage
                );
            }
            bail!(
                "Source directory coverage ({:.0}%) is below minimum ({}%)",
                src_coverage.coverage_percent,
                min_coverage
            );
        }
    }

    if !quiet {
        print_check_results(&result);

        if let Ok(gc_metrics) =
            knowledge.analyze_gc_metrics(DEFAULT_MAX_FILE_LINES, DEFAULT_MAX_TOTAL_LINES)
        {
            println!();
            println!("{}", "GC Analysis:".cyan().bold());
            for file_metric in &gc_metrics.per_file {
                println!(
                    "  {} {} lines",
                    file_metric.file_type.filename(),
                    file_metric.line_count,
                );
            }
            if gc_metrics.gc_recommended {
                if let Some(first_reason) = gc_metrics.reasons.first() {
                    println!("\n  {} GC advisory: {}", "⚠".yellow(), first_reason);
                }
            }
        }

        println!("\n{} Knowledge check passed", "✓".green().bold());
    }

    Ok(())
}

fn analyze_knowledge_completeness(
    knowledge: &KnowledgeDir,
    project_root: &std::path::Path,
    src_path: Option<String>,
) -> Result<KnowledgeCheckResult> {
    let mut file_results = Vec::new();

    for file_type in KnowledgeFile::all() {
        let path = knowledge.file_path(*file_type);
        let exists = path.exists();
        let (has_content, section_count) = if exists {
            let content = std::fs::read_to_string(&path).unwrap_or_default();
            count_content_sections(&content)
        } else {
            (false, 0)
        };

        file_results.push(FileCheckResult {
            file_type: *file_type,
            exists,
            has_content,
            section_count,
        });
    }

    let src_coverage = analyze_src_coverage(knowledge, project_root, src_path)?;

    let arch_has_content = file_results
        .iter()
        .find(|r| r.file_type == KnowledgeFile::Architecture)
        .map(|r| r.has_content)
        .unwrap_or(false);

    let overall_pass = arch_has_content;

    Ok(KnowledgeCheckResult {
        directory_exists: true,
        file_results,
        src_coverage,
        overall_pass,
    })
}

fn count_content_sections(content: &str) -> (bool, usize) {
    let mut section_count = 0;
    for line in content.lines() {
        if line.starts_with("## ")
            && !line.contains("(Add ")
            && !line.contains("append-only")
            && !line.contains("placeholder")
        {
            section_count += 1;
        }
    }
    (section_count > 0, section_count)
}

fn analyze_src_coverage(
    knowledge: &KnowledgeDir,
    project_root: &std::path::Path,
    src_path: Option<String>,
) -> Result<Option<SrcCoverageResult>> {
    let src_directories = get_src_subdirectories(project_root, src_path)?;

    if src_directories.is_empty() {
        return Ok(None);
    }

    let arch_path = knowledge.file_path(KnowledgeFile::Architecture);
    let arch_content = if arch_path.exists() {
        std::fs::read_to_string(&arch_path).unwrap_or_default()
    } else {
        return Ok(Some(SrcCoverageResult {
            src_directories,
            mentioned_directories: Vec::new(),
            coverage_percent: 0.0,
        }));
    };

    let mentioned_directories: Vec<String> = src_directories
        .iter()
        .filter(|dir| is_directory_mentioned(dir, &arch_content))
        .cloned()
        .collect();

    let coverage_percent = if src_directories.is_empty() {
        100.0
    } else {
        (mentioned_directories.len() as f64 / src_directories.len() as f64) * 100.0
    };

    Ok(Some(SrcCoverageResult {
        src_directories,
        mentioned_directories,
        coverage_percent,
    }))
}

pub fn get_src_subdirectories(
    project_root: &std::path::Path,
    src_path: Option<String>,
) -> Result<Vec<String>> {
    let src_dir = if let Some(custom_path) = src_path {
        project_root.join(custom_path)
    } else {
        let candidates = ["src", "loom/src", "lib", "app"];
        let mut found = None;
        for candidate in &candidates {
            let path = project_root.join(candidate);
            if path.is_dir() {
                found = Some(path);
                break;
            }
        }
        match found {
            Some(p) => p,
            None => return Ok(Vec::new()),
        }
    };

    if !src_dir.is_dir() {
        return Ok(Vec::new());
    }

    let mut directories = Vec::new();
    if let Ok(entries) = std::fs::read_dir(&src_dir) {
        for entry in entries.flatten() {
            if entry.path().is_dir() {
                if let Some(name) = entry.file_name().to_str() {
                    if !name.starts_with('.') && name != "target" && name != "__pycache__" {
                        directories.push(name.to_string());
                    }
                }
            }
        }
    }

    directories.sort();
    Ok(directories)
}

pub fn is_directory_mentioned(dir_name: &str, content: &str) -> bool {
    let content_lower = content.to_lowercase();
    let dir_lower = dir_name.to_lowercase();

    let patterns = [
        format!("{}/", dir_lower),
        format!("/{}/", dir_lower),
        format!("`{}`", dir_lower),
        format!("**{}**", dir_lower),
        format!(" {} ", dir_lower),
        format!("\n{}\n", dir_lower),
        format!("{} -", dir_lower),
        format!("- {}", dir_lower),
        format!("/{}", dir_lower),
        format!("{}:", dir_lower),
        format!("## {}", dir_lower),
        format!("### {}", dir_lower),
        dir_lower.replace('_', " "),
        dir_lower.replace('_', "-"),
    ];

    patterns.iter().any(|p| content_lower.contains(p))
}

pub fn print_check_results(result: &KnowledgeCheckResult) {
    println!("{}", "Knowledge Check Results".bold());
    println!();

    println!("{}", "Files:".cyan().bold());
    for file_result in &result.file_results {
        let status_icon = if file_result.has_content {
            "✓".green()
        } else if file_result.exists {
            "○".yellow()
        } else {
            "✗".red()
        };

        let status_text = if file_result.has_content {
            format!("{} sections", file_result.section_count)
        } else if file_result.exists {
            "empty".to_string()
        } else {
            "missing".to_string()
        };

        println!(
            "  {} {} ({})",
            status_icon,
            file_result.file_type.filename(),
            status_text.dimmed()
        );
    }

    if let Some(ref src_coverage) = result.src_coverage {
        println!();
        println!("{}", "Source Coverage:".cyan().bold());
        println!(
            "  Coverage: {:.0}% ({}/{})",
            src_coverage.coverage_percent,
            src_coverage.mentioned_directories.len(),
            src_coverage.src_directories.len()
        );

        if !src_coverage.mentioned_directories.is_empty() {
            println!(
                "  {} Documented: {}",
                "✓".green(),
                src_coverage.mentioned_directories.join(", ")
            );
        }

        let missing: Vec<_> = src_coverage
            .src_directories
            .iter()
            .filter(|d| !src_coverage.mentioned_directories.contains(d))
            .collect();

        if !missing.is_empty() {
            println!(
                "  {} Missing: {}",
                "○".yellow(),
                missing
                    .iter()
                    .map(|s| s.as_str())
                    .collect::<Vec<_>>()
                    .join(", ")
            );
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serial_test::serial;
    use std::fs;
    use tempfile::TempDir;

    #[test]
    fn test_count_content_sections() {
        let content = r#"# Architecture

> This file is append-only

## Component A

Description here

## Component B

More description

(Add patterns as you discover them)
"#;
        let (has_content, count) = count_content_sections(content);
        assert!(has_content);
        assert_eq!(count, 2);
    }

    #[test]
    fn test_count_content_sections_empty() {
        let content = r#"# Architecture

> This file is append-only

(Add patterns as you discover them)
"#;
        let (has_content, count) = count_content_sections(content);
        assert!(!has_content);
        assert_eq!(count, 0);
    }

    #[test]
    fn test_is_directory_mentioned() {
        let content = r#"
## Directory Structure

- commands/ - CLI command implementations
- daemon/ - Background daemon
- orchestrator/ - Core orchestration
"#;
        assert!(is_directory_mentioned("commands", content));
        assert!(is_directory_mentioned("daemon", content));
        assert!(is_directory_mentioned("orchestrator", content));
        assert!(!is_directory_mentioned("nonexistent", content));
    }

    #[test]
    fn test_is_directory_mentioned_various_formats() {
        assert!(is_directory_mentioned("src", "located at src/lib.rs"));
        assert!(is_directory_mentioned("models", "the `models` directory"));
        assert!(is_directory_mentioned("utils", "the **utils** module"));
        assert!(is_directory_mentioned("api", "## api\n\nAPI routes"));
    }

    #[test]
    #[serial]
    fn test_check_missing_directory() {
        let temp_dir = TempDir::new().expect("Failed to create temp dir");
        let test_dir = temp_dir.path().to_path_buf();

        let original_dir = std::env::current_dir().expect("Failed to get current dir");
        std::env::set_current_dir(&test_dir).expect("Failed to change dir");

        let result = check(50, None, true);
        assert!(result.is_err());
        let err_msg = result.unwrap_err().to_string();
        assert!(err_msg.contains("does not exist"));

        std::env::set_current_dir(original_dir).expect("Failed to restore dir");
    }

    #[test]
    #[serial]
    fn test_check_empty_architecture() {
        let temp_dir = TempDir::new().expect("Failed to create temp dir");
        let test_dir = temp_dir.path().to_path_buf();

        let original_dir = std::env::current_dir().expect("Failed to get current dir");
        std::env::set_current_dir(&test_dir).expect("Failed to change dir");

        crate::commands::knowledge::init().expect("Failed to init knowledge");

        let result = check(50, None, true);
        assert!(result.is_err());
        let err_msg = result.unwrap_err().to_string();
        assert!(err_msg.contains("architecture.md is empty"));

        std::env::set_current_dir(original_dir).expect("Failed to restore dir");
    }

    #[test]
    #[serial]
    fn test_check_passes_with_content() {
        let temp_dir = TempDir::new().expect("Failed to create temp dir");
        let test_dir = temp_dir.path().to_path_buf();

        let original_dir = std::env::current_dir().expect("Failed to get current dir");
        std::env::set_current_dir(&test_dir).expect("Failed to change dir");

        crate::commands::knowledge::init().expect("Failed to init knowledge");
        crate::commands::knowledge::update(
            "architecture".to_string(),
            Some("## Overview\n\nProject architecture here".to_string()),
        )
        .expect("Failed to update architecture");

        let result = check(50, None, true);
        assert!(result.is_ok());

        std::env::set_current_dir(original_dir).expect("Failed to restore dir");
    }

    #[test]
    #[serial]
    fn test_check_coverage_calculation() {
        let temp_dir = TempDir::new().expect("Failed to create temp dir");
        let test_dir = temp_dir.path().to_path_buf();

        let src_dir = test_dir.join("src");
        fs::create_dir_all(src_dir.join("commands")).unwrap();
        fs::create_dir_all(src_dir.join("models")).unwrap();
        fs::create_dir_all(src_dir.join("utils")).unwrap();
        fs::create_dir_all(src_dir.join("daemon")).unwrap();

        let original_dir = std::env::current_dir().expect("Failed to get current dir");
        std::env::set_current_dir(&test_dir).expect("Failed to change dir");

        crate::commands::knowledge::init().expect("Failed to init knowledge");
        crate::commands::knowledge::update(
            "architecture".to_string(),
            Some("## Overview\n\n- commands/ - CLI\n- models/ - Data".to_string()),
        )
        .expect("Failed to update architecture");

        let result = check(50, None, true);
        assert!(result.is_ok());

        let result = check(75, None, true);
        assert!(result.is_err());

        std::env::set_current_dir(original_dir).expect("Failed to restore dir");
    }

    #[test]
    fn test_get_src_subdirectories() {
        let temp_dir = TempDir::new().expect("Failed to create temp dir");
        let test_dir = temp_dir.path().to_path_buf();

        let src_dir = test_dir.join("src");
        fs::create_dir_all(src_dir.join("commands")).unwrap();
        fs::create_dir_all(src_dir.join("models")).unwrap();
        fs::create_dir_all(src_dir.join(".hidden")).unwrap();
        fs::create_dir_all(src_dir.join("target")).unwrap();

        let dirs = get_src_subdirectories(&test_dir, None).unwrap();
        assert!(dirs.contains(&"commands".to_string()));
        assert!(dirs.contains(&"models".to_string()));
        assert!(!dirs.contains(&".hidden".to_string()));
        assert!(!dirs.contains(&"target".to_string()));
    }

    #[test]
    #[serial]
    fn test_check_includes_gc_analysis() {
        let temp_dir = TempDir::new().expect("Failed to create temp dir");
        let test_dir = temp_dir.path().to_path_buf();
        let original_dir = std::env::current_dir().expect("Failed to get current dir");
        std::env::set_current_dir(&test_dir).expect("Failed to change dir");

        crate::commands::knowledge::init().expect("Failed to init knowledge");
        crate::commands::knowledge::update(
            "architecture".to_string(),
            Some("## Overview\n\nProject architecture here".to_string()),
        )
        .expect("Failed to update architecture");

        let result = check(50, None, false);
        assert!(result.is_ok());

        std::env::set_current_dir(original_dir).expect("Failed to restore dir");
    }
}
