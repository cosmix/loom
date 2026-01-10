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
    pub fn from_sections(
        stable: &str,
        semi_stable: &str,
        dynamic: &str,
        recitation: &str,
    ) -> Self {
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
    content.push_str("- **All context (plan overview, handoff, structure map) is embedded below** - reading main repo files is **FORBIDDEN**\n");
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
    content.push_str("**Delegation & Efficiency:**\n");
    content.push_str(
        "- **Use PARALLEL subagents** - spawn multiple appropriate subagents concurrently when tasks are independent\n",
    );
    content.push_str("- **Use Skills** - invoke relevant skills wherever applicable\n");
    content.push_str("- **Use TodoWrite** to plan and track progress\n\n");
    content.push_str("**Completion:**\n");
    content.push_str("- **Verify acceptance criteria** before marking stage complete\n");
    content.push_str("- **Create handoff** if context exceeds 75%\n\n");
    content.push_str("**Git Staging (CRITICAL):**\n");
    content
        .push_str("- **ALWAYS use `git add <specific-files>`** - stage only files you modified\n");
    content.push_str("- **NEVER use `git add -A` or `git add .`** - these include `.work` which must NOT be committed\n");
    content.push_str("- `.work` is a symlink to shared orchestration state - never stage it\n\n");

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
    }

    #[test]
    fn test_stable_prefix_is_stable() {
        let prefix1 = generate_stable_prefix();
        let prefix2 = generate_stable_prefix();
        assert_eq!(prefix1, prefix2, "Stable prefix should be deterministic");
    }
}
