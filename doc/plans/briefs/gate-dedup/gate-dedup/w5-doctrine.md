# W5: Remove the Verifier Subagent; One Gate Flow on Every Surface

Tier: sonnet. Files owned are listed in the stage's worker table. Read `common.md` first, especially section 4, which is the flow you write.

## Why

The `loom-verifier` subagent had two jobs, and both are covered elsewhere:

1. **Running the gate without filling the orchestrator's context.** `loom check` already does this: it runs as a host job, prints a per-criterion summary, and writes full logs to files.
2. **Judging the diff against each criterion's intent.** The `loom-code-reviewer` round, which every v2 stage already needs, does this.

The verifier only repeated runs the orchestrator, `loom check` and `loom stage complete` already made. Remove it everywhere.

## Rules

- No net byte growth in `CLAUDE.md.template` (160 bytes of headroom), the standard stable prefix (`STABLE_PREFIX_MAX_BYTES` 8,192) or the standard floor (`STANDARD_SIGNAL_BOILERPLATE_FLOOR_MAX_BYTES` 10,240; `orchestrator/signals/tests_size.rs`). `AGENTS.md.template` stays under 12,288 (`assets/mod.rs:22`). Removing the verifier text should leave room.
- Write current truth only: no "previously", "now" or "removed" narration. History belongs in CHANGELOG.md, which you do not edit.
- Follow the writing style in `CLAUDE.md.template` Rule 19.

## Edits (line numbers at `1b5a0c91`; re-read at base, since host-verification's W7 rewrote some of these)

1. **Delete `agents/loom-verifier.md`.**
   - Check how `loom install-assets` installs agents (`loom/build/assets.rs`, `loom/src/assets/install.rs`).
   - If it prunes loom-owned agent files that the bundle no longer ships, nothing more is needed.
   - If it does not, add `loom-verifier.md` to the install's removal of retired loom assets, if such a list exists.
   - Otherwise record a `loom memory note` saying installed copies stay in `~/.claude/agents/` until removed by hand, and report it. Do not build pruning: it is outside this plan.
2. **`agents/loom-code-reviewer.md`.** Add one short instruction: judge the diff against each acceptance criterion in the stage signal, and report a change that passes a criterion's letter but misses its intent as a finding with file:line evidence.
3. **`orchestrator/signals/cache/blocks.rs`.**
   - Replace `VERIFIER_GATE` (:90) with a gate block for the orchestrator:
     - after the workers return, run the formatter, then `loom check <stage-id>` once, plus the steps the stage lists outside acceptance, while the final review round runs;
     - a fix round runs `loom check <stage-id> --criterion <n>` for the failed criteria plus the lint and format criteria;
     - never re-run a test, build or lint command the check covers.
   - Rename the constant and `append_verifier_completion_header` so neither name says verifier.
   - Checklist item 3 (:137): "loom check output plus ALL stderr ..." replaces "full suite plus ALL stderr".
4. **`orchestrator/signals/cache.rs`.**
   - `CODE_STAGE_GATE` (:78-80): the gate is the stage's criteria through `loom check`, and the full suite belongs to integration-verify.
   - `INTEGRATION_VERIFY_OVERRIDE` (:115-126): the integration-verify orchestrator runs `loom check integration-verify` once (it carries the full suite), and reviewers run only targeted checks. Drop the canonical-verifier subagent wording.
   - `format/codex.rs:211-212`: align the wording.
5. **Doctrine tests.** `tests_doctrine_prefixes.rs:60-67` pins "**The gate runs in a `loom-verifier`**", and `tests_doctrine_blocks.rs:59` pins the verifier's agent type and model text. Re-point both pins at the new gate block's text. Never delete a pin without a replacement.
6. **`CLAUDE.md.template`.**
   - Rule 4 (:90): the commit follows a green `loom check` and the final review.
   - Rule 5 (:96): a standard stage's gate is the orchestrator's single `loom check`, and the main agent never re-runs it.
   - Delete the `loom-verifier` row from the Rule 7 agent table (:119).
   - Net bytes ≤ 0.
