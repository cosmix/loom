//! The codex-implementer doctrine block.
//!
//! Split out of `sections.rs` as a pure move: `sections.rs` was growing past
//! its maintainability-ledger line count, and this function's generated-prose
//! doctrine text is the natural thing to lift out on its own. The doctrine
//! text is NOT pinned byte-for-byte: `tests_doctrine.rs` and `tests_cache.rs`
//! pin specific needles (the sentinel, the model/effort constants, the
//! evidence trailer, the blast-radius rules, "MIXED FAN-OUT", "PER SUBAGENT")
//! rather than the surrounding prose, so wording may be tightened as long as
//! those needles survive. Most of what this block used to spell out is now
//! carried by `hooks/codex-forward.sh` itself, prepended to every forwarded
//! prompt - this doctrine only needs to tell the orchestrator that kit exists.

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
        push_codex_unavailable_fallback(&mut content);
        return content;
    }

    push_codex_intro(&mut content, implementers);
    push_codex_spawn_rules(&mut content);
    push_codex_prompt_rules(&mut content);
    push_codex_blast_radius_and_evidence(&mut content);

    content
}

/// Push the fallback doctrine used when the codex lane is licensed but unavailable.
fn push_codex_unavailable_fallback(content: &mut String) {
    content.push_str(&format!(
        "This stage lists codex in `implementers`, but the lane is UNAVAILABLE on this machine.\n\
         Do NOT spawn `loom-codex-forwarder`; route codex-tier work to sonnet\n\
         (`loom-software-engineer`) instead - {CODEX_IMPLEMENTER_MODEL_TERRA}'s tier (common\n\
         implementation, integration tests) and {CODEX_IMPLEMENTER_MODEL_LUNA}'s tier\n\
         (boilerplate, scaffolding, simple unit tests) alike.\n"
    ));
}

/// Push the licensed-lanes summary and the always-applies judgment/verification paragraph.
fn push_codex_intro(content: &mut String, implementers: &Implementers) {
    content.push_str(&format!(
        "Implementation lanes licensed for this stage: {implementers}.\n"
    ));
    if implementers.is_mixed() {
        content.push_str(&format!(
            "This stage MIXES lanes. Choose the lane PER SUBAGENT, not once for the whole stage -\n\
             reach for {} first (terra: common implementation/integration tests; luna: boilerplate,\n\
             scaffolding, simple unit tests), and use the other lane where the work calls for it.\n\
             Codex for one file set and loom-software-engineer (sonnet) for another in one stage is\n\
             the intended shape, not a contradiction.\n",
            implementers.preferred()
        ));
    } else {
        content.push_str(
            "Codex is the lane for this stage's terra- and luna-tier work; the Claude escalation\n\
             paths below were never implementation lanes and still apply.\n",
        );
    }
    content.push_str(
        "Regardless of the list: YOU (opus) keep the work needing architectural judgment,\n\
         loom-advisor (fable) is available on a second failure, and verification never moves off\n\
         you - see below.\n\n",
    );
}

/// Push the spawn-mechanics rules: agent type, sentinel, model/effort flags, and recovery.
fn push_codex_spawn_rules(content: &mut String) {
    content.push_str(&format!(
        "- Spawn with the Agent tool, subagent_type: \"loom-codex-forwarder\" - never the plugin's\n\
         codex:codex-rescue directly (its tools restriction is ignored by design and it has been\n\
         observed implementing on sonnet instead of forwarding). THE FIRST LINE of every prompt is\n\
         exactly \"{CODEX_FORWARD_SENTINEL}\" - the codex-forward-guard hook blocks a forwarder that\n\
         reads or edits instead of forwarding; never put this token in a prompt for any other lane.\n"
    ));
    content.push_str(&format!(
        "- State the model/effort IN THE PROMPT: \"--model {CODEX_IMPLEMENTER_MODEL_TERRA} --effort\n\
         {CODEX_IMPLEMENTER_EFFORT} <task>\" for common implementation/integration tests, or\n\
         \"--model {CODEX_IMPLEMENTER_MODEL_LUNA} --effort {CODEX_IMPLEMENTER_EFFORT} <task>\" for\n\
         boilerplate/scaffolding/simple unit tests, plus an explicit Bash timeout (e.g. 900000 ms) -\n\
         the forwarder makes ONE Bash call and never raises the tool's 120s default, so a longer run\n\
         backgrounds under a CLAUDE CODE task id, not a codex job id. Recover with `codex-companion.mjs status\n"
    ));
    content.push_str(
        "  --all` for the real job id, and cancel runaways with `codex-companion.mjs cancel <id>`.\n",
    );
}

