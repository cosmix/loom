//! BLOCK-C: the subagent-waiting doctrine, cross-surface pinning.
//!
//! Split out of `tests_doctrine.rs` (see that file's module doc for BLOCK-A and
//! BLOCK-B) purely to keep both files under the line-count ceiling — this is
//! the same doctrine-pinning mechanism, just a third block.
//!
//! BLOCK-C is the "how do I check on a subagent" rule: it answers what to do
//! when a subagent goes quiet, keyed on the frozen `loom subagents`
//! list/harvest/watch CLI surface and the one-owned-wait pattern
//! (`loom subagents watch --worker ... --timeout 3600` through the Bash tool's
//! `run_in_background`, never re-armed or polled).
//!
//! It lives on exactly ONE surface — `skills/loom-orchestration/SKILL.md`
//! Rule 6, which the stable prefix tells a stage's main agent to load first —
//! and is deliberately ABSENT from every generated signal
//! (`generate_stable_prefix`, `generate_integration_verify_stable_prefix`,
//! `generate_knowledge_stable_prefix`, `generate_knowledge_distill_stable_prefix`).
//! It used to live in `CLAUDE.md.template` Rule 6 and, before that, in the
//! signal too; both copies paid residency cost in sessions that never spawn a
//! subagent. The skill copy below is pinned so it cannot drift or vanish.

use super::cache::{
    generate_integration_verify_stable_prefix, generate_knowledge_distill_stable_prefix,
    generate_knowledge_stable_prefix, generate_stable_prefix,
};

const ORCHESTRATION_SKILL: &str = include_str!("../../../../skills/loom-orchestration/SKILL.md");

/// A stable-prefix generator, named for its failure message.
type PrefixGenerator = fn() -> String;

/// BLOCK-C, verbatim. `skills/loom-orchestration/SKILL.md` must carry this text byte for byte.
const BLOCK_C: &str = "**Checking on subagents: use one owned `loom subagents` wait, never a hand-rolled poll loop.** Spawn every worker first and capture each worker ID: the Claude agent ID from the spawn result, or the Codex unit ID you assigned with `--unit-id`. Then run ONE `loom subagents watch --worker claude:<agent-id> --worker codex:<unit-id> --timeout 3600` through the Bash tool's `run_in_background`, with one `--worker` for every worker. It binds those workers once, holds one lease for the parent session, prints one initial record and one terminal record, then exits. Treat its exit distinctly:

1. **Exit 0** — every bound worker has fresh, correlated success evidence.
2. **Exit 2** — the wait deadline passed. This is not proof that any worker died.
3. **Exit 3** — a bound worker failed or was cancelled.
4. **Exit 4** — a wait for this parent session already exists: `AlreadyWaiting` for the same worker set or `Busy` for a different set. No second monitor was started.
5. **Exit 5** — worker identity or terminal evidence is unknown. This is never success.
6. **Exit 6** — a bound worker is hung: no transcript growth past the stall budget (`--stall-secs`, default the stage's `subagent_timeout_secs`, else 600 s) for a Claude worker, or a codex job whose process is gone or whose log stopped growing. `TaskStop` the Claude worker, confirm it stopped, then RE-DELEGATE the remainder in a smaller brief.

Harvest each worker's terminal report exactly once. Never re-arm the watch and never poll with `loom subagents list`, `loom subagents harvest`, `git status`, `wc`, or `ls`; `list` and `harvest` remain one-shot diagnostics. Only exact authoritative terminal evidence permits completion. Exit 6 is the channel that reports a worker idle past the stage's `subagent_timeout_secs` budget with no transcript growth — the only positive evidence of death. `TaskStop` it, confirm it stopped, then RE-DELEGATE the remainder to a fresh subagent. Never absorb the work into yourself — the orchestrator decomposes, delegates, verifies, and commits; it does not implement (hard stop 6). Re-read the tree before writing the new brief: a stale brief is worse than no brief. Never complete the stage while any subagent is still out (Rule 4).";

#[test]
fn block_c_lives_in_the_orchestration_skill() {
    assert!(
        ORCHESTRATION_SKILL.contains(BLOCK_C),
        "skills/loom-orchestration/SKILL.md does not carry BLOCK-C (the subagent-waiting \
         doctrine) verbatim. Expected to find:\n{BLOCK_C}"
    );
}

/// BLOCK-C reaches an agent through the `loom-orchestration` skill; a second
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
             reaches the agent through the loom-orchestration skill, so a \
             second verbatim copy in the signal is pure residency cost"
        );
    }
}

/// Pins the identity-bound, single-wait CLI contract and its terminal states.
#[test]
fn block_c_pins_the_owned_wait_contract() {
    assert!(BLOCK_C.contains(
        "loom subagents watch --worker claude:<agent-id> --worker codex:<unit-id> --timeout 3600"
    ));
    assert!(BLOCK_C.contains("capture each worker ID"));
    assert!(
        BLOCK_C.contains("Exit 0** — every bound worker has fresh, correlated success evidence")
    );
    assert!(BLOCK_C.contains("Exit 2** — the wait deadline passed"));
    assert!(BLOCK_C.contains("Exit 3** — a bound worker failed or was cancelled"));
    assert!(
        BLOCK_C.contains("`AlreadyWaiting` for the same worker set or `Busy` for a different set")
    );
    assert!(BLOCK_C.contains("Exit 5** — worker identity or terminal evidence is unknown"));
    assert!(BLOCK_C.contains("Harvest each worker's terminal report exactly once"));
    assert!(BLOCK_C.contains("Never re-arm the watch"));
    assert!(BLOCK_C.contains("Only exact authoritative terminal evidence permits completion"));
}
