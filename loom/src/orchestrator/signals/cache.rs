use sha2::{Digest, Sha256};

/// Metrics about a generated signal for debugging and optimization
#[derive(Debug, Clone, Default)]
pub struct SignalMetrics {
    /// Total size of the signal in bytes
    pub signal_size_bytes: usize,
    /// Estimated token count (approximate: bytes / 4)
    pub estimated_tokens: usize,
    /// SHA-256 hash of the stable prefix for cache debugging
    pub stable_prefix_hash: String,
    /// Size of stable prefix in bytes
    pub stable_prefix_bytes: usize,
    /// Size of semi-stable section in bytes
    pub semi_stable_bytes: usize,
    /// Size of dynamic section in bytes
    pub dynamic_bytes: usize,
    /// Size of recitation section in bytes
    pub recitation_bytes: usize,
}

impl SignalMetrics {
    /// Compute metrics from signal sections
    pub fn from_sections(stable: &str, semi_stable: &str, dynamic: &str, recitation: &str) -> Self {
        let stable_bytes = stable.len();
        let semi_stable_bytes = semi_stable.len();
        let dynamic_bytes = dynamic.len();
        let recitation_bytes = recitation.len();
        let total_bytes = stable_bytes + semi_stable_bytes + dynamic_bytes + recitation_bytes;

        Self {
            signal_size_bytes: total_bytes,
            estimated_tokens: total_bytes / 4,
            stable_prefix_hash: compute_hash(stable),
            stable_prefix_bytes: stable_bytes,
            semi_stable_bytes,
            dynamic_bytes,
            recitation_bytes,
        }
    }
}

/// Compute SHA-256 hash of content, returning first 16 hex characters
pub fn compute_hash(content: &str) -> String {
    let mut hasher = Sha256::new();
    hasher.update(content.as_bytes());
    let result = hasher.finalize();
    hex::encode(&result[..8])
}

