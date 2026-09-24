# review-harvest-gate / W4 — reviewer output format, v2 review/suggestion/distill signal sections

Read first: `doc/plans/briefs/verification-v2/DESIGN.md` D0, D12 (the reviewer format and
finding definition), D16 (every bullet, and BLOCK-E verbatim). Knowledge:
`patterns/doctrine-cross-surface.md`; `mistakes/doctrine-and-acceptance.md` "An Agent Doc May Only
Name Commands Its Guard Allows (2026-09-13)". Code: `agents/loom-code-reviewer.md` (85 lines;
Output Format at 77-86); `orchestrator/signals/v2_section.rs` (contract-phase:
`append_v2_section` with one function per block).

## Files you own

`agents/loom-code-reviewer.md`, `loom/src/orchestrator/signals/v2_section.rs`,
`loom/src/orchestrator/signals/v2_section_review.rs` (new),
`loom/src/orchestrator/signals/v2_section_tests.rs` (new).

## Tasks

1. `agents/loom-code-reviewer.md` Output Format: keep Critical/Important/Suggestions for the
   human-readable part, then require the final fenced `loom-review` block exactly as D12 shows,
   and define what goes where:
   - a finding needs `file`, `line`, `claim`, and either a concrete failure scenario (input or
     state → wrong outcome) or a cited rule (a project convention, knowledge entry, size limit,
     lint rule or plan requirement);
   - everything else is a suggestion;
   - `resolved`/`unresolved` list the ids of the open findings the brief gave you;
   - the block is the last thing in the message.

   The agent stays read-only (Read, Glob, Grep). Name no command it cannot run.
2. `v2_section_review.rs`: blocks appended by `append_v2_section` (edit `v2_section.rs` only to
   call them). Every loom command they name must exist in the merged CLI at the end of this
   stage:
   - standard and IV: "## Review Gate": every finding blocks completion; spawn a
     `loom-code-reviewer` for the stage diff; paste `loom stage review status <stage>` into every
     re-review brief; a re-review covers the files changed since the previous round plus the
     open findings; BLOCK-E verbatim; carried findings listed with ids, from `load_carried`;
   - IV only: "## Reviewer Suggestions": every pending `suggestion` entry of the plan's stages
     (read the memory journals the way `loom memory pending` does), with ids, and the D16 IV
     rule;
   - knowledge-distill only: "## Unimplemented Suggestions": the D16 distill rule.
   - `dispute-findings` does not exist yet; dispute-kinds adds that line. Do not mention it now.

## Named tests

None are counted in the acceptance for you, but write in `v2_section_tests.rs`: the review gate
block contains BLOCK-E byte for byte; a v1 stage renders no v2 block; an IV stage lists a pending
suggestion by id.

## Proof (one command, once)

`cargo test --manifest-path loom/Cargo.toml --lib orchestrator::signals::v2_section`

## Report

Files changed; the exact BLOCK-E text as rendered; the proof result.
