# W2: project view, problems, checklist

Tier: opus (`loom-senior-software-engineer`). Read `doc/plans/briefs/feat-planner-01-documents/common.md`,
then `doc/plans/briefs/feat-planner-01-documents/checklist-check/common.md` (the checklist and problem rules
are DECIDED there; implement them exactly), then this file. You start after W1's parsers exist.

## Files you own

`loom/src/planning/project.rs`, `loom/src/planning/problems.rs`, `loom/src/planning/checklist.rs`,
`loom/src/planning/checklist/offers.rs`, `loom/src/planning/checklist/tests.rs`,
`loom/src/planning/mod.rs`.

## `project.rs`

`Project` and `ProjectDoc` exactly as in common.md ("Loaded project view"). `Project::load(root)`:

1. `discover(root)`; a `Missing` or `Symlinked` `doc/` gives an empty project with one
   `doc-missing` problem and a checklist computed from the empty tree.
2. For every Markdown document (`.md`): `read_doc`, `sections_of`, title (first level-1 heading's
   title, else the file name), `open_questions`, `references`. A read failure (too large, invalid
   UTF-8, a swapped symlink) sets `read_error` and leaves the derived fields empty; it never fails the
   load. Non-Markdown documents get no sections.
3. `vision_rows` from the vision's text (or `Err(VisionProblem::NoHeading)` with no vision; record
   absence separately in the checklist, not as a problem). `metaplans`: every `is_metaplan` document's
   entries, sorted by path. `implements` for every plan (current winner path). `implemented_by`: the
   reverse map over `Implements::Sections`. `locked = LockedSet::scan(root)?` (an error here fails the
   load: fail closed). Each document's `lock` from `locked.lock_reason`.
4. `ParsedDocs` and `build_checklist`, then `problems::collect(&project_parts)`.

`refresh(&mut self, batch)`: a batch with any rename, remove or add under `doc/plans/` is handled by
a full `load` (slugs, strays and locks can change); otherwise drop the `ProjectDoc` of every removed
path and of every rename's old path (anywhere under `doc/`), re-read the changed, added and
renamed-to documents, re-run `discover` for the tree (cheap), then recompute `vision_rows`, `metaplans`, `implements`, `implemented_by`, `locked`,
`checklist`, `problems`. Equal results to a fresh `load` on the same tree are REQUIRED (test it).
`content_hash`, `section_text` as in common.md; `section_text` returns `Ok(None)` when the file's
current `file_hash` differs from the cached one (the caller refreshes first).

## `problems.rs`

`pub(crate) fn collect(...) -> Vec<Problem>` implementing the table in checklist-check/common.md,
with the locked-document exemption, 1-based lines and the sort order. `broken-brief-reference`
calls W1's `briefs::plan_briefs` for every unlocked plan (stage ids from its `DocSections::stages`).
Messages are one plain sentence naming the thing (`plan has no Implements: line`).

## `checklist.rs`, `checklist/offers.rs`

`build_checklist(tree, parsed) -> Checklist` implementing the step marks, current step, items and
offers tables exactly. Put `Offer`, `Offer::id`, `Offer::target` and the offer ordering in
`offers.rs`. Serialization: `CurrentStep` serialises to one string (custom `Serialize`), `steps` to
`[{ "step", "mark" }]`, offers as objects with `id`, `kind`, `target` and variant fields (a manual
`Serialize` impl or a `#[derive(Serialize)]` view struct). `ProjectState` as in common.md.

## `mod.rs`

The seed declared every module; keep it as the module index with a doc comment that lists each
module in one line. Add nothing else.

## Tests (yours, `checklist/tests.rs`)

Every row of both offer tables, each step-mark rule, the secondary-offer extras (pressure offers
with gaps, AddSectionToVision only with a vision, AddPlanToMetaplan only with a metaplan, StartRun
only without gaps in steps 1-4), first-ness by table order and heading order (a table listing 03
before 01), the locked-document exemption for problems, `refresh` equality with `load` (derive `PartialEq` on `Project` and its parts) after a section
edit, a plan rename, a section removal, a rename under `doc/product/` and a batch of 30 mixed
changes (contract `refresh-matches-load`), the title rule (first level-1 heading, else the file name, for a
document whose first heading is level 2), an empty or blank-only listed section counted as a gap
(contract `empty-section-is-a-gap`), and several metaplans contributing entries in path order. Build trees in `tempfile::TempDir`s.

## Proof

`cargo test --lib planning::`, `cargo test --test planning_checklist_contracts`,
`cargo test --test planning_documents_contracts`, `cargo clippy --all-targets -- -D warnings`.
