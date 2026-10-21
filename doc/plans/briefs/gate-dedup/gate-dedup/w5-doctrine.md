# W5: One Gate Flow on Every Guidance Surface

Tier: sonnet. Files owned are listed in the stage's worker table. Read `common.md` first, especially section 4, which is the flow you write.

## Rules

- No net byte growth in `CLAUDE.md.template` (160 bytes of headroom), the standard stable prefix (`STABLE_PREFIX_MAX_BYTES` 8,192) or the standard floor (`STANDARD_SIGNAL_BOILERPLATE_FLOOR_MAX_BYTES` 10,240; `orchestrator/signals/tests_size.rs`). `AGENTS.md.template` stays under 12,288 (`assets/mod.rs:22`). Make room by cutting the text you replace.
- Write current truth only: no "previously" or "now".
- Follow the writing style in `CLAUDE.md.template` Rule 19.
- Keep the needle `**The gate runs in a \`loom-verifier\`**` (pinned by `tests_doctrine_prefixes.rs:60-67`) and the verifier's agent type and model text (pinned by `tests_doctrine_blocks.rs:59`).

## Edits (line numbers at `1b5a0c91`; re-read at base, since host-verification's W7 rewrote some of these)

1. `agents/loom-verifier.md`, "What You Run" (:22-30) and "What You Need" (:14-20).
   - The verifier runs `loom check <stage-id>` once, reading its first `Host job:` line and re-running while it reports requested or pending. It also runs each step the orchestrator lists as outside acceptance, once, and judges the diff.
   - It runs no build, test, lint or format command in its own shell. The one exception is a single filtered rerun of one failing criterion, to classify it RUNNER-ENV.
   - In a fix round, it runs `loom check <stage-id> --criterion <n>` for the named criteria.
   - Remove "each as the complete command, never a filtered subset".
2. `orchestrator/signals/cache/blocks.rs`.
   - `VERIFIER_GATE` (:90): say the same flow in the same or fewer bytes.
   - Checklist item 3 (:137): "loom check output plus ALL stderr ..." replaces "full suite plus ALL stderr".
3. `orchestrator/signals/cache.rs`.
   - `CODE_STAGE_GATE` (:78-80): the gate is the stage's criteria through `loom check`, and the full suite belongs to integration-verify.
   - `INTEGRATION_VERIFY_OVERRIDE` (:115-126): the canonical verifier runs `loom check integration-verify` once, which carries the full suite.
   - `format/codex.rs:211-212`: align the wording.
4. `CLAUDE.md.template`.
   - Rule 4 (:90): "the full gate is green" becomes "`loom check` is green".
   - Rule 5 (:96): a standard stage's gate is one `loom-verifier` running `loom check`; the main agent never re-runs it.
   - Net bytes ≤ 0.
5. `AGENTS.md.template:54-57`: codex never runs a stage's suite; the gate is `loom check`. Edit only if the current text conflicts.
6. `skills/loom-orchestration/SKILL.md`.
   - Rule 4 "The gate in a standard stage" (:66-73): the flow, the `--criterion` fix round, and "Do not repeat its run yourself" kept.
   - Review loop: the final review round runs beside the gate once the tree is final and fmt is clean; re-reviews cover the changed files and open findings only.
   - Net bytes ≤ 0.
7. `skills/loom-plan-writer/SKILL.md` and references.
   - A stage description briefs the verifier with `loom check <stage>` plus only the steps acceptance cannot carry, never a shell list of the full suite and every criterion.
   - Lint and format are acceptance criteria.
   - In integration-verify, a specific test's proof goes into `stdout_contains` on the full-suite criterion instead of a second filtered run (the new subsumed lint warns).
   - Update `references/verification-rules.md:51-58`, `references/v2-contracts.md:136-140`, `references/sandbox.md:143` and `references/plan-template.md:101-110` to the same flow.
   - `references/v2-contracts.md:129`: an accepted contract dispute re-freezes the stage's contract and harness files at their current content and drops deleted harness files (W2's change).
8. `loom-hooks/_subagent-preamble.txt:12-22`: the no-verify rule adds that the main agent re-checks one criterion with `loom check <stage-id> --criterion <n>`.
9. `README.md`: `loom check` usage (:430) gains `[--criterion <n>]...`; the verification section (:858-872) states the flow.
10. New `orchestrator/signals/tests_doctrine_gate.rs`, declared in `orchestrator/signals/mod.rs` under `#[cfg(test)]`. It asserts:
    - `agents/loom-verifier.md` (via `include_str!`), `VERIFIER_GATE` and `references/plan-template.md` each contain `loom check`;
    - none of them contains "never a filtered subset" or instructs a full `cargo test --all-targets` run in the verifier's shell;
    - `agents/loom-verifier.md` contains `--criterion`.

## Proof

`cargo test --lib orchestrator::signals::`, and `wc -c ../CLAUDE.md.template ../AGENTS.md.template ../skills/loom-orchestration/SKILL.md` before and after, reported.
