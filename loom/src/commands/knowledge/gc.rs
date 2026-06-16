//! Knowledge GC command — spawn Claude session to compact knowledge files.

use anyhow::{Context, Result};
use colored::Colorize;
use std::process::Command;

use crate::claude::find_claude_path;
use crate::fs::knowledge::{
    GcMetrics, KnowledgeDir, DEFAULT_MAX_FILE_LINES, DEFAULT_MAX_TOTAL_LINES,
};

/// Execute the knowledge gc command — compact knowledge files via Claude session.
pub fn gc(model: Option<String>, dry_run: bool, quick: bool) -> Result<()> {
    let project_root = super::spawn::resolve_project_root()?;
    let knowledge = KnowledgeDir::new(&project_root);

    if !knowledge.exists() {
        println!(
            "{} Knowledge directory not found. Run 'loom knowledge init' to create it.",
            "─".dimmed()
        );
        return Ok(());
    }

    // Pre-check: bail early if nothing to compact.
    let metrics = knowledge.analyze_gc_metrics(DEFAULT_MAX_FILE_LINES, DEFAULT_MAX_TOTAL_LINES)?;
    if !metrics.gc_recommended {
        println!(
            "{} Knowledge files are clean. Nothing to compact.",
            "✓".green().bold()
        );
        println!(
            "  (Run '{}' to see metrics.)",
            "loom knowledge audit".cyan()
        );
        return Ok(());
    }

    print_compaction_targets(&metrics);

    let claude_path = find_claude_path()?;
    // GC is judgement-heavy: deciding what is stale vs. precious requires
    // architectural taste, so default to Opus with the 1M context window.
    let effective_model = model.unwrap_or_else(|| "opus".to_string());

    // NOTE: knowledge file contents are deliberately NOT embedded in the prompt.
    // The session Reads and Edits those files directly — embedding them would be
    // redundant and, at scale, blows past Linux's 128 KiB per-argv-entry limit
    // (MAX_ARG_STRLEN), failing with "Argument list too long".
    let system_prompt = build_gc_system_prompt(&effective_model, dry_run, &metrics);
    let initial_prompt = build_gc_initial_prompt(&effective_model, dry_run);

    // Sandbox: in dry-run, deny all writes.
    let settings_backup = super::spawn::write_knowledge_sandbox(&project_root, !dry_run)?;

    let mode_label = if dry_run { "dry-run" } else { "compaction" };
    println!(
        "\n{} Spawning Claude session ({})...\n",
        "→".cyan().bold(),
        mode_label
    );
    println!("  {} Model: {}", "→".cyan(), effective_model.cyan());

    // Bash allowlist EXCLUDES `loom knowledge gc` to prevent recursion.
    // In dry-run, also exclude update/replace-section to belt-and-suspenders the read-only mode.
    let bash_allow = if dry_run {
        "Bash(loom knowledge audit*),Bash(loom knowledge show*),Bash(loom knowledge list*)"
    } else {
        "Bash(loom knowledge audit*),\
         Bash(loom knowledge show*),\
         Bash(loom knowledge list*),\
         Bash(loom knowledge update*),\
         Bash(loom knowledge replace-section*)"
    };

    let tool_allow = if dry_run {
        format!("Read,Glob,Grep,{},Agent", bash_allow)
    } else {
        format!("Read,Glob,Grep,Edit,Write,{},Agent", bash_allow)
    };

    let mut cmd = Command::new(&claude_path);
    cmd.arg("--permission-mode").arg("auto");
    cmd.arg("--allowedTools").arg(&tool_allow);
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

    super::spawn::restore_sandbox_settings(&project_root, settings_backup)?;

    if !status.success() {
        let code = status.code().unwrap_or(-1);
        if code == 130 || code == 2 {
            println!("\n{} Session interrupted by user.", "─".dimmed());
        } else {
            println!(
                "\n{} Claude session exited with code {}",
                "!".yellow().bold(),
                code
            );
        }
    }

    if !dry_run {
        // Print post-compaction audit so user sees the result.
        let post = knowledge.analyze_gc_metrics(DEFAULT_MAX_FILE_LINES, DEFAULT_MAX_TOTAL_LINES)?;
        println!();
        println!("{}", "Post-compaction audit:".cyan().bold());
        println!("  Total: {} lines", post.total_lines);
        if post.gc_recommended {
            println!("  {} Still recommends GC:", "⚠".yellow());
            for reason in &post.reasons {
                println!("    - {}", reason);
            }
        } else {
            println!("  {} Knowledge files are clean.", "✓".green());
        }
        println!();
        println!("  Review with: {}", "git diff doc/loom/knowledge/".cyan());
    }

    Ok(())
}

