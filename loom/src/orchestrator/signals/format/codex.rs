//! Codex implementer guidance for stage signals.

use crate::codex::{
    CodexLane, CODEX_FORWARD_SENTINEL, CODEX_IMPLEMENTER_EFFORT, CODEX_IMPLEMENTER_MODEL_LUNA,
    CODEX_IMPLEMENTER_MODEL_SOL, CODEX_IMPLEMENTER_MODEL_TERRA,
};
use crate::models::stage::Implementers;

/// Format the codex-implementer doctrine block.
///
/// Emitted for any stage whose licensed lanes include [`Implementer::Codex`] —
/// gated on [`Implementers::includes_codex`], NOT on where codex sits in the
/// list. A stage that spawns even one codex subagent needs the blast-radius
/// rules below, so a mixed stage carries them exactly as a codex-only stage
/// does. The models and effort are interpolated from [`CODEX_IMPLEMENTER_MODEL_TERRA`],
/// [`CODEX_IMPLEMENTER_MODEL_SOL`], [`CODEX_IMPLEMENTER_MODEL_LUNA`], and [`CODEX_IMPLEMENTER_EFFORT`] rather than
/// repeated as literals — one source of truth for the lane's settings.
///
/// `lane` is [`crate::codex::codex_lane`] evaluated once by the caller and selects
/// one of three texts. [`CodexLane::Legacy`] keeps the forwarder spawn rules.
/// [`CodexLane::Native`] tells the main agent to launch workers with
/// `loom subagents start`. [`CodexLane::Unavailable`] replaces the doctrine by a
/// short fallback block that forbids spawning `loom-codex-forwarder`, routes the
/// codex tiers' work to sonnet and states the reason - the lane being licensed
/// in the plan does not mean it is usable on the machine actually running it.
pub(crate) fn format_codex_implementers_section(
    implementers: &Implementers,
    lane: &CodexLane,
) -> String {
    let mut content = String::new();
    content.push_str("## Codex Implementers\n\n");

    match lane {
        CodexLane::Unavailable(reason) => push_codex_unavailable_fallback(&mut content, reason),
        CodexLane::Native => codex_native::push_native_doctrine(&mut content, implementers),
        CodexLane::Legacy => {
            push_codex_intro(&mut content, implementers);
            push_codex_spawn_rules(&mut content);
            push_codex_prompt_rules(&mut content);
            push_codex_blast_radius_and_evidence(&mut content);
        }
    }

    content
}

/// Push the fallback doctrine used when the codex lane is licensed but unavailable.
fn push_codex_unavailable_fallback(content: &mut String, reason: &str) {
    content.push_str(&format!(
        "This stage lists codex in `implementers`, but the lane is UNAVAILABLE on this machine.\n\
         Do NOT spawn `loom-codex-forwarder`; route {CODEX_IMPLEMENTER_MODEL_TERRA}- and\n\
         {CODEX_IMPLEMENTER_MODEL_SOL}-tier work (implementation, integration tests, substantial units) to\n\
         sonnet (`loom-software-engineer`) and boilerplate, scaffolding and simple unit tests\n\
         to haiku (`loom-software-developer`).\n"
    ));
    if !reason.is_empty() {
        content.push_str(&format!("Reason: {reason}\n"));
    }
}

