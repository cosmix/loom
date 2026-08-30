//! BLOCK-C: the subagent-waiting doctrine, cross-surface pinning.
//!
//! Split out of `tests_doctrine.rs` (see that file's module doc for BLOCK-A and
//! BLOCK-B) purely to keep both files under the line-count ceiling — this is
//! the same doctrine-pinning mechanism, just a third block.
//!
//! BLOCK-C is the "how do I check on a subagent" rule: it answers what to do
//! when a subagent goes quiet, keyed on the frozen `loom subagents`
//! list/harvest/watch CLI surface and the one-background-watch pattern
//! (`loom subagents watch --timeout <secs>` through the Bash tool's
//! `run_in_background`, never a re-armed foreground poll).
//!
//! It now lives on exactly ONE surface — `CLAUDE.md.template` Rule 6 — and is
//! deliberately ABSENT from every generated signal (`generate_stable_prefix`,
//! `generate_integration_verify_stable_prefix`, `generate_knowledge_stable_prefix`,
//! `generate_knowledge_distill_stable_prefix`). It used to be pushed
//! byte-identical into the signal as well, but a subagent already has
//! CLAUDE.md in its own context in the same session: a second verbatim copy
//! in the signal taught nothing new and paid its ~1KB in residency cost on
//! every fresh spawn for the stage. Dropping the signal copy is safe because
//! the doctrine now lives on exactly one surface. The CLAUDE.md.template copy
//! below is still pinned so it cannot drift or silently vanish.

use super::cache::{
    generate_integration_verify_stable_prefix, generate_knowledge_distill_stable_prefix,
    generate_knowledge_stable_prefix, generate_stable_prefix,
};

const CLAUDE_MD_TEMPLATE: &str = include_str!("../../../../CLAUDE.md.template");

/// A stable-prefix generator, named for its failure message.
type PrefixGenerator = fn() -> String;

/// BLOCK-C, verbatim. `CLAUDE.md.template` must carry this text byte for byte.
const BLOCK_C: &str = "**Checking on subagents: use `loom subagents`, never a hand-rolled poll loop.** Spawn everyone, then run ONE `loom subagents watch --timeout <secs>` (3600 is the normal value) through the Bash tool's `run_in_background`: the harness re-invokes you when it exits, and no request is made while it waits. Do not re-arm a foreground watch every few minutes, and do not poll with `git status`, `wc -l`, or `ls`. `loom subagents list`/`harvest` give a one-shot look; per-subagent state is `done`, `tool-wait`, `generating`, or `unknown`. Three cases, not one:

1. **`done` but silent** — the subagent's turn ended and its report is on disk. Harvest it and proceed immediately; a missing notification is not a missing result.
2. **`tool-wait` / `generating`** — genuinely alive. Issue another background watch; slow is not dead. The stage's `subagent_timeout_secs` is the advisory idle budget you judge death against — never a deadline on the subagent's own work, and never the `--timeout` you pass.
3. **Idle past the budget with no transcript growth** — the only case with positive evidence of death. `TaskStop` it, confirm it stopped, then RE-DELEGATE the remainder to a fresh subagent. Never absorb the work into yourself — the orchestrator decomposes, delegates, verifies, and commits; it does not implement (hard stop 6). Re-read the tree before writing the new brief: a stale brief is worse than no brief.

Elapsed time alone is still never evidence of death. Never complete the stage while any subagent is still out (Rule 4).";

#[test]
fn block_c_lives_in_claude_md_template() {
    assert!(
        CLAUDE_MD_TEMPLATE.contains(BLOCK_C),
        "CLAUDE.md.template does not carry BLOCK-C (the subagent-waiting \
         doctrine) verbatim. Expected to find:\n{BLOCK_C}"
    );
}

/// BLOCK-C reaches an agent through CLAUDE.md in the same session; a second
/// verbatim copy in the signal is pure residency cost, not redundant safety.
/// Pin its absence from every stable prefix so it cannot silently regrow.
#[test]
fn block_c_absent_from_every_stable_prefix() {
    let generators: [(&str, PrefixGenerator); 4] = [
        ("generate_stable_prefix", generate_stable_prefix),
        (
            "generate_integration_verify_stable_prefix",
            generate_integration_verify_stable_prefix,
        ),
        (
            "generate_knowledge_stable_prefix",
            generate_knowledge_stable_prefix,
        ),
        (
            "generate_knowledge_distill_stable_prefix",
            generate_knowledge_distill_stable_prefix,
        ),
    ];

    for (name, generator) in generators {
        let prefix = generator();
        assert!(
            !prefix.contains(BLOCK_C),
            "{name} must not carry BLOCK-C: the subagent-waiting doctrine \
             reaches the agent through CLAUDE.md in the same session, so a \
             second verbatim copy in the signal is pure residency cost"
        );
    }
}

/// Guards against inventing flags on the frozen `loom subagents` surface: the
/// doctrine may name only `list`, `harvest`, and `watch`.
#[test]
fn block_c_names_only_the_frozen_subagents_cli_surface() {
    assert!(BLOCK_C.contains("loom subagents watch --timeout <secs>"));
    assert!(BLOCK_C.contains("loom subagents list`/`harvest"));
    assert!(BLOCK_C.contains("`done`, `tool-wait`, `generating`, or `unknown`"));
}