/// Stable prefix content that rarely changes (Manus KV-cache pattern)
///
/// This section contains:
/// - Fixed header with execution rules
/// - Worktree context and isolation boundaries
/// - CLAUDE.md rule reminders
///
/// These elements are constant across signals for the same agent type,
/// enabling KV-cache reuse when the LLM sees the same prefix.
pub fn generate_stable_prefix() -> String {
    let mut content = String::new();

    // Fixed header that NEVER changes
    content.push_str("## Worktree Context\n\n");
    content.push_str(
        "You are in an **isolated git worktree**. This signal contains everything you need:\n\n",
    );
    content.push_str("- **Your stage assignment and acceptance criteria are below** - this file is self-contained\n");
    content.push_str("- **All context (plan overview, handoff, knowledge) is embedded below** - reading main repo files is **FORBIDDEN**\n");
    content.push_str(
        "- **Commit to your worktree branch** - it will be merged after verification\n\n",
    );

    // Explicit isolation boundaries
    content.push_str("**Isolation Boundaries (STRICT):**\n\n");
    content.push_str("- You are **CONFINED** to this worktree - do not access files outside it\n");
    content.push_str(
        "- All context you need is embedded below - reading main repo files is **FORBIDDEN**\n",
    );
    content
        .push_str("- Git commands must target THIS worktree only - no `git -C`, no `cd ../..`\n\n");

    // Path boundaries subsection
    content.push_str("### Path Boundaries\n\n");
    content.push_str("| Type | Paths |\n");
    content.push_str("|------|-------|\n");
    content
        .push_str("| **ALLOWED** | `.` (this worktree), `.work/` (symlink to orchestration) |\n");
    content.push_str(
        "| **FORBIDDEN** | `../..`, absolute paths to main repo, any path outside worktree |\n\n",
    );

    // working_dir reminder for acceptance criteria
    content.push_str(
        "**working_dir Reminder:** Acceptance criteria execute from `WORKTREE + working_dir`.\n",
    );
    content.push_str("Check the Target section below for the exact execution path.\n\n");

    // Add reminder to follow CLAUDE.md rules
    content.push_str("## Execution Rules\n\n");
    content.push_str("Follow your `~/.claude/CLAUDE.md` and project `CLAUDE.md` rules (both are symlinked into this worktree). Key reminders:\n\n");
    content.push_str("**Worktree Isolation (CRITICAL):**\n");
    content.push_str(
        "- **STAY IN THIS WORKTREE** - never read files from main repo or other worktrees\n",
    );
    content.push_str(
        "- **All context is embedded above** - you have everything you need in this signal\n",
    );
    content.push_str("- **No path escaping** - do not use `../..`, `cd` to parent directories, or absolute paths outside worktree\n\n");
    content.push_str("**Delegation & Efficiency (CRITICAL):**\n\n");
    content.push_str(
        "**Parallel subagents and appropriate skills should be used WHEREVER POSSIBLE.**\n\n",
    );
    content.push_str(
        "- **Use PARALLEL subagents** - spawn multiple appropriate subagents concurrently when tasks are independent\n",
    );
    content.push_str(
        "- **Use Skills** - check if /auth, /testing, /ci-cd, /logging-observability apply\n",
    );
    content.push_str(
        "- **Use specialized agents** - security-engineer, senior-infrastructure-engineer, etc.\n",
    );
    content.push_str("- **Use TodoWrite** to plan and track progress\n\n");
    content.push_str("**Completion:**\n");
    content.push_str("- **Verify acceptance criteria** before marking stage complete\n");
    content.push_str("- **Create handoff** if context exceeds 75%\n");
    content.push_str("- **IMPORTANT: Before running `loom stage complete`, ensure you are at the worktree root directory**\n\n");
    content.push_str("**Session Memory - MEMORY ONLY (MANDATORY):**\n\n");
    content.push_str("```text\n");
    content.push_str("⚠️  IMPLEMENTATION STAGES USE `loom memory` ONLY - NEVER `loom knowledge`\n");
    content.push_str("    Only integration-verify stages can promote memories to knowledge.\n");
    content.push_str("```\n\n");
    content.push_str(
        "- **Record discoveries** as you find them: `loom memory note \"observation\"`\n",
    );
    content.push_str("- **Record decisions** when you choose between alternatives: `loom memory decision \"choice\" --context \"why\"`\n");
    content.push_str("- **Record mistakes** immediately when they occur: `loom memory note \"mistake: description\"`\n");
    content.push_str("- **FORBIDDEN**: `loom knowledge update` commands - these are ONLY for knowledge-bootstrap and integration-verify stages\n");
    content
        .push_str("- Memory entries persist across sessions - they will be promoted to knowledge during integration-verify\n\n");
    content.push_str("**Git Staging (CRITICAL - READ CAREFULLY):**\n\n");
    content.push_str("```text\n");
    content.push_str("  ⛔ DANGER: .work is a SYMLINK to shared state in worktrees\n");
    content.push_str("     Committing it CORRUPTS the main repository!\n");
    content.push_str("```\n\n");
    content.push_str("- **ALWAYS** use `git add <specific-files>` - stage only files you modified\n");
    content.push_str("- **NEVER** use `git add -A`, `git add --all`, or `git add .`\n");
    content.push_str("- **NEVER** stage `.work` - it is orchestration state shared across stages\n\n");
    content.push_str("**Example:**\n");
    content.push_str("```bash\n");
    content.push_str("# CORRECT:\n");
    content.push_str("git add src/main.rs src/lib.rs tests/\n\n");
    content.push_str("# WRONG (will stage .work):\n");
    content.push_str("git add -A  # DON'T DO THIS\n");
    content.push_str("git add .   # DON'T DO THIS\n");
    content.push_str("```\n\n");
    content.push_str("**Binary Usage (CRITICAL when working on loom):**\n");
    content.push_str("- **ALWAYS use `loom`** - the installed binary from PATH\n");
    content.push_str("- **NEVER use `target/debug/loom`** or `./loom/target/debug/loom`\n");
    content.push_str("- Development binaries cause version mismatches and state corruption\n\n");
    content.push_str("**State Files (CRITICAL):**\n");
    content.push_str("- **NEVER edit `.work/` files directly** - always use loom CLI\n");
    content.push_str("- State is managed by the orchestrator, not by agents\n");
    content.push_str("- Direct edits corrupt state and cause phantom completions\n\n");

    content
}