fn print_compaction_targets(metrics: &GcMetrics) {
    println!("{}", "Knowledge GC".bold());
    println!();
    println!("{}", "Targets:".cyan().bold());
    for file_metric in &metrics.per_file {
        if file_metric.has_issues {
            println!(
                "  {} {} ({} lines, {} dups, {} promoted)",
                "⚠".yellow(),
                file_metric.file_type.filename().cyan(),
                file_metric.line_count,
                file_metric.duplicate_headers.len(),
                file_metric.promoted_block_count,
            );
        }
    }
    println!();
    println!("{}", "Reasons:".cyan().bold());
    for reason in &metrics.reasons {
        println!("  - {}", reason);
    }
}

fn build_gc_system_prompt(model: &str, dry_run: bool, metrics: &GcMetrics) -> String {
    let targets: Vec<String> = metrics
        .per_file
        .iter()
        .filter(|m| m.has_issues)
        .map(|m| {
            format!(
                "- doc/loom/knowledge/{} ({} lines, {} duplicate headers, {} promoted blocks)",
                m.file_type.filename(),
                m.line_count,
                m.duplicate_headers.len(),
                m.promoted_block_count,
            )
        })
        .collect();

    let mode_clause = if dry_run {
        "## Mode: DRY-RUN\n\n\
         You are in DRY-RUN mode. You MUST NOT write or edit any files. \
         Instead, produce a clear textual diff/proposal showing exactly what you would change \
         in each file, then stop. Sandbox enforces this — write attempts will be denied."
    } else {
        "## Mode: COMPACT\n\n\
         Edit knowledge files directly via Edit/Write. After all changes, run \
         `loom knowledge audit` to verify the metrics improved."
    };

    let targets_str = if targets.is_empty() {
        "(no specific targets — full review)".to_string()
    } else {
        targets.join("\n")
    };

    format!(
        "You are a senior software architect compacting curated knowledge files.\n\n\
         ## Your Goal\n\n\
         Compact the knowledge files at doc/loom/knowledge/ by:\n\
         1. Merging duplicate headers into single consolidated sections\n\
         2. Summarizing curated/promoted memory blocks into concise knowledge\n\
         3. Removing content that is no longer accurate or has been superseded\n\
         4. Reducing total size while preserving every meaningful insight\n\n\
         ## Hard Rules\n\n\
         - DO NOT delete a section unless you are confident the information is stale, \
         duplicated elsewhere, or no longer accurate. When unsure: KEEP IT.\n\
         - DO NOT change the file structure — top-level headers (## Architecture, etc.) stay.\n\
         - DO NOT invent new content. Only condense, dedupe, and remove stale.\n\
         - File paths with line numbers are precious context — preserve them.\n\
         - DO NOT remove information that helps loom-spawned agents self-improve: \
         recorded mistakes, gotchas, prevention rules, root-cause analyses, and \
         hard-won lessons. This is the highest-value content in the knowledge base — \
         condense its wording if verbose, but never drop the lesson itself.\n\
         - Use `loom knowledge audit` to verify your work; do NOT run `loom knowledge gc` (recursion).\n\n\
         ## Targets (these files need work)\n\n\
         {targets_str}\n\n\
         {mode_clause}\n\n\
         ## Strategy\n\n\
         CLAUDE_CODE_EXPERIMENTAL_AGENT_TEAMS=1 is set — use an agent team for this \
         work, not fire-and-forget subagents. Compaction needs coordination: teammates \
         must surface content that belongs in a different file, and you must reconcile \
         overlapping edits. Create a team with one teammate per knowledge file that \
         needs work (use model \"{model}\" for every teammate), assign each teammate its \
         file, and have them report cross-file moves back to you. YOU are the team lead: \
         you own the final cross-file synthesis pass and you shut down ALL teammates \
         before finishing. After teammates finish, do a final cross-file pass to check \
         for content that should move between files (e.g., a pattern in architecture.md \
         that belongs in patterns.md).\n\n\
         When spawning teammates or Agent subagents, ALWAYS use model: \"{model}\".\n\n\
         ## Knowledge Files\n\n\
         The knowledge files are at doc/loom/knowledge/ — Read them directly. \
         Their contents are intentionally NOT embedded here.\n",
    )
}

