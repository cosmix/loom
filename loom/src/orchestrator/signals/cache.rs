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

// ── Shared content blocks ────────────────────────────────────────────

/// Append path boundaries table (shared by standard and integration-verify prefixes)
fn append_path_boundaries(content: &mut String) {
    content.push_str("### Path Boundaries\n\n");
    content.push_str("| Type | Paths |\n");
    content.push_str("|------|-------|\n");
    content
        .push_str("| **ALLOWED** | `.` (this worktree), `.work/` (symlink to orchestration) |\n");
    content.push_str(
        "| **FORBIDDEN** | `../..`, absolute paths to main repo, any path outside worktree |\n\n",
    );
}

/// Append subagent restrictions (shared by standard and integration-verify, last line differs)
fn append_subagent_restrictions(content: &mut String, agents_role: &str) {
    content.push_str("**Subagent Restrictions (CRITICAL - PREVENTS LOST WORK):**\n\n");
    content.push_str("When spawning subagents via Task tool, they MUST be told:\n");
    content.push_str("- ⛔ **NEVER run `git commit`** - only the main agent commits\n");
    content.push_str(
        "- ⛔ **NEVER run `loom stage complete`** - only the main agent completes stages\n",
    );
    content.push_str("- ⛔ **NEVER run `git add -A` or `git add .`** - only specific files\n");
    content.push_str(agents_role);
}

/// Append completion rules shared between standard and integration-verify prefixes
fn append_completion_rules(content: &mut String) {
    content.push_str("- **Verify acceptance criteria** before marking stage complete\n");
    content.push_str("- **Create handoff** if context exceeds 75%\n");
    content.push_str("- **IMPORTANT: Before running `loom stage complete`, ensure you are at the worktree root directory**\n");
    content.push_str("- **If acceptance criteria fail**: Fix the issues and run `loom stage complete <stage-id>` again\n");
    content.push_str("- **NEVER use `loom stage retry` from an active session** — it creates a parallel session\n\n");
}

/// Append binary usage, state files, and context recovery (shared by all prefix types)
fn append_common_footer(content: &mut String) {
    content.push_str("**Binary Usage (CRITICAL when working on loom):**\n");
    content.push_str("- **ALWAYS use `loom`** - the installed binary from PATH\n");
    content.push_str("- **NEVER use `target/debug/loom`** or `./loom/target/debug/loom`\n");
    content.push_str("- Development binaries cause version mismatches and state corruption\n\n");
    content.push_str("**State Files (CRITICAL):**\n");
    content.push_str("- **NEVER edit `.work/` files directly** - always use loom CLI\n");
    content.push_str("- State is managed by the orchestrator, not by agents\n");
    content.push_str("- Direct edits corrupt state and cause phantom completions\n\n");
    content.push_str("**Context Recovery (after compaction):**\n\n");
    content.push_str("If your context was recently compacted or you feel disoriented:\n");
    content.push_str("1. Run: `loom memory list` (see your session notes)\n");
    content.push_str("2. Check: `.work/handoffs/` for handoff files for your stage\n");
    content.push_str("3. Read the latest handoff to restore working context\n");
    content.push_str("4. Resume from where you left off - do NOT restart from scratch\n\n");
}

/// Append git staging rules with danger box (standard prefix only)
fn append_git_staging_full(content: &mut String) {
    content.push_str("**Git Staging (CRITICAL - READ CAREFULLY):**\n\n");
    content.push_str("```text\n");
    content.push_str("  ⛔ DANGER: .work is a SYMLINK to shared state in worktrees\n");
    content.push_str("     Committing it CORRUPTS the main repository!\n");
    content.push_str("```\n\n");
    append_git_staging_rules(content);
    content.push_str("**Example:**\n");
    content.push_str("```bash\n");
    content.push_str("# CORRECT:\n");
    content.push_str("git add src/main.rs src/lib.rs tests/\n\n");
    content.push_str("# WRONG (will stage .work):\n");
    content.push_str("git add -A  # DON'T DO THIS\n");
    content.push_str("git add .   # DON'T DO THIS\n");
    content.push_str("```\n\n");
}

/// Append the 3 core git staging rules (shared by standard and integration-verify)
fn append_git_staging_rules(content: &mut String) {
    content
        .push_str("- **ALWAYS** use `git add <specific-files>` - stage only files you modified\n");
    content.push_str("- **NEVER** use `git add -A`, `git add --all`, or `git add .`\n");
    content
        .push_str("- **NEVER** stage `.work` - it is orchestration state shared across stages\n\n");
}

