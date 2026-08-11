//! Command handler implementations for memory subcommands.

use anyhow::{bail, Context, Result};
use colored::Colorize;
use std::env;

use crate::fs::memory::{
    append_entry, init_memory_dir, list_journals, query_entries, read_journal, validate_content,
    MemoryEntry, MemoryEntryType,
};
use crate::git::worktree::{find_repo_root_from_cwd, find_worktree_root_from_cwd};

use super::formatters::{format_entry_compact, format_entry_full, format_record_success};

/// Sentinel stage ID used by the four recording commands (`note`, `decision`,
/// `change`, `question`) when neither `--stage` nor `LOOM_STAGE_ID` supplies
/// one. Lets ad-hoc/interactive sessions (no active loom plan) still record
/// insights instead of erroring outright.
const AD_HOC_STAGE_ID: &str = "ad-hoc";

/// Get the .work directory, handling worktree symlinks
///
/// When called from within a worktree (or its subdirectory), finds the worktree root
/// which has a `.work` symlink pointing to the main repo's `.work`.
/// When called from the main repo, walks up to find the repo root's `.work`.
fn get_work_dir() -> Result<std::path::PathBuf> {
    let cwd = env::current_dir().context("Failed to get current directory")?;

    // First check if we're in a worktree
    if let Some(worktree_root) = find_worktree_root_from_cwd(&cwd) {
        let work_dir = worktree_root.join(".work");
        if work_dir.exists() {
            return Ok(work_dir);
        }
    }

    // Not in a worktree (or worktree missing .work) - find repo root
    if let Some(repo_root) = find_repo_root_from_cwd(&cwd) {
        let work_dir = repo_root.join(".work");
        if work_dir.exists() {
            return Ok(work_dir);
        }
    }

    // Fallback: check current directory (original behavior)
    let work_dir = cwd.join(".work");
    if work_dir.exists() {
        return Ok(work_dir);
    }

    bail!(".work directory not found. Run 'loom init' first.");
}

/// Get the .work directory for RECORDING commands, creating it if necessary.
///
/// Tries the existing `get_work_dir` search first, unchanged. If nothing is
/// found and cwd is inside a git repo, creates `<repo_root>/.work/memory/`
/// (via `init_memory_dir`) so `note`/`decision`/`change`/`question` still
/// work from an ad-hoc session with no active loom plan. `find_repo_root_from_cwd`
/// already resolves to the main repo root even when cwd is inside a
/// `.worktrees/<stage>` checkout, so this never creates a second `.work`
/// alongside a worktree's `.work` symlink. Read-only commands (`query`,
/// `list`, `show`) must NOT call this — they degrade instead of creating
/// anything.
///
/// Still fails with the original message when cwd is not inside a git repo.
fn get_or_create_work_dir() -> Result<std::path::PathBuf> {
    if let Ok(work_dir) = get_work_dir() {
        return Ok(work_dir);
    }

    let cwd = env::current_dir().context("Failed to get current directory")?;
    // `find_repo_root_from_cwd` falls back to returning `cwd` itself when it
    // walks all the way to the filesystem root without finding a `.git`, so
    // `Some` alone doesn't mean "inside a git repo" - confirm the candidate
    // root actually has a `.git` entry before trusting it.
    let repo_root = find_repo_root_from_cwd(&cwd).filter(|root| root.join(".git").exists());
    let Some(repo_root) = repo_root else {
        bail!(".work directory not found. Run 'loom init' first.");
    };

    let work_dir = repo_root.join(".work");
    init_memory_dir(&work_dir)?;

    eprintln!(
        "{} No .work directory found; recording to {} (stage '{}')",
        "ℹ".blue(),
        work_dir.display(),
        AD_HOC_STAGE_ID
    );

    Ok(work_dir)
}