/// Push the navigation-kit, prompt-writing, and fan-out rules.
fn push_codex_prompt_rules(content: &mut String) {
    content.push_str(
        "- CODEX ALREADY CARRIES A NAVIGATION KIT: the wrapper prepends `loom map --find-all`,\n\
         `loom map --outline`, `loom map --impact`, and `loom knowledge context --query` anchors to\n\
         every prompt, plus the instruction that codex reads AGENTS.md, never CLAUDE.md or\n\
         doc/loom/knowledge/ - do not re-paste that guidance. Its one blind spot: the index reflects\n\
         the branch point, so it cannot see a sibling subagent's edits this session - name the file.\n",
    );
    content.push_str(
        "- WRITE THE PROMPT LIKE A SONNET ONE, PLUS ANCHORS: files it owns (write) and may read, the\n\
         symbols/files to start from by name, what done means and the command that proves it, and\n\
         any constraint the graph can't show - not pasted signatures or file bodies, which it looks\n\
         up faster than you can quote them. NEVER prepend the Claude subagent preamble to a codex\n\
         prompt - codex never reads CLAUDE.md, and `hooks/codex-forward.sh` already prepends its own\n\
         rules to every forwarded prompt.\n",
    );
    content.push_str(
        "- loom-codex-forwarder forwards with --write by default. PARALLEL FAN-OUT: run up to 6 at\n\
         once, each owning a DISJOINT file set, in the same file-ownership table as any sonnet\n\
         subagents in the wave.\n",
    );
    content.push_str(
        "- MIXED FAN-OUT: codex and Claude subagents may share a wave - file ownership keeps them\n\
         apart, enforced across lanes just as within one. FOREGROUND ONLY, and skip `--resume-last`:\n\
         the plugin's job-state file has no lock, so a background or resumed job can attach to a\n\
         sibling's. A foreground run is one long Bash call - no PostToolUse fires, so the daemon's\n\
         \"appears hung\" warning past 300s is ADVISORY ONLY; nothing is killed or retried.\n",
    );
}

/// Push the blast-radius, lane-scope, evidence, and verification-ownership rules.
fn push_codex_blast_radius_and_evidence(content: &mut String) {
    content.push_str(
        "- BLAST RADIUS: codex runs approval `never` - inside its own `workspace-write` sandbox where the\n\
         stage sandbox lets it nest one (Linux), or with `--sandbox danger-full-access` where it does not\n\
         (macOS refuses a nested Seatbelt, so the wrapper falls back to a direct `codex exec`) - and either\n\
         way it edits anything under the git root (the worktree) without asking, and loom's PreToolUse\n\
         hooks never see commands it runs in its own session. NEVER give it a path under `.loom/work/` (a\n\
         symlink to state shared with every parallel stage); tell it not to run git at all; check\n\
         `git status --short` after each run - anything touched outside its files is yours to catch.\n",
    );
    content.push_str(
        "- WHAT CODEX IS FOR: terra takes common implementation/integration tests (the sonnet tier);\n\
         luna takes boilerplate/scaffolding/simple unit tests. Not opus work (architecture,\n\
         algorithms, cross-cutting refactors, security-sensitive code), fable work (visual/UI\n\
         design, a bug that survived a delegated fix, hard algorithmic design), or loom-advisor's\n\
         role on a second failure - route by what the task needs, not by what the stage lists.\n",
    );
    content.push_str(
        "- ACCEPT A REPORT ONLY WITH EVIDENCE: the report is the forwarder's FINAL MESSAGE, and a\n\
         genuine forward returns codex stdout followed by a \"--- LOOM-CODEX-EVIDENCE ---\" trailer\n\
         carrying `exit:` and `mode:`. `mode: companion` lists companion state jobs/*.json paths -\n\
         verify the newest record for THIS worktree has \"phase\": \"done\". `mode: direct` (macOS\n\
         inside the stage sandbox) lists `session:`, the codex rollout under ~/.codex/sessions/ -\n\
         verify it exists and is newer than the spawn; codex's own `exec ... succeeded` lines precede\n\
         the trailer. No trailer, or edits with no matching record or rollout, is a FAILED delegation\n\
         (the wrapper did the work itself): revert and respawn, or review the edits as strictly as\n\
         sonnet output.\n",
    );
    content.push_str(
        "- VERIFICATION STAYS WITH YOU (opus): codex subagents implement and report, never verify,\n\
         commit, or run `loom stage complete`. YOU run the full build/test/lint gate and the\n\
         six-dimension review, then commit at the end of the stage - never take a codex agent's word\n\
         its own work is correct, and never have codex review its own output.\n\n",
    );
}
