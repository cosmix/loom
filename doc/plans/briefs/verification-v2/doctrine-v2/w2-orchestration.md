# doctrine-v2 / W2 — orchestration skill for v2 stages, BLOCK-E and coverage pins

Read first: `doc/plans/briefs/verification-v2/DESIGN.md` D12–D17 (BLOCK-E verbatim in D16).
Then read the MERGED code for every behaviour you describe: `orchestrator/signals/v2_section.rs`,
`v2_section_review.rs`, the stage `contracts`/`review`/dispute commands. Where code and DESIGN
differ, describe the code and record the difference with `loom memory note`. Code:
`orchestrator/signals/tests_doctrine.rs` (399 lines, at the cap: add nothing to it),
`tests_doctrine_blocks.rs` (BLOCK-A/B/D consts and `RETIRED_PHRASES`), `signals/mod.rs` (the
`#[path]` test module declarations: the file explains why they are declared there).

## Files you own

`skills/loom-orchestration/SKILL.md`, `loom/src/orchestrator/signals/tests_doctrine_blocks.rs`,
`loom/src/orchestrator/signals/tests_doctrine_v2.rs` (new),
`loom/src/orchestrator/signals/mod.rs`, `loom/src/orchestrator/signals/v2_section_review.rs`.

## Tasks

1. `skills/loom-orchestration/SKILL.md`: a `## Plan version 2 stages` section.
   - Loom has already run the contract session and frozen the contracts before you start. Never
     edit a contract file; `loom stage contracts show`/`restore`; `dispute-contract` when a
     contract is wrong.
   - The review loop: spawn `loom-code-reviewer`; brief every re-review with
     `loom stage review status`; each finding is either fixed and re-reviewed or disputed;
     dispute a round's findings in one command.
   - BLOCK-E byte for byte.
   - Test integrity (`loom stage review integrity`, revert or `dispute-integrity`).
   - What completion checks and in what order.
   - Integration-verify: consider every reviewer suggestion; resolve implemented ones with
     `--outcome implemented`; never defer a finding.

   Rule 4's three commit conditions stay as they are; add one sentence that in a v2 stage the
   review condition is enforced by `loom stage complete`.
2. `tests_doctrine_blocks.rs`: `pub(super) const BLOCK_E: &str` = the DESIGN D16 text exactly.
3. `tests_doctrine_v2.rs` (declared in `signals/mod.rs` next to the other doctrine test modules,
   for the reason that file's comment gives). Named tests (binding):
   - `block_e_agrees_across_every_surface`: `BLOCK_E` appears verbatim in the rendered v2 review
     section of a v2 standard stage, and in `skills/loom-orchestration/SKILL.md`
     (`include_str!`).
   - `every_adapter_is_named_in_its_language_skill`: for every adapter in
     `testrun::registry::all()`, the skill `testrun::languages::skill_for(adapter.language())`
     contains `` `<adapter name>` `` inside its `## Loom Test Runner Adapter` section (read
     `skills/<skill>/SKILL.md` from `CARGO_MANIFEST_DIR/../skills`).
   - `plan_writer_skill_names_project_detect`: `skills/loom-plan-writer/SKILL.md` contains
     `loom project detect` and `references/v2-contracts.md`.
4. `v2_section_review.rs`: only if its BLOCK-E text differs from DESIGN D16; then make it equal.

## Proof (one command, once, after W1 reports)

`cargo test --manifest-path loom/Cargo.toml --lib orchestrator::signals::tests_doctrine`

## Report

Files changed; every place where code and DESIGN differed (also recorded in loom memory); the
proof result.