/// Stable prefix for knowledge stages (runs in main repo, no worktree)
///
/// Knowledge stages have different rules:
/// - Run in the main repository, not a worktree
/// - No commits or merges required
/// - Focus on exploring codebase and populating doc/loom/knowledge/
/// - No git staging restrictions (they don't commit)
pub fn generate_knowledge_stable_prefix() -> String {
    let mut content = String::new();

    // Fixed header for knowledge stages
    content.push_str("## Knowledge Stage Context\n\n");
    content.push_str(
        "You are running a **knowledge-gathering stage** in the **main repository**.\n\n",
    );
    content.push_str("**Key Differences from Regular Stages:**\n\n");
    content
        .push_str("- **NO WORKTREE** - You are in the main repository, not an isolated worktree\n");
    content.push_str("- **NO COMMITS REQUIRED** - Knowledge stages do NOT require git commits\n");
    content.push_str("- **NO MERGING** - Your work stays in doc/loom/knowledge/ directly\n");
    content.push_str(
        "- **EXPLORATION FOCUS** - Your goal is to understand and document the codebase\n\n",
    );

    // What knowledge stages DO
    content.push_str("**Your Mission:**\n\n");
    content.push_str("1. **Explore** the codebase hierarchically (entry points → modules → patterns → conventions)\n");
    content.push_str(
        "2. **Document** findings using `loom knowledge update <file> <content>` commands\n",
    );
    content.push_str("3. **Verify** acceptance criteria before completing\n\n");

    // Add prominent knowledge update reminder box for knowledge stages
    content.push_str("```text\n");
    content.push_str("┌────────────────────────────────────────────────────────────────────┐\n");
    content.push_str("│  📝 RECORD YOUR DISCOVERIES                                        │\n");
    content.push_str("│                                                                    │\n");
    content.push_str("│  As you explore, UPDATE doc/loom/knowledge/:                       │\n");
    content.push_str("│  - Entry points: Key files and their purposes                      │\n");
    content.push_str("│  - Patterns: Architectural patterns and best practices             │\n");
    content.push_str("│  - Conventions: Coding standards and naming schemes                │\n");
    content.push_str("│  - Mistakes: Document ANY errors you encounter                     │\n");
    content.push_str("│                                                                    │\n");
    content.push_str("│  Use: loom knowledge update <file> \"content\"                       │\n");
    content.push_str("└────────────────────────────────────────────────────────────────────┘\n");
    content.push_str("```\n\n");

    // Add reminder to follow CLAUDE.md rules
    content.push_str("## Execution Rules\n\n");
    content.push_str(
        "Follow your `~/.claude/CLAUDE.md` and project `CLAUDE.md` rules. Key reminders:\n\n",
    );
    content.push_str("**Delegation & Efficiency (CRITICAL):**\n\n");
    content.push_str(
        "**Parallel subagents and appropriate skills should be used WHEREVER POSSIBLE.**\n\n",
    );
    content.push_str(
        "- **Use PARALLEL subagents** - spawn multiple appropriate subagents concurrently when tasks are independent\n",
    );
    content.push_str(
        "- **Use Skills** - check if /auth, /testing, /ci-cd, /logging-observability apply\n",
    );
    content.push_str(
        "- **Use specialized agents** - security-engineer, senior-infrastructure-engineer, etc.\n",
    );
    content.push_str("- **Use TodoWrite** to plan and track progress\n\n");
    content.push_str("**Completion:**\n");
    content.push_str("- **Verify acceptance criteria** before marking stage complete\n");
    content.push_str("- **Create handoff** if context exceeds 75%\n");
    content
        .push_str("- **Run `loom stage complete <stage-id>`** when done (from the repo root)\n\n");
    content.push_str("**Binary Usage (CRITICAL when working on loom):**\n");
    content.push_str("- **ALWAYS use `loom`** - the installed binary from PATH\n");
    content.push_str("- **NEVER use `target/debug/loom`** or `./loom/target/debug/loom`\n");
    content.push_str("- Development binaries cause version mismatches and state corruption\n\n");
    content.push_str("**State Files (CRITICAL):**\n");
    content.push_str("- **NEVER edit `.work/` files directly** - always use loom CLI\n");
    content.push_str("- State is managed by the orchestrator, not by agents\n");
    content.push_str("- Direct edits corrupt state and cause phantom completions\n\n");

    // Knowledge-specific instructions
    content.push_str("**Knowledge Commands:**\n\n");
    content.push_str("```bash\n");
    content.push_str("# Update a knowledge file\n");
    content.push_str(
        "loom knowledge update entry-points \"## Section\\n\\n- path/file.rs - description\"\n",
    );
    content.push_str("loom knowledge update patterns \"## Pattern Name\\n\\n- How it works\"\n");
    content.push_str("loom knowledge update conventions \"## Convention\\n\\n- Details\"\n");
    content.push_str("loom knowledge update mistakes \"## What happened\\n\\n- Details\"\n");
    content.push_str("\n# Show current knowledge\n");
    content.push_str("loom knowledge show\n");
    content.push_str("loom knowledge show entry-points\n");
    content.push_str("```\n\n");

    content
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_compute_hash_deterministic() {
        let content = "test content";
        let hash1 = compute_hash(content);
        let hash2 = compute_hash(content);
        assert_eq!(hash1, hash2);
        assert_eq!(hash1.len(), 16);
    }

    #[test]
    fn test_compute_hash_different_content() {
        let hash1 = compute_hash("content A");
        let hash2 = compute_hash("content B");
        assert_ne!(hash1, hash2);
    }

    #[test]
    fn test_signal_metrics_from_sections() {
        let stable = "stable content here";
        let semi_stable = "semi-stable";
        let dynamic = "dynamic content";
        let recitation = "recitation at end";

        let metrics = SignalMetrics::from_sections(stable, semi_stable, dynamic, recitation);

        assert_eq!(metrics.stable_prefix_bytes, stable.len());
        assert_eq!(metrics.semi_stable_bytes, semi_stable.len());
        assert_eq!(metrics.dynamic_bytes, dynamic.len());
        assert_eq!(metrics.recitation_bytes, recitation.len());
        assert_eq!(
            metrics.signal_size_bytes,
            stable.len() + semi_stable.len() + dynamic.len() + recitation.len()
        );
        assert_eq!(metrics.estimated_tokens, metrics.signal_size_bytes / 4);
        assert!(!metrics.stable_prefix_hash.is_empty());
    }

    #[test]
    fn test_generate_stable_prefix_contains_required_sections() {
        let prefix = generate_stable_prefix();

        assert!(prefix.contains("## Worktree Context"));
        assert!(prefix.contains("Isolation Boundaries"));
        assert!(prefix.contains("Path Boundaries"));
        assert!(prefix.contains("## Execution Rules"));
        assert!(prefix.contains("STAY IN THIS WORKTREE"));
        assert!(prefix.contains("git add <specific-files>"));
        // Critical: parallel subagents guidance must be verbatim
        assert!(prefix.contains(
            "Parallel subagents and appropriate skills should be used WHEREVER POSSIBLE."
        ));
        // Critical: worktree root directory reminder for loom stage complete
        assert!(prefix.contains(
            "Before running `loom stage complete`, ensure you are at the worktree root directory"
        ));
        // Critical: specific skill examples
        assert!(prefix.contains("/auth"));
        assert!(prefix.contains("/testing"));
        assert!(prefix.contains("security-engineer"));
    }

    #[test]
    fn test_stable_prefix_is_stable() {
        let prefix1 = generate_stable_prefix();
        let prefix2 = generate_stable_prefix();
        assert_eq!(prefix1, prefix2, "Stable prefix should be deterministic");
    }

    #[test]
    fn test_knowledge_stable_prefix_contains_required_sections() {
        let prefix = generate_knowledge_stable_prefix();

        assert!(prefix.contains("## Knowledge Stage Context"));
        assert!(prefix.contains("main repository"));
        assert!(prefix.contains("NO WORKTREE"));
        assert!(prefix.contains("NO COMMITS REQUIRED"));
        assert!(prefix.contains("NO MERGING"));
        assert!(prefix.contains("## Execution Rules"));
        assert!(prefix.contains("loom knowledge update"));
        assert!(prefix.contains("loom stage complete"));
        // Critical: parallel subagents guidance must be verbatim
        assert!(prefix.contains(
            "Parallel subagents and appropriate skills should be used WHEREVER POSSIBLE."
        ));
        // Critical: specific skill examples
        assert!(prefix.contains("/auth"));
        assert!(prefix.contains("/testing"));
        assert!(prefix.contains("security-engineer"));
    }

    #[test]
    fn test_knowledge_stable_prefix_is_stable() {
        let prefix1 = generate_knowledge_stable_prefix();
        let prefix2 = generate_knowledge_stable_prefix();
        assert_eq!(
            prefix1, prefix2,
            "Knowledge stable prefix should be deterministic"
        );
    }
}
