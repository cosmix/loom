//! Knowledge command - manage curated codebase knowledge.
//!
//! Design principle: Claude Code already has Glob, Grep, Read, LSP tools.
//! We curate high-level knowledge that helps agents know WHERE to look,
//! not raw indexing.

use crate::fs::knowledge::{
    KnowledgeDir, KnowledgeFile, DEFAULT_MAX_FILE_LINES, DEFAULT_MAX_TOTAL_LINES,
};
use crate::fs::work_dir::WorkDir;
use anyhow::{bail, Context, Result};
use colored::Colorize;

/// Show the knowledge summary or a specific knowledge file
pub fn show(file: Option<String>) -> Result<()> {
    let work_dir = WorkDir::new(".")?;

    let main_project_root = work_dir
        .main_project_root()
        .context("Could not determine main project root")?;
    let knowledge = KnowledgeDir::new(main_project_root);

    if !knowledge.exists() {
        println!(
            "{} Knowledge directory not found. Run 'loom knowledge init' to create it.",
            "─".dimmed()
        );
        return Ok(());
    }

    match file {
        Some(file_name) => {
            // Show specific file
            let file_type = parse_file_type(&file_name)?;
            let content = knowledge.read(file_type)?;
            println!("{content}");
        }
        None => {
            // Show summary
            let summary = knowledge.generate_summary()?;
            if summary.is_empty() {
                println!("{} No knowledge files have content yet.", "─".dimmed());
            } else {
                println!("{summary}");
            }
        }
    }

    Ok(())
}

/// Update (append to) a knowledge file
pub fn update(file: String, content: String) -> Result<()> {
    let work_dir = WorkDir::new(".")?;

    let main_project_root = work_dir
        .main_project_root()
        .context("Could not determine main project root")?;
    let knowledge = KnowledgeDir::new(main_project_root);

    if !knowledge.exists() {
        knowledge
            .initialize()
            .context("Failed to initialize knowledge directory")?;
    }

    let file_type = parse_file_type(&file)?;
    knowledge.append(file_type, &content)?;

    println!(
        "{} Appended to {}",
        "✓".green().bold(),
        file_type.filename()
    );

    Ok(())
}

/// Initialize the knowledge directory with default files
pub fn init() -> Result<()> {
    let work_dir = WorkDir::new(".")?;

    let main_project_root = work_dir
        .main_project_root()
        .context("Could not determine main project root")?;
    let knowledge = KnowledgeDir::new(main_project_root);

    if knowledge.exists() {
        println!(
            "{} Knowledge directory already exists at {}",
            "─".dimmed(),
            knowledge.root().display()
        );
        return Ok(());
    }

    knowledge.initialize()?;

    println!("{} Initialized knowledge directory", "✓".green().bold());
    println!();
    println!("Created files:");
    for file_type in KnowledgeFile::all() {
        println!("  {} - {}", file_type.filename(), file_type.description());
    }

    Ok(())
}

/// List all knowledge files
pub fn list() -> Result<()> {
    let work_dir = WorkDir::new(".")?;

    let main_project_root = work_dir
        .main_project_root()
        .context("Could not determine main project root")?;
    let knowledge = KnowledgeDir::new(main_project_root);

    if !knowledge.exists() {
        println!(
            "{} Knowledge directory not found. Run 'loom knowledge init' to create it.",
            "─".dimmed()
        );
        return Ok(());
    }

    let files = knowledge.list_files()?;

    if files.is_empty() {
        println!("{} No knowledge files found.", "─".dimmed());
        return Ok(());
    }

    println!("{}", "Knowledge Files".bold());
    println!();

    for (file_type, path) in files {
        let content = std::fs::read_to_string(&path).ok();
        let line_count = content.as_ref().map(|c| c.lines().count()).unwrap_or(0);

        println!(
            "  {} {} ({} lines)",
            "─".dimmed(),
            file_type.filename().cyan(),
            line_count
        );
        println!("    {}", file_type.description().dimmed());
    }

    Ok(())
}

