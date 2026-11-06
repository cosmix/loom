# R1: trace wire, routes and trace_changed (trace-server)

Read `../common.md` "Wire" and "Routes" in full; they are your specification.

## Files you own

`loom/src/plan_server/trace/**` (except `contract_tests.rs`, frozen), `wire/trace.rs` (+
`wire/trace/`), `wire/fixtures/trace.json`, `wire/mod.rs`, `wire/message.rs`,
`wire/tests_schema.rs`, `routes.rs`, `watch.rs`, `plan_server/mod.rs`,
`web/src/plan/api/schema-trace.ts`, `schema-trace.test.ts`, `fixtures/trace.json`,
`web/src/plan/api/schema.ts`.

## Tasks

1. **Wire.** `TraceView` and its parts from `planning::trace::Trace` (`From<&Trace>`), as
   common.md says. Paths as repository-relative strings with `/` separators.
2. **Writer test.** A test builds the DEMO fixture of `loom/tests/planning_trace_contracts.rs`
   plus a settled section, a `findings.md` with one open and one dismissed finding, and a stage
   line, serializes the `TraceView` pretty-printed with a trailing newline, and compares it to
   `wire/fixtures/trace.json`. When the env var `LOOM_WRITE_FIXTURES=1` is set it writes the file
   instead. Copy the bytes to `web/src/plan/api/fixtures/trace.json`.
3. **Routes** in `plan_server/trace/` (`mod.rs` with `routes()` and `install`, one module per
   route family): register with one line in `routes.rs::all_routes`. Settle and status build the
   new text with `planning::trace::settle` and `planning::findings::set_status`, then call
   `files::save::save_document` with the request's hash, so authoring, the lock, the hash check,
   locked documents and symlinks follow the save route. Map `SaveOutcome` and errors the way
   `save_route` does. Validate `path` exactly as save does. The settle route reads the project's
   `TraceDoc` for the path to get `current`, `my_calls` and `open_questions`; a path with no
   `TraceDoc`, or one whose role cannot be settled, is 400. `settled-version` runs
   `git log -S<line> -1 --format=%H -- <path>` through `crate::git::runner::run_git`, with the
   copy-then-drop-the-read-guard rule
   (`plan-server.md` Review Loop) and answers `{ commit: null }` when git finds nothing or fails.
4. **trace_changed.** In `watch.rs::apply_batch` the checklist views are compared inside the
   `ctx.project_mut()` block (`before != after`); compare the trace views in the same block.
    `PlanMessage::TraceChanged { feature }` (`PlanMessage` derives `Clone, Debug, PartialEq`, so the
   variant's fields need those; an arm in
   `wire/tests_schema.rs::all_plan_messages`, and `trace_changed` in `schema.ts`'s
   `PLAN_MESSAGE_TYPES` and message schemas). In `watch.rs`, after a batch is applied, compare
   each feature's `TraceView` with the previous one and broadcast for those that changed.
5. **Web schema.** `schema-trace.ts`: strict zod schemas mirroring the wire, exported types, and
   `schema-trace.test.ts` parsing the fixture. Add `review_feature` to `OFFER_KINDS` in
   `schema.ts` for R2.

## Tests

Route tests beside the module (conflict, unresolved, locked, unknown id, a planned detail that
is not a plan token, a folded detail naming no entry, viewer role on settle refused), the writer
test, and `schema-trace.test.ts`. The six route contracts in `trace/contract_tests.rs` are frozen.

## Done

`cargo test --manifest-path loom/Cargo.toml --lib -- plan_server::` and the web schema test pass;
`wire/message.rs` stays under 400 lines.
