# PLAN: Repurpose `loom knowledge gc` as Real Compaction; Rename Analyzer to `audit`

## Context

`loom knowledge gc` today only *analyzes* knowledge files (line counts, duplicate headers, promoted-memory blocks) and prints manual compaction instructions — the name `gc` is misleading because it never collects garbage.

This change:

1. **Renames** the current analyzer to `loom knowledge audit` (reports issues, takes no action).
2. **Repurposes** `loom knowledge gc` to spawn a Claude session that *actually* compacts knowledge files — mirroring the existing `loom knowledge bootstrap` spawn pattern.
3. **Adds** a `--dry-run` flag for safety, enforced via sandbox (no write permission in dry-run).
4. **Bails early** if `analyze_gc_metrics` reports nothing needs compaction (saves a Claude session).
5. **Updates** all references in CLI, dispatch, completions, README, knowledge docs.

This is unreleased software — **no backwards-compat alias**. Hard rename.

---

## Design Decisions (already settled)

- Renamed command: **`audit`**
- Pre-check: **bail early if clean**
- Safety: **`--dry-run` flag** (sandbox-enforced)
- Recursion guard: spawned Claude's bash allowlist excludes `loom knowledge gc`

---

## Affected Files Summary

| File                                                | Change                                                                                                              |
| --------------------------------------------------- | ------------------------------------------------------------------------------------------------------------------- |
| `loom/src/commands/knowledge/gc.rs`                 | **Rename → `audit.rs`**, rename fn `gc()` → `audit()`. No behavior change. Tests renamed too.                       |
| `loom/src/commands/knowledge/gc.rs` (new file)      | **New** compaction impl that spawns Claude. Full code below.                                                        |
| `loom/src/commands/knowledge/spawn.rs` (new file)   | **New** shared spawn helpers extracted from `bootstrap.rs`. Full code below.                                        |
| `loom/src/commands/knowledge/bootstrap.rs`          | Remove the 4 helpers being extracted; import them from `super::spawn` instead.                                      |
| `loom/src/commands/knowledge/mod.rs`                | Add `pub mod audit;` and `pub mod spawn;`. Keep `pub mod gc;` (now points to new impl).                             |
| `loom/src/cli/types_memory.rs`                      | Rename variant `Gc` → `Audit`; add new `Gc` variant with `model`, `dry_run`, `quick`. Full diff below.              |
| `loom/src/cli/dispatch.rs:162-166`                  | Update match arm. Full diff below.                                                                                  |
| `loom/src/completions/dynamic/commands.rs:59,113`   | Update subcommand list + flag arms. Full diff below.                                                                |
| `README.md:157`                                     | Replace one line with two. Full diff below.                                                                         |
| `doc/loom/knowledge/concerns.md:62`                 | Update subcommand list referenced in text. Full diff below.                                                         |

Unchanged (these are the underlying utilities — both `audit` and `gc` reuse them):

- `loom/src/fs/knowledge/gc.rs` — `analyze_gc_metrics()`, `FileGcMetrics`, `GcMetrics`, `DEFAULT_MAX_FILE_LINES` (200), `DEFAULT_MAX_TOTAL_LINES` (800), `DEFAULT_MAX_PROMOTED_BLOCKS` (3).
- `loom/src/fs/knowledge/dir.rs:369` — `KnowledgeDir::analyze_gc_metrics()` wrapper.
- `loom/src/commands/knowledge/check.rs:92-104` — keeps calling `analyze_gc_metrics()` for its "GC Analysis" section.
- `loom/src/claude.rs:10-33` — `find_claude_path()` helper (used by both bootstrap and new gc).

---

## Step 1 — Create `loom/src/commands/knowledge/spawn.rs`

Extract from `bootstrap.rs`. Functions are functionally unchanged but become `pub(super)` so both `bootstrap` and `gc` can use them. The `write_knowledge_sandbox` function gains a `dry_run: bool` parameter so `gc --dry-run` can deny writes.