fn build_gc_initial_prompt(model: &str, dry_run: bool) -> String {
    let action = if dry_run {
        "Produce a textual diff proposal for each file. Do NOT write."
    } else {
        "Compact the files via Edit/Write. Then run `loom knowledge audit` and report metrics."
    };
    format!(
        "Compact the knowledge files at doc/loom/knowledge/. \
         Create an agent team (model \"{model}\" for every teammate) — one teammate per \
         file that needs work — to dedupe headers, summarize promoted blocks, and remove \
         stale content. Preserve recorded mistakes, gotchas, and prevention rules that \
         help loom-spawned agents self-improve — condense wording, never drop the lesson. \
         As team lead, do the final cross-file synthesis pass and shut down all teammates. \
         {action}",
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::fs::knowledge::{FileGcMetrics, KnowledgeFile};
    use serial_test::serial;
    use tempfile::TempDir;

    fn setup_test_env() -> (TempDir, std::path::PathBuf) {
        let temp_dir = TempDir::new().expect("Failed to create temp dir");
        let test_dir = temp_dir.path().to_path_buf();
        (temp_dir, test_dir)
    }

    fn fake_metrics_recommended() -> GcMetrics {
        GcMetrics {
            total_lines: 1000,
            per_file: vec![FileGcMetrics {
                file_type: KnowledgeFile::Architecture,
                line_count: 500,
                duplicate_headers: vec!["## Overview".to_string()],
                promoted_block_count: 5,
                has_issues: true,
            }],
            gc_recommended: true,
            reasons: vec!["architecture.md exceeds 200 lines (500)".to_string()],
        }
    }

    #[test]
    fn test_gc_system_prompt_dry_run_includes_dry_run_clause() {
        let metrics = fake_metrics_recommended();
        let prompt = build_gc_system_prompt("sonnet", true, &metrics);
        assert!(prompt.contains("DRY-RUN"));
        assert!(prompt.contains("MUST NOT write"));
        assert!(!prompt.contains("Mode: COMPACT"));
    }

    #[test]
    fn test_gc_system_prompt_compact_mode() {
        let metrics = fake_metrics_recommended();
        let prompt = build_gc_system_prompt("sonnet", false, &metrics);
        assert!(prompt.contains("Mode: COMPACT"));
        assert!(prompt.contains("Edit knowledge files directly"));
        assert!(!prompt.contains("DRY-RUN"));
    }

    #[test]
    fn test_gc_system_prompt_includes_targets() {
        let metrics = fake_metrics_recommended();
        let prompt = build_gc_system_prompt("sonnet", false, &metrics);
        assert!(prompt.contains("architecture.md"));
        assert!(prompt.contains("500 lines"));
    }

    #[test]
    fn test_gc_system_prompt_recursion_warning() {
        let metrics = fake_metrics_recommended();
        let prompt = build_gc_system_prompt("sonnet", false, &metrics);
        assert!(prompt.contains("do NOT run `loom knowledge gc`"));
    }

    #[test]
    fn test_gc_system_prompt_does_not_embed_file_contents() {
        // Regression: the gc system prompt must NOT embed knowledge file
        // contents — that overflows Linux's per-argv-entry limit.
        let metrics = fake_metrics_recommended();
        let prompt = build_gc_system_prompt("sonnet", false, &metrics);
        assert!(!prompt.contains("Existing Knowledge"));
        assert!(prompt.contains("Read them directly"));
    }

    #[test]
    fn test_gc_initial_prompt_embeds_model() {
        let prompt = build_gc_initial_prompt("opus", false);
        assert!(prompt.contains("model \"opus\""));
        assert!(prompt.contains("Compact the files via Edit/Write"));
    }

    #[test]
    fn test_gc_initial_prompt_dry_run() {
        let prompt = build_gc_initial_prompt("sonnet", true);
        assert!(prompt.contains("Do NOT write"));
    }

    #[test]
    fn test_gc_initial_prompt_uses_agent_team() {
        let prompt = build_gc_initial_prompt("opus", false);
        assert!(prompt.contains("agent team"));
    }

    #[test]
    fn test_gc_system_prompt_uses_agent_team() {
        let metrics = fake_metrics_recommended();
        let prompt = build_gc_system_prompt("opus", false, &metrics);
        assert!(prompt.contains("agent team"));
        assert!(prompt.contains("CLAUDE_CODE_EXPERIMENTAL_AGENT_TEAMS"));
    }

    #[test]
    fn test_gc_system_prompt_protects_self_improvement_content() {
        // Recorded mistakes / gotchas / prevention rules are the highest-value
        // content — GC must condense but never drop them.
        let metrics = fake_metrics_recommended();
        let prompt = build_gc_system_prompt("opus", false, &metrics);
        assert!(prompt.contains("self-improve"));
        assert!(prompt.contains("prevention rules"));
    }

    #[test]
    #[serial]
    fn test_gc_bails_when_clean() {
        // When knowledge is clean (no GC recommended), gc() must return Ok
        // without attempting to spawn Claude. We can't easily intercept the
        // spawn, so we just ensure the early-return path executes without error
        // on an initialized-but-empty knowledge dir.
        let (_temp_dir, test_dir) = setup_test_env();
        let original_dir = std::env::current_dir().expect("Failed to get current dir");
        std::env::set_current_dir(&test_dir).expect("Failed to change dir");

        crate::commands::knowledge::init().expect("Failed to init knowledge");
        let result = gc(None, true, true);
        assert!(result.is_ok());

        std::env::set_current_dir(original_dir).expect("Failed to restore dir");
    }
}