7. **`AGENTS.md.template:54-57`.** Codex never runs a stage's suite; the gate is `loom check`. Edit only if the current text conflicts.
8. **`skills/loom-orchestration/SKILL.md`.**
   - Rule 4 "The gate in a standard stage" (:66-73) states the flow: no verifier subagent, `--criterion` for fix rounds, "Do not repeat its run yourself" kept.
   - The review loop: the final round runs beside the check once the tree is formatted, judges criteria intent, and re-reviews cover the changed files and open findings only.
   - Rule 7: remove every verifier mention (:80, :85, :237) and the verifier line in the BLOCK-B playbook (point 3, "the GATE ... runs in ONE loom-verifier subagent").
   - Net bytes ≤ 0.
9. **`skills/loom-plan-writer/SKILL.md` and references.**
   - Remove every verifier mention: `SKILL.md:186-188` and the BLOCK-B copy (it mirrors the orchestration skill: keep the two identical), `references/codex-implementers.md`, `references/parallelization.md`, `references/stage-sizing.md`.
   - State the flow: a stage description never lists a shell gate beyond `loom check` plus the steps acceptance cannot carry, and lint and format are acceptance criteria.
   - In integration-verify, a specific test's proof goes into `stdout_contains` on the full-suite criterion instead of a second filtered run.
   - Update `references/verification-rules.md:51-58`, `references/v2-contracts.md:136-140`, `references/sandbox.md:143` and `references/plan-template.md:101-110` to the same flow.
   - `references/v2-contracts.md:129`: an accepted contract dispute re-freezes the stage's contract and harness files at their current content and drops deleted harness files (W2's change).
10. **`loom-hooks/_subagent-preamble.txt:12-22`.**
    - Remove the loom-verifier exception (:21).
    - The no-verify rule says a subagent never runs the stage's gate, and that the main agent re-checks one criterion with `loom check <stage-id> --criterion <n>`.
    - Keep the integration-verify exception.
11. **`loom-hooks/spawn-guard.sh:214`.** Remove `loom-verifier (opus xhigh, gate only)` from `TYPED_AGENTS`. In `loom-hooks/tests/loom-relay-kinds.sh:173`, fix the comment that names a loom-verifier.
12. **Code comments and data:**
    - `commands/usage/sections/lifecycle_orientation.rs:13`: remove the `"loom-verifier"` entry, and update the test that counts the list if one does;
    - `relay/matrix_host.rs:9` and `orchestrator/core/inbox_drain/check.rs:61`: comment wording;
    - `models/stage/defaults.rs:139`: the comment's reason for standard effort medium no longer cites a verifier subagent.
13. **`README.md`:**
    - `loom check` usage (:430) gains `[--criterion <n>]...`;
    - the verification section (:858-872) states the flow;
    - :319, :1031 and the `loom-verifier` table row at :1041: no verifier subagent; the orchestrator runs `loom check`.
14. **`CONTRIBUTING.md`:** remove or rewrite its verifier mention.
15. **New `orchestrator/signals/tests_doctrine_gate.rs`**, declared in `orchestrator/signals/mod.rs` under `#[cfg(test)]`. It asserts:
    - `agents/loom-verifier.md` does not exist;
    - the new gate block, `skills/loom-orchestration/SKILL.md`, `CLAUDE.md.template`, `_subagent-preamble.txt` and `references/plan-template.md` (via `include_str!`) contain no `loom-verifier`;
    - the gate block and the orchestration skill contain `loom check` and `--criterion`;
    - `agents/loom-code-reviewer.md` mentions acceptance-criteria intent.

Do not edit `loom/tests/fixtures/signal_replay/**`: those are recorded inputs.

## Proof

- `cargo test --lib orchestrator::signals::` and `cargo test --lib commands::usage::`.
- `rg -n loom-verifier` over the repository, excluding `CHANGELOG.md`, `doc/`, `loom/tests/fixtures/` and `.loom/`, returns nothing.
- `wc -c ../CLAUDE.md.template ../AGENTS.md.template ../skills/loom-orchestration/SKILL.md`, before and after, reported.