/// Get the .work directory for READ-ONLY commands (`query`, `list`, `show`).
///
/// Returns `None` instead of erroring when no `.work` exists, so these
/// commands degrade gracefully rather than failing. This matters because
/// `loom memory list` is the first step of the post-compaction recovery flow
/// (see CLAUDE.md Rule 3b) - a hard failure there would derail recovery
/// before it starts. Unlike `get_or_create_work_dir`, this never creates
/// anything.
fn get_work_dir_readonly() -> Option<std::path::PathBuf> {
    get_work_dir().ok()
}

/// Validate stage ID to prevent path traversal attacks
fn validate_stage_id(id: &str) -> Result<()> {
    if id.contains('/') || id.contains("..") || id.contains('\\') {
        bail!("Invalid stage ID: contains path separators");
    }
    Ok(())
}

/// Record a note in the memory journal
pub fn note(text: String, stage_id: Option<String>) -> Result<()> {
    validate_content(&text)?;
    if let Some(ref id) = stage_id {
        validate_stage_id(id)?;
    }

    let work_dir = get_or_create_work_dir()?;
    let stage = stage_id
        .or_else(|| std::env::var("LOOM_STAGE_ID").ok())
        .unwrap_or_else(|| AD_HOC_STAGE_ID.to_string());

    let entry = MemoryEntry::new(MemoryEntryType::Note, text.clone());
    append_entry(&work_dir, &stage, &entry)?;

    println!(
        "{}",
        format_record_success(&MemoryEntryType::Note, &stage, &text)
    );

    Ok(())
}

/// Record a decision in the memory journal
pub fn decision(text: String, context: Option<String>, stage_id: Option<String>) -> Result<()> {
    validate_content(&text)?;
    if let Some(ref ctx) = context {
        validate_content(ctx)?;
    }
    if let Some(ref id) = stage_id {
        validate_stage_id(id)?;
    }

    let work_dir = get_or_create_work_dir()?;
    let stage = stage_id
        .or_else(|| std::env::var("LOOM_STAGE_ID").ok())
        .unwrap_or_else(|| AD_HOC_STAGE_ID.to_string());

    let entry = match context {
        Some(ctx) => MemoryEntry::with_context(MemoryEntryType::Decision, text.clone(), ctx),
        None => MemoryEntry::new(MemoryEntryType::Decision, text.clone()),
    };
    append_entry(&work_dir, &stage, &entry)?;

    println!(
        "{}",
        format_record_success(&MemoryEntryType::Decision, &stage, &text)
    );

    Ok(())
}

/// Record a file change in the memory journal
pub fn change(text: String, stage_id: Option<String>) -> Result<()> {
    validate_content(&text)?;
    if let Some(ref id) = stage_id {
        validate_stage_id(id)?;
    }

    let work_dir = get_or_create_work_dir()?;
    let stage = stage_id
        .or_else(|| std::env::var("LOOM_STAGE_ID").ok())
        .unwrap_or_else(|| AD_HOC_STAGE_ID.to_string());

    let entry = MemoryEntry::new(MemoryEntryType::Change, text.clone());
    append_entry(&work_dir, &stage, &entry)?;

    println!(
        "{}",
        format_record_success(&MemoryEntryType::Change, &stage, &text)
    );

    Ok(())
}

/// Record a question in the memory journal
pub fn question(text: String, stage_id: Option<String>) -> Result<()> {
    validate_content(&text)?;
    if let Some(ref id) = stage_id {
        validate_stage_id(id)?;
    }

    let work_dir = get_or_create_work_dir()?;
    let stage = stage_id
        .or_else(|| std::env::var("LOOM_STAGE_ID").ok())
        .unwrap_or_else(|| AD_HOC_STAGE_ID.to_string());

    let entry = MemoryEntry::new(MemoryEntryType::Question, text.clone());
    append_entry(&work_dir, &stage, &entry)?;

    println!(
        "{}",
        format_record_success(&MemoryEntryType::Question, &stage, &text)
    );

    Ok(())
}