// ── Prefix generators ────────────────────────────────────────────────

/// Stable prefix content that rarely changes (Manus KV-cache pattern)
pub fn generate_stable_prefix() -> String {
    let mut content = String::new();

    // Worktree context header
    content.push_str("## Worktree Context\n\n");
    content.push_str(
        "You are in an **isolated git worktree**. This signal contains everything you need:\n\n",
    );
    content.push_str("- **Your stage assignment and acceptance criteria are below** - this file is self-contained\n");
    content.push_str("- **All context (plan overview, handoff, knowledge) is embedded below** - reading main repo files is **FORBIDDEN**\n");
    content.push_str(
        "- **Commit to your worktree branch** - it will be merged after verification\n\n",
    );

    // Isolation boundaries
    content.push_str("**Isolation Boundaries (STRICT):**\n\n");
    content.push_str("- You are **CONFINED** to this worktree - do not access files outside it\n");
    content.push_str(
        "- All context you need is embedded below - reading main repo files is **FORBIDDEN**\n",
    );
    content
        .push_str("- Git commands must target THIS worktree only - no `git -C`, no `cd ../..`\n\n");

    append_path_boundaries(&mut content);

    // working_dir reminder
    content.push_str(
        "**working_dir Reminder:** Acceptance criteria execute from `WORKTREE + working_dir`.\n",
    );
    content.push_str("Check the Target section below for the exact execution path.\n\n");

    // Execution rules
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
    content.push_str("**USE THE TASK TOOL** to spawn parallel subagents for multi-part work:\n");
    content.push_str("- Independent file changes, multiple components, tests + implementation → spawn parallel subagents\n");
    content.push_str("- Pattern: `Task(subagent_type=\"software-engineer\", prompt=\"...\")` - send MULTIPLE in ONE message\n");
    content.push_str("- Agents: `software-engineer`, `senior-software-engineer`, `Explore`\n");
    content.push_str("- Skills: /auth, /testing, /ci-cd, /logging-observability\n\n");
    append_subagent_restrictions(
        &mut content,
        "- Subagents write code and report results; main agent handles git\n\n",
    );
    content.push_str("**Agent Teams (WHEN AVAILABLE):**\n\n");
    content.push_str("If CLAUDE_CODE_EXPERIMENTAL_AGENT_TEAMS=1 is set, you can create\n");
    content.push_str("agent teams for richer coordination than subagents:\n");
    content.push_str(
        "- Teams provide: inter-agent messaging, shared task lists, idle/wake lifecycle\n",
    );
    content
        .push_str("- Teams cost ~7x tokens - use ONLY when coordination benefit justifies cost\n");
    content.push_str(
        "- YOU are the team lead - only YOU may run git commit and loom stage complete\n",
    );
    content.push_str("- Teammates CANNOT commit, complete stages, or update memory/knowledge\n");
    content.push_str("- Record teammate insights: loom memory note \"Teammate found: ...\"\n");
    content
        .push_str("- Keep context for coordination (<40% utilization), delegate implementation\n");
    content.push_str("- Shut down ALL teammates before completing the stage\n\n");
    content.push_str("**Completion:**\n");
    append_completion_rules(&mut content);
    content.push_str("**Stage Memory - MEMORY ONLY (MANDATORY):**\n\n");
    content.push_str("```text\n");
    content.push_str("⚠️  IMPLEMENTATION STAGES USE `loom memory` ONLY - NEVER `loom knowledge`\n");
    content.push_str("    Only integration-verify stages can curate memories into knowledge.\n");
    content.push_str("```\n\n");
    content.push_str(
        "- **Record discoveries** as you find them: `loom memory note \"observation\"`\n",
    );
    content.push_str("- **Record decisions** when you choose between alternatives: `loom memory decision \"choice\" --context \"why\"`\n");
    content.push_str("- **Record mistakes** immediately when they occur: `loom memory note \"mistake: description\"`\n");
    content.push_str("- **FORBIDDEN**: `loom knowledge update` commands - these are ONLY for knowledge-bootstrap and integration-verify stages\n");
    content
        .push_str("- Memory entries persist across sessions - they will be reviewed and curated into knowledge during integration-verify\n\n");
    append_git_staging_full(&mut content);
    append_common_footer(&mut content);

    content
}

