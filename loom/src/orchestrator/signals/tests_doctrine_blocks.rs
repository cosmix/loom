//! The pinned doctrine literals `tests_doctrine.rs` checks every guidance
//! surface against, plus the phrase table its retirement-regression test
//! sweeps for. Split out purely to keep `tests_doctrine.rs` itself readable
//! and under its line budget - these are DATA the parent's `#[test]`
//! functions assert against, not test logic of their own. Transitively
//! `#[cfg(test)]`: the parent module is gated in `signals/mod.rs`.

/// BLOCK-A - the no-verify rule, verbatim. Every surface except the
/// integration-verify prefix must carry this text byte for byte;
/// `loom-hooks/subagent-verify-guard.sh` prefixes it with a hook-only
/// "BLOCKED" framing line, which is deliberately not part of the block.
pub(super) const BLOCK_A: &str = "VERIFICATION IS THE MAIN AGENT'S JOB - NOT YOURS:
- Compile the files YOU wrote as often as you need, and format them,
  with the project's own build and formatter commands scoped to those
  files. No linter and no formatter run over the whole project.
- Run the tests for the code YOU wrote, and its module, when you are
  confident your code is ready or your fix is correct. Never the whole
  test suite: the stage's gate runs it.
- Report instead: files changed, assumptions made, anything unresolved.
  The stage's gate compiles, tests, and lints; the MAIN AGENT fixes.";

/// BLOCK-B - the model playbook, verbatim.
///
/// A raw literal: point 4 quotes the phrase an orchestrator uses to talk
/// itself into escalating ("This feels subtle"), so it carries `"` inside.
pub(super) const BLOCK_B: &str = r#"1. DELEGATION IS A COST DECISION: TOKENS TIMES MODEL TIER (hard stop 6). A
   stage's main agent decomposes the work, briefs subagents, owns the gate and
   commits. A spawn costs a written brief, the subagent's boot (about 28,000
   tokens before it reads anything) and a harvest turn. The main agent makes a
   change itself only when ALL of these hold: at most 20 changed lines, in at
   most 2 files it has already read this session, no further exploration, and
   one command proves it. Anything larger is delegated. A main session running
   FABLE delegates even those: a cheaper tier can do them, and every fable turn
   costs more than the spawn.
2. INVESTIGATION ENDS IN A BRIEF OR IN A SMALL CHANGE. The moment you finish
   reading the code and know what the fix is, apply point 1's test. If it
   fails, you are at the delegation boundary: write the understanding down
   (file:line, root cause, the change to make, signatures, patterns to match,
   acceptance) and spawn. The diagnosis being yours does not make a large
   change yours.
3. EVERYTHING BEYOND POINT 1 IS DELEGATED, to as FEW subagents as the work
   allows, at the CHEAPEST tier that can do the piece. Size each assignment so
   the subagent typically finishes under about 400,000 tokens, and never split
   below what that needs: every extra spawn pays the boot cost again. Pick PER
   SUBAGENT by what that piece needs, never once for the whole stage, and
   default downward. HAIKU (loom-software-developer, effort high): a piece
   that is extremely simple, or whose brief is highly detailed — files,
   exact change and acceptance stated. Never haiku: an open decision, a bug
   of unknown cause, concurrency, security-sensitive code, a data-loss path. Codex gpt-6-luna takes boilerplate and simple
   unit tests only when the user or the plan asks. SONNET
   (loom-software-engineer): pieces that need exploration or the initial
   iteration on failing tests, or are too large to brief in that detail;
   codex gpt-6.1-terra where such a piece is well specified. OPUS
   (loom-senior-software-engineer, effort high; `effort: xhigh` for the
   hardest) implements where a wrong design is expensive and tests will not
   catch it: architecture, new algorithms, concurrency, distributed systems,
   security-sensitive code, bugs of unknown cause. FABLE implements only
   visual/UI design and extremely challenging algorithmic design. Codex
   tiers (effort xhigh, via loom-codex-forwarder) exist only on stages listing
   codex in implementers AND when the codex CLI + plugin are installed;
   otherwise terra-tier work goes to sonnet and luna-tier work to haiku (loom
   warns at startup when a stage lists codex it cannot use). In a standard
   stage the GATE is the orchestrator's single `loom check` (a fix round
   re-checks with `--criterion <n>`); an integration-verify stage runs the
   gate its own signal names. Either way the orchestrator owns fixes and the
   commit. Spawn BY AGENT TYPE.
4. FAILURE MOVES THE WORK UP ONE TIER. A failure misses clear acceptance:
   a failing test, a correctness finding. A vague brief, or a worker that
   stopped on an open decision, is an orchestrator failure: complete the
   brief and respawn the same tier.
   Settle every decision one lookup would answer before choosing a tier; one
   left open is no reason to pick sonnet over haiku. A real failure moves up
   one tier, haiku to sonnet to opus, with every attempt and its
   evidence in the new brief. Never skip a tier or rerun one on the same
   failure.
5. THE LADDER ENDS AT `loom-advisor` (fable, read-only), RARELY: spawn it
   only after an opus implementer failed, typically a stubborn bug, with
   every attempt. Its diagnosis goes to an opus implementer."#;

