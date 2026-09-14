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
//! carried by `loom-hooks/codex-forward.sh` itself, prepended to every forwarded
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
    content.push_str(&format!(concat!(
        "- State the model/effort IN THE PROMPT: \"--model {CODEX_IMPLEMENTER_MODEL_TERRA} --effort\n",
        "  {CODEX_IMPLEMENTER_EFFORT} <task>\" for common implementation/integration tests, or\n",
        "  \"--model {CODEX_IMPLEMENTER_MODEL_LUNA} --effort {CODEX_IMPLEMENTER_EFFORT} <task>\" for\n",
        "  boilerplate/scaffolding/simple unit tests. Give every logical Codex unit a stable unique id\n",
        "  matching `[A-Za-z0-9][A-Za-z0-9._-]{{0,63}}` and state `--unit-id <unit>` in the prompt. The forwarder\n",
        "  makes ONE foreground Bash call with timeout 600000 ms and exact shape\n",
        "  `~/.claude/hooks/loom/codex-forward.sh task '<task>' --model <m> --effort <e> --write --unit-id <unit>`.\n",
        "  It passes the unit verbatim and never supplies `--invocation-id`; the guard injects it.\n",
    ),
        CODEX_IMPLEMENTER_MODEL_TERRA = CODEX_IMPLEMENTER_MODEL_TERRA,
        CODEX_IMPLEMENTER_EFFORT = CODEX_IMPLEMENTER_EFFORT,
        CODEX_IMPLEMENTER_MODEL_LUNA = CODEX_IMPLEMENTER_MODEL_LUNA,
    ));
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
         prompt - codex never reads CLAUDE.md, and `loom-hooks/codex-forward.sh` already prepends its own\n\
         rules to every forwarded prompt.\n",
    );
    content.push_str(
        "- loom-codex-forwarder forwards with --write by default. PARALLEL FAN-OUT: run up to 6 at\n\
         once, each owning a DISJOINT file set, in the same file-ownership table as any sonnet\n\
         subagents in the wave.\n",
    );
    content.push_str(concat!(
        "- MIXED FAN-OUT: codex and Claude subagents may share a wave - file ownership keeps them\n",
        "  apart, enforced across lanes just as within one. FOREGROUND ONLY, and skip `--resume-last`:\n",
        "  the wrapper waits at most 540000 ms for one exact status snapshot. A still-running job reports\n",
        "  `state: active` with no `LOOM-FORWARD-END` and remains under daemon ownership. Start ONE\n",
        "  background `loom subagents watch`, naming one `--worker codex:<unit-id>` for each forwarded\n",
        "  unit and passing `--timeout 3600`; never use a model-driven status loop, a\n",
        "  second forward, or `codex-companion.mjs status --all`. Retrying a logical unit means a fresh\n",
        "  forwarder spawn with the SAME unit id; the guard mints a fresh invocation, so it cannot revive\n",
        "  the previous job. The watch binds those workers once, holds one lease for the parent session,\n",
        "  prints one initial record and one terminal record, then exits. Treat exits distinctly: 0 only\n",
        "  when every bound worker has fresh, correlated success evidence; 2 when the wait deadline\n",
        "  passed (not proof any worker died); 3 when a bound worker failed or was cancelled; 4 when a\n",
        "  wait for this parent session already exists (`AlreadyWaiting` for the same worker set or `Busy`\n",
        "  for a different set), with no second monitor started; 5 when worker identity or terminal\n",
        "  evidence is unknown, which is never success.\n",
        "  A foreground run is one long Bash call - no PostToolUse fires, so the daemon's \"appears hung\"\n",
        "  warning past 300s is ADVISORY ONLY.\n",
    ));
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
    content.push_str(concat!(
        "- ACCEPT A REPORT ONLY WITH EVIDENCE: the report is the forwarder's FINAL MESSAGE, and a\n",
        "  completed forward has wrapper-owned `LOOM-FORWARD-START` and `LOOM-FORWARD-END` markers, then\n",
        "  `--- LOOM-FORWARD-OUTPUT ---`, then a \"--- LOOM-CODEX-EVIDENCE ---\" trailer carrying `exit:`\n",
        "  and `mode:`. In `mode: companion`, the trailer orders `job:`, then `unit:`, then `invocation:`,\n",
        "  then `record:`. Accept only when the named record has completed status with \"phase\":\"done\",\n",
        "  the trailer unit equals the unit you assigned, and its invocation matches that job's authorization.\n",
        "  An active report deliberately has `state: active` and no `LOOM-FORWARD-END`. In `mode: direct (...)`\n",
        "  (macOS inside the stage sandbox), `thread:` names one exact thread.\n",
        "  No matching markers, trailer, exact record, or exact thread leaves the forward state unresolved\n",
        "  for orchestrator review.\n",
    ));
    content.push_str(
        "- VERIFICATION STAYS WITH YOU (opus): codex subagents implement and report, never verify,\n\
         commit, or run `loom stage complete`. YOU run the full build/test/lint gate and the\n\
         six-dimension review, then commit at the end of the stage - never take a codex agent's word\n\
         its own work is correct, and never have codex review its own output.\n\n",
    );
}

#[cfg(test)]
mod tests {
    use super::format_codex_implementers_section;
    use crate::models::stage::{Implementer, Implementers};

    #[test]
    fn codex_doctrine_requires_exact_lifecycle_recovery() {
        let implementers = Implementers::new(vec![Implementer::Codex]);
        let rendered = format_codex_implementers_section(&implementers, true);
        let collapsed = rendered.split_whitespace().collect::<Vec<_>>().join(" ");

        assert!(collapsed.contains("[A-Za-z0-9][A-Za-z0-9._-]{0,63}"));
        assert!(collapsed.contains("--write --unit-id <unit>"));
        assert!(collapsed.contains("SAME unit id"));
        assert!(collapsed.contains("Start ONE background `loom subagents watch`"));
        assert!(collapsed.contains("one `--worker codex:<unit-id>` for each forwarded unit"));
        assert!(collapsed.contains("passing `--timeout 3600`"));
        assert!(collapsed.contains("`state: active` with no `LOOM-FORWARD-END`"));
        assert!(collapsed.contains("holds one lease for the parent session"));
        assert!(collapsed.contains("one initial record and one terminal record"));
        assert!(collapsed
            .contains("0 only when every bound worker has fresh, correlated success evidence; 2"));
        assert!(collapsed.contains("3 when a bound worker failed or was cancelled; 4"));
        assert!(collapsed
            .contains("`AlreadyWaiting` for the same worker set or `Busy` for a different set"));
        assert!(collapsed.contains(
            "5 when worker identity or terminal evidence is unknown, which is never success"
        ));
        assert!(collapsed.contains("\"phase\":\"done\""));
        assert!(collapsed.contains("`codex-companion.mjs status --all`"));

        for forbidden in [
            "loom subagents wait --receipt",
            "Recover with `codex-companion.mjs status",
            "run `codex-companion.mjs status --all`",
            "Recover with `--resume-last`",
            "Use `--resume-last` to recover",
        ] {
            assert!(
                !collapsed.contains(forbidden),
                "codex doctrine must not recover via {forbidden:?}"
            );
        }
    }
}