```rust
//! Shared helpers for knowledge commands that spawn Claude sessions.

use anyhow::{Context, Result};
use std::path::{Path, PathBuf};
use std::process::Command;

use crate::fs::knowledge::KnowledgeDir;
use crate::fs::work_dir::WorkDir;

/// Resolve the project root directory.
///
/// Tries WorkDir first (works when .work/ exists), then falls back to
/// `git rev-parse --show-toplevel`, then current directory.
pub(super) fn resolve_project_root() -> Result<PathBuf> {
    if let Ok(work_dir) = WorkDir::new(".") {
        if let Some(root) = work_dir.project_root().map(|p| p.to_path_buf()) {
            return Ok(root);
        }
    }

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

    std::env::current_dir().context("Failed to get current directory")
}

/// Read existing knowledge files and format them for context embedding.
///
/// Files that only contain the default template (≤5 lines) are skipped.
pub(super) fn read_existing_knowledge(knowledge: &KnowledgeDir) -> String {
    if !knowledge.exists() {
        return String::new();
    }

    let mut sections = Vec::new();
    if let Ok(files) = knowledge.read_all() {
        for (file_type, content) in files {
            let trimmed = content.trim().to_string();
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

/// Write sandbox settings for a knowledge-scoped Claude session.
///
/// When `allow_writes` is false (dry-run), Write is denied entirely.
/// Returns original `.claude/settings.local.json` content for restoration.
pub(super) fn write_knowledge_sandbox(
    project_root: &Path,
    allow_writes: bool,
) -> Result<Option<String>> {
    let claude_dir = project_root.join(".claude");
    std::fs::create_dir_all(&claude_dir).context("Failed to create .claude directory")?;

    let settings_path = claude_dir.join("settings.local.json");

    let backup = if settings_path.exists() {
        Some(
            std::fs::read_to_string(&settings_path)
                .context("Failed to read existing settings.local.json")?,
        )
    } else {
        None
    };

    let allow = if allow_writes {
        serde_json::json!([
            "Write(doc/loom/knowledge/**)",
            "Edit(doc/loom/knowledge/**)",
            "Bash(loom *)"
        ])
    } else {
        // Dry-run: no write/edit permission anywhere.
        serde_json::json!(["Bash(loom *)"])
    };

    let settings = serde_json::json!({
        "sandbox": {
            "enabled": true,
            "autoAllowBashIfSandboxed": true,
            "excludedCommands": ["loom"]
        },
        "permissions": {
            "allow": allow,
            "deny": [
                "Read(~/.ssh/**)",
                "Read(~/.aws/**)",
                "Read(~/.config/gcloud/**)",
                "Read(~/.gnupg/**)",
                "Write(**)"
            ]
        }
    });

    let content =
        serde_json::to_string_pretty(&settings).context("Failed to serialize sandbox settings")?;
    std::fs::write(&settings_path, content).context("Failed to write sandbox settings")?;

    Ok(backup)
}

/// Restore original settings after a knowledge session completes.
pub(super) fn restore_sandbox_settings(
    project_root: &Path,
    backup: Option<String>,
) -> Result<()> {
    let settings_path = project_root.join(".claude").join("settings.local.json");
    match backup {
        Some(original) => {
            std::fs::write(&settings_path, original)
                .context("Failed to restore original settings.local.json")?;
        }
        None => {
            let _ = std::fs::remove_file(&settings_path);
        }
    }
    Ok(())
}
```

### Modify `loom/src/commands/knowledge/bootstrap.rs`

Delete the four functions (`resolve_project_root`, `read_existing_knowledge`, `write_bootstrap_sandbox`, `restore_sandbox_settings`) from `bootstrap.rs` (currently lines 107-131, 137-167, 240-282, 285-300). Replace call sites:

- Line 15: `let project_root = resolve_project_root()?;` → `let project_root = super::spawn::resolve_project_root()?;`
- Line 37: `let existing_knowledge = read_existing_knowledge(&knowledge);` → `let existing_knowledge = super::spawn::read_existing_knowledge(&knowledge);`
- Line 47: `let settings_backup = write_bootstrap_sandbox(&project_root)?;` → `let settings_backup = super::spawn::write_knowledge_sandbox(&project_root, true)?;`
- Line 81: `restore_sandbox_settings(&project_root, settings_backup)?;` → `super::spawn::restore_sandbox_settings(&project_root, settings_backup)?;`

The bootstrap tests at lines 332-401 use `build_system_prompt`, `build_initial_prompt`, and `read_existing_knowledge` — the first two stay in bootstrap.rs; the last needs to be updated to `super::spawn::read_existing_knowledge` (or keep a test-only re-export). Simpler: in the test for `test_read_existing_knowledge_*`, call `super::spawn::read_existing_knowledge` directly.