/// Stable prefix for integration-verify stages (final quality gate)
pub fn generate_integration_verify_stable_prefix() -> String {
    let mut content = String::new();

    // Integration-verify header
    content.push_str("## Integration Verification Context\n\n");
    content.push_str(
        "You are running an **integration-verify stage** - the **FINAL QUALITY GATE** before merge.\n\n",
    );

    // Zero tolerance
    content.push_str("**ZERO TOLERANCE FOR ISSUES:**\n\n");
    content.push_str("- **ALL** compiler warnings must be fixed - not suppressed, FIXED\n");
    content.push_str("- **ALL** linter errors must be resolved - no exceptions\n");
    content.push_str("- **ALL** test failures must be addressed\n");
    content.push_str("- **ALL** IDE warnings should be investigated and resolved\n");
    content.push_str("- **NOTHING** is \"pre-existing\" - if it's broken, fix it now\n");
    content
        .push_str("- **NOTHING** is \"too trivial\" - small issues compound into big problems\n\n");

    // Mission
    content.push_str("**Your Mission:**\n\n");
    content.push_str("1. **REVIEW** code for quality, security, and correctness issues\n");
    content.push_str("2. **FIX** every warning, error, and issue you encounter\n");
    content.push_str("3. **VERIFY** all acceptance criteria pass\n");
    content.push_str("4. **TEST** that the feature actually works end-to-end\n");
    content.push_str("5. **PROMOTE** valuable learnings to knowledge\n\n");

    // Code review box
    content.push_str("```text\n");
    content.push_str("┌────────────────────────────────────────────────────────────────────┐\n");
    content.push_str("│  🔍 CODE REVIEW + VERIFICATION EXECUTION STRATEGY                  │\n");
    content.push_str("│                                                                    │\n");
    content.push_str("│  Use PARALLEL SPECIALIZED AGENTS for comprehensive review:        │\n");
    content.push_str("│                                                                    │\n");
    content.push_str("│  1. security-engineer    - Security vulnerabilities, OWASP, auth  │\n");
    content.push_str("│  2. senior-software-engineer - Architecture, patterns, quality    │\n");
    content.push_str("│  3. Build/test runner    - Full test suite, clippy, compile       │\n");
    content.push_str("│  4. Functional verifier  - Wiring, reachability, smoke tests      │\n");
    content.push_str("│                                                                    │\n");
    content.push_str("│  Spawn these as PARALLEL subagents to maximize efficiency.        │\n");
    content.push_str("└────────────────────────────────────────────────────────────────────┘\n");
    content.push_str("```\n\n");

    // Agent teams for IV
    content.push_str("**Agent Teams for Integration Verification:**\n\n");
    content.push_str("Consider using an agent team for multi-dimension review and verification:\n");
    content.push_str("- Security review teammate: OWASP, auth, secrets, dependencies\n");
    content.push_str("- Architecture review teammate: patterns, coupling, maintainability\n");
    content.push_str("- Build/test teammate: full test suite, clippy, compile, format check\n");
    content.push_str(
        "- Functional verification teammate: wiring, reachability, smoke tests, end-to-end\n",
    );
    content.push_str("- Knowledge curation teammate: review memory, curate to knowledge\n");
    content.push_str("Teams allow verification tasks to coordinate on discovered issues.\n\n");

    // Isolation + path boundaries (shared)
    content.push_str("**Isolation Boundaries (STRICT):**\n\n");
    content.push_str("- You are **CONFINED** to this worktree - do not access files outside it\n");
    content
        .push_str("- Git commands must target THIS worktree only - no `git -C`, no `cd ../..`\n\n");

    append_path_boundaries(&mut content);

    // Execution rules
    content.push_str("## Execution Rules\n\n");
    content.push_str(
        "Follow your `~/.claude/CLAUDE.md` and project `CLAUDE.md` rules. Key reminders:\n\n",
    );

    content.push_str("**Delegation & Efficiency (CRITICAL):**\n\n");
    content.push_str("**USE THE TASK TOOL** to spawn parallel subagents for verification:\n");
    content.push_str("- Run tests, linting, and build checks in parallel where possible\n");
    content.push_str("- Pattern: `Task(subagent_type=\"software-engineer\", prompt=\"...\")` - send MULTIPLE in ONE message\n");
    content.push_str("- Agents: `software-engineer`, `senior-software-engineer`, `Explore`\n");
    content.push_str("- Skills: /testing, /auth, /ci-cd, /logging-observability\n\n");

    append_subagent_restrictions(
        &mut content,
        "- Subagents fix issues and report results; main agent handles git\n\n",
    );

    content.push_str("**Completion:**\n");
    content.push_str(
        "- **Fix ALL issues** - do not mark complete with any warnings or errors remaining\n",
    );
    append_completion_rules(&mut content);

    // Knowledge curation
    content.push_str("**Knowledge Curation (CAN UPDATE KNOWLEDGE):**\n\n");
    content.push_str("Integration-verify stages CURATE memory into knowledge:\n");
    content.push_str("1. Read ALL stage memory: `loom memory show --all`\n");
    content.push_str("2. Evaluate each insight — is it worth keeping permanently?\n");
    content.push_str("3. Write curated content: `loom knowledge update <file> \"content\"`\n");
    content.push_str(
        "   Focus on: mistakes worth avoiding, patterns worth reusing, architectural decisions\n",
    );
    content.push_str("4. DO NOT blindly copy entries — synthesize and curate\n\n");

    // Git staging (shorter version)
    content.push_str("**Git Staging (CRITICAL):**\n");
    append_git_staging_rules(&mut content);

    append_common_footer(&mut content);

    content
}

