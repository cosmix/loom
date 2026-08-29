//! The codex-implementer doctrine block.
//!
//! Split out of `sections.rs` as a pure move: `sections.rs` was growing past
//! its maintainability-ledger line count, and this function's generated-prose
//! doctrine text is the natural thing to lift out on its own. The doctrine
//! text itself is pinned byte-for-byte by `tests_doctrine.rs` and
//! `tests_cache.rs` and must not change here.

use crate::codex::{
    CODEX_FORWARD_SENTINEL, CODEX_IMPLEMENTER_EFFORT, CODEX_IMPLEMENTER_MODEL_LUNA,
    CODEX_IMPLEMENTER_MODEL_TERRA,
};
use crate::models::stage::Implementers;

/// Format the codex-implementer doctrine block.
///
/// Emitted for any stage whose licensed lanes include [`Implementer::Codex`] —
/// gated on [`Implementers::includes_codex`], NOT on codex being the preferred
/// lane. A stage that spawns even one codex subagent needs the blast-radius
/// rules below, so a mixed stage carries them exactly as a codex-first stage
/// does. The models and effort are interpolated from [`CODEX_IMPLEMENTER_MODEL_TERRA`],
/// [`CODEX_IMPLEMENTER_MODEL_LUNA`], and [`CODEX_IMPLEMENTER_EFFORT`] rather than
/// repeated as literals — one source of truth for the lane's settings.
///
/// `codex_available` is [`crate::codex::codex_lane_available`] evaluated by the
/// caller: when the codex CLI or its plugin's companion runtime is missing on
/// this machine, the full doctrine below is replaced by a short fallback block
/// that forbids spawning `loom-codex-forwarder` and routes the codex tiers'
/// work to sonnet instead - the lane being licensed in the plan does not mean
/// it is installed on the machine actually running it.
pub(crate) fn format_codex_implementers_section(
    implementers: &Implementers,
    codex_available: bool,
) -> String {
    let mut content = String::new();

    content.push_str("## Codex Implementers\n\n");

    if !codex_available {
        content.push_str(&format!(
            "This stage lists codex in `implementers`, but the codex lane is UNAVAILABLE on this\n\
             machine - the codex CLI or its plugin's companion runtime is not installed. Do NOT\n\
             spawn `loom-codex-forwarder`. Route the codex tiers' work to sonnet\n\
             (`loom-software-engineer`) instead: {CODEX_IMPLEMENTER_MODEL_TERRA}'s tier (common\n\
             implementation, integration tests) and {CODEX_IMPLEMENTER_MODEL_LUNA}'s tier\n\
             (boilerplate, scaffolding, simple unit tests) alike. Every other rule of this signal\n\
             is unchanged.\n"
        ));
        return content;
    }

    content.push_str(&format!(
        "Implementation lanes licensed for this stage: {implementers}.\n"
    ));

    // The lane list is a per-SUBAGENT choice, not a per-stage mode. Say so
    // explicitly: the failure this wording exists to prevent is an orchestrator
    // reading one listed lane as "every subagent must be codex".
    if implementers.is_mixed() {
        content.push_str(&format!(
            "This stage MIXES lanes. Choose the lane PER SUBAGENT, not once for the whole stage:\n\
             reach for {} first on its tiers' work (terra: common implementation and integration\n\
             tests; luna: boilerplate, scaffolding, simple unit tests), and use the other lane\n\
             wherever the work calls for it. A single stage spawning codex implementers for one\n\
             file set and loom-software-engineer (sonnet) subagents for another is the intended\n\
             shape, not a contradiction.\n",
            implementers.preferred()
        ));
    } else {
        content.push_str(
            "Codex is the lane for this stage's terra- and luna-tier work. The Claude escalation\n\
             paths below still apply - they are not implementation lanes and never needed listing.\n",
        );
    }
    content.push_str(
        "Regardless of the list: YOU (the orchestrator) are opus, opus keeps the work that needs\n\
         architectural judgment, and loom-advisor (fable) is always available on a second failure.\n\
         Verification does NOT move - see below.\n\n",
    );
    content.push_str(
        "- Spawn codex implementation work with the Agent tool, subagent_type: \"loom-codex-forwarder\" -\n\
         loom's own forwarding shim. Do NOT spawn the plugin's codex:codex-rescue directly: plugin\n\
         agents' tools restriction is ignored by design, and such an unrestricted wrapper has been\n\
         observed implementing the task itself on sonnet instead of forwarding - a silent lane downgrade.\n",
    );
    content.push_str(&format!(
        "- THE FIRST LINE of every codex prompt is exactly \"{CODEX_FORWARD_SENTINEL}\". The\n\
         codex-forward-guard hook keys on that token to block a forwarder that reads or edits instead\n\
         of forwarding. Never put the token in a prompt for any other lane.\n"
    ));
    content.push_str(&format!(
        "- State the model and effort IN THE PROMPT TEXT: \"--model {CODEX_IMPLEMENTER_MODEL_TERRA} --effort {CODEX_IMPLEMENTER_EFFORT} <task>\"\n\
         for common implementation and integration tests, or \"--model {CODEX_IMPLEMENTER_MODEL_LUNA} --effort {CODEX_IMPLEMENTER_EFFORT} <task>\"\n\
         for boilerplate, scaffolding, and simple unit tests.\n"
    ));
    content.push_str("  The forwarder invokes only `~/.claude/hooks/loom/codex-forward.sh task '<task>' --model\n");
    content.push_str("  <model> --effort <effort> --write`; it single-quotes the task so shell operators remain data.\n");
    content.push_str("- STATE AN EXPLICIT BASH TIMEOUT IN THE PROMPT TEXT, e.g. \"make your single Bash call with an\n");
    content.push_str("  explicit timeout of 900000 ms\". The forwarder makes ONE Bash call and never raises the tool's\n");
    content.push_str("  120s default, so any longer codex run is backgrounded by the harness. When that happens the id\n");
    content.push_str("  the wrapper hands back is a CLAUDE CODE task id, not a codex job id - `codex-companion.mjs\n");
    content.push_str("  result <that id>` will not resolve it. Recover a stranded run with `codex-companion.mjs status\n");
    content.push_str("  --all` to get the real `task-*` id, and cancel runaways with `codex-companion.mjs cancel <id>`.\n");
    content.push_str("- CODEX CAN NAVIGATE THIS REPOSITORY - GIVE IT ANCHORS, NOT A TRANSCRIPT. Codex does not run in\n");
    content.push_str("  Claude Code and never sees its tools: the OpenAI harness is shell-based BY DESIGN, so it reads by\n");
    content.push_str("  running perl/sed and writes by applying patches. Paging files that way is slow, which is why the\n");
    content.push_str("  forwarding wrapper prepends a navigation kit to every codex prompt: `loom map --find-all`,\n");
    content.push_str("  `loom map --outline`, `loom map --impact` and `loom knowledge context --query` each answer in\n");
    content.push_str(
        "  under a second from loom's own index. They never write inside the worktree, but the cache they\n",
    );
    content.push_str(
        "  try to refresh lives outside the stage sandbox, so in a worktree they print a\n",
    );
    content.push_str(
        "  `warning: could not refresh ...` line and answer from the published base layer. That warning is\n",
    );
    content.push_str(
        "  expected: it names `Read-only file system`, so do NOT read a codex report quoting it as a sandbox\n",
    );
    content.push_str(
        "  block. Plan around the one real consequence - the index reflects the branch point, so codex\n",
    );
    content.push_str(
        "  cannot see edits made during this session. When a task depends on a file a sibling subagent just\n",
    );
    content.push_str("  changed, say so and name the file.\n");
    content.push_str("- SO WRITE A CODEX PROMPT THE WAY YOU WOULD WRITE A SONNET ONE, PLUS ANCHORS: the files it owns\n");
    content.push_str(
        "  (write) and the files it may read; the symbols and files to START FROM, by name; what done means\n",
    );
    content.push_str(
        "  and the exact command that proves it; and any constraint the graph cannot show it - an invariant,\n",
    );
    content.push_str(
        "  an ordering that matters, a trap the knowledge files record. Do NOT paste signatures, file bodies,\n",
    );
    content.push_str("  or surrounding style: it looks those up faster than you can quote them.\n");
    content.push_str(
        "- NEVER PREPEND THE CLAUDE SUBAGENT PREAMBLE TO A CODEX PROMPT. \"READ CLAUDE.md IMMEDIATELY AND\n",
    );
    content.push_str(
        "  FOLLOW ALL ITS RULES\" is addressed to Claude subagents. Codex reads AGENTS.md, never CLAUDE.md, so\n",
    );
    content.push_str(
        "  that line is the ONLY thing that sends it into the knowledge base - and it obeys by paging the\n",
    );
    content.push_str(
        "  whole corpus, which is where the ten-minute codex runs came from. The wrapper's preamble already\n",
    );
    content.push_str(
        "  carries codex's own rules: navigation, file ownership, no `.work/`, no git, no verification.\n",
    );
    content.push_str("- loom-codex-forwarder forwards with --write by default; do not ask for read-only when you want edits.\n");
    content.push_str("- PARALLEL FAN-OUT: you may run up to 6 codex implementers at once, each owning a DISJOINT file set,\n");
    content.push_str("  with the same file-ownership table you would write for sonnet subagents. Two codex agents writing\n");
    content.push_str("  one file is lost work, exactly as with any other subagent.\n");
    content.push_str("- MIXED FAN-OUT: codex and Claude subagents may run in the SAME wave. File ownership is what keeps\n");
    content.push_str("  them apart, and it is enforced across lanes, not within one - a codex agent and a sonnet agent\n");
    content.push_str("  writing one file is lost work just as surely as two codex agents. Put every subagent from every\n");
    content.push_str("  lane in ONE file-ownership table, and note each row's lane so you know which rules apply to it.\n");
    content.push_str("- Run parallel codex implementers in the FOREGROUND. Do NOT fan out --background jobs: the plugin\n");
    content.push_str("  tracks jobs in a shared state file written without a lock, and a background result is fetched\n");
    content.push_str("  through the very record a concurrent write can drop. Foreground results come back through stdout\n");
    content.push_str("  and do not depend on it.\n");
    content.push_str("- Do NOT use --resume-last under fan-out. It resolves \"the last job\" out of that same shared\n");
    content.push_str("  state file and can attach to a sibling's thread. Use fresh runs.\n");
    content.push_str("- A foreground codex run is ONE long Bash call: no PostToolUse fires, so the loom heartbeat goes\n");
    content.push_str("  stale and the daemon prints a spurious \"appears hung\" warning after 300s. That warning is\n");
    content.push_str("  ADVISORY ONLY - nothing is killed or retried. Ignore it.\n");
    content.push_str("- BLAST RADIUS. Codex runs with sandbox `workspace-write` and approval policy `never`: it edits\n");
    content.push_str("  anything under the git root without asking. In a loom worktree the git root IS the worktree,\n");
    content.push_str(
        "  so that is your isolation boundary - with two holes you must cover yourself:\n",
    );
    content.push_str("    * NEVER give a codex agent a path under `.work/`. It is a SYMLINK to orchestration state\n");
    content.push_str("      shared with every parallel stage; a write through it escapes worktree isolation and\n");
    content.push_str(
        "      corrupts other stages. `.work/` is yours via the loom CLI only (Rule 11).\n",
    );
    content.push_str("    * Loom's PreToolUse hooks (commit-filter, git-add-guard, the subagent guards) intercept\n");
    content.push_str("      CLAUDE CODE's Bash tool. They do NOT see commands codex runs inside its own session, so\n");
    content.push_str("      for codex those rules are prose, not enforcement. Tell every codex subagent it must not\n");
    content.push_str("      run git at all, and check `git status --short` after each run: anything staged, committed\n");
    content.push_str("      or touched outside that agent's assigned file set is YOUR problem to find, because no\n");
    content.push_str("      hook will.\n");
    content.push_str("- WHAT CODEX IS FOR: terra takes common implementation and integration tests (the sonnet\n");
    content.push_str("  tier); luna takes boilerplate, scaffolding, and simple unit tests. It does NOT take opus work\n");
    content.push_str("  (mainstream architecture, algorithm implementation, cross-cutting refactors, security-sensitive\n");
    content.push_str("  code), fable work (visual/UI design, a bug that survived a delegated fix attempt, extremely challenging algorithmic design), or\n");
    content.push_str("  loom-advisor's role on a second failure. Route each piece of work by what the work needs; the\n");
    content.push_str("  lane list says what is available, not what is mandatory. Sending a task to codex because the\n");
    content.push_str("  stage lists codex - rather than because the task fits a codex tier - is the misread this\n");
    content.push_str("  section exists to prevent.\n");
    content.push_str("- ACCEPT A CODEX REPORT ONLY WITH EVIDENCE. A genuine forward returns codex stdout followed by a\n");
    content.push_str("  \"--- LOOM-CODEX-EVIDENCE ---\" trailer listing companion state jobs/*.json paths. Verify the\n");
    content.push_str("  newest record for THIS worktree exists and its \"phase\" is \"done\". A report with no trailer -\n");
    content.push_str("  or edits in the tree with no matching job record - is a FAILED delegation: the wrapper did the\n");
    content.push_str("  work itself. Treat those edits as output from an unknown lane: revert and respawn the forwarder,\n");
    content.push_str(
        "  or keep them only after reviewing them as strictly as you would review sonnet output.\n",
    );
    content.push_str("- VERIFICATION STAYS WITH YOU (opus). Codex subagents implement and report; they never verify, never\n");
    content.push_str("  commit, and never run loom stage complete (Rule 5). YOU run the full build/test/lint gate, YOU run\n");
    content.push_str("  the six-dimension mini adversarial code review, and only THEN — after both — YOU commit, at the end of the stage. Never accept a codex agent's own\n");
    content.push_str(
        "  claim that its work is correct, and never have codex review its own output - use\n",
    );
    content.push_str("  loom-code-reviewer or your own reading.\n\n");

    content
}
