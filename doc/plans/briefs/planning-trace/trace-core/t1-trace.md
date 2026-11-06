# T1: settled marks and the trace build (trace-core, wave 2)

Read `../common.md` in full; "Settled marks" and "The trace model" are your specification. T2's
`trace::tokens` and T3's `planning::findings` exist when you start (T3 runs beside you: if
`findings::parse` is not there yet, code against the signature in common.md and the orchestrator
resolves the order).

## Files you own

`loom/src/planning/trace/mod.rs`, `trace/settle.rs` (+ `settle/`), `trace/build.rs` (+ `build/`),
`loom/src/planning/project.rs` (+ a new `project/` directory if you must move code out),
`loom/src/planning/mod.rs`, `loom/src/planning/problems.rs`.

## Tasks

1. **`trace::settle`.** The API in common.md. `short_hash` takes `normalized_hash` output
   (`sha256:<hex>`) or raw bytes; pick one and use it everywhere. The Settled line parser accepts
   exactly `Settled: <12 hex>` followed by optional `stub <12 hex>` and `upstream <12 hex>`
   in that order; anything else starting `Settled:` is `settled-bad`. Placement and removal as
   common.md says, preserving the rest of the text byte for byte (line endings included).
2. **`trace::build`.** `pub fn build_trace(project: &Project, feature: &Feature) -> Trace`, or the
   form `derive` can call before `Feature` is complete; it must be a function named
   `build_trace` called from `project.rs` (a wiring check pins the call).
   Inputs you need: the feature's vision rows (stub text per section number; read how
   `vision_rows` is stored), sections written, metaplan entries (T2's `covers`, `span_hash`),
   plans (lifecycle state: read `plan_state_name` in `plan_server/concept/links.rs` and the lock
   reasons in `planning/paths.rs` to find the planning-side source), `project.implements`
   (anchors), stage lines (T2's `stage_lines` over each plan's text), each plan's bundle
   (T2's `briefs::bundle_files`, read from the loaded project, never from disk), each document's text (`Project::text`), open questions
   (`ProjectDoc.open_questions`), and the ledger at `doc/plans/<f>/findings.md` parsed with
   `findings::parse`. Resolve About tokens to `NodeRef`s (`TraceFinding.about`).
3. **Hashes.** `TraceDoc.current` is the line settling would write now, and `settle` compares the
   document's own Settled line to it in the order common.md gives. The plan body hash covers its
   bundle; the upstream hash covers implemented heading spans (`HeadingSection.span.hash`) and
   whole-section file hashes. Keep every hash input in a deterministic order.
4. **Progress, coverage, gaps, frontier, revisit.** As common.md defines them. A plan that is both
   in a metaplan entry and in the feature home is one document.
5. **Project wiring.** Add `pub trace: Trace` to `Feature` (default for features built before
   derive finishes), and call `build_trace` at the end of `derive` for every feature.
   `project.rs` is at 393 lines: move a helper or `derive`'s body into `project/` if needed so the
   file stays at or under 400. `refresh` must produce the same trace as a fresh load (it re-runs
   `derive`; confirm).
6. **Problems.** The Settled line's `settled-duplicate` and `settled-bad`, About tokens that do not resolve
   (`about-unresolved`), and the ledger's own problems from `findings::parse`. Wire T2's
   `problems/trace.rs` through `problems.rs`.

## Tests

Unit tests in `trace/settle/tests.rs` and `trace/build/tests.rs`: CRLF text keeps its endings,
re-marking is a no-op, clear restores the text, each settle state, a plan whose brief changes
reads revised, coverage by entry, plan, whole section and stage line, revisit by fold and by open
finding (for an unwritten entry, for a written plan not yet running, and none once it runs), frontier order and its cap of five with one gap per document. The contracts
`settle-mark-round-trips`, `settle-states-follow-body-stub-and-upstream` and
`uncovered-headings-are-gaps` are frozen: make them pass without editing them.

## Done

The stage's Rust acceptance commands pass for your modules; no file over 400 lines and no
function over 50; report the final public names to the orchestrator.
