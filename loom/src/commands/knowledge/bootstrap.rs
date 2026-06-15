//! Knowledge bootstrap command - spawn Claude session to explore and populate knowledge.

use anyhow::{Context, Result};
use colored::Colorize;
use std::process::Command;

use crate::claude::find_claude_path;
use crate::fs::knowledge::{KnowledgeDir, KnowledgeFile};
use crate::map::{analyze_codebase, AnalysisResult};

/// Execute the knowledge bootstrap command
pub fn execute(model: Option<String>, skip_map: bool, quick: bool) -> Result<()> {
    let project_root = super::spawn::resolve_project_root()?;
    let claude_path = find_claude_path()?;

    crate::utils::print_logo_header("Knowledge Bootstrap");
    println!("{} Starting...", "→".cyan().bold());

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

    // Spawn Claude session
    let effective_model = model.unwrap_or_else(|| "sonnet".to_string());

    // Build prompts (model is embedded so subagents use the same model).
    // NOTE: knowledge file contents are deliberately NOT embedded in the prompt.
    // The session Reads those files directly — embedding them would, at scale,
    // blow past Linux's 128 KiB per-argv-entry limit (MAX_ARG_STRLEN), failing
    // with "Argument list too long".
    let system_prompt = build_system_prompt(&effective_model);
    let initial_prompt = build_initial_prompt(&effective_model);

    // Write sandbox settings to restrict Claude's access
    let settings_backup = super::spawn::write_knowledge_sandbox(&project_root, true)?;

    println!(
        "\n{} Spawning Claude session for knowledge exploration...\n",
        "→".cyan().bold()
    );
    println!("  {} Model: {}", "→".cyan(), effective_model.cyan());

    let mut cmd = Command::new(&claude_path);
    cmd.arg("--permission-mode").arg("auto");
    cmd.arg("--allowedTools")
        .arg("Read,Glob,Grep,Bash(loom knowledge*),Agent");
    cmd.arg("--system-prompt").arg(&system_prompt);
    cmd.arg("--model").arg(&effective_model);

    if quick {
        cmd.arg("-p");
    }

    cmd.arg(&initial_prompt);

    cmd.env("CLAUDE_CODE_EXPERIMENTAL_AGENT_TEAMS", "1");
    cmd.current_dir(&project_root);
    if quick {
        cmd.stdin(std::process::Stdio::null());
    } else {
        cmd.stdin(std::process::Stdio::inherit());
    }
    cmd.stdout(std::process::Stdio::inherit());
    cmd.stderr(std::process::Stdio::inherit());

    let status = cmd.status().context("Failed to spawn Claude session")?;

    // Restore original settings
    super::spawn::restore_sandbox_settings(&project_root, settings_backup)?;

    if !status.success() {
        let code = status.code().unwrap_or(-1);
        if code == 130 || code == 2 {
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

/// Build the system prompt for the Claude session.
fn build_system_prompt(model: &str) -> String {
    format!(
        "You are a senior software architect exploring this codebase to populate knowledge files.\n\n\
         ## Your Goal\n\n\
         Populate the project's knowledge files using `loom knowledge update` commands.\n\n\
         ## Rules\n\n\
         1. Use `loom knowledge update <file> - <<'EOF'\\n<content>\\nEOF` for long content (heredoc syntax)\n\
         2. Use `loom knowledge update <file> \"<content>\"` for short content\n\
         3. Valid files: architecture, entry-points, patterns, conventions, mistakes, stack, concerns\n\
         4. Be specific: include file paths with line numbers (e.g., `src/auth.ts:45-80`)\n\
         5. Focus on PATTERNS and RELATIONSHIPS, not just listing files\n\
         6. Go BEYOND the auto loom map — exhaustively map ALL components and their relationships, not just what `loom map` surfaces automatically\n\
         7. Each knowledge update should add a complete section with a ## heading\n\
         8. When spawning Agent subagents, ALWAYS set model: \"{model}\" so they use the same model\n\n\
         ## Existing Knowledge\n\n\
         The knowledge files already exist at doc/loom/knowledge/ and may contain prior \
         findings. BEFORE writing, Read the file you intend to update so you do NOT \
         duplicate existing content — only add NEW discoveries.\n\n\
         ## Strategy\n\n\
         Use parallel Agent calls (with model: \"{model}\") to explore 4 dimensions simultaneously:\n\
         - Architecture and data flow -> architecture.md (exhaustively map ALL components and their relationships)\n\
         - Patterns and conventions -> patterns.md, conventions.md\n\
         - Stack and entry points -> stack.md, entry-points.md\n\
         - Concerns and tech debt -> concerns.md\n\n\
         After agents complete, do a final synthesis pass on architecture.md to confirm no major area was left unmapped — if gaps exist, spawn additional exploration before completing.\n",
    )
}

/// Build the initial user prompt for the Claude session.
fn build_initial_prompt(model: &str) -> String {
    format!(
        "Explore this codebase and populate the knowledge files. \
         Spawn 4 parallel agents (set model: \"{model}\" on each) to explore different dimensions:\n\n\
         Agent 1 - Architecture: Exhaustively map ALL components and their relationships — entry points, every module, data flow, cross-cutting concerns, blast radius of areas the plan will change. \
         Write findings to architecture.md.\n\n\
         Agent 2 - Patterns & Conventions: Identify error handling patterns, state management, \
         coding conventions, naming schemes. Write to patterns.md and conventions.md.\n\n\
         Agent 3 - Stack & Entry Points: Document dependencies, frameworks, tooling, \
         and key entry point files. Write to stack.md and entry-points.md.\n\n\
         Agent 4 - Concerns: Find technical debt, fixme markers, security concerns, \
         and architectural issues. Write to concerns.md.\n\n\
         After all agents complete, do a final synthesis pass on architecture.md \
         to confirm no major area was left unmapped and cross-cutting concerns are captured.",
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
    fn test_build_system_prompt() {
        let prompt = build_system_prompt("sonnet");
        assert!(prompt.contains("senior software architect"));
        assert!(prompt.contains("loom knowledge update"));
        assert!(prompt.contains("architecture.md"));
        assert!(prompt.contains("model: \"sonnet\""));
    }

    #[test]
    fn test_build_system_prompt_does_not_embed_file_contents() {
        // Regression: the system prompt must NOT embed knowledge file contents —
        // that overflows Linux's per-argv-entry limit (MAX_ARG_STRLEN). It should
        // instead instruct the session to Read the files directly.
        let prompt = build_system_prompt("sonnet");
        assert!(prompt.contains("Read the file"));
        assert!(prompt.contains("doc/loom/knowledge/"));
    }

    #[test]
    fn test_build_initial_prompt_contains_agent_instructions() {
        let prompt = build_initial_prompt("sonnet");
        assert!(prompt.contains("Agent 1"));
        assert!(prompt.contains("Agent 2"));
        assert!(prompt.contains("Agent 3"));
        assert!(prompt.contains("Agent 4"));
        assert!(prompt.contains("architecture.md"));
        assert!(prompt.contains("conventions.md"));
        assert!(prompt.contains("concerns.md"));
        assert!(prompt.contains("model: \"sonnet\""));
    }
}