Remove from bootstrap.rs imports:

```rust
use crate::fs::work_dir::WorkDir;  // no longer needed in bootstrap.rs
```

(But `KnowledgeDir` and `KnowledgeFile` imports stay — bootstrap still uses them.)

---

## Step 2 — Rename `gc.rs` → `audit.rs`

```bash
git mv loom/src/commands/knowledge/gc.rs loom/src/commands/knowledge/audit.rs
```

Then in the renamed `audit.rs`:

- Line 8: `pub fn gc(...)` → `pub fn audit(...)`.
- Line 52: `println!("GC recommended: {}"` → `println!("Audit result: {}"`.
- Line 59: `println!("{}", "Compaction Instructions:"...)` → keep the heading "Compaction Instructions:" (it's still accurate advice — users now run `loom knowledge gc` to actually do it). Add one line at the end of the instructions:
  ```text
  println!("  Or: run '{}' to compact automatically.", "loom knowledge gc".cyan());
  ```
- Line 26: `println!("{}", "Knowledge GC Analysis".bold());` → `println!("{}", "Knowledge Audit".bold());`.
- Tests: rename `test_gc_clean` → `test_audit_clean`, `test_gc_large_file` → `test_audit_large_file`. Update internal `gc(200, 800, true)` calls to `audit(200, 800, true)`.

Note: do NOT rename `analyze_gc_metrics`, `gc_recommended` field, `DEFAULT_MAX_FILE_LINES` etc. in `loom/src/fs/knowledge/gc.rs` — those are internal terminology and renaming them creates ripple churn (check.rs:93,104 etc.). The user-facing CLI uses `audit`; the internal metric API keeps its name.

---

## Step 3 — Create new `loom/src/commands/knowledge/gc.rs`

```rust
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
    let effective_model = model.unwrap_or_else(|| "sonnet".to_string());
    let existing = super::spawn::read_existing_knowledge(&knowledge);

    let system_prompt = build_gc_system_prompt(&existing, &effective_model, dry_run, &metrics);
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
        println!(
            "  Review with: {}",
            "git diff doc/loom/knowledge/".cyan()
        );
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

fn build_gc_system_prompt(
    existing: &str,
    model: &str,
    dry_run: bool,
    metrics: &GcMetrics,
) -> String {
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
         - Use `loom knowledge audit` to verify your work; do NOT run `loom knowledge gc` (recursion).\n\n\
         ## Targets (these files need work)\n\n\
         {targets}\n\n\
         {mode_clause}\n\n\
         ## Strategy\n\n\
         Use parallel Agent calls (with model: \"{model}\") to compact files independently \
         since each knowledge file is a separate concern. After agents finish, do a final \
         cross-file pass to check for content that should move between files (e.g., a \
         pattern in architecture.md that belongs in patterns.md).\n\n\
         When spawning Agent subagents, ALWAYS set model: \"{model}\".\n\
         {existing_block}",
        targets = if targets.is_empty() {
            "(no specific targets — full review)".to_string()
        } else {
            targets.join("\n")
        },
        mode_clause = mode_clause,
        model = model,
        existing_block = if existing.is_empty() {
            String::new()
        } else {
            format!("\n{existing}\n")
        },
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
         Spawn parallel agents (set model: \"{model}\" on each) — one per file that needs work \
         — to dedupe headers, summarize promoted blocks, and remove stale content. \
         {action}",
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::fs::knowledge::FileGcMetrics;
    use loom::fs::knowledge::KnowledgeFile;
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
        let prompt = build_gc_system_prompt("", "sonnet", true, &metrics);
        assert!(prompt.contains("DRY-RUN"));
        assert!(prompt.contains("MUST NOT write"));
        assert!(!prompt.contains("Mode: COMPACT"));
    }

    #[test]
    fn test_gc_system_prompt_compact_mode() {
        let metrics = fake_metrics_recommended();
        let prompt = build_gc_system_prompt("", "sonnet", false, &metrics);
        assert!(prompt.contains("Mode: COMPACT"));
        assert!(prompt.contains("Edit knowledge files directly"));
        assert!(!prompt.contains("DRY-RUN"));
    }

    #[test]
    fn test_gc_system_prompt_includes_targets() {
        let metrics = fake_metrics_recommended();
        let prompt = build_gc_system_prompt("", "sonnet", false, &metrics);
        assert!(prompt.contains("architecture.md"));
        assert!(prompt.contains("500 lines"));
    }

    #[test]
    fn test_gc_system_prompt_recursion_warning() {
        let metrics = fake_metrics_recommended();
        let prompt = build_gc_system_prompt("", "sonnet", false, &metrics);
        assert!(prompt.contains("do NOT run `loom knowledge gc`"));
    }

    #[test]
    fn test_gc_initial_prompt_embeds_model() {
        let prompt = build_gc_initial_prompt("opus", false);
        assert!(prompt.contains("model: \"opus\""));
        assert!(prompt.contains("Compact the files via Edit/Write"));
    }

    #[test]
    fn test_gc_initial_prompt_dry_run() {
        let prompt = build_gc_initial_prompt("sonnet", true);
        assert!(prompt.contains("Do NOT write"));
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
```

---

## Step 4 — Update `loom/src/commands/knowledge/mod.rs`

Change top of file:

```rust
//! Knowledge command - manage curated codebase knowledge.
pub mod audit;
pub mod bootstrap;
pub mod check;
pub mod gc;
pub mod spawn;
```

---

## Step 5 — Update `loom/src/cli/types_memory.rs` (lines 60-73)

Replace:

```rust
    /// Analyze knowledge files for size, duplicates, and curated blocks
    Gc {
        /// Max lines per file before GC is recommended
        #[arg(long, default_value_t = DEFAULT_MAX_FILE_LINES)]
        max_file_lines: usize,

        /// Max total lines before GC is recommended
        #[arg(long, default_value_t = DEFAULT_MAX_TOTAL_LINES)]
        max_total_lines: usize,

        /// Only show metrics, skip compaction instructions
        #[arg(short, long)]
        quiet: bool,
    },
```

With:

```rust
    /// Analyze knowledge files for size, duplicates, and curated blocks
    Audit {
        /// Max lines per file before compaction is recommended
        #[arg(long, default_value_t = DEFAULT_MAX_FILE_LINES)]
        max_file_lines: usize,

        /// Max total lines before compaction is recommended
        #[arg(long, default_value_t = DEFAULT_MAX_TOTAL_LINES)]
        max_total_lines: usize,

        /// Only show metrics, skip compaction instructions
        #[arg(short, long)]
        quiet: bool,
    },

    /// Spawn Claude session to compact knowledge files (dedupe, summarize, drop stale)
    Gc {
        /// Model to use for the Claude session (e.g., "sonnet", "opus")
        #[arg(long)]
        model: Option<String>,

        /// Preview proposed changes without writing
        #[arg(long)]
        dry_run: bool,

        /// Run in non-interactive mode (no terminal UI)
        #[arg(long)]
        quick: bool,
    },
```

---

## Step 6 — Update `loom/src/cli/dispatch.rs` (lines 162-166)

Replace:

```rust
            KnowledgeCommands::Gc {
                max_file_lines,
                max_total_lines,
                quiet,
            } => knowledge::gc::gc(max_file_lines, max_total_lines, quiet),
```

With:

```rust
            KnowledgeCommands::Audit {
                max_file_lines,
                max_total_lines,
                quiet,
            } => knowledge::audit::audit(max_file_lines, max_total_lines, quiet),
            KnowledgeCommands::Gc {
                model,
                dry_run,
                quick,
            } => knowledge::gc::gc(model, dry_run, quick),
```

---

## Step 7 — Update `loom/src/completions/dynamic/commands.rs`

Line 59 — replace:

```rust
"knowledge" => &["bootstrap", "check", "gc", "init", "list", "show", "update"],
```

With:

```rust
"knowledge" => &["audit", "bootstrap", "check", "gc", "init", "list", "show", "update"],
```

Line 113 — replace:

```rust
["knowledge", "gc"] => &["--max-file-lines", "--max-total-lines", "--quiet"],
```

With:

```rust
["knowledge", "audit"] => &["--max-file-lines", "--max-total-lines", "--quiet"],
["knowledge", "gc"] => &["--dry-run", "--model", "--quick"],
```

---

## Step 8 — Update `README.md:157`

Replace:

```text
loom knowledge gc [--max-file-lines N] [--max-total-lines N] [--quiet]
```

With:

```text
loom knowledge audit [--max-file-lines N] [--max-total-lines N] [--quiet]   # Report size/duplicate/promoted-block issues
loom knowledge gc [--model NAME] [--dry-run] [--quick]                       # Spawn Claude to compact (dedupe, summarize, drop stale)
```

If surrounding lines on `README.md` describe the `gc` command in prose, update them too (read README.md lines 145-175 in context and adjust the description to match the new split).

---

## Step 9 — Update `doc/loom/knowledge/concerns.md:62`

Current line:

```text
`bootstrap.rs:57` uses `Bash(loom knowledge*)` which allows all knowledge subcommands (init, check, gc, show) not just `update`. Harmless since other subcommands are read-only, but could be tightened to `Bash(loom knowledge update*)` for principle of least privilege.
```

Replace with:

```text
`bootstrap.rs:57` uses `Bash(loom knowledge*)` which allows all knowledge subcommands (init, check, audit, show, list, gc) not just `update`. Most are read-only, but `gc` spawns Claude — so allowing it from inside another Claude session could cause recursion. The new `knowledge/gc.rs` and `knowledge/spawn.rs` already exclude `loom knowledge gc` from their own bash allowlist; `bootstrap`'s allowlist should be tightened to `Bash(loom knowledge update*)`, `Bash(loom knowledge replace-section*)`, `Bash(loom knowledge audit*)` for principle of least privilege.
```

---

## Step 10 — Verification

```bash
cd loom
cargo fmt
cargo fmt --check
cargo clippy -- -D warnings
cargo test                                       # all tests pass
cargo test commands::knowledge::audit            # renamed tests
cargo test commands::knowledge::gc               # new prompt + early-bail tests
cargo test commands::knowledge::bootstrap        # bootstrap still passes after extraction
cargo test fs::knowledge::gc                     # underlying analyzer untouched
```

After installing the new binary (`./dev-install.sh` per memory):

```bash
# CLI surface checks
loom knowledge --help                            # both `audit` and `gc` listed
loom knowledge audit --help                      # shows --max-file-lines, --max-total-lines, --quiet
loom knowledge gc --help                         # shows --model, --dry-run, --quick

# Behavior checks (in a project with clean knowledge):
loom knowledge audit                             # same output format as old `gc`
loom knowledge gc                                # prints "Nothing to compact" and exits without spawning Claude
loom knowledge gc --dry-run                      # also exits early (no targets)

# Bloated-knowledge end-to-end check:
# 1. Append 300 lines of repeated content to doc/loom/knowledge/architecture.md
# 2. loom knowledge audit                        → gc_recommended: YES, lists architecture.md
# 3. loom knowledge gc --dry-run                 → spawns Claude, prints proposal, no file changes (git diff is empty)
# 4. loom knowledge gc                           → spawns Claude, rewrites files
# 5. git diff doc/loom/knowledge/                → shows compaction
# 6. loom knowledge audit                        → fewer reasons (ideally gc_recommended: NO)

# Negative checks:
rg "knowledge gc" README.md doc/loom/knowledge/  # no stale references to old gc semantics
rg "knowledge::gc::gc.*max_file_lines"           # confirm no leftover old signature in code
```

---

## Notes & Risks

- **Recursion guard:** the bash allowlist for the spawned session in `gc.rs` excludes `loom knowledge gc` explicitly (`Bash(loom knowledge audit*)`, etc., never `Bash(loom knowledge gc*)`).
- **Sandbox `dry_run` enforcement:** if the prompt fails to deter writes, the sandbox layer denies them. Both prompt + sandbox give belt-and-suspenders.
- **`bootstrap.rs` test impact:** removing helpers will break `test_read_existing_knowledge_*` tests unless they're updated to call `super::spawn::read_existing_knowledge`. Plan to update the call sites in the tests during step 1.
- **No e2e CI test for spawn:** spawn behavior is verified manually. CI covers prompt builders and pre-check branch.
- **Module file `loom/src/fs/knowledge/gc.rs` keeps its name:** renaming would ripple through `check.rs`, `dir.rs`, `mod.rs` exports, and 5 tests for no semantic gain. The user-facing CLI uses `audit`; the internal metric module stays `gc`.
- **`audit` output keeps "Compaction Instructions:" heading** because the advice is still useful — it now ends with a pointer to `loom knowledge gc` for users who want it done automatically.