/// Analyze knowledge files and print compaction instructions
pub fn gc(max_file_lines: usize, max_total_lines: usize, quiet: bool) -> Result<()> {
    let work_dir = WorkDir::new(".")?;

    let main_project_root = work_dir
        .main_project_root()
        .context("Could not determine main project root")?;
    let knowledge = KnowledgeDir::new(main_project_root);

    if !knowledge.exists() {
        println!(
            "{} Knowledge directory not found. Run 'loom knowledge init' to create it.",
            "─".dimmed()
        );
        return Ok(());
    }

    let metrics = knowledge.analyze_gc_metrics(max_file_lines, max_total_lines)?;

    // Header
    println!("{}", "Knowledge GC Analysis".bold());
    println!();

    // Files section
    println!("{}", "Files:".cyan().bold());
    for file_metric in &metrics.per_file {
        let icon = if file_metric.has_issues {
            "⚠".yellow().to_string()
        } else {
            "─".dimmed().to_string()
        };

        println!(
            "  {} {} ({} lines, {} dups, {} promoted)",
            icon,
            file_metric.file_type.filename().cyan(),
            file_metric.line_count,
            file_metric.duplicate_headers.len(),
            file_metric.promoted_block_count,
        );
    }

    println!();
    println!("Total: {} lines", metrics.total_lines);
    println!();

    if metrics.gc_recommended {
        println!("GC recommended: {}", "YES".yellow().bold());
        for reason in &metrics.reasons {
            println!("  - {}", reason);
        }

        if !quiet {
            println!();
            println!("{}", "Compaction Instructions:".cyan().bold());
            println!("  1. Review each knowledge file for outdated or redundant content");
            println!("  2. Merge duplicate headers into single consolidated sections");
            println!("  3. Summarize promoted memory blocks into concise knowledge");
            println!("  4. Remove any content that is no longer accurate");
            println!("  5. Edit files directly in doc/loom/knowledge/");
        }
    } else {
        println!(
            "{}",
            "Knowledge files are clean. No compaction needed.".green()
        );
    }

    Ok(())
}

/// Result of checking a single knowledge file
#[derive(Debug)]
pub struct FileCheckResult {
    pub file_type: KnowledgeFile,
    pub exists: bool,
    pub has_content: bool,
    pub section_count: usize,
}

/// Result of checking src/ directory coverage
#[derive(Debug)]
pub struct SrcCoverageResult {
    pub src_directories: Vec<String>,
    pub mentioned_directories: Vec<String>,
    pub coverage_percent: f64,
}

/// Overall result of the knowledge check
#[derive(Debug)]
pub struct KnowledgeCheckResult {
    pub directory_exists: bool,
    pub file_results: Vec<FileCheckResult>,
    pub src_coverage: Option<SrcCoverageResult>,
    pub overall_pass: bool,
}