/// Query memory entries by search term
pub fn query(search: String, stage_id: Option<String>) -> Result<()> {
    if let Some(ref id) = stage_id {
        validate_stage_id(id)?;
    }

    let Some(work_dir) = get_work_dir_readonly() else {
        println!(
            "{} No memory recorded yet (no .work directory found)",
            "ℹ".blue()
        );
        return Ok(());
    };

    let stages_to_search: Vec<String> = match stage_id {
        Some(id) => vec![id],
        None => list_journals(&work_dir)?,
    };

    if stages_to_search.is_empty() {
        println!("{} No memory journals found", "ℹ".blue());
        return Ok(());
    }

    let mut total_results = 0;

    for stage in &stages_to_search {
        let journal = read_journal(&work_dir, stage)?;
        let results = query_entries(&journal, &search);

        if results.is_empty() {
            continue;
        }

        let count = results.len();
        println!("\n{} ({})", stage.bold(), count);
        println!("{}", "─".repeat(60));

        for entry in &results {
            println!("{}", format_entry_compact(entry));
        }

        total_results += count;
    }

    if total_results == 0 {
        println!(
            "{} No entries found matching '{}'",
            "ℹ".blue(),
            search.cyan()
        );
    } else {
        println!("\n{} {} total results", "Found".bold(), total_results);
    }

    Ok(())
}

/// Print a single stage's journal entries (compact), applying an optional type filter.
///
/// Returns the number of entries displayed (after filtering). A zero return means
/// the journal had no entries matching the filter and nothing was printed.
fn print_journal_entries(
    work_dir: &std::path::Path,
    stage: &str,
    type_filter: Option<MemoryEntryType>,
    limit: usize,
) -> Result<usize> {
    let journal = read_journal(work_dir, stage)?;

    let entries: Vec<_> = journal
        .entries
        .iter()
        .filter(|e| type_filter.is_none_or(|t| e.entry_type == t))
        .collect();

    if entries.is_empty() {
        return Ok(0);
    }

    println!(
        "\n{} ({} {})",
        stage.bold(),
        entries.len(),
        if entries.len() == 1 {
            "entry"
        } else {
            "entries"
        }
    );
    println!("{}", "─".repeat(60));

    for entry in entries.iter().rev().take(limit) {
        println!("{}", format_entry_compact(entry));
    }

    if entries.len() > limit {
        println!("  {} {} more...", "...".dimmed(), entries.len() - limit);
    }

    Ok(entries.len())
}

/// List memory entries.
///
/// With an explicit `--stage`, lists only that stage's journal. Without one,
/// aggregates every journal in the plan so a running stage sees all memories
/// recorded so far — not just its own. `LOOM_STAGE_ID` no longer scopes `list`;
/// use `--stage` to narrow to a single stage.
pub fn list(stage_id: Option<String>, entry_type: Option<String>) -> Result<()> {
    if let Some(ref id) = stage_id {
        validate_stage_id(id)?;
    }

    let Some(work_dir) = get_work_dir_readonly() else {
        println!(
            "{} No memory recorded yet (no .work directory found)",
            "ℹ".blue()
        );
        return Ok(());
    };
    let type_filter: Option<MemoryEntryType> = entry_type.map(|t| t.parse()).transpose()?;

    // Explicit stage: scope to that single journal.
    if let Some(stage) = stage_id {
        let shown = print_journal_entries(&work_dir, &stage, type_filter, 20)?;
        if shown == 0 {
            println!(
                "{} No {} entries in memory journal for stage '{}'",
                "ℹ".blue(),
                type_filter
                    .map(|t| t.to_string())
                    .unwrap_or_else(|| "matching".to_string()),
                stage
            );
        }
        return Ok(());
    }

    // No explicit stage: aggregate all journals in the plan.
    let mut journals = list_journals(&work_dir)?;
    if journals.is_empty() {
        println!("{} No memory journals found", "ℹ".blue());
        return Ok(());
    }
    journals.sort();

    let current_stage = std::env::var("LOOM_STAGE_ID").ok();
    println!(
        "{} Plan Memory — {} journal{}",
        "📚".bold(),
        journals.len(),
        if journals.len() == 1 { "" } else { "s" }
    );
    if let Some(ref cur) = current_stage {
        println!("{} {}", "Current stage:".dimmed(), cur.cyan());
    }

    let mut total_shown = 0;
    for stage_name in &journals {
        total_shown += print_journal_entries(&work_dir, stage_name, type_filter, 20)?;
    }

    if total_shown == 0 {
        println!(
            "\n{} No {} entries found across {} journal(s)",
            "ℹ".blue(),
            type_filter
                .map(|t| t.to_string())
                .unwrap_or_else(|| "matching".to_string()),
            journals.len()
        );
    } else {
        println!(
            "\n{} {} entr{} across {} journal{}",
            "Total:".bold(),
            total_shown,
            if total_shown == 1 { "y" } else { "ies" },
            journals.len(),
            if journals.len() == 1 { "" } else { "s" }
        );
    }

    Ok(())
}

