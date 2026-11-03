# gate-waivers W4: doctrine for gate disputes, the needs-person verdict and the block rule

Tier: haiku. Wave 2. Read `doc/plans/briefs/autonomous-recovery/common.md` first.

## What changed (state it, in these words where a sentence needs them)

- `loom stage dispute-criteria <stage> --field gate --gate <unwired|duplicates|impact-tests>`
  disputes one of loom's own completion gates; an accepted verdict waives that gate for that
  stage, recorded with the dispute and the loom version, and integration-verify's signal lists
  every waiver and amendment.
- `loom stage block` is refused while any failing check, open review finding or integrity
  event has a dispute route. A check that cannot pass until a person installs a host tool,
  provides a credential or makes a decision is disputed; the adjudicator's `needs-person`
  verdict blocks the stage with `needs a host install: <what>` (or `needs a credential`, `needs
  a decision`), and the operator resumes with `loom stage retry`.
- The live-test-lane plan's rule stands: a required `live` entry that could not run for want of
  the live credential has no dispute route, so `loom stage block` is allowed for it; any other
  live failure is rerun with `loom check --live <stage>` or disputed with `--field live`. Keep
  every sentence the live plan's R4 wrote in the plan-writer files.

## Edits (you own these files)

1. `src/cli/types_stage.rs`: the doc comment of the `Block` variant (it is the
   `loom stage block --help` text) states the refusal rule above in two sentences.
2. `src/orchestrator/signals/helpers.rs`: the block-versus-dispute sentence (rg `stage block`).
   Replace text; the stable-prefix size test must keep passing.
3. `../skills/loom-plan-writer/SKILL.md` Section 9, the paragraph starting "A criterion that
   fails only because of where completion runs": one sentence each for gate disputes and the
   needs-person verdict.
4. `../skills/loom-plan-writer/references/v2-contracts.md` Section 5 "Disputes" bullet: the
   same two facts, beside the `--field live` text R4 added there.
5. `../skills/loom-orchestration/SKILL.md`: the paragraph that tells a main agent when to
   block (rg `stage block`).
6. `../CLAUDE.md.template` Rule 13: the parenthetical "(refused while the latest `loom stage
   complete` shows a failing criterion)" becomes "(refused while the latest `loom stage
   complete` shows a failing check with a dispute route; a check that fails for want of a host
   install, credential or decision is disputed, and the adjudicator blocks the stage with that
   need, except a required live entry missing only the live credential, which blocks
   directly)". Change nothing else in that file.

Plain prose, CLAUDE.md Rule 19. Report each file and the sentence you replaced.
