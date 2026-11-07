# verdict-completion W3: what follows a verdict, on every surface an agent reads

Tier: haiku (`loom-software-developer`). Wave 1, beside W1. Read
`doc/plans/briefs/fewer-disputes/verdict-completion/common.md` sections "What the stage builds"
and "Style" first. Every edit below is a text replacement: find the OLD text with `rg -nF`,
replace it with the NEW text exactly, and change nothing else in the file. If an OLD text is not
found because an earlier stage reworded the sentence, apply the same meaning to the sentence that
replaced it and say so in your report.

## Files you own

- `src/orchestrator/signals/helpers.rs` (the `STAGE_EXIT_RULES` constant only)
- `src/orchestrator/signals/tests_doctrine_v2.rs` (the `BLOCK_F` constant, one new test)
- `src/orchestrator/signals/v2_section_review.rs` (one sentence in `append_dispute_commands`)
- `src/commands/stage/dispute_transport.rs` (`Dispute::wording` only)
- `../skills/loom-plan-writer/SKILL.md` (Section 9, one sentence)
- `../skills/loom-plan-writer/references/v2-contracts.md` (two sentences)
- `../skills/loom-orchestration/SKILL.md` (review gate item 4, one line; net zero or negative in
  bytes)

Two size caps bind these edits. `skills/loom-orchestration/SKILL.md` is capped at 20,530 bytes
(`ORCHESTRATION_SKILL_MAX_BYTES` in `src/orchestrator/signals/tests_size.rs`, 20,515 at
`e6680a9d`): run `wc -c ../skills/loom-orchestration/SKILL.md` before and after Edit 4; the after
count must not exceed the before count, and the cap is never raised. BLOCK-F sits in the stable
prefix capped at 8,192 bytes (`STABLE_PREFIX_MAX_BYTES`): Edit 1 grows `STAGE_EXIT_RULES` by at
most 250 bytes in all (Replacement A and B below add 232).

`CLAUDE.md.template` states no rule about what follows a verdict: leave it alone.

## Edit 1. BLOCK-F: `STAGE_EXIT_RULES` in `helpers.rs` and `BLOCK_F` in `tests_doctrine_v2.rs`

The two constants must stay byte-identical (`block_f_agrees_across_every_surface`). Make the same
two replacements in both. Both constants are one Rust string literal on one line; `\"` inside it
stays escaped as it is.

Replacement A. OLD:

```text
Filing ends this session; the daemon starts a fresh one with the verdict.
```

NEW:

```text
Filing ends this session. When the verdict changes nothing, loom reruns completion on the tree your last `loom stage complete` ran on, even if only its impact tests failed, and completes the stage if it passes; else a fresh session starts.
```

Replacement B. OLD:

```text
Commit before filing either.
```

NEW:

```text
Commit before filing either; run `loom stage complete` on that commit before filing a dispute.
```

After both, `helpers.rs` keeps its line count (the constant is a single line), and the constant
is at most 250 bytes longer than before.

## Edit 2. `../skills/loom-plan-writer/SKILL.md`, Section 9 first paragraph

OLD:

```text
Filing a dispute ends the stage session by design; the daemon starts a fresh session with the verdict, so the agent never waits on it.
```

NEW:

```text
Filing a dispute ends the stage session by design, so the agent never waits on a verdict. An accepted integrity or impact-tests dispute, or a findings verdict that upholds nothing, lets the daemon complete the stage without a session when the committed tree is still the one the latest `loom stage complete` ran on, that run passed or failed only its impact-selected tests, the stage's check definition has not changed since, and the daemon's rerun of the completion on that tree passes; every other verdict, and a failing rerun, starts a fresh session with the verdict. Run `loom stage complete` on the committed tree before filing, so a verdict that changes nothing can finish the stage.
```

## Edit 3. `../skills/loom-plan-writer/references/v2-contracts.md`

In the `**Disputes.**` bullet (Section 5), OLD:

```text
Filing ends the session by design: the daemon starts a fresh session with the verdict, so the agent never waits on it.
```

NEW:

```text
Filing ends the session by design, so the agent never waits on a verdict. An accepted integrity or impact-tests dispute, or a findings verdict that upholds nothing, lets the daemon complete the stage without a session when the committed tree is still the one the latest `loom stage complete` ran on, that run passed or failed only its impact-selected tests, the stage's check definition has not changed since, and the daemon's rerun of the completion on that tree passes; every other verdict, and a failing rerun, starts a fresh session with the verdict.
```

In the `**Dispute in batches.**` bullet (Section 6), OLD:

```text
filing ends the session by design and a fresh session starts with the verdict, so two disputes cost two respawns.
```

NEW:

```text
filing ends the session by design, and each filing costs a judge session plus, unless its verdict lets the daemon complete the stage, a fresh session.
```

