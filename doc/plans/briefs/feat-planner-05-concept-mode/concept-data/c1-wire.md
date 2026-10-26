# concept-data C1: wire types for concept pages

Codex unit (gpt-6.1-terra, effort high). Working dir: loom/.

## Files

- Owns: `src/plan_server/wire/concept.rs`
- May read: `doc/plans/PLAN-feat-planner-05-concept-mode.md (or its IN_PROGRESS- spelling) stage concept-data, the PUBLIC SURFACE bullet that starts with wire::concept`; `src/plan_server/wire/message.rs (the existing PlanMessage enum, read only)`; `src/plan_server/wire/tests.rs (plan 2 parity pattern)`

## Entry points

Types: ConceptPage, KeyChip, DocLink, Block, Sentence, Mark, Citation, SourceAnchor, SummaryView, OutOfDate, ImplementsView, SectionLink, PlanLink, EmptyState, Progress, ConceptChanged, plus the string enums: SummaryView state (its five values are listed in the PUBLIC SURFACE bullet; copy them from there), OutOfDate reason (source_changed, rebuilding, awaiting_approval, cannot_rebuild, failed) and EmptyState kind (no_vision, sections_no_vision, no_metaplan, plans_no_metaplan, never_built, building).

## Steps

1. Write every type exactly as the PUBLIC SURFACE bullet lists it, field for field, with derive(Serialize, Deserialize, Debug, Clone, PartialEq), serde rename_all snake_case, and serde tag kind with rename_all snake_case on Block, Citation and ImplementsView.
2. Add the field file_hash: Option<String> to ConceptPage and give every path field a rustdoc line saying it is repo-relative and starts with doc/.
3. Add one doc comment at the top of the file naming the route prefix /api/concept/ and the WebSocket message ConceptChanged.

## Done

The file compiles with the module line the orchestrator already added in wire/mod.rs, and every type named above exists with the listed fields.

Proof (the orchestrator runs it after you): `cargo test --lib --no-run`

## Traps

Do not add the PlanMessage variant: another worker owns message.rs. The page key is a plain String on the wire (product, engineering, product/section_04, engineering/<slug>), not an enum.

Rules: Rust 2021, no new dependency, no unwrap outside tests, keep the file under 360 lines and
every function under 45 (cargo fmt grows them; the limits are 400 and 50). Double literal braces
inside format!. Put CLI placeholders inside backticks in rustdoc. Your own check is compile-only:
cargo test --lib --no-run from loom/. Do not run git. Do not touch any dot-loom or dot-work path.
