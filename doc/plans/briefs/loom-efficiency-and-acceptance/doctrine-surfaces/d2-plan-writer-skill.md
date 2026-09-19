# D2 — plan-writer skill: template criteria, sizing rubric, split into references

Tier: opus (`loom-senior-software-engineer`). Starts after D1 returns. Read `../common.md` first.

## Goal

The skill authors copy from stops shipping defective criteria, teaches the sizing rule the
operator set, documents what `loom plan verify` now enforces, and costs less to load. Evidence:
report section 4.1 — the canonical knowledge-distill block (`SKILL.md:1081-1085`) carries two
criteria that cannot fail (`rg -q "## "` on a Markdown file; 119 slots in 38 plans, 17 of the 30
criteria proven green at base) and an unbounded `--strict` gate that produced 11 of 17 disputes.
The skill is one 111 KB file, about 30,000 tokens, read whole on every plan task.

## Files you own (write)

- `skills/loom-plan-writer/SKILL.md`
- `skills/loom-plan-writer/references/*.md` (new directory; no skill uses one yet, so you set the
  convention: SKILL.md links each reference with a one-line "read when" note)
- `loom/src/assets/tests/skill_references.rs` (new) and its `mod` line in the assets tests module

Read-only: `loom/src/orchestrator/signals/tests_doctrine_blocks.rs` (BLOCK-B as D1 left it),
the merged code of the plan-verification and knowledge stages for exact flag and field names
(`rg -n 'baseline' loom/src/commands/knowledge/check.rs`,
`rg -n 'skills' loom/src/plan/schema/types.rs`, `loom/src/plan/schema/validation/`).

## Steps

1. Canonical template, both bookends. Delete the four `rg -q "## " doc/loom/knowledge/...`
   lines. knowledge-bootstrap acceptance becomes
   `loom knowledge check --strict --baseline doc/loom/knowledge/check-baseline.txt`;
   knowledge-distill acceptance becomes that line plus `loom memory pending --strict`. Remove
   the tier-routing paragraph that defends the deleted criterion. Add the distill step
   `loom memory pending --group` as the stage's starting point, and
   `loom knowledge check --write-baseline doc/loom/knowledge/check-baseline.txt` as its last
   step when the stage removed issues.
2. Section 4 rubric. Replace "Size every worker task to finish inside about 40 requests and 120k
   of context" (`:478`) and the surrounding "120k worker budget" wording with the common.md
   numbers: typical completion under about 400,000 tokens, boot cost about 28,000 per spawn, so
   group small tasks and never split below what 400,000 needs. Keep the 500,000-token stage
   guidance (`:489`, `:533`, `:1160`). Replace BLOCK-B with D1's text byte-for-byte. Rewrite the
   sentences around it that say the main agent never implements, and the "a stage with no
   subagent assignments is a red flag" sentence, to the cost rule.
3. New stage field. Document `skills:` in the metadata skeleton (Section 7), the canonical
   template's feature stages, and the pre-STOP checklist: name the skills each stage's agents
   need; `loom plan verify` rejects unknown names.
4. Enforced rules become pointers. Where Sections 6 to 8 argue at length for something
   `loom plan verify` now checks (`|| true`, `HOME=` from a variable, bare `mktemp -d`, network
   binaries, `doc/plans/` paths, `vitest -t`, test runners in `wiring_tests`, criteria green at
   base), keep one line naming the check and the logged incident, and tell authors to run
   `loom plan verify --strict`. Add the three missing rules: a criterion whose paths are disjoint
   from the stage's `files:` is a repo-wide gate on a narrow stage; `wiring_tests` commands have
   a 30 s cap and acceptance 300 s; a merged plan's pinned criteria go red after later renames,
   so pin behaviour over paths where possible. Document the worker-table parser's rule that a
   Files-owned cell holds paths only.
5. Split. SKILL.md keeps Sections 2, 3, 5, 7, the canonical template and the pre-STOP checklist,
   plus a short form of Section 1's checklist. Move to `references/`: the Section 1 protocols
   (blast radius, reuse, wireability, destructive paths, new runtime, cross-plan), the codex
   implementer detail, the realizability and produced-artifact rules of Section 6, and Section 8
   sandbox detail. Target: SKILL.md at most 45,000 bytes. Every moved section keeps its text.

## Traps

- `tests_doctrine.rs:131-166` pins BLOCK-B in this file and asserts it contains the codex
  constants from `loom/src/codex.rs`. BLOCK-B stays in SKILL.md, not in a reference.
- The build embeds every file under a `loom-*` skill directory (`loom/build/assets.rs:170-191`,
  `walk_files`), so `references/` files are embedded. Add one test,
  `loom/src/assets/tests/skill_references.rs` (you own it and its `mod` line), asserting that the
  embedded asset table contains `loom-plan-writer/references/` entries and that the install
  path for one of them keeps the `references/` component.
- The plan-writer's own instructions forbid triple backticks inside YAML descriptions; keep the
  template valid by running nothing — the main agent runs `loom plan verify` on a plan rendered
  from your template.

## Proof

`cargo test --offline --locked --manifest-path loom/Cargo.toml --lib orchestrator::signals::tests_doctrine`
— run once.