## Edit 4. `../skills/loom-orchestration/SKILL.md`, review gate item 4

The item's last line (after its three-space indent), OLD:

```text
Filing ends the session; the daemon restarts with the verdict. Allow three disputes per kind (findings, contract, integrity, impact tests); one more means `NeedsHumanReview`.
```

NEW (15 bytes shorter, so the file shrinks; BLOCK-F already lists the dispute kinds):

```text
Filing ends the session; a verdict that upholds no finding lets the daemon complete a passing tree. Three disputes per kind; a fourth means `NeedsHumanReview`.
```

If an earlier stage changed this line, keep its other sentences, replace only the filing
sentence with the first NEW sentence, and trim the same item until `wc -c` is no larger than
before. Do not touch the `**Review order (plan v2):**` paragraph below it: it is BLOCK-E, pinned byte for
byte.

## Edit 5. `v2_section_review.rs::append_dispute_commands`

The first bullet's string ends with these source lines:

```rust
         command: each filing ends this session by design, and the daemon starts a fresh one \
         with the verdict.\n\
```

Replace them with:

```rust
         command: each filing ends this session by design. A verdict that upholds no finding \
         lets loom rerun the completion of the tree your latest `loom stage complete` ran on \
         and complete the stage if it passes; any other verdict starts a fresh session.\n\
```

Keep the indentation and the trailing `\` continuation exactly as the surrounding lines use them,
so the rendered text has single spaces between words.

## Edit 6. `dispute_transport.rs::wording`

The `match self.kind` that picks `successor` has a `DisputeKind::Criterion { .. }` arm and a `_`
arm (`"with the verdict applied"`). Add, between them, an arm for
`DisputeKind::Findings { .. } | DisputeKind::Integrity { .. } | DisputeKind::ImpactTests { .. }`
whose tuple is:

```rust
(
    "with the verdict applied, unless the verdict accepts this dispute (for findings: upholds \
     none) and loom's rerun of the completion of the tree your latest `loom stage complete` ran \
     on passes, when loom completes the stage without one",
    "FileDispute",
    RequestKind::FileDispute,
),
```

The `_` arm keeps `"with the verdict applied"` (contract and freeze-refusal disputes). Leave
`handle_response` and its message untouched; it already interpolates `wording.successor`.

## Edit 7. The pin test in `tests_doctrine_v2.rs`

Add one test after `block_f_agrees_across_every_surface`. If the file already declares a constant
holding `include_str!("../../../../skills/loom-plan-writer/references/v2-contracts.md")` (an earlier
stage may have added one), reuse it; otherwise add
`const V2_CONTRACTS: &str = include_str!("../../../../skills/loom-plan-writer/references/v2-contracts.md");`
beside `PLAN_WRITER_SKILL`.

```rust
/// What follows a verdict, as every agent-facing surface states it: a verdict that changes
/// nothing lets the daemon complete the stage, and no surface still says that every verdict
/// starts a fresh session.
#[test]
fn verdict_completion_doctrine_agrees_across_every_surface() {
    let prefix = super::cache::generate_stable_prefix();
    for (label, text, needle) in [
        ("skills/loom-plan-writer/SKILL.md", PLAN_WRITER_SKILL, "lets the daemon complete the stage without a session"),
        ("references/v2-contracts.md", V2_CONTRACTS, "lets the daemon complete the stage without a session"),
        ("skills/loom-orchestration/SKILL.md", ORCHESTRATION_SKILL, "lets the daemon complete a passing tree"),
        ("the standard stable prefix", prefix.as_str(), "loom reruns completion on the tree your last `loom stage complete` ran on"),
    ] {
        assert!(text.contains(needle), "{label} must say that a verdict changing nothing lets loom complete the stage; expected: {needle}");
        for stale in [
            "the daemon starts a fresh session with the verdict, so the agent never waits on it",
            "the daemon restarts with the verdict",
            "the daemon starts a fresh one with the verdict",
        ] {
            assert!(!text.contains(stale), "{label} still says every verdict starts a fresh session: {stale}");
        }
    }
}
```

Format it with the file's style (rustfmt will wrap the long tuples; that is fine). Check whether
`generate_stable_prefix` returns `String` or `&str` in `signals/cache.rs` and adapt the
`prefix.as_str()` call to it.

## Done

Report each file and edit applied, any OLD text you did not find verbatim, the `wc -c` of
`../skills/loom-orchestration/SKILL.md` before and after Edit 4, and the byte growth of
`STAGE_EXIT_RULES`. Run no test: the library's test target cannot compile until W2 lands, and the
main agent's `cargo test --lib orchestrator::signals::` runs the doctrine and size tests. Never
edit `doc/loom/knowledge/`.