/// Stable prefix for knowledge stages (runs in main repo, no worktree)
pub fn generate_knowledge_stable_prefix() -> String {
    let mut content = String::new();

    // Knowledge header
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

    // Mission
    content.push_str("**Your Mission:**\n\n");
    content.push_str("1. **Explore** the codebase hierarchically (entry points → modules → patterns → conventions)\n");
    content.push_str(
        "2. **Document** findings using `loom knowledge update <file> <content>` commands\n",
    );
    content.push_str("3. **Verify** acceptance criteria before completing\n\n");

    // Agent teams for knowledge
    content.push_str("**Agent Teams for Knowledge Bootstrap:**\n\n");
    content.push_str("Consider using an agent team for coordinated exploration:\n");
    content.push_str("- Architecture explorer: component relationships, data flow\n");
    content.push_str("- Patterns explorer: error handling, state management, idioms\n");
    content.push_str("- Conventions explorer: naming, file structure, testing patterns\n");
    content.push_str("Teams allow explorers to share discoveries that inform each other.\n\n");

    // Record discoveries box
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

    // Execution rules
    content.push_str("## Execution Rules\n\n");
    content.push_str(
        "Follow your `~/.claude/CLAUDE.md` and project `CLAUDE.md` rules. Key reminders:\n\n",
    );
    content.push_str("**Delegation & Efficiency (CRITICAL):**\n\n");
    content.push_str("**USE THE TASK TOOL** to spawn parallel subagents for exploration:\n");
    content.push_str("- Different codebase areas, multiple knowledge files, independent research → spawn parallel Explore agents\n");
    content.push_str("- Pattern: `Task(subagent_type=\"Explore\", prompt=\"...\")` - send MULTIPLE in ONE message\n");
    content.push_str("- Agents: `Explore`, `software-engineer`, `senior-software-engineer`\n");
    content.push_str("- Skills: /auth, /testing, /ci-cd, /logging-observability\n\n");
    content.push_str("**Completion:**\n");
    content.push_str("- **Verify acceptance criteria** before marking stage complete\n");
    content.push_str("- **Create handoff** if context exceeds 75%\n");
    content.push_str("- **Run `loom stage complete <stage-id>`** when done (from the repo root)\n");
    content.push_str("- **If acceptance criteria fail**: Fix the issues and run `loom stage complete <stage-id>` again\n");
    content.push_str("- **NEVER use `loom stage retry` from an active session** — it creates a parallel session\n\n");
    append_common_footer(&mut content);

    // Knowledge-specific commands
    content.push_str("**Knowledge Commands:**\n\n");
    content.push_str("```bash\n");
    content.push_str("# Update a knowledge file\n");
    content.push_str(
        "loom knowledge update entry-points \"## Section\\n\\n- path/file.rs - description\"\n",
    );
    content.push_str("loom knowledge update patterns \"## Pattern Name\\n\\n- How it works\"\n");
    content.push_str("loom knowledge update conventions \"## Convention\\n\\n- Details\"\n");
    content.push_str("loom knowledge update mistakes \"## What happened\\n\\n- Details\"\n");
    content.push_str("\n# For long content, use heredoc/stdin:\n");
    content.push_str("loom knowledge update patterns - <<'EOF'\n");
    content.push_str("## Section Title\n");
    content.push_str("Content here, can be as long as needed.\n");
    content.push_str("EOF\n");
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
        // Critical: Task tool guidance
        assert!(prefix.contains("USE THE TASK TOOL"));
        assert!(prefix.contains("Task(subagent_type="));
        assert!(prefix.contains("MULTIPLE in ONE message"));
        // Critical: worktree root directory reminder for loom stage complete
        assert!(prefix.contains(
            "Before running `loom stage complete`, ensure you are at the worktree root directory"
        ));
        // Critical: specific skill examples
        assert!(prefix.contains("/auth"));
        assert!(prefix.contains("/testing"));
        assert!(prefix.contains("software-engineer"));
        // Critical: Agent Teams guidance
        assert!(prefix.contains("Agent Teams (WHEN AVAILABLE)"));
        assert!(prefix.contains("CLAUDE_CODE_EXPERIMENTAL_AGENT_TEAMS=1"));
        assert!(prefix.contains("~7x tokens"));
        assert!(prefix.contains("Shut down ALL teammates"));
        // Context recovery instructions
        assert!(prefix.contains("Context Recovery"));
        assert!(prefix.contains("loom memory list"));
        assert!(prefix.contains("handoffs"));
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
        // Critical: Task tool guidance
        assert!(prefix.contains("USE THE TASK TOOL"));
        assert!(prefix.contains("Task(subagent_type="));
        assert!(prefix.contains("MULTIPLE in ONE message"));
        // Critical: specific skill examples
        assert!(prefix.contains("/auth"));
        assert!(prefix.contains("/testing"));
        assert!(prefix.contains("Explore"));
        // Agent Teams guidance for knowledge bootstrap
        assert!(prefix.contains("Agent Teams for Knowledge Bootstrap"));
        assert!(prefix.contains("coordinated exploration"));
        assert!(prefix.contains("Architecture explorer"));
        // Context recovery instructions
        assert!(prefix.contains("Context Recovery"));
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

    #[test]
    fn test_integration_verify_stable_prefix_contains_required_sections() {
        let prefix = generate_integration_verify_stable_prefix();

        // Integration-verify specific context
        assert!(prefix.contains("## Integration Verification Context"));
        assert!(prefix.contains("FINAL QUALITY GATE"));

        // Zero tolerance emphasis - the key differentiator
        assert!(prefix.contains("ZERO TOLERANCE"));
        assert!(prefix.contains("ALL"));
        assert!(prefix.contains("NOTHING"));
        assert!(prefix.contains("pre-existing"));
        assert!(prefix.contains("too trivial"));

        // Code review content (merged from code-review prefix)
        assert!(prefix.contains("REVIEW"));
        assert!(prefix.contains("security-engineer"));
        assert!(prefix.contains("senior-software-engineer"));
        assert!(prefix.contains("CODE REVIEW + VERIFICATION EXECUTION STRATEGY"));

        // Worktree isolation
        assert!(prefix.contains("Isolation Boundaries"));
        assert!(prefix.contains("Path Boundaries"));
        assert!(prefix.contains("CONFINED"));

        // Execution rules
        assert!(prefix.contains("## Execution Rules"));
        assert!(prefix.contains("git add <specific-files>"));

        // Task tool guidance
        assert!(prefix.contains("USE THE TASK TOOL"));
        assert!(prefix.contains("Task(subagent_type="));

        // Can update knowledge
        assert!(prefix.contains("CAN UPDATE KNOWLEDGE"));
        assert!(prefix.contains("loom memory show --all"));
        assert!(prefix.contains("loom knowledge update"));

        // Worktree root directory reminder
        assert!(prefix.contains(
            "Before running `loom stage complete`, ensure you are at the worktree root directory"
        ));
        // Agent Teams guidance for integration verification (now includes review dimensions)
        assert!(prefix.contains("Agent Teams for Integration Verification"));
        assert!(prefix.contains("multi-dimension"));
        assert!(prefix.contains("Build/test teammate"));
        assert!(prefix.contains("Security review teammate"));
        // Context recovery instructions
        assert!(prefix.contains("Context Recovery"));
    }

    #[test]
    fn test_integration_verify_stable_prefix_is_stable() {
        let prefix1 = generate_integration_verify_stable_prefix();
        let prefix2 = generate_integration_verify_stable_prefix();
        assert_eq!(
            prefix1, prefix2,
            "Integration-verify stable prefix should be deterministic"
        );
    }
}
