# E1: labelled corpora for rust, typescript, tsx, javascript, python, go

- **Agent type / tier:** `loom-software-engineer` (sonnet)
- **Wave:** 2, alongside E2, after E3 has delivered the evaluator and
  `loom map --eval-edges`.
- **Design:** read `doc/plans/briefs/source-graph-mechanism/design.md` section 14.1 in
  full, and sections 6.2 and 7 so you know what an honest graph can and cannot bind.

## Goal

Six small, realistic, multi-file projects whose `labels.yaml` records what a compiler
would bind. They are written from language semantics, before you look at loom's
output, so the evaluator measures the graph against ground truth, never against
itself.

## Files you own (write)

- `loom/tests/fixtures/source/labeled/{rust,typescript,tsx,javascript,python,go}/**`,
  one directory per dialect, each with source files and `labels.yaml`

Read-only: everything else, and `loom/tests/edge_quality_contracts.rs` (frozen).

## Per corpus

- 4–8 source files, 60–200 lines in total, in a plausible layout for the language
  (`src/` for Rust and TS, a package directory for Python, a module with two packages
  for Go).
- One file under a test directory or name (`tests/`, `*_test.go`, `test_*.py`,
  `*.test.ts`), and the rest production.
- Cover every case in design 14.1:
  - duplicate same-file names;
  - aliases;
  - re-exports;
  - receiver calls;
  - a dynamic receiver;
  - an external import;
  - nested scopes;
  - one syntax-error file;
  - a test file and a production file.
- `labels.yaml`:
  - `declarations` lists **every** declaration a compiler sees that the dialect's
    extractor claims to extract (its `capabilities()`): functions, methods, types,
    interfaces, modules. Syntax-error files have none.
  - `references` lists every call site in production files, with `expect` set to a
    `target` id, `external: true`, or the `ambiguous` candidate set.
    - An `ambiguous` label lists exactly the definitions a reader could not choose
      between without types (for example a dynamic receiver `obj.save()` with two
      `save` methods in the family).
    - Label targets with loom's id rule (`<path>#<kind>:<scope joined ::>`, plus the
      `@<sig8>` suffix when two declarations share an id: compute `sig8` by design 4,
      `sha256` of the whitespace-collapsed first line).
  - `impact` has at least 2 entries (depth 1 and depth 2).
  - A label's `symbol` is the callee's name. The evaluator matches an edge from that
    path whose site covers the labelled line and whose written symbol equals the label's
    symbol or ends with it after a `::` or `.` separator (`util::parse`,
    `Widget::new`).
  - `Thresholds::check` fails a corpus that lacks at least one `target` label, one
    `external` label, one `ambiguous` label, one `impact` label and one
    `syntax_error_files` entry, and any ratio with a zero denominator fails. Every
    corpus therefore carries all five, with the syntax-error file listed under
    `syntax_error_files:`.
- Label from semantics. If loom would bind a call a compiler binds differently, the
  label records the compiler's answer; the evaluator then reports the miss. Never
  weaken a label to match loom.

## Traps

- `external: true` means the definition is outside the corpus: an npm package, `std`,
  or an unlisted crate.
- A Rust corpus needs `src/lib.rs` or `src/main.rs` so `crate::` paths anchor; add a
  `Cargo.toml` stub so the path conventions see a crate root. It is data, not a
  workspace member: put it under `tests/fixtures`, which cargo never builds.
- A Go corpus needs `go.mod` with a module path, and imports that use it.
- Keep every file under 400 lines (the maintainability scanner reads `.rs` under
  `tests/`), and never commit a `.rs` file with a syntax error under `tests/`. Name
  the Rust syntax-error fixture `broken.rs.txt`, and add its path to `labels.yaml`
  under `syntax_error_files:` so E3's evaluator loads it with a `.rs` path. The
  existing fixture `loom/tests/fixtures/source/rust/syntax_error.rs.broken` shows the
  precedent.

## Order of work

Write every corpus and every label first, from semantics alone. Only then run the
check below. It reports where loom disagrees with your labels. Never edit a label
because loom disagrees: a disagreement is either a loom defect (report it) or a
labelling mistake you can show from the language rules (fix it, and say which rule in
your report).

## Your one check (run once)

`cargo run --manifest-path loom/Cargo.toml --quiet -- map --eval-edges loom/tests/fixtures/source/labeled 2>&1 | tail -60`

## Report

Report:

- each corpus's files;
- the label counts (declarations, references by expect kind, impact);
- the evaluator's output for your six dialects;
- each disagreement, classified as a loom defect or a label fix, with the rule that
  decided it.