/// BLOCK-D - the subagent context-ceiling rule, verbatim. Pinned against BOTH
/// the subagent preamble (`loom-hooks/_subagent-preamble.txt`, which
/// `spawn-guard.sh` prepends to a spawn prompt) and the emitted signal
/// prefixes (`cache.rs::append_subagent_ceiling_block`) in the SAME test,
/// because a subagent's route to it cannot rely on either alone: an untyped
/// spawn never gets the preamble, so the signal-side copy the orchestrator
/// pastes is the fallback that reaches it anyway.
pub(super) const BLOCK_D: &str = "CONTEXT CEILING - HOOK-REPORTED ONLY:
- Your ceiling is roughly 800,000 tokens, the same number your orchestrator gets - reading a handful of files never gets you close.
- The hook line beginning `SUBAGENT CEILING REACHED:` in your own tool output is the sole evidence you reached it. Never estimate, infer, or assume one.
- A turn that ends with zero files written, on a task that asked for files, counts as a FAILED unit of work even if the report reads well. If genuinely blocked, name the real blocker - the hook line above is the only thing that counts as a context blocker.";

/// Phrasing RETIRED doctrines used, across the no-verify rule (BLOCK-A), the
/// model playbook (BLOCK-B) and the subagent-waiting rule (BLOCK-C). Acceptance criteria only grep for the
/// wording a doctrine INTRODUCES, so they cannot catch a surface that still
/// carries the instruction it replaced - which is exactly how the enforcement
/// layer once landed while three guidance files still told subagents to run
/// the suite. Grep for what was retired, not only for what was added.
///
/// Each phrase is assembled with `concat!` so this file never contains one as a
/// contiguous literal. The plan's own acceptance criteria grep this very
/// directory for the retired wording and must find nothing; a checker that
/// trips over its own checklist is worse than no checker. Keep them split, and
/// keep the split out of the middle of a word so the phrase stays readable.
pub(super) const RETIRED_PHRASES: &[&str] = &[
    concat!("verify your ", "subtree"),
    concat!("verifies its ", "subtree"),
    concat!("Test as ", "you go"),
    concat!("Zero IDE ", "diagnostics"),
    concat!("haiku stays rare", " and trivial"),
    concat!("haiku (rare, trivial ", "mechanical edits)"),
    // Retired: haiku was limited to renames and config values; it takes
    // extremely simple or highly detailed pieces, and luna runs only on request.
    concat!("mechanical edits such as a rename", " or a config value"),
    concat!("take the work ", "over"),
    concat!("report back ", "within"),
    concat!("hard ceiling on any", " single check"),
    // Retired with the full-suite-at-the-gate rule: a subagent runs the tests
    // for its own code and module, with no count limit.
    concat!("AT MOST ONE ", "narrowly-scoped"),
    concat!("never a repeated or ", "looping check"),
    // BLOCK-C: superseded by `loom subagents watch`'s own deadline/state.
    concat!("a check firing is NOT", " a deadline"),
    concat!("MUST carry a liveness", " signal"),
    // Retired: CLAUDE.md is already in the subagent's context, so ordering it
    // to re-read the file spends a read on text the agent already has.
    concat!("READ CLAUDE.md ", "IMMEDIATELY"),
    // Retired with the token ceiling: the trigger is the PostToolUse hook's
    // message, not a percentage the session watches for itself.
    concat!("if context ", "exceeds 75%"),
    // Retired with the one-background-watch rule: re-arming a foreground watch
    // is a poll loop with extra steps.
    concat!("Re-arm `watch` and keep", " waiting"),
    // Retired with the ceiling raise (both main and subagent now 800,000,
    // one fraction of the shared 1M window): the old fixed 150k figure must
    // never return to doctrine text - the ceiling is model-derived, reported
    // by the hook, never a hardcoded number a session compares itself
    // against.
    concat!("150,", "000"),
    // Retired along with the differentiated-ceiling framing this doctrine
    // briefly carried: a subagent's ceiling is NOT smaller than its parent's
    // - both are 800,000, told apart only by which hook line reports them.
    // Guards against the split creeping back in.
    concat!("600,", "000"),
    // BLOCK-B point 1's old heading, retired when delegation became a cost
    // decision: the main agent now makes a small change itself, so a surface
    // still forbidding every edit contradicts the playbook.
    concat!("THE MAIN AGENT NEVER ", "IMPLEMENTS"),
    // BLOCK-F: a needed block or a wrong check now names its command
    // (`loom stage block`, `loom stage dispute-*`); "report it as a blocker"
    // named none, and a dispute no longer "ends your turn" for adjudication.
    concat!("report a needed sandbox block", " as a blocker"),
    concat!("STOP and report it as", " a blocker"),
    concat!(
        "ends your turn and sends the stage to ",
        "adjudication; loom retires"
    ),
    // Retired when a standard stage's gate moved off the orchestrator's own
    // shell: the orchestrator owns fixes and the commit, and runs the gate
    // only as `loom check`.
    concat!("Verification NEVER ", "delegates"),
    concat!("verification never ", "delegates"),
];