/// Check knowledge completeness and src/ coverage
pub fn check(min_coverage: u8, src_path: Option<String>, quiet: bool) -> Result<()> {
    let work_dir = WorkDir::new(".")?;

    let main_project_root = work_dir
        .main_project_root()
        .context("Could not determine main project root")?;
    let knowledge = KnowledgeDir::new(&main_project_root);

    // Check if knowledge directory exists
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

    // Check for critical failure: architecture.md empty
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

    // Check src coverage if applicable
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

        // GC advisory (non-blocking, shown after main results)
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

/// Analyze knowledge file completeness and src coverage
fn analyze_knowledge_completeness(
    knowledge: &KnowledgeDir,
    project_root: &std::path::Path,
    src_path: Option<String>,
) -> Result<KnowledgeCheckResult> {
    let mut file_results = Vec::new();

    // Check each knowledge file
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

    // Analyze src coverage
    let src_coverage = analyze_src_coverage(knowledge, project_root, src_path)?;

    // Determine overall pass (architecture has content)
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

/// Count meaningful content sections in a knowledge file
fn count_content_sections(content: &str) -> (bool, usize) {
    let mut section_count = 0;
    for line in content.lines() {
        // Count ## headers that aren't placeholder text
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

/// Analyze how well architecture.md covers src/ directories
fn analyze_src_coverage(
    knowledge: &KnowledgeDir,
    project_root: &std::path::Path,
    src_path: Option<String>,
) -> Result<Option<SrcCoverageResult>> {
    let src_directories = get_src_subdirectories(project_root, src_path)?;

    if src_directories.is_empty() {
        return Ok(None);
    }

    // Read architecture.md content
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

    // Check which directories are mentioned
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

/// Get subdirectories of src/ (or specified path)
fn get_src_subdirectories(
    project_root: &std::path::Path,
    src_path: Option<String>,
) -> Result<Vec<String>> {
    // Determine the source path to scan
    let src_dir = if let Some(custom_path) = src_path {
        project_root.join(custom_path)
    } else {
        // Try common source directories
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

    // Get immediate subdirectories
    let mut directories = Vec::new();
    if let Ok(entries) = std::fs::read_dir(&src_dir) {
        for entry in entries.flatten() {
            if entry.path().is_dir() {
                if let Some(name) = entry.file_name().to_str() {
                    // Skip hidden and test directories
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

/// Check if a directory name is mentioned in the content
fn is_directory_mentioned(dir_name: &str, content: &str) -> bool {
    let content_lower = content.to_lowercase();
    let dir_lower = dir_name.to_lowercase();

    // Check for various mention patterns
    let patterns = [
        format!("{}/", dir_lower),    // path reference
        format!("/{}/", dir_lower),   // absolute path
        format!("`{}`", dir_lower),   // code reference
        format!("**{}**", dir_lower), // bold reference
        format!(" {} ", dir_lower),   // word boundary
        format!("\n{}\n", dir_lower), // line boundary
        format!("{} -", dir_lower),   // list item
        format!("- {}", dir_lower),   // list item start
        format!("/{}", dir_lower),    // path start
        format!("{}:", dir_lower),    // section header
        format!("## {}", dir_lower),  // markdown header
        format!("### {}", dir_lower), // markdown subheader
        dir_lower.replace('_', " "),  // underscore to space
        dir_lower.replace('_', "-"),  // underscore to dash
    ];

    patterns.iter().any(|p| content_lower.contains(p))
}

/// Print the check results in a formatted way
fn print_check_results(result: &KnowledgeCheckResult) {
    println!("{}", "Knowledge Check Results".bold());
    println!();

    // File status
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

    // Src coverage
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

/// Parse a file type from a string argument
fn parse_file_type(file: &str) -> Result<KnowledgeFile> {
    // Try exact filename match first
    if let Some(file_type) = KnowledgeFile::from_filename(file) {
        return Ok(file_type);
    }

    // Try matching without .md extension
    let with_ext = format!("{file}.md");
    if let Some(file_type) = KnowledgeFile::from_filename(&with_ext) {
        return Ok(file_type);
    }

    // Try common aliases
    match file.to_lowercase().as_str() {
        "arch" | "architecture" | "map" | "overview" => Ok(KnowledgeFile::Architecture),
        "entry" | "entries" | "entry-point" | "entrypoints" => Ok(KnowledgeFile::EntryPoints),
        "pattern" => Ok(KnowledgeFile::Patterns),
        "convention" | "conventions" | "code" | "coding" => Ok(KnowledgeFile::Conventions),
        "mistake" | "mistakes" | "lessons" | "lesson" => Ok(KnowledgeFile::Mistakes),
        "stack" | "deps" | "dependencies" | "tech" | "tooling" => Ok(KnowledgeFile::Stack),
        "concerns" | "concern" | "debt" | "issues" | "warnings" => Ok(KnowledgeFile::Concerns),
        _ => {
            let valid_files: Vec<_> = KnowledgeFile::all().iter().map(|f| f.filename()).collect();
            bail!(
                "Unknown knowledge file: '{}'. Valid files: {}",
                file,
                valid_files.join(", ")
            )
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serial_test::serial;
    use std::fs;
    use tempfile::TempDir;

    fn setup_test_env() -> (TempDir, std::path::PathBuf) {
        let temp_dir = TempDir::new().expect("Failed to create temp dir");
        let test_dir = temp_dir.path().to_path_buf();

        // NO .work directory needed - knowledge commands work without it
        // This test proves the fix: knowledge commands should work in any git repo

        (temp_dir, test_dir)
    }

    #[test]
    fn test_parse_file_type() {
        assert_eq!(
            parse_file_type("entry-points.md").unwrap(),
            KnowledgeFile::EntryPoints
        );
        assert_eq!(
            parse_file_type("entry-points").unwrap(),
            KnowledgeFile::EntryPoints
        );
        assert_eq!(
            parse_file_type("patterns").unwrap(),
            KnowledgeFile::Patterns
        );
        assert_eq!(
            parse_file_type("conventions").unwrap(),
            KnowledgeFile::Conventions
        );
        assert_eq!(
            parse_file_type("entry").unwrap(),
            KnowledgeFile::EntryPoints
        );
        // Test mistakes and its aliases
        assert_eq!(
            parse_file_type("mistakes").unwrap(),
            KnowledgeFile::Mistakes
        );
        assert_eq!(
            parse_file_type("mistakes.md").unwrap(),
            KnowledgeFile::Mistakes
        );
        assert_eq!(parse_file_type("mistake").unwrap(), KnowledgeFile::Mistakes);
        assert_eq!(parse_file_type("lessons").unwrap(), KnowledgeFile::Mistakes);
        assert_eq!(parse_file_type("lesson").unwrap(), KnowledgeFile::Mistakes);
        // Test new Stack type
        assert_eq!(parse_file_type("stack").unwrap(), KnowledgeFile::Stack);
        assert_eq!(parse_file_type("stack.md").unwrap(), KnowledgeFile::Stack);
        assert_eq!(parse_file_type("deps").unwrap(), KnowledgeFile::Stack);
        assert_eq!(
            parse_file_type("dependencies").unwrap(),
            KnowledgeFile::Stack
        );
        assert_eq!(parse_file_type("tech").unwrap(), KnowledgeFile::Stack);
        // Test new Concerns type
        assert_eq!(
            parse_file_type("concerns").unwrap(),
            KnowledgeFile::Concerns
        );
        assert_eq!(
            parse_file_type("concerns.md").unwrap(),
            KnowledgeFile::Concerns
        );
        assert_eq!(parse_file_type("debt").unwrap(), KnowledgeFile::Concerns);
        assert_eq!(parse_file_type("issues").unwrap(), KnowledgeFile::Concerns);
        // Test unknown
        assert!(parse_file_type("unknown").is_err());
    }

    #[test]
    #[serial]
    fn test_knowledge_init() {
        let (_temp_dir, test_dir) = setup_test_env();

        let original_dir = std::env::current_dir().expect("Failed to get current dir");
        std::env::set_current_dir(&test_dir).expect("Failed to change dir");

        let result = init();
        assert!(result.is_ok());

        // Verify files were created at doc/loom/knowledge
        let knowledge_dir = test_dir.join("doc/loom/knowledge");
        assert!(knowledge_dir.exists());
        assert!(knowledge_dir.join("entry-points.md").exists());
        assert!(knowledge_dir.join("patterns.md").exists());
        assert!(knowledge_dir.join("conventions.md").exists());

        std::env::set_current_dir(original_dir).expect("Failed to restore dir");
    }

    #[test]
    #[serial]
    fn test_knowledge_update() {
        let (_temp_dir, test_dir) = setup_test_env();

        let original_dir = std::env::current_dir().expect("Failed to get current dir");
        std::env::set_current_dir(&test_dir).expect("Failed to change dir");

        // Initialize first
        init().expect("Failed to init knowledge");

        // Update entry-points
        let result = update(
            "entry-points".to_string(),
            "## New Section\n\n- New entry".to_string(),
        );
        assert!(result.is_ok());

        // Verify content was appended at doc/loom/knowledge
        let content =
            fs::read_to_string(test_dir.join("doc/loom/knowledge/entry-points.md")).unwrap();
        assert!(content.contains("## New Section"));
        assert!(content.contains("- New entry"));

        std::env::set_current_dir(original_dir).expect("Failed to restore dir");
    }

    /// Helper to set up a worktree-like structure with symlinked .work
    fn setup_worktree_env() -> (TempDir, std::path::PathBuf, std::path::PathBuf) {
        let temp_dir = TempDir::new().expect("Failed to create temp dir");
        let base = temp_dir.path();

        // Create main repo structure: base/main-repo/.work/
        let main_repo = base.join("main-repo");
        let main_work = main_repo.join(".work");
        fs::create_dir_all(&main_work).expect("Failed to create main .work dir");

        // Create required subdirectories in main .work
        for subdir in &[
            "runners",
            "tracks",
            "signals",
            "handoffs",
            "archive",
            "stages",
            "sessions",
            "logs",
            "crashes",
            "checkpoints",
            "task-state",
        ] {
            fs::create_dir(main_work.join(subdir)).expect("Failed to create subdir");
        }

        // Create worktree structure: base/main-repo/.worktrees/my-worktree/
        let worktree = main_repo.join(".worktrees").join("my-worktree");
        fs::create_dir_all(&worktree).expect("Failed to create worktree dir");

        // Create symlink: worktree/.work -> ../../.work
        let worktree_work = worktree.join(".work");
        #[cfg(unix)]
        std::os::unix::fs::symlink("../../.work", &worktree_work)
            .expect("Failed to create symlink");

        (temp_dir, main_repo, worktree)
    }

    #[test]
    #[serial]
    #[cfg(unix)] // Symlink tests only work reliably on Unix
    fn test_knowledge_update_in_worktree_writes_to_main_repo() {
        let (_temp_dir, main_repo, worktree) = setup_worktree_env();

        let original_dir = std::env::current_dir().expect("Failed to get current dir");
        std::env::set_current_dir(&worktree).expect("Failed to change dir to worktree");

        // Initialize knowledge - should write to main repo
        let result = init();
        assert!(result.is_ok(), "init() failed: {result:?}");

        // Verify files were created in MAIN REPO, not worktree
        let main_knowledge_dir = main_repo.join("doc/loom/knowledge");
        let worktree_knowledge_dir = worktree.join("doc/loom/knowledge");

        assert!(
            main_knowledge_dir.exists(),
            "Knowledge dir should exist in main repo at {main_knowledge_dir:?}"
        );
        assert!(
            !worktree_knowledge_dir.exists(),
            "Knowledge dir should NOT exist in worktree at {worktree_knowledge_dir:?}"
        );
        assert!(main_knowledge_dir.join("entry-points.md").exists());

        // Update knowledge - should also write to main repo
        let result = update(
            "entry-points".to_string(),
            "## Test Entry\n\n- test/file.rs - Test description".to_string(),
        );
        assert!(result.is_ok(), "update() failed: {result:?}");

        // Verify content was written to main repo
        let content = fs::read_to_string(main_knowledge_dir.join("entry-points.md")).unwrap();
        assert!(
            content.contains("## Test Entry"),
            "Content should be in main repo"
        );

        // Double-check worktree doesn't have the file
        assert!(
            !worktree_knowledge_dir.exists(),
            "Worktree should still not have knowledge dir"
        );

        std::env::set_current_dir(original_dir).expect("Failed to restore dir");
    }

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
        // Test path format
        assert!(is_directory_mentioned("src", "located at src/lib.rs"));
        // Test code format
        assert!(is_directory_mentioned("models", "the `models` directory"));
        // Test bold format
        assert!(is_directory_mentioned("utils", "the **utils** module"));
        // Test header format
        assert!(is_directory_mentioned("api", "## api\n\nAPI routes"));
    }

    #[test]
    #[serial]
    fn test_check_missing_directory() {
        let temp_dir = TempDir::new().expect("Failed to create temp dir");
        let test_dir = temp_dir.path().to_path_buf();

        let original_dir = std::env::current_dir().expect("Failed to get current dir");
        std::env::set_current_dir(&test_dir).expect("Failed to change dir");

        // Don't initialize knowledge - directory should not exist
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

        // Initialize knowledge but don't add content
        init().expect("Failed to init knowledge");

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

        // Initialize and add content
        init().expect("Failed to init knowledge");
        update(
            "architecture".to_string(),
            "## Overview\n\nProject architecture here".to_string(),
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

        // Create a src directory with subdirectories
        let src_dir = test_dir.join("src");
        fs::create_dir_all(src_dir.join("commands")).unwrap();
        fs::create_dir_all(src_dir.join("models")).unwrap();
        fs::create_dir_all(src_dir.join("utils")).unwrap();
        fs::create_dir_all(src_dir.join("daemon")).unwrap();

        let original_dir = std::env::current_dir().expect("Failed to get current dir");
        std::env::set_current_dir(&test_dir).expect("Failed to change dir");

        // Initialize and add content mentioning only 2 of 4 directories
        init().expect("Failed to init knowledge");
        update(
            "architecture".to_string(),
            "## Overview\n\n- commands/ - CLI\n- models/ - Data".to_string(),
        )
        .expect("Failed to update architecture");

        // With 50% coverage (2/4 = 50%), should pass at min_coverage=50
        let result = check(50, None, true);
        assert!(result.is_ok());

        // With 75% minimum, should fail (only have 50%)
        let result = check(75, None, true);
        assert!(result.is_err());

        std::env::set_current_dir(original_dir).expect("Failed to restore dir");
    }

    #[test]
    fn test_get_src_subdirectories() {
        let temp_dir = TempDir::new().expect("Failed to create temp dir");
        let test_dir = temp_dir.path().to_path_buf();

        // Create src with subdirs
        let src_dir = test_dir.join("src");
        fs::create_dir_all(src_dir.join("commands")).unwrap();
        fs::create_dir_all(src_dir.join("models")).unwrap();
        fs::create_dir_all(src_dir.join(".hidden")).unwrap(); // Should be skipped
        fs::create_dir_all(src_dir.join("target")).unwrap(); // Should be skipped

        let dirs = get_src_subdirectories(&test_dir, None).unwrap();
        assert!(dirs.contains(&"commands".to_string()));
        assert!(dirs.contains(&"models".to_string()));
        assert!(!dirs.contains(&".hidden".to_string()));
        assert!(!dirs.contains(&"target".to_string()));
    }

    #[test]
    #[serial]
    fn test_gc_clean() {
        let (_temp_dir, test_dir) = setup_test_env();
        let original_dir = std::env::current_dir().expect("Failed to get current dir");
        std::env::set_current_dir(&test_dir).expect("Failed to change dir");

        init().expect("Failed to init knowledge");
        update(
            "architecture".to_string(),
            "## Overview\n\nSmall content".to_string(),
        )
        .expect("Failed to update");

        let result = gc(200, 800, true);
        assert!(result.is_ok());

        std::env::set_current_dir(original_dir).expect("Failed to restore dir");
    }

    #[test]
    #[serial]
    fn test_gc_large_file() {
        let (_temp_dir, test_dir) = setup_test_env();
        let original_dir = std::env::current_dir().expect("Failed to get current dir");
        std::env::set_current_dir(&test_dir).expect("Failed to change dir");

        init().expect("Failed to init knowledge");

        // Add >200 lines to architecture
        let mut big_content = String::from("## Big Section\n\n");
        for i in 0..250 {
            big_content.push_str(&format!("- Line {}\n", i));
        }
        update("architecture".to_string(), big_content).expect("Failed to update");

        let result = gc(200, 800, true);
        assert!(result.is_ok());

        std::env::set_current_dir(original_dir).expect("Failed to restore dir");
    }

    #[test]
    #[serial]
    fn test_check_includes_gc_analysis() {
        let (_temp_dir, test_dir) = setup_test_env();
        let original_dir = std::env::current_dir().expect("Failed to get current dir");
        std::env::set_current_dir(&test_dir).expect("Failed to change dir");

        init().expect("Failed to init knowledge");
        update(
            "architecture".to_string(),
            "## Overview\n\nProject architecture here".to_string(),
        )
        .expect("Failed to update architecture");

        // check should still pass - GC analysis is advisory only
        let result = check(50, None, false);
        assert!(result.is_ok());

        std::env::set_current_dir(original_dir).expect("Failed to restore dir");
    }
}
