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
    content.push_str("- Each subagent MUST own exclusive files - two subagents writing the same file = LOST WORK\n");
    content
        .push_str("- 📝 **MUST record memories** — subagents MUST use `loom memory` to record:\n");
    content.push_str(
        "  - Mistakes: `loom memory note \"mistake: tried X, failed because Y, fixed by Z\"`\n",
    );
    content.push_str(
        "  - Decisions: `loom memory decision \"chose X over Y\" --context \"because Z\"`\n",
    );
    content.push_str(
        "  - Surprises: `loom memory note \"found: unexpected behavior in file:line\"`\n",
    );
    content.push_str(
        "  - Do NOT record procedural actions (\"read file\", \"ran tests\", \"spawned agents\")\n",
    );
    content.push_str(
        "- ⛔ **NEVER use auto-memory** — do NOT call Write/Edit on `~/.claude/projects/*/memory/` files. Use `loom memory` only.\n",
    );
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
    content.push_str("- Pattern: `Task(subagent_type=\"loom-senior-software-engineer\", prompt=\"...\")` - send MULTIPLE in ONE message\n");
    content.push_str("- Agents: `loom-senior-software-engineer` (DEFAULT — use for all work requiring judgment), `loom-software-engineer` (ONLY for unit tests, boilerplate, scaffolding), `Explore`\n");
    content.push_str(
        "- Skills: /loom-auth, /loom-testing, /loom-ci-cd, /loom-logging-observability\n\n",
    );
    content.push_str("- **FILE EXCLUSIVITY**: Each subagent must own exclusive write files. Overlap = lost work. List file assignments in each Task prompt.\n");
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

    content.push_str("**Self-Review Before Completion (MANDATORY):**\n\n");
    content.push_str("Before running `loom stage complete`, perform these checks:\n\n");
    content.push_str("- **Wiring Check**: Is the module imported? Is the command/endpoint/component registered? Can the user reach it?\n");
    content.push_str("- **Silent Failure Check**: Review ALL command output. Did stderr contain warnings despite exit 0?\n");
    content.push_str("  Look for: \"connection refused\", \"permission denied\", \"failed to download\", \"blocked\", \"sandbox\"\n");
    content.push_str("  If sandbox blocked something you need — STOP and report as blocker, do NOT work around silently\n");
    content.push_str("- **Code Correctness**: Error paths handled? No incomplete stubs or placeholders? Tests actually test the feature?\n");
    content.push_str("- **Integration Points**: Callbacks connected? Events published? Dependencies available?\n\n");

    content.push_str("**Stage Memory - MEMORY ONLY (MANDATORY):**\n\n");
    content.push_str("```text\n");
    content.push_str("⚠️  IMPLEMENTATION STAGES USE `loom memory` ONLY - NEVER `loom knowledge`\n");
    content.push_str("    Only integration-verify stages can curate memories into knowledge.\n");
    content.push('\n');
    content.push_str(
        "⛔  DO NOT use Claude Code's auto-memory system (~/.claude/projects/*/memory/)\n",
    );
    content.push_str("    NEVER call Write or Edit on files under ~/.claude/projects/*/memory/\n");
    content
        .push_str("    Use ONLY `loom memory` commands. Loom memory is embedded in signals and\n");
    content
        .push_str("    shared across sessions. Claude Code's auto-memory is disconnected from\n");
    content.push_str(
        "    orchestration and invisible to other stages — anything saved there is LOST.\n",
    );
    content.push_str("```\n\n");
    content.push_str("**WHEN to record (triggers — do it IMMEDIATELY, not at stage end):**\n\n");
    content.push_str("- **Mistake/error** → `loom memory note \"mistake: tried X, failed because Y, fixed by Z\"`\n");
    content.push_str("- **User correction** → `loom memory note \"mistake: user said do Y instead of X because Z\"`\n");
    content.push_str("- **Approach chosen** → `loom memory decision \"chose X over Y\" --context \"because Z\"`\n");
    content.push_str("- **Surprising discovery** → `loom memory note \"found: unexpected behavior in file:line\"`\n");
    content.push_str("- **Gotcha/trap** → `loom memory note \"gotcha: X looks like it should work but actually Y\"`\n");
    content.push_str(
        "- **File changes** → `loom memory change \"src/file.rs - what changed and why\"`\n\n",
    );
    content.push_str("**What NOT to record** (these waste memory and obscure real insights):\n\n");
    content.push_str("- Procedural narration: \"spawned 3 subagents\", \"read the config\", \"ran cargo test\"\n");
    content
        .push_str("- Obvious outcomes: \"tests passed\", \"build succeeded\", \"file created\"\n");
    content.push_str("- Task restating: repeating the assignment or acceptance criteria\n\n");
    content.push_str("- **FORBIDDEN**: `loom knowledge update` commands (ONLY for knowledge-bootstrap and integration-verify)\n");
    content.push_str("- Memory persists across sessions and is curated into knowledge during integration-verify\n\n");
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
    content.push_str("- **NOTHING** is \"pre-existing\" or \"out of scope\" - every issue is YOUR issue, treat it as relevant and critical\n");
    content
        .push_str("- **NOTHING** is \"too trivial\" - small issues compound into big problems\n\n");

    // Mission
    content.push_str("**Your Mission:**\n\n");
    content.push_str("1. **REVIEW** code for quality, security, and correctness issues\n");
    content.push_str("2. **FIX** every warning, error, and issue you encounter\n");
    content.push_str("3. **VERIFY** all acceptance criteria pass\n");
    content.push_str("4. **TEST** that the feature actually works end-to-end\n");
    content.push_str("5. **PROMOTE** valuable learnings to knowledge\n");
    content.push_str("6. **GENERATE** review document: `loom review`\n\n");

    // Code review execution strategy - detailed instructions
    content.push_str("```text\n");
    content.push_str("┌────────────────────────────────────────────────────────────────────┐\n");
    content.push_str("│  🔍 CODE REVIEW + VERIFICATION EXECUTION STRATEGY                  │\n");
    content.push_str("│                                                                    │\n");
    content.push_str("│  MUST use PARALLEL SPECIALIZED AGENTS for comprehensive review:   │\n");
    content.push_str("│                                                                    │\n");
    content.push_str("│  1. security-engineer    - OWASP Top 10, secrets, dependencies    │\n");
    content
        .push_str("│  2. loom-senior-software-engineer - Coupling, error handling, dead code│\n");
    content.push_str("│  3. Build/test/sandbox   - Full suite + stderr + sandbox verify    │\n");
    content.push_str("│  4. Functional verifier  - End-to-end test, wiring, reachability   │\n");
    content.push_str("│                                                                    │\n");
    content.push_str("│  Spawn these as PARALLEL subagents to maximize efficiency.        │\n");
    content.push_str("└────────────────────────────────────────────────────────────────────┘\n");
    content.push_str("```\n\n");

    // Detailed review dimension instructions
    content.push_str("**Review Dimension Details:**\n\n");
    content.push_str(
        "1. **security-engineer** — Check for OWASP Top 10 (injection, XSS, auth bypass). ",
    );
    content.push_str("Verify no hardcoded secrets or credentials. ");
    content.push_str("Check dependency security (known vulnerabilities). ");
    content.push_str("Validate input sanitization at boundaries. ");
    content.push_str("Review error messages for information leakage.\n\n");
    content.push_str(
        "2. **loom-senior-software-engineer** — Check code organization and module coupling. ",
    );
    content.push_str("Verify error handling is complete (no swallowed errors). ");
    content.push_str("Check for proper abstraction (not over/under-engineered). ");
    content.push_str("Verify naming conventions and code style consistency. ");
    content.push_str("Check for dead code, unused imports, unreachable paths.\n\n");
    content.push_str(
        "3. **Build/test/sandbox verifier** — Run full test suite AND read ALL stderr output. ",
    );
    content.push_str("Check for warnings even when tests pass. ");
    content.push_str("Verify no sandbox interference (blocked downloads, denied writes). ");
    content.push_str("If ANY stderr contains \"blocked\", \"denied\", \"connection refused\", ");
    content.push_str("\"failed to download\" — investigate and resolve. ");
    content.push_str("Confirm all external dependencies are actually present.\n\n");
    content.push_str("4. **Functional verifier** — Actually RUN the feature end-to-end. ");
    content.push_str("Verify output is correct (not just that it doesn't crash). ");
    content.push_str("Check wiring: is feature registered, mounted, callable? ");
    content.push_str("Test primary use case with realistic inputs.\n\n");

    // SILENT FAILURE DETECTION section
    content.push_str("**SILENT FAILURE DETECTION:**\n\n");
    content.push_str("- EXIT CODE 0 does NOT mean success\n");
    content.push_str("- Sandbox can block downloads silently (tool uses cached/stale data)\n");
    content.push_str("- MUST check stderr of ALL commands for failure indicators\n");
    content.push_str("- If sandbox blocked something needed, report as BLOCKER\n");
    content.push_str("- Verify external dependencies are present, not just referenced\n\n");

    // Agent teams for IV - changed to MUST
    content.push_str("**Agent Teams for Integration Verification:**\n\n");
    content.push_str(
        "MUST use an agent team when available for multi-dimension review and verification:\n",
    );
    content.push_str("- Security review: specific OWASP checks, dependency audit\n");
    content
        .push_str("- Architecture review: coupling analysis, pattern compliance, error handling\n");
    content.push_str("- Build/test/sandbox: full suite + stderr analysis + sandbox verification\n");
    content.push_str("- Functional verification: end-to-end feature test, wiring check\n");
    content.push_str("- Knowledge curation: memory review, insight synthesis\n");
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
    content.push_str("- Pattern: `Task(subagent_type=\"loom-senior-software-engineer\", prompt=\"...\")` - send MULTIPLE in ONE message\n");
    content.push_str("- Agents: `loom-senior-software-engineer` (DEFAULT — use for all work requiring judgment), `loom-software-engineer` (ONLY for unit tests, boilerplate, scaffolding), `Explore`\n");
    content.push_str(
        "- Skills: /loom-testing, /loom-auth, /loom-ci-cd, /loom-logging-observability\n\n",
    );
    content.push_str("- **FILE EXCLUSIVITY**: Each subagent must own exclusive write files. Overlap = lost work. List file assignments in each Task prompt.\n");

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
    content.push_str("**Knowledge Distillation (MANDATORY):**\n\n");
    content
        .push_str("Integration-verify is where ALL stage memories become permanent knowledge.\n");
    content.push_str(
        "Memories that are not distilled into knowledge are LOST when the plan completes.\n\n",
    );
    content.push_str("**CRITICAL ORDERING — Record your OWN memories FIRST, then distill:**\n\n");
    content
        .push_str("1. **RECORD your findings** — As you review code and verify, record your own\n");
    content
        .push_str("   discoveries to `loom memory` (bugs found, security issues, architectural\n");
    content.push_str(
        "   insights, test gaps). These are just as valuable as implementation memories.\n",
    );
    content.push_str("2. Read ALL stage memories (including yours): `loom memory show --all`\n");
    content.push_str("3. Review the code changes to understand what was actually built\n");
    content
        .push_str("4. **DISTILL** all memories into `loom knowledge` — synthesize insights from\n");
    content.push_str("   ALL stages (implementation AND your own verification findings):\n");
    content.push_str("   - `architecture` — new components, data flows, integration points\n");
    content.push_str("   - `entry-points` — new files, commands, endpoints added\n");
    content.push_str("   - `patterns` — patterns introduced or discovered during implementation\n");
    content.push_str(
        "   - `conventions` — coding conventions learned from user feedback or code review\n",
    );
    content.push_str("   - `mistakes` — errors made and how to avoid them\n");
    content.push_str("   - `stack` — new dependencies, tooling changes\n");
    content.push_str("   - `concerns` — tech debt introduced, known issues\n");
    content.push_str("5. DO NOT blindly copy memory entries — synthesize and curate\n");
    content.push_str("6. Generate review document: `loom review`\n\n");
    content.push_str("**IMPORTANT — Do NOT modify the project's CLAUDE.md:**\n\n");
    content.push_str("- CLAUDE.md is the user's file — loom agents must not write to it\n");
    content.push_str("- ALL system knowledge belongs in `loom knowledge update` exclusively\n");
    content.push_str(
        "- This includes architecture, conventions, best practices, and lessons learned\n\n",
    );
    content.push_str("```text\n");
    content.push_str(
        "⛔  DO NOT use Claude Code's auto-memory system (~/.claude/projects/*/memory/)\n",
    );
    content.push_str("    NEVER call Write or Edit on files under ~/.claude/projects/*/memory/\n");
    content.push_str("    Use ONLY `loom memory` commands for recording insights.\n");
    content.push_str("    Claude Code's auto-memory is disconnected from orchestration —\n");
    content.push_str("    anything saved there is INVISIBLE to other stages and will be LOST.\n");
    content.push_str("```\n\n");

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
    content.push_str(
        "Capture a thorough snapshot of the system AS IT EXISTS NOW. Implementation stages\n",
    );
    content.push_str(
        "will build on this foundation, and integration-verify will later distill new learnings.\n",
    );
    content.push_str(
        "The more complete your picture, the fewer mistakes implementation stages will make.\n\n",
    );
    content.push_str("1. **Explore** the codebase hierarchically (entry points → modules → patterns → conventions)\n");
    content.push_str(
        "2. **Document** findings using `loom knowledge update <file> <content>` commands\n",
    );
    content.push_str("3. **Backfill** any knowledge gaps — if existing knowledge files are sparse, enrich them\n");
    content.push_str("4. **Contextualize the plan** — understand what the plan intends to change and document the current state of those areas\n");
    content.push_str("5. **Verify** acceptance criteria before completing\n\n");
    content.push_str("**Do NOT modify the project's CLAUDE.md** — it is the user's file. All knowledge goes to `loom knowledge update`.\n\n");
    content.push_str("**Memory System:** In loom workspaces, use ONLY `loom memory` commands for recording insights.\n");
    content
        .push_str("Do NOT use Claude Code's auto-memory system (`~/.claude/projects/*/memory/`). ");
    content.push_str("NEVER call Write or Edit on files under `~/.claude/projects/*/memory/`. ");
    content.push_str(
        "Auto-memory is disconnected from loom orchestration — anything saved there is LOST.\n\n",
    );

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
    content.push_str("- Agents: `Explore`, `loom-senior-software-engineer` (DEFAULT for implementation), `loom-software-engineer` (ONLY for unit tests, boilerplate, scaffolding)\n");
    content.push_str(
        "- Skills: /loom-auth, /loom-testing, /loom-ci-cd, /loom-logging-observability\n\n",
    );
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
        // Subagent memory recording requirement
        assert!(prefix.contains("MUST record memories"));
        // Critical: worktree root directory reminder for loom stage complete
        assert!(prefix.contains(
            "Before running `loom stage complete`, ensure you are at the worktree root directory"
        ));
        // Critical: specific skill examples
        assert!(prefix.contains("/loom-auth"));
        assert!(prefix.contains("/loom-testing"));
        assert!(prefix.contains("loom-software-engineer"));
        // Critical: Agent Teams guidance
        assert!(prefix.contains("Agent Teams (WHEN AVAILABLE)"));
        assert!(prefix.contains("CLAUDE_CODE_EXPERIMENTAL_AGENT_TEAMS=1"));
        assert!(prefix.contains("~7x tokens"));
        assert!(prefix.contains("Shut down ALL teammates"));
        // Context recovery instructions
        assert!(prefix.contains("Context Recovery"));
        assert!(prefix.contains("loom memory list"));
        assert!(prefix.contains("handoffs"));
        // File change tracking
        assert!(prefix.contains("loom memory change"));
        // Memory quality guidance
        assert!(prefix.contains("WHEN to record"));
        assert!(prefix.contains("What NOT to record"));
        assert!(prefix.contains("Procedural narration"));
        // Self-review before completion
        assert!(prefix.contains("Self-Review Before Completion"));
        assert!(prefix.contains("Wiring Check"));
        assert!(prefix.contains("Silent Failure Check"));
        // File exclusivity guidance
        assert!(prefix.contains("FILE EXCLUSIVITY"));
        assert!(prefix.contains("exclusive"));
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
        assert!(prefix.contains("/loom-auth"));
        assert!(prefix.contains("/loom-testing"));
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
        assert!(prefix.contains("loom-senior-software-engineer"));
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

        // Knowledge distillation
        assert!(prefix.contains("Knowledge Distillation (MANDATORY)"));
        assert!(prefix.contains("loom memory show --all"));
        assert!(prefix.contains("loom knowledge update"));
        // Must not modify CLAUDE.md
        assert!(prefix.contains("Do NOT modify the project's CLAUDE.md"));

        // Worktree root directory reminder
        assert!(prefix.contains(
            "Before running `loom stage complete`, ensure you are at the worktree root directory"
        ));
        // Agent Teams guidance for integration verification (now includes review dimensions)
        assert!(prefix.contains("Agent Teams for Integration Verification"));
        assert!(prefix.contains("multi-dimension review"));
        assert!(prefix.contains("Build/test/sandbox"));
        assert!(prefix.contains("Security review"));
        // Silent failure detection
        assert!(prefix.contains("SILENT FAILURE DETECTION"));
        assert!(prefix.contains("EXIT CODE 0 does NOT mean success"));
        assert!(prefix.contains("MUST check stderr"));
        // Review dimension details
        assert!(prefix.contains("Review Dimension Details"));
        assert!(prefix.contains("OWASP Top 10"));
        // Context recovery instructions
        assert!(prefix.contains("Context Recovery"));
        // Review document generation
        assert!(prefix.contains("loom review"));
        // File exclusivity guidance (must match standard prefix)
        assert!(prefix.contains("FILE EXCLUSIVITY"));
        assert!(prefix.contains("exclusive"));
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
