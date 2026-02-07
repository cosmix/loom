//! Knowledge command - manage curated codebase knowledge.
pub mod check;
pub mod gc;

use crate::fs::knowledge::{KnowledgeDir, KnowledgeFile};
use crate::fs::work_dir::WorkDir;
use anyhow::{bail, Context, Result};
use colored::Colorize;

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
            let file_type = parse_file_type(&file_name)?;
            let content = knowledge.read(file_type)?;
            println!("{content}");
        }
        None => {
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

fn read_content_from_stdin() -> Result<String> {
    use std::io::Read;
    let limit = (crate::validation::MAX_KNOWLEDGE_CONTENT_LENGTH + 1) as u64;
    let mut buffer = String::new();
    std::io::stdin()
        .take(limit)
        .read_to_string(&mut buffer)
        .context("Failed to read from stdin")?;
    let trimmed = buffer.trim().to_string();
    if trimmed.is_empty() {
        bail!("No content received from stdin");
    }
    crate::validation::validate_knowledge_content(&trimmed)?;
    Ok(trimmed)
}

pub fn update(file: String, content: Option<String>) -> Result<()> {
    let content = match content {
        Some(c) if c == "-" => read_content_from_stdin()?,
        Some(c) => c,
        None => read_content_from_stdin()?,
    };

    crate::validation::validate_knowledge_content(&content)?;

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

fn parse_file_type(file: &str) -> Result<KnowledgeFile> {
    if let Some(file_type) = KnowledgeFile::from_filename(file) {
        return Ok(file_type);
    }

    let with_ext = format!("{file}.md");
    if let Some(file_type) = KnowledgeFile::from_filename(&with_ext) {
        return Ok(file_type);
    }

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
        assert_eq!(parse_file_type("stack").unwrap(), KnowledgeFile::Stack);
        assert_eq!(parse_file_type("stack.md").unwrap(), KnowledgeFile::Stack);
        assert_eq!(parse_file_type("deps").unwrap(), KnowledgeFile::Stack);
        assert_eq!(
            parse_file_type("dependencies").unwrap(),
            KnowledgeFile::Stack
        );
        assert_eq!(parse_file_type("tech").unwrap(), KnowledgeFile::Stack);
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

        init().expect("Failed to init knowledge");

        let result = update(
            "entry-points".to_string(),
            Some("## New Section\n\n- New entry".to_string()),
        );
        assert!(result.is_ok());

        let content =
            fs::read_to_string(test_dir.join("doc/loom/knowledge/entry-points.md")).unwrap();
        assert!(content.contains("## New Section"));
        assert!(content.contains("- New entry"));

        std::env::set_current_dir(original_dir).expect("Failed to restore dir");
    }

    #[test]
    #[serial]
    #[cfg(unix)]
    fn test_knowledge_update_in_worktree_writes_to_main_repo() {
        let temp_dir = TempDir::new().expect("Failed to create temp dir");
        let base = temp_dir.path();

        let main_repo = base.join("main-repo");
        let main_work = main_repo.join(".work");
        fs::create_dir_all(&main_work).expect("Failed to create main .work dir");

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

        let worktree = main_repo.join(".worktrees").join("my-worktree");
        fs::create_dir_all(&worktree).expect("Failed to create worktree dir");

        let worktree_work = worktree.join(".work");
        #[cfg(unix)]
        {
            let target = std::path::PathBuf::from("..").join("..").join(".work");
            std::os::unix::fs::symlink(&target, &worktree_work).expect("Failed to create symlink");
        }

        let original_dir = std::env::current_dir().expect("Failed to get current dir");
        std::env::set_current_dir(&worktree).expect("Failed to change dir to worktree");

        let result = init();
        assert!(result.is_ok(), "init() failed: {result:?}");

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

        let result = update(
            "entry-points".to_string(),
            Some("## Test Entry\n\n- test/file.rs - Test description".to_string()),
        );
        assert!(result.is_ok(), "update() failed: {result:?}");

        let content = fs::read_to_string(main_knowledge_dir.join("entry-points.md")).unwrap();
        assert!(
            content.contains("## Test Entry"),
            "Content should be in main repo"
        );

        assert!(
            !worktree_knowledge_dir.exists(),
            "Worktree should still not have knowledge dir"
        );

        std::env::set_current_dir(original_dir).expect("Failed to restore dir");
    }

    #[test]
    #[serial]
    fn test_update_with_explicit_content() {
        let (_temp_dir, test_dir) = setup_test_env();
        let original_dir = std::env::current_dir().expect("Failed to get current dir");
        std::env::set_current_dir(&test_dir).expect("Failed to change dir");

        init().expect("Failed to init knowledge");

        let result = update(
            "patterns".to_string(),
            Some("## Test Pattern\n\nExplicit content".to_string()),
        );
        assert!(result.is_ok());

        let content = fs::read_to_string(test_dir.join("doc/loom/knowledge/patterns.md")).unwrap();
        assert!(content.contains("## Test Pattern"));
        assert!(content.contains("Explicit content"));

        std::env::set_current_dir(original_dir).expect("Failed to restore dir");
    }
}
