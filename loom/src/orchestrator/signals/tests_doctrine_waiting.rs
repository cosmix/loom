//! BLOCK-C: the subagent-waiting doctrine, cross-surface pinning.
//!
//! Split out of `tests_doctrine.rs` (see that file's module doc for BLOCK-A and
//! BLOCK-B) purely to keep both files under the line-count ceiling — this is
//! the same doctrine-pinning mechanism, just a third block.
//!
//! BLOCK-C is the "how do I check on a subagent" rule: it answers what to do
//! when a subagent goes quiet, keyed on the frozen `loom subagents`
//! list/harvest/watch CLI surface. It lives once, built by
//! `cache::append_subagent_waiting_doctrine`, and is pushed byte-identical into
//! every stable prefix whose stage type spawns subagents (standard,
//! integration-verify, knowledge) and into `CLAUDE.md.template` Rule 6.
//! knowledge-distill spawns none (see its own stable prefix's "Work
//! single-agent — do NOT spawn subagents" section) and must never carry it.

use super::super::cache::{
    generate_integration_verify_stable_prefix, generate_knowledge_distill_stable_prefix,
    generate_knowledge_stable_prefix, generate_stable_prefix,
};

const CLAUDE_MD_TEMPLATE: &str = include_str!("../../../../CLAUDE.md.template");

/// BLOCK-C, verbatim. Every subagent-spawning stable prefix and
/// `CLAUDE.md.template` must carry this text byte for byte.
const BLOCK_C: &str = "**Checking on subagents: use `loom subagents`, never a hand-rolled poll loop.** `loom subagents watch --timeout <secs>` blocks until every subagent settles or the timeout fires, exits 0 vs. 2, and states which branch fired — that alone satisfies the bounded-check rule (deadline ≤300s, terminates on both branches, reports which fired). `loom subagents list`/`harvest` give a one-shot look; per-subagent state is `done`, `tool-wait`, `generating`, or `unknown`. Three cases, not one:

1. **`done` but silent** — the subagent's turn ended and its report is on disk. Harvest it and proceed immediately; a missing notification is not a missing result.
2. **`tool-wait` / `generating`** — genuinely alive. Re-arm `watch` and keep waiting; slow is not dead. `subagent_timeout_secs` only widens the cadence you re-arm against, never a deadline on the subagent's own work.
3. **Idle past the budget with no transcript growth** — the only case with positive evidence of death. `TaskStop` it, confirm it stopped, then RE-DELEGATE the remainder to a fresh subagent. Never absorb the work into yourself — the orchestrator decomposes, delegates, verifies, and commits; it does not implement (hard stop 6). Re-read the tree before writing the new brief: a stale brief is worse than no brief.

Elapsed time alone is still never evidence of death. Never complete the stage while any subagent is still out (Rule 4).";

#[test]
fn block_c_agrees_across_every_surface() {
    let signal_prefix = generate_stable_prefix();
    let iv_prefix = generate_integration_verify_stable_prefix();
    let knowledge_prefix = generate_knowledge_stable_prefix();

    for (label, text) in [
        ("CLAUDE.md.template", CLAUDE_MD_TEMPLATE),
        ("signal stable prefix", signal_prefix.as_str()),
        ("signal integration-verify prefix", iv_prefix.as_str()),
        ("signal knowledge prefix", knowledge_prefix.as_str()),
    ] {
        assert!(
            text.contains(BLOCK_C),
            "{label} does not carry BLOCK-C (the subagent-waiting doctrine) \
             verbatim. The rule must be byte-identical on every surface an \
             agent reads it from; reword one and you must reword all of them. \
             Expected to find:\n{BLOCK_C}"
        );
    }
}

/// knowledge-distill never spawns subagents (its own stable prefix says so
/// explicitly), so a doctrine about checking on them would be pure noise there.
#[test]
fn block_c_absent_from_knowledge_distill_prefix() {
    let prefix = generate_knowledge_distill_stable_prefix();
    assert!(
        !prefix.contains(BLOCK_C),
        "knowledge-distill spawns no subagents and must not carry BLOCK-C"
    );
}

/// Guards against inventing flags on the frozen `loom subagents` surface: the
/// doctrine may name only `list`, `harvest`, and `watch`.
#[test]
fn block_c_names_only_the_frozen_subagents_cli_surface() {
    assert!(BLOCK_C.contains("loom subagents watch --timeout <secs>"));
    assert!(BLOCK_C.contains("loom subagents list`/`harvest"));
    assert!(BLOCK_C.contains("`done`, `tool-wait`, `generating`, or `unknown`"));
}