/// Push the licensed-lanes summary and the always-applies judgment/verification paragraph.
fn push_codex_intro(content: &mut String, implementers: &Implementers) {
    content.push_str(&format!(
        "Implementation lanes licensed for this stage: {implementers}.\n"
    ));
    if implementers.is_mixed() {
        content.push_str(
            "This stage MIXES lanes. Choose the lane PER SUBAGENT, not once for the whole stage -\n\
             list order means nothing. Codex takes straightforward, well-specified pieces when its\n\
             quota has room (terra: implementation, integration tests, boilerplate, scaffolding, simple\n\
             unit tests; sol: a substantial well-specified unit or a terra unit that failed once; luna:\n\
             only when the user or plan asks); Claude takes the rest.\n\
             Codex for one file set and loom-software-engineer (sonnet) for another in one stage is\n\
             the intended shape, not a contradiction.\n\
             Codex may take a bounded exploratory piece: one that explores an unfamiliar path inside the\n\
             files it owns, using the navigation kit, with pinned interfaces, an equivalent sandbox and\n\
             mechanical acceptance. Open-ended exploration across modules, unknown-cause debugging,\n\
             failing-test convergence, verification, review and planning stay with Claude.\n",
        );
    } else {
        content.push_str(
            "Codex takes this stage's straightforward, well-specified pieces: terra implementation, integration\n\
             tests, boilerplate, scaffolding and simple unit tests; sol a substantial well-specified unit\n\
             or a terra unit that failed once; luna only when the user or plan asks. The Claude\n\
             escalation paths below were never implementation lanes and still apply.\n\
             Codex may take a bounded exploratory piece: one that explores an unfamiliar path inside the\n\
             files it owns, using the navigation kit, with pinned interfaces, an equivalent sandbox and\n\
             mechanical acceptance. Open-ended exploration across modules, unknown-cause debugging,\n\
             failing-test convergence, verification, review and planning stay with Claude.\n",
        );
    }
    content.push_str(
        "Regardless of the list: YOU, the stage's main agent, keep the decisions and route\n\
         architecture implementation to opus, loom-advisor (fable) diagnoses after an opus failure at effort high,\n\
         and verification never moves to codex - see below.\n\n",
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
        "- State the model/effort IN THE PROMPT, one form per tier:\n",
        "  \"--model {CODEX_IMPLEMENTER_MODEL_TERRA} --effort {CODEX_IMPLEMENTER_EFFORT} <task>\" for implementation, integration tests,\n",
        "  boilerplate, scaffolding and simple unit tests;\n",
        "  \"--model {CODEX_IMPLEMENTER_MODEL_SOL} --effort {CODEX_IMPLEMENTER_EFFORT} <task>\" for a substantial well-specified unit\n",
        "  (about 4 to 6 file-sized changes) or a terra unit that failed once;\n",
        "  \"--model {CODEX_IMPLEMENTER_MODEL_LUNA} --effort {CODEX_IMPLEMENTER_EFFORT} <task>\" only when the user or plan asks.\n",
        "  Give every logical Codex unit a stable unique id\n",
        "  matching `[A-Za-z0-9][A-Za-z0-9._-]{{0,63}}` and state `--unit-id <unit>` in the prompt. The forwarder\n",
        "  (haiku, about 20,000 tokens per job) makes ONE foreground Bash call with timeout 1560000 ms and exact shape\n",
        "  `~/.claude/hooks/loom/codex-forward.sh task '<task>' --model <m> --effort <e> --write --unit-id <unit>`.\n",
        "  It passes the unit verbatim and never supplies `--invocation-id`; the guard injects it.\n",
    ),
        CODEX_IMPLEMENTER_MODEL_TERRA = CODEX_IMPLEMENTER_MODEL_TERRA,
        CODEX_IMPLEMENTER_MODEL_SOL = CODEX_IMPLEMENTER_MODEL_SOL,
        CODEX_IMPLEMENTER_EFFORT = CODEX_IMPLEMENTER_EFFORT,
        CODEX_IMPLEMENTER_MODEL_LUNA = CODEX_IMPLEMENTER_MODEL_LUNA,
    ));
    content.push_str(concat!(
        "- A codex job is ONE CONCERN: code, consumers and tests together in dependency order, in one brief.\n",
        "  It may span several files; two jobs never write one file. Size per file-sized change at high effort:\n",
        "  gpt-6.2-sol takes about 3 minutes and 50,000 tokens of context; gpt-6.1-terra about 1 minute and\n",
        "  20,000 tokens. Each job starts near 23,000 tokens. Budget twice the time at xhigh.\n",
        "  Keep peak context at most about 250,000 tokens and finish well inside the 1500000 ms deadline:\n",
        "  about 4 to 6 file-sized changes on sol, about 12 on terra. The wrapper cancels at the deadline\n",
        "  and exits 124 timed_out. A timed-out job is NEVER re-forwarded as is: re-split against its partial\n",
        "  tree, giving each part its own id. A retry of the same logical job keeps the SAME unit id.\n",
        "- Fill only cohesive jobs: every turn re-reads the whole context, so cost grows with its length\n",
        "  squared. Four jobs of six changes cost about 1.8 times the Codex tokens of 24 single-change jobs.\n",
        "  Claude quota is scarcer: a haiku forwarder costs about 20,000 tokens per job plus orchestrator\n",
        "  turns, which can justify cohesive bundles. Never bundle unrelated concerns to reach a size.\n",
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
    content.push_str(concat!(
        "- WRITE THE PROMPT LIKE A SONNET ONE, PLUS ANCHORS: write each job brief to a file naming files\n",
        "  owned/read, entry symbols, the done condition and proof command, and traps. Forward ONE line\n",
        "  naming its absolute path; the forwarder never retypes the brief. Without a committed brief,\n",
        "  write one under the worktree scratch directory first. The apostrophe, complete-substring and\n",
        "  loom stage restrictions apply to that one line. Declared skills are for Claude workers: quote\n",
        "  applicable rules in the Codex brief. The wrapper sets LOOM_CODEX_FORWARD=1: loom's\n",
        "  context-injecting hooks stay silent while guard hooks still run. Never prepend the Claude preamble.\n",
        "- Waves follow real dependencies only: order work inside a job first; a later wave exists only\n",
        "  when a job needs another job result on disk. Never slice jobs into waves by the cap of 6.\n",
    ));
    content.push_str(
        "- loom-codex-forwarder forwards with --write by default. PARALLEL FAN-OUT: run up to 6 at\n\
         once, each owning a DISJOINT file set, in the same file-ownership table as any sonnet\n\
         subagents in the wave.\n",
    );
    content.push_str(concat!(
        "- MIXED FAN-OUT: codex and Claude subagents may share a wave - file ownership keeps them\n",
        "  apart, enforced across lanes just as within one. FOREGROUND ONLY, and skip `--resume-last`:\n",
        "  the wrapper waits at most 1500000 ms for one exact status snapshot, then cancels a still-running\n",
        "  job and exits 124 with `\"outcome\":\"timed_out\"` - the unit was too large; re-split it. Start ONE\n",
        "  background `loom subagents watch`, naming one `--worker codex:<unit-id>` for each forwarded\n",
        "  unit and passing `--timeout 3600`; never use a model-driven status loop, a\n",
        "  second forward, or `codex-companion.mjs status --all`. Retrying a logical unit means a fresh\n",
        "  forwarder spawn with the SAME unit id; the guard mints a fresh invocation, so it cannot revive\n",
        "  the previous job. The watch binds those workers once, holds one lease for the parent session,\n",
        "  prints one initial record and one terminal record, then exits. Treat exits distinctly: 0 only\n",
    ));
    push_watch_exit_meanings(content);
    content.push_str(concat!(
        "  A foreground run is one long Bash call - no PostToolUse fires, so the daemon's \"appears hung\"\n",
        "  warning past 300s is ADVISORY ONLY.\n",
    ));
}

/// Push the exit-code meanings of `loom subagents watch`, shared by the legacy and native text.
fn push_watch_exit_meanings(content: &mut String) {
    content.push_str(concat!(
        "  when every bound worker has fresh, correlated success evidence; 2 when the wait deadline\n",
        "  passed (not proof any worker died); 3 when a bound worker failed or was cancelled; 4 when a\n",
        "  wait for this parent session already exists (`AlreadyWaiting` for the same worker set or `Busy`\n",
        "  for a different set), with no second monitor started; 5 when worker identity or terminal\n",
        "  evidence is unknown, which is never success; 6 when a bound worker is hung: a Claude worker\n",
        "  with no transcript growth past the stall budget (`--stall-secs`, default the stage's\n",
        "  `subagent_timeout_secs`, else 600 s), or a codex job whose process is gone or whose log stopped\n",
        "  growing. On 6: `TaskStop` the Claude worker (a cancelled codex job needs nothing), confirm it\n",
        "  stopped, re-delegate the remainder in a smaller brief.\n",
    ));
}

/// Push the blast-radius, lane-scope, evidence, and verification-ownership rules.
fn push_codex_blast_radius_and_evidence(content: &mut String) {
    content.push_str(
        "- BLAST RADIUS: codex runs approval `never` - inside its own `workspace-write` sandbox where the\n\
         stage sandbox lets it nest one, or with `--sandbox danger-full-access` where it does not\n\
         (a nested bubblewrap or Seatbelt is refused, so the wrapper falls back to a direct `codex exec`) - and either\n\
         way it edits anything under the git root (the worktree) without asking, and loom's PreToolUse\n\
         hooks never see commands it runs in its own session. NEVER give it a path under `.loom/work/` (a\n\
         symlink to state shared with every parallel stage); tell it not to run git at all; check\n\
         `git status --short` after each run - anything touched outside its files is yours to catch.\n",
    );
    push_codex_scope_bullet(content);
    content.push_str(concat!(
        "- ACCEPT A REPORT ONLY WITH EVIDENCE: the report is the forwarder's FINAL MESSAGE, and a\n",
        "  completed forward has wrapper-owned `LOOM-FORWARD-START` and `LOOM-FORWARD-END` markers, then\n",
        "  `--- LOOM-FORWARD-OUTPUT ---`, then a \"--- LOOM-CODEX-EVIDENCE ---\" trailer carrying `exit:`\n",
        "  and `mode:`. In `mode: companion`, the trailer orders `job:`, then `unit:`, then `invocation:`,\n",
        "  then `record:`. Accept only when the named record has completed status with \"phase\":\"done\",\n",
        "  the trailer unit equals the unit you assigned, and its invocation matches that job's authorization.\n",
        "  A report with `state: timed_out` and an END marker carrying `\"outcome\":\"timed_out\"` means the\n",
        "  wrapper cancelled the job at the 1500000 ms deadline: the unit was too large; re-split it. In\n",
        "  `mode: direct (...)` (where the stage sandbox refuses nesting), `thread:` names one exact thread;\n",
        "  its START is printed when Codex reports the thread, its END only when the thread finishes.\n",
        "  A direct `state: unknown` (END outcome `unknown`, exit 125) is never success: retained ownership\n",
        "  means a Codex descendant may still be writing. Do not re-forward that unit until\n",
        "  `git status --short` and the unit's files show the tree has settled.\n",
        "  No matching markers, trailer, exact record, or exact thread leaves the forward state unresolved\n",
        "  for orchestrator review.\n",
    ));
    push_codex_verification_bullet(content);
}

/// Push the lane-scope bullet, shared by the legacy and native text.
fn push_codex_scope_bullet(content: &mut String) {
    content.push_str(
        "- WHAT CODEX IS FOR: terra takes implementation, integration tests, boilerplate, scaffolding and\n\
         simple unit tests; sol takes substantial units and failed terra units; luna only on request.\n\
         Codex takes these pieces when the stage licenses it, it is available and quota has room. Not\n\
         opus work (architecture across modules, new algorithms, concurrency, security-sensitive\n\
         code), fable work (visual/UI design, hard algorithmic design), or loom-advisor's\n\
         role after an opus failure - route by what the task needs, not by what the stage lists.\n",
    );
}

/// Push the verification-ownership bullet, shared by the legacy and native text.
fn push_codex_verification_bullet(content: &mut String) {
    content.push_str(
        "- VERIFICATION NEVER GOES TO CODEX: codex subagents implement and report, never verify,\n\
         commit, or run `loom stage complete`. The gate is the orchestrator's\n\
         single `loom check` (integration-verify keeps its own gate); YOU own the six-dimension review\n\
         and commit at the end of the stage through `loom stage commit` - never take a codex\n\
         agent's word its own work is correct, and never have codex review its own output.\n\n",
    );
}

#[cfg(test)]
mod tests {
    use super::format_codex_implementers_section;
    use crate::codex::CodexLane;
    use crate::models::stage::{Implementer, Implementers};

    #[test]
    fn codex_doctrine_requires_exact_lifecycle_recovery() {
        let implementers = Implementers::new(vec![Implementer::Codex]);
        let rendered = format_codex_implementers_section(&implementers, &CodexLane::Legacy);
        let collapsed = rendered.split_whitespace().collect::<Vec<_>>().join(" ");

        assert!(collapsed.contains("[A-Za-z0-9][A-Za-z0-9._-]{0,63}"));
        assert!(collapsed.contains("--write --unit-id <unit>"));
        assert!(collapsed.contains("SAME unit id"));
        assert!(collapsed.contains("Start ONE background `loom subagents watch`"));
        assert!(collapsed.contains("one `--worker codex:<unit-id>` for each forwarded unit"));
        assert!(collapsed.contains("passing `--timeout 3600`"));
        assert!(collapsed.contains(
            "then cancels a still-running job and exits 124 with `\"outcome\":\"timed_out\"`"
        ));
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
        assert!(collapsed.contains("6 when a bound worker is hung"));
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

    #[test]
    fn unavailable_text_appends_the_reason() {
        let implementers = Implementers::new(vec![Implementer::Codex]);
        let lane = CodexLane::Unavailable("no qualified sandbox".to_string());
        let rendered = format_codex_implementers_section(&implementers, &lane);

        let unavailable_at = rendered.find("UNAVAILABLE").expect("names UNAVAILABLE");
        let reason_at = rendered
            .find("Reason: no qualified sandbox\n")
            .expect("carries the reason");
        assert!(unavailable_at < reason_at);
        assert!(rendered.contains("loom-software-engineer"));
        assert!(!rendered.contains("loom subagents start"));
        assert!(!rendered.contains("subagent_type"));
    }

    #[test]
    fn unavailable_text_with_empty_reason_has_no_reason_line() {
        let implementers = Implementers::new(vec![Implementer::Codex]);
        let lane = CodexLane::Unavailable(String::new());
        let rendered = format_codex_implementers_section(&implementers, &lane);

        assert!(rendered.contains("UNAVAILABLE"));
        assert!(!rendered.contains("Reason:"));
    }
}

#[path = "codex_native.rs"]
mod codex_native;

#[cfg(all(test, unix))]
#[path = "tests_codex_wrapper.rs"]
mod tests_codex_wrapper;
