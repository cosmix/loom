//! Knowledge command - manage curated codebase knowledge.
//!
//! Design principle: Claude Code already has Glob, Grep, Read, LSP tools.
//! We curate high-level knowledge that helps agents know WHERE to look,
//! not raw indexing.

use crate::fs::knowledge::{KnowledgeDir, KnowledgeFile};
use crate::fs::work_dir::WorkDir;
use anyhow::{bail, Context, Result};
use colored::Colorize;

/// Show the knowledge summary or a specific knowledge file
pub fn show(file: Option<String>) -> Result<()> {
    let work_dir = WorkDir::new(".")?;
    work_dir.load()?;

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
    work_dir.load()?;

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
    work_dir.load()?;

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
    work_dir.load()?;

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
        "entry" | "entries" | "entry-point" | "entrypoints" => Ok(KnowledgeFile::EntryPoints),
        "pattern" | "arch" | "architecture" => Ok(KnowledgeFile::Patterns),
        "convention" | "conventions" | "code" | "coding" => Ok(KnowledgeFile::Conventions),
        "mistake" | "mistakes" | "lessons" | "lesson" => Ok(KnowledgeFile::Mistakes),
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

        // Create minimal .work directory structure
        let work_dir_path = test_dir.join(".work");
        fs::create_dir_all(&work_dir_path).expect("Failed to create .work dir");

        // Create required subdirectories
        for subdir in &[
            "runners", "tracks", "signals", "handoffs", "archive", "stages", "sessions", "logs",
            "crashes",
        ] {
            fs::create_dir(work_dir_path.join(subdir)).expect("Failed to create subdir");
        }

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
        assert_eq!(
            parse_file_type("mistake").unwrap(),
            KnowledgeFile::Mistakes
        );
        assert_eq!(
            parse_file_type("lessons").unwrap(),
            KnowledgeFile::Mistakes
        );
        assert_eq!(
            parse_file_type("lesson").unwrap(),
            KnowledgeFile::Mistakes
        );
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
        assert!(result.is_ok(), "init() failed: {:?}", result);

        // Verify files were created in MAIN REPO, not worktree
        let main_knowledge_dir = main_repo.join("doc/loom/knowledge");
        let worktree_knowledge_dir = worktree.join("doc/loom/knowledge");

        assert!(
            main_knowledge_dir.exists(),
            "Knowledge dir should exist in main repo at {:?}",
            main_knowledge_dir
        );
        assert!(
            !worktree_knowledge_dir.exists(),
            "Knowledge dir should NOT exist in worktree at {:?}",
            worktree_knowledge_dir
        );
        assert!(main_knowledge_dir.join("entry-points.md").exists());

        // Update knowledge - should also write to main repo
        let result = update(
            "entry-points".to_string(),
            "## Test Entry\n\n- test/file.rs - Test description".to_string(),
        );
        assert!(result.is_ok(), "update() failed: {:?}", result);

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
}
