//! Knowledge GC command — spawn Claude session to restructure knowledge files.

use anyhow::{Context, Result};
use colored::Colorize;
use std::process::Command;

use crate::claude::find_claude_path;
use crate::fs::knowledge::{
    GcMetrics, KnowledgeDir, KnowledgeLayout, DEFAULT_MAX_TIER1_LINES, DEFAULT_MAX_TOPIC_LINES,
};

/// Execute the knowledge gc command — restructure knowledge files via Claude session.
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

    let is_legacy = knowledge.layout() == KnowledgeLayout::Legacy;

    // Pre-check: bail early if nothing to restructure.
    let metrics = knowledge.analyze_gc_metrics(DEFAULT_MAX_TIER1_LINES, DEFAULT_MAX_TOPIC_LINES)?;
    if !metrics.gc_recommended {
        println!(
            "{} Knowledge files are clean. Nothing to restructure.",
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
    let system_prompt = build_gc_system_prompt(&effective_model, dry_run, is_legacy, &metrics);
    let initial_prompt = build_gc_initial_prompt(&effective_model, dry_run, is_legacy);

    // Sandbox: in dry-run, deny all writes.
    let mut sandbox_guard = super::spawn::KnowledgeSandboxGuard::install(&project_root, !dry_run)?;
    super::spawn::arm_sandbox_restore(&sandbox_guard)?;

    let mode_label = if dry_run { "dry-run" } else { "restructuring" };
    println!(
        "\n{} Spawning Claude session ({})...\n",
        "→".cyan().bold(),
        mode_label
    );
    println!("  {} Model: {}", "→".cyan(), effective_model.cyan());
    if quick {
        // -p is Claude Code's print mode: it emits nothing until the whole turn
        // finishes, so an unannounced multi-minute silence reads as a hang.
        println!(
            "  {} Headless (--quick): no output until the session finishes. \
             Restructuring the knowledge base takes several minutes — drop \
             {} to watch it work interactively.",
            "→".cyan(),
            "--quick".cyan()
        );
    }

    // Bash allowlist EXCLUDES `loom knowledge gc` to prevent recursion. The
    // non-dry-run branch adds `loom knowledge index` so the session can finish
    // with the mandatory re-index step; the dry-run branch stays read-only.
    let bash_allow = if dry_run {
        "Bash(loom knowledge audit*),Bash(loom knowledge show*),Bash(loom knowledge list*)"
    } else {
        "Bash(loom knowledge audit*),\
         Bash(loom knowledge show*),\
         Bash(loom knowledge list*),\
         Bash(loom knowledge update*),\
         Bash(loom knowledge replace-section*),\
         Bash(loom knowledge index*)"
    };

    // Both prompts mandate an agent team, so the team tools must be allowed too —
    // `Agent` alone only covers fire-and-forget subagents.
    let team_allow = "Agent,TeamCreate,SendMessage,TaskCreate,TaskList,TaskUpdate,TaskGet";

    let tool_allow = if dry_run {
        format!("Read,Glob,Grep,{},{}", bash_allow, team_allow)
    } else {
        format!("Read,Glob,Grep,Edit,Write,{},{}", bash_allow, team_allow)
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

    let status_result = cmd.status().context("Failed to spawn Claude session");
    let status = super::spawn::restore_after(&mut sandbox_guard, status_result)?;

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
        let post =
            knowledge.analyze_gc_metrics(DEFAULT_MAX_TIER1_LINES, DEFAULT_MAX_TOPIC_LINES)?;
        println!();
        println!("{}", "Post-restructuring audit:".cyan().bold());
        println!("  Total: {} lines (informational)", post.total_lines);
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
    println!("{}", "Tier 1 targets:".cyan().bold());
    for tier1 in &metrics.tier1 {
        if tier1.has_issues {
            println!(
                "  {} {} ({} lines, {} dups, {} promoted, {} oversized sections)",
                "⚠".yellow(),
                tier1.file_type.filename().cyan(),
                tier1.line_count,
                tier1.duplicate_headers.len(),
                tier1.promoted_block_count,
                tier1.oversized_sections.len(),
            );
        }
    }

    let topic_issues: Vec<_> = metrics.topics.iter().filter(|t| t.has_issues).collect();
    if !topic_issues.is_empty() {
        println!();
        println!("{}", "Tier 2 targets:".cyan().bold());
        for topic in topic_issues {
            let orphan = if topic.is_orphan { " [orphan]" } else { "" };
            println!(
                "  {} {} ({} lines, {} dups){}",
                "⚠".yellow(),
                topic.relative_path().cyan(),
                topic.line_count,
                topic.duplicate_headers.len(),
                orphan,
            );
        }
    }

    println!();
    println!("{}", "Reasons:".cyan().bold());
    for reason in &metrics.reasons {
        println!("  - {}", reason);
    }
}

fn build_gc_system_prompt(
    model: &str,
    dry_run: bool,
    is_legacy: bool,
    metrics: &GcMetrics,
) -> String {
    let tier1_targets: Vec<String> = metrics
        .tier1
        .iter()
        .filter(|m| m.has_issues)
        .map(|m| {
            format!(
                "- doc/loom/knowledge/{} ({} lines, {} duplicate headers, {} promoted blocks, {} oversized sections)",
                m.file_type.filename(),
                m.line_count,
                m.duplicate_headers.len(),
                m.promoted_block_count,
                m.oversized_sections.len(),
            )
        })
        .collect();

    let topic_targets: Vec<String> = metrics
        .topics
        .iter()
        .filter(|m| m.has_issues)
        .map(|m| {
            format!(
                "- doc/loom/knowledge/{} ({} lines, {} duplicate headers{})",
                m.relative_path(),
                m.line_count,
                m.duplicate_headers.len(),
                if m.is_orphan {
                    ", orphaned — nothing links to it"
                } else {
                    ""
                },
            )
        })
        .collect();

    let migration_clause = if is_legacy {
        "## Legacy Migration\n\n\
         This knowledge directory is currently FLAT (legacy layout, no INDEX.md). \
         This GC run performs the migration into the tiered hierarchy: as you extract \
         oversized sections, create the tier-2 topic files under the matching category \
         directory (e.g. `architecture/merge-flow.md`) via `loom knowledge update \
         architecture/merge-flow`, then finish with `loom knowledge index` to generate \
         INDEX.md for the first time.\n\n"
    } else {
        ""
    };

    let mode_clause = if dry_run {
        "## Mode: DRY-RUN\n\n\
         You are in DRY-RUN mode. You MUST NOT write or edit any files. \
         Instead, produce a clear textual diff/proposal showing exactly what you would \
         restructure in each file, then stop. Sandbox enforces this — write attempts will be denied."
    } else {
        "## Mode: RESTRUCTURE\n\n\
         Edit knowledge files directly via Edit/Write. After all changes, run \
         `loom knowledge index` to regenerate INDEX.md, then `loom knowledge audit` to \
         verify the metrics improved."
    };

    let tier1_str = if tier1_targets.is_empty() {
        "(no tier-1 files flagged)".to_string()
    } else {
        tier1_targets.join("\n")
    };
    let topic_str = if topic_targets.is_empty() {
        String::new()
    } else {
        format!("\n\n## Tier-2 Targets\n\n{}", topic_targets.join("\n"))
    };

    format!(
        "You are a senior software architect restructuring curated knowledge files into \
         a tiered hierarchy.\n\n\
         ## Your Goal\n\n\
         Restructure the knowledge files at doc/loom/knowledge/ by:\n\
         1. Extracting oversized tier-1 sections into tier-2 topic files \
         (`<category>/<slug>.md`), replacing each with a 2-4 line summary plus a \
         relative markdown link to the new topic\n\
         2. Merging duplicate headers into single consolidated sections\n\
         3. Repairing broken links and adopting orphan topics by linking them from the \
         right tier-1 file\n\
         4. Deleting only genuinely stale content — never a recorded lesson\n\
         5. Finishing with `loom knowledge index` to keep INDEX.md current\n\n\
         ## Hard Rules\n\n\
         - NEVER delete a lesson to hit a line count — EXTRACT it into a tier-2 topic \
         instead. Recorded mistakes, gotchas, and prevention rules are the highest-value \
         content in the knowledge base; condense wording, never drop the lesson.\n\
         - DO NOT invent new content. Only restructure, dedupe, and remove genuinely stale material.\n\
         - File paths with line numbers are precious context — preserve them.\n\
         - There is no total-lines budget. `Total: N lines` in the audit is informational \
         only — never restructure just to reduce it.\n\
         - Use `loom knowledge audit` to verify your work; do NOT run `loom knowledge gc` (recursion).\n\
         - The mandatory final step is `loom knowledge index`.\n\n\
         {migration_clause}\
         ## Tier-1 Targets\n\n\
         {tier1_str}{topic_str}\n\n\
         {mode_clause}\n\n\
         ## Strategy\n\n\
         CLAUDE_CODE_EXPERIMENTAL_AGENT_TEAMS=1 is set — use an agent team for this \
         work, not fire-and-forget subagents. Restructuring needs coordination: teammates \
         must surface content that belongs in a different tier-1 file or a shared topic, \
         and you must reconcile overlapping edits. Create a team with one teammate per \
         knowledge file that needs work (use model \"{model}\" for every teammate), assign \
         each teammate its file, and have them report cross-file moves back to you. YOU \
         are the team lead: you own the final cross-file synthesis pass, the final \
         `loom knowledge index` run, and you shut down ALL teammates before finishing.\n\n\
         When spawning teammates or Agent subagents, ALWAYS use model: \"{model}\".\n\n\
         ## Knowledge Files\n\n\
         The knowledge files are at doc/loom/knowledge/ — Read them directly. \
         Their contents are intentionally NOT embedded here.\n",
    )
}

fn build_gc_initial_prompt(model: &str, dry_run: bool, is_legacy: bool) -> String {
    let action = if dry_run {
        "Produce a textual restructuring proposal for each file. Do NOT write."
    } else {
        "Restructure the files via Edit/Write, extracting oversized sections into tier-2 \
         topics. Then run `loom knowledge index` and report the new metrics."
    };
    let migration_note = if is_legacy {
        " This directory is still flat (legacy) — this run migrates it into the tiered \
         hierarchy as you extract topics."
    } else {
        ""
    };
    format!(
        "Restructure the knowledge files at doc/loom/knowledge/.{migration_note} \
         Create an agent team (model \"{model}\" for every teammate) — one teammate per \
         file that needs work — to extract oversized sections into tier-2 topics, dedupe \
         headers, repair broken links, and remove genuinely stale content. NEVER delete a \
         recorded mistake, gotcha, or prevention rule to hit a line count — extract it \
         instead. As team lead, do the final cross-file synthesis pass, run `loom \
         knowledge index`, and shut down all teammates. {action}",
    )
}

#[cfg(test)]
#[path = "tests_gc.rs"]
mod tests;