/// Show full memory journal
pub fn show(stage_id: Option<String>, all: bool) -> Result<()> {
    if let Some(ref id) = stage_id {
        validate_stage_id(id)?;
    }

    let Some(work_dir) = get_work_dir_readonly() else {
        println!(
            "{} No memory recorded yet (no .work directory found)",
            "ℹ".blue()
        );
        return Ok(());
    };

    if all {
        let journals = list_journals(&work_dir)?;
        if journals.is_empty() {
            println!("{} No memory journals found", "ℹ".blue());
            return Ok(());
        }
        for stage_name in &journals {
            let journal = read_journal(&work_dir, stage_name)?;
            if journal.entries.is_empty() {
                continue;
            }
            println!("{}", "═".repeat(60));
            println!("{}", format!("Memory Journal: {stage_name}").bold());
            println!("{} entries", journal.entries.len());
            println!("{}", "═".repeat(60));
            for entry in &journal.entries {
                println!("{}", format_entry_full(entry));
            }
            println!();
        }
        return Ok(());
    }

    let stage = match stage_id {
        Some(id) => id,
        None => std::env::var("LOOM_STAGE_ID")
            .map_err(|_| anyhow::anyhow!("No stage ID provided or detected. Use --stage <id>"))?,
    };

    let journal = read_journal(&work_dir, &stage)?;

    if journal.entries.is_empty() {
        println!(
            "{} No entries in memory journal for stage '{}'",
            "ℹ".blue(),
            stage
        );
        return Ok(());
    }

    println!("{}", "═".repeat(60));
    println!("{}", format!("Memory Journal: {stage}").bold());
    println!("{} {}", "Stage:".dimmed(), journal.stage_id);
    println!("{} entries", journal.entries.len());
    println!("{}", "═".repeat(60));

    for entry in &journal.entries {
        println!("{}", format_entry_full(entry));
    }

    println!("\n{}", "═".repeat(60));

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use serial_test::serial;
    use std::process::Command;
    use tempfile::TempDir;

    /// Create a temp dir with a real `git init`-ed repo. Required because
    /// `get_or_create_work_dir`/`find_repo_root_from_cwd` only trust a
    /// candidate root that actually has a `.git` entry.
    fn init_git_repo() -> TempDir {
        let temp_dir = TempDir::new().unwrap();
        let run_git = |args: &[&str]| {
            Command::new("git")
                .args(args)
                .current_dir(temp_dir.path())
                .output()
                .unwrap()
        };
        run_git(&["init", "--initial-branch=main"]);
        run_git(&["config", "user.email", "test@test.com"]);
        run_git(&["config", "user.name", "Test"]);
        temp_dir
    }

    /// Restores cwd and `LOOM_STAGE_ID` on drop. Tests mutate process-global
    /// state (cwd, env vars) so `#[serial]` plus this guard keep them isolated.
    struct EnvGuard {
        original_dir: std::path::PathBuf,
        original_stage_id: Option<String>,
    }

    impl EnvGuard {
        fn new() -> Self {
            Self {
                original_dir: env::current_dir().unwrap(),
                original_stage_id: env::var("LOOM_STAGE_ID").ok(),
            }
        }
    }

    impl Drop for EnvGuard {
        fn drop(&mut self) {
            env::set_current_dir(&self.original_dir).unwrap();
            match &self.original_stage_id {
                Some(v) => env::set_var("LOOM_STAGE_ID", v),
                None => env::remove_var("LOOM_STAGE_ID"),
            }
        }
    }

    #[test]
    #[serial]
    fn note_creates_work_dir_when_missing_using_ad_hoc_stage() {
        let _guard = EnvGuard::new();
        env::remove_var("LOOM_STAGE_ID");
        let repo = init_git_repo();
        env::set_current_dir(repo.path()).unwrap();
        assert!(!repo.path().join(".work").exists());

        note("probe text".to_string(), None).unwrap();

        let journal_path = repo.path().join(".work/memory/ad-hoc.md");
        assert!(
            journal_path.exists(),
            ".work/memory/ad-hoc.md should be auto-created"
        );
        let content = std::fs::read_to_string(&journal_path).unwrap();
        assert!(content.contains("probe text"));
    }

    #[test]
    #[serial]
    fn note_uses_loom_stage_id_env_var_over_sentinel() {
        let _guard = EnvGuard::new();
        let repo = init_git_repo();
        env::set_current_dir(repo.path()).unwrap();
        env::set_var("LOOM_STAGE_ID", "env-stage");

        note("from env".to_string(), None).unwrap();

        assert!(repo.path().join(".work/memory/env-stage.md").exists());
        assert!(!repo.path().join(".work/memory/ad-hoc.md").exists());
    }

    #[test]
    #[serial]
    fn note_explicit_stage_overrides_env_var() {
        let _guard = EnvGuard::new();
        let repo = init_git_repo();
        env::set_current_dir(repo.path()).unwrap();
        env::set_var("LOOM_STAGE_ID", "env-stage");

        note("explicit wins".to_string(), Some("cli-stage".to_string())).unwrap();

        assert!(repo.path().join(".work/memory/cli-stage.md").exists());
        assert!(!repo.path().join(".work/memory/env-stage.md").exists());
    }

    #[test]
    #[serial]
    fn note_outside_git_repo_still_fails() {
        let _guard = EnvGuard::new();
        env::remove_var("LOOM_STAGE_ID");
        // A plain temp dir (no `git init`) has no `.git` anywhere in its
        // ancestry, so this must fail exactly like the pre-existing behavior.
        let plain_dir = TempDir::new().unwrap();
        env::set_current_dir(plain_dir.path()).unwrap();

        let result = note("should not be recorded".to_string(), None);

        assert!(result.is_err());
        assert!(result
            .unwrap_err()
            .to_string()
            .contains(".work directory not found"));
        assert!(!plain_dir.path().join(".work").exists());
    }

    #[test]
    #[serial]
    fn list_and_show_degrade_without_creating_work_dir() {
        let _guard = EnvGuard::new();
        let repo = init_git_repo();
        env::set_current_dir(repo.path()).unwrap();

        assert!(list(None, None).is_ok());
        assert!(
            !repo.path().join(".work").exists(),
            "list must not create .work"
        );

        assert!(show(None, true).is_ok());
        assert!(
            !repo.path().join(".work").exists(),
            "show --all must not create .work"
        );
    }

    #[test]
    #[serial]
    fn note_reuses_existing_work_dir_without_recreating() {
        let _guard = EnvGuard::new();
        env::remove_var("LOOM_STAGE_ID");
        let repo = init_git_repo();
        env::set_current_dir(repo.path()).unwrap();
        // Pre-existing `.work` (as a real loom plan would leave behind) must
        // be found by `get_work_dir()` and reused, not recreated.
        std::fs::create_dir_all(repo.path().join(".work")).unwrap();

        note("reuse me".to_string(), None).unwrap();

        let journal_path = repo.path().join(".work/memory/ad-hoc.md");
        assert!(journal_path.exists());
        let content = std::fs::read_to_string(&journal_path).unwrap();
        assert!(content.contains("reuse me"));
    }
}
