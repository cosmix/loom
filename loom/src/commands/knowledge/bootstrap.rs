//! Knowledge bootstrap command - spawn interactive Claude session to explore and populate knowledge.

use anyhow::{Context, Result};
use colored::Colorize;
use std::path::PathBuf;
use std::process::Command;

use crate::claude::find_claude_path;
use crate::fs::knowledge::{KnowledgeDir, KnowledgeFile};
use crate::fs::work_dir::WorkDir;
use crate::map::{analyze_codebase, AnalysisResult};

/// Execute the knowledge bootstrap command
pub fn execute(model: Option<String>, skip_map: bool, quick: bool) -> Result<()> {
    let project_root = resolve_project_root()?;
    let claude_path = find_claude_path()?;

    println!("{} Knowledge bootstrap starting...", "→".cyan().bold());

    // Initialize knowledge directory if needed
    let knowledge = KnowledgeDir::new(&project_root);
    if !knowledge.exists() {
        knowledge.initialize()?;
        println!("  {} Initialized knowledge directory", "✓".green());
    }

    // Run codebase map unless skipped
    if !skip_map {
        println!("  {} Running codebase analysis...", "→".cyan());
        let result = analyze_codebase(&project_root, true, None)?;
        write_map_results(&knowledge, &result)?;
        println!("  {} Codebase mapped", "✓".green());
    }

    // Read existing knowledge for context embedding
    let existing_knowledge = read_existing_knowledge(&knowledge);

    // Build prompts
    let system_prompt = build_system_prompt(&existing_knowledge);
    let initial_prompt = build_initial_prompt();

    // Spawn Claude session
    println!(
        "\n{} Spawning Claude session for knowledge exploration...\n",
        "→".cyan().bold()
    );

    let mut cmd = Command::new(&claude_path);
    cmd.arg("--permission-mode").arg("auto");
    cmd.arg("--allowedTools")
        .arg("Read,Glob,Grep,Bash(loom knowledge*),Agent");
    cmd.arg("--system-prompt").arg(&system_prompt);

    if let Some(ref m) = model {
        cmd.arg("--model").arg(m);
    }

    if quick {
        cmd.arg("-p");
    }

    cmd.arg(&initial_prompt);

    cmd.env("CLAUDE_CODE_EXPERIMENTAL_AGENT_TEAMS", "1");
    cmd.current_dir(&project_root);
    cmd.stdin(std::process::Stdio::inherit());
    cmd.stdout(std::process::Stdio::inherit());
    cmd.stderr(std::process::Stdio::inherit());

    let status = cmd.status().context("Failed to spawn Claude session")?;

    if !status.success() {
        let code = status.code().unwrap_or(-1);
        if code == 130 {
            // User interrupted (Ctrl+C / SIGINT)
            println!("\n{} Session interrupted by user.", "─".dimmed());
        } else {
            println!(
                "\n{} Claude session exited with code {}",
                "!".yellow().bold(),
                code
            );
        }
    }

    // Print summary
    print_summary(&knowledge)?;

    Ok(())
}

/// Resolve the project root directory.
///
/// Tries WorkDir first (works when .work/ exists), then falls back to
/// `git rev-parse --show-toplevel`, then current directory.
fn resolve_project_root() -> Result<PathBuf> {
    // Try WorkDir first (works when .work/ exists)
    if let Ok(work_dir) = WorkDir::new(".") {
        if let Some(root) = work_dir.main_project_root() {
            return Ok(root);
        }
    }

    // Fall back to git rev-parse
    let output = Command::new("git")
        .args(["rev-parse", "--show-toplevel"])
        .output()
        .context("Failed to run git rev-parse")?;

    if output.status.success() {
        let root = String::from_utf8(output.stdout)
            .context("Invalid UTF-8 in git output")?
            .trim()
            .to_string();
        return Ok(PathBuf::from(root));
    }

    // Last resort: current directory
    std::env::current_dir().context("Failed to get current directory")
}

/// Read existing knowledge files and format them for context embedding.
///
/// Files that only contain the default template (5 lines or fewer) are skipped
/// to avoid embedding uninformative placeholder content.
fn read_existing_knowledge(knowledge: &KnowledgeDir) -> String {
    if !knowledge.exists() {
        return String::new();
    }

    let mut sections = Vec::new();

    if let Ok(files) = knowledge.read_all() {
        for (file_type, content) in files {
            let trimmed = content.trim().to_string();
            // Skip files that only have the template header
            if trimmed.lines().count() > 5 {
                sections.push(format!(
                    "### Existing {}\n\n{}",
                    file_type.filename(),
                    trimmed
                ));
            }
        }
    }

    if sections.is_empty() {
        return String::new();
    }

    format!(
        "## Existing Knowledge (DO NOT DUPLICATE)\n\n\
         The following knowledge has already been documented. \
         Do NOT repeat this information. Only add NEW discoveries.\n\n{}",
        sections.join("\n\n---\n\n")
    )
}

/// Build the system prompt for the Claude session.
fn build_system_prompt(existing_knowledge: &str) -> String {
    let mut prompt = String::from(
        "You are a senior software architect exploring this codebase to populate knowledge files.\n\n\
         ## Your Goal\n\n\
         Populate the project's knowledge files using `loom knowledge update` commands.\n\n\
         ## Rules\n\n\
         1. Use `loom knowledge update <file> - <<'EOF'\\n<content>\\nEOF` for long content (heredoc syntax)\n\
         2. Use `loom knowledge update <file> \"<content>\"` for short content\n\
         3. Valid files: architecture, entry-points, patterns, conventions, mistakes, stack, concerns\n\
         4. Be specific: include file paths with line numbers (e.g., `src/auth.ts:45-80`)\n\
         5. Focus on PATTERNS and RELATIONSHIPS, not just listing files\n\
         6. Each knowledge update should add a complete section with a ## heading\n\n\
         ## Strategy\n\n\
         Use parallel Agent calls to explore 4 dimensions simultaneously:\n\
         - Architecture and data flow -> architecture.md\n\
         - Patterns and conventions -> patterns.md, conventions.md\n\
         - Stack and entry points -> stack.md, entry-points.md\n\
         - Concerns and tech debt -> concerns.md\n\n\
         After agents complete, do a final synthesis pass on architecture.md.\n",
    );

    if !existing_knowledge.is_empty() {
        prompt.push('\n');
        prompt.push_str(existing_knowledge);
        prompt.push('\n');
    }

    prompt
}

/// Build the initial user prompt for the Claude session.
fn build_initial_prompt() -> String {
    String::from(
        "Explore this codebase and populate the knowledge files. \
         Spawn 4 parallel agents to explore different dimensions:\n\n\
         Agent 1 - Architecture: Map component relationships, data flow, module dependencies. \
         Write findings to architecture.md.\n\n\
         Agent 2 - Patterns & Conventions: Identify error handling patterns, state management, \
         coding conventions, naming schemes. Write to patterns.md and conventions.md.\n\n\
         Agent 3 - Stack & Entry Points: Document dependencies, frameworks, tooling, \
         and key entry point files. Write to stack.md and entry-points.md.\n\n\
         Agent 4 - Concerns: Find technical debt, TODOs, FIXMEs, security concerns, \
         and architectural issues. Write to concerns.md.\n\n\
         After all agents complete, do a final synthesis pass on architecture.md \
         to ensure cross-cutting concerns are captured.",
    )
}

/// Write map analysis results directly into knowledge files.
fn write_map_results(knowledge: &KnowledgeDir, result: &AnalysisResult) -> Result<()> {
    if !result.architecture.is_empty() {
        knowledge.append(KnowledgeFile::Architecture, &result.architecture)?;
    }
    if !result.stack.is_empty() {
        knowledge.append(KnowledgeFile::Stack, &result.stack)?;
    }
    if !result.conventions.is_empty() {
        knowledge.append(KnowledgeFile::Conventions, &result.conventions)?;
    }
    if !result.concerns.is_empty() {
        knowledge.append(KnowledgeFile::Concerns, &result.concerns)?;
    }
    Ok(())
}

/// Print a summary of knowledge file line counts after the session completes.
fn print_summary(knowledge: &KnowledgeDir) -> Result<()> {
    println!("\n{} Knowledge bootstrap complete!", "✓".green().bold());
    println!();

    if let Ok(files) = knowledge.list_files() {
        for (file_type, path) in files {
            let line_count = std::fs::read_to_string(&path)
                .ok()
                .map(|c| c.lines().count())
                .unwrap_or(0);
            println!(
                "  {} {} ({} lines)",
                "─".dimmed(),
                file_type.filename().cyan(),
                line_count
            );
        }
    }

    println!();
    println!("  Run '{}' to view results.", "loom knowledge show".cyan());

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_build_system_prompt_without_existing_knowledge() {
        let prompt = build_system_prompt("");
        assert!(prompt.contains("senior software architect"));
        assert!(prompt.contains("loom knowledge update"));
        assert!(prompt.contains("architecture.md"));
        assert!(!prompt.contains("Existing Knowledge"));
    }

    #[test]
    fn test_build_system_prompt_with_existing_knowledge() {
        let existing = "## Existing Knowledge (DO NOT DUPLICATE)\n\nSome prior knowledge.";
        let prompt = build_system_prompt(existing);
        assert!(prompt.contains("senior software architect"));
        assert!(prompt.contains("Existing Knowledge"));
        assert!(prompt.contains("Some prior knowledge."));
    }

    #[test]
    fn test_build_initial_prompt_contains_agent_instructions() {
        let prompt = build_initial_prompt();
        assert!(prompt.contains("Agent 1"));
        assert!(prompt.contains("Agent 2"));
        assert!(prompt.contains("Agent 3"));
        assert!(prompt.contains("Agent 4"));
        assert!(prompt.contains("architecture.md"));
        assert!(prompt.contains("conventions.md"));
        assert!(prompt.contains("concerns.md"));
    }

    #[test]
    fn test_read_existing_knowledge_empty_dir() {
        let knowledge = KnowledgeDir::new("/nonexistent/path");
        let result = read_existing_knowledge(&knowledge);
        assert!(result.is_empty());
    }

    #[test]
    fn test_read_existing_knowledge_skips_short_files() {
        let temp = tempfile::TempDir::new().unwrap();
        let knowledge = KnowledgeDir::new(temp.path());
        // Create a knowledge dir with only a short file (<=5 lines)
        std::fs::create_dir_all(knowledge.root()).unwrap();
        std::fs::write(
            knowledge.file_path(KnowledgeFile::Architecture),
            "# Architecture\n\n> Short.\n",
        )
        .unwrap();
        let result = read_existing_knowledge(&knowledge);
        assert!(result.is_empty());
    }

    #[test]
    fn test_read_existing_knowledge_includes_populated_files() {
        let temp = tempfile::TempDir::new().unwrap();
        let knowledge = KnowledgeDir::new(temp.path());
        knowledge.initialize().unwrap();
        // Add substantial content that exceeds 5-line threshold
        knowledge
            .append(
                KnowledgeFile::Architecture,
                "## Overview\n\nLine 1\nLine 2\nLine 3\nLine 4\nLine 5\nLine 6",
            )
            .unwrap();
        let result = read_existing_knowledge(&knowledge);
        assert!(result.contains("Existing Knowledge"));
        assert!(result.contains("## Overview"));
    }
}
