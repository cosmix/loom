# gate-noise-and-review: rules every worker follows

Read this file in full, then your own brief. The plan is `doc/plans/PLAN-fewer-disputes.md`
(`IN_PROGRESS-PLAN-fewer-disputes.md` once it runs). Its Decisions 8, 9 and 10 are authoritative
where a brief and the plan differ, except where the stage description lists a correction.

## Where you are

- The stage runs with `working_dir: loom`. Paths in briefs are relative to `loom/` unless they
  start with `../` (the repository root: `../loom-hooks/`, `../skills/`, `../agents/`).
- Line numbers in briefs are advisory. Locate every seam by symbol first:
  `loom map --outline <file>`, `loom map --find-all <name>`, `loom map --impact <symbol>`. A
  named file or symbol that moved: `loom memory note "found: <what moved> (<file:line>)"` and use
  the merged name.
- The stage `criteria-authoring` has merged before this stage. It changed other files in
  `skills/loom-plan-writer/` and the `Stage` model (`consumer_deferred`); read the merged text,
  never your memory of the plan.

## The design in one page

The test-integrity gate (`src/verify/integrity/`) raises events when a stage weakens tests it found
at its base. This stage changes three things and adds one consumer of the result:

1. `TI-edit-<path>` compares assertion STATEMENTS, not diff lines. A statement starts at an
   assertion-regex match (`src/testrun/languages.rs` profile `assertion`) and ends at the first `;`
   or line break at bracket depth zero outside string literals; its comparison key is the text
   passed through the existing `normalize` (`edits.rs`). A file's lost statements are its base
   statements less its head statements, as multisets of keys. Reflows, multi-line asserts and
   rewraps beside real edits raise nothing; a changed or deleted statement raises. Count events
   (`TI-decl-*`, `TI-assert-*`, `count.rs`) stay as they are.
2. A lost statement cancels against an added statement of ANOTHER changed or new test file of the
   same language when both sit in a function node that `context::extract` finds in both files with
   the same last scope segment and an identical function identity: the normalized body PLUS its
   decoration prefix (below). The destination file must still be collected by the runner. No
   grammar, parse error, oversized file or unprofiled language: no cross-file cancellation.
   Recomputed on every scan, never stored.
3. `TI-ratchet-<path>` raises only when the ratchet file LOOSENS (`ratchet_diff.rs`).
4. `loom hook reviewer-brief` prints a brief of the stage's current integrity events; the spawn
   guard appends it to every `loom-code-reviewer` spawn of a live stage session. For an
   integration-verify stage it adds plan-wide context. Integration-verify's parallel review gains a
   Performance dimension.

## Function identity and collection (cross-file moves keep the mechanical floor)

A moved test that stops running is a weakened test, so cancellation requires that the moved
function is the same AND still runs.

- Identity = `normalize(decoration prefix + "\n" + text from the start of the span's first line to
  span.start_byte + body)`. The decoration prefix is the contiguous lines directly above the span's
  first line whose trimmed text starts with `#[`, `#![`, `@`, or is a bare `[Name...]` attribute
  line (C#, Swift-style); blank lines and comments end the prefix. The text between the line start
  and `span.start_byte` carries a JS/TS wrapper such as `it.skip("x", `. The extractor spans start
  at `fn`, `def`, the method name or the arrow function, so attributes and decorators sit OUTSIDE
  them and need this prefix. Per language: Rust `#[ignore]` (prefix); Python `@pytest.mark.skip`,
  `@unittest.skip` (prefix); Java `@Disabled`, Kotlin/Scala `@Ignore` (prefix, or inside the span
  when the grammar puts modifiers in it: both are covered); C# `[Ignore]`, `[Fact(Skip="x")]`
  (prefix or span); PHP `#[...]` and `markTestSkipped` (prefix, body); Go `t.Skip` and Ruby `skip`
  (body); JS/TS `it.skip`, `test.skip`, `describe.skip`, `xit` (line head before the span; an
  anonymous callback has no named function node, so it never cancels); Elixir `@tag :skip`, Dart
  `skip:` (prefix, body). The same function in the source and destination must have an identical
  identity, so a gained or lost skip marker makes a move raise.
- Collection of a destination file ABSENT at base (a file that existed at base is collected as it
  was): Rust only when a `mod` chain from a crate root or a `tests/` target reaches it, checked
  by `crate::verify::contracts::collection::rust::chain_problems(source, file, test)` (empty means
  reached; `source` reads head files; `test` is the module path of the file plus the function's
  scope joined by `::`). Every other language only when the file path matches one of
  `profile.test_file_globs` (`src/testrun/languages.rs`; `for_path` already maps the path to a
  profile, so check the globs through the same matcher `count::base_test_files` uses). Not
  collected: no cancellation, the source file raises.

## The accepted-event pin (stays; one addition)

An accepted `TI-edit` or `TI-ratchet` event is pinned to the file's sha256
(`IntegrityEvent.current_sha256`, compared in `gate.rs::shortfall`). That stays byte for byte.
Cross-file cancellation adds a hole the file's own hash cannot close: file A loses statements x
(moved to B, cancelled) and y (really deleted, event accepted at A's sha); B is then edited so x no
longer matches; A's lost set grows to {x, y} while A's sha is unchanged, so the old acceptance
passes. Decided: when at least one statement of A was cancelled against another file,
`current_sha256` of A's event is `sha256_hex` of the text
`<A sha>\n<partner path>\t<partner sha>\n...` over A's partner files sorted by path, where a
partner is every file that supplied a cancelling statement. An event with no partner keeps the
plain file sha256. Owner: W1.

## Ownership

- You write only the files your row of the stage's worker table owns. A compile error in a file no
  row owns goes to the row whose change caused it; record it with `loom memory note`.
- Two workers never write one file. Need a change in another worker's file: stop and report the
  exact change.
- Never run git, never write under `.loom/`, never spawn subagents. Codex units never run `loom`
  state commands beyond the navigation kit.

## Code rules

- Files at most 400 lines, functions at most 50 lines (`cargo test --test maintainability` scans
  `src/` and `tests/`; `maintainability-baseline.txt` is a ratchet file and is never edited).
  Split by concern into a sibling module before a file crosses 400.
- A value that arrives from an agent, a file or the wire is untrusted: strip control characters with
  `crate::verify::review::report::single_line` or `crate::models::dispute::strip_control`, cut text
  with `crate::models::dispute::truncate_to_byte_limit` (never a byte slice), validate ids with
  `crate::validation::validate_id` before any path join.
- Errors: `anyhow` with `.context(...)` at the boundary. A hook that must fail open returns
  `Ok(())` after printing `{}`; it never prints an error the guard could mistake for a brief.
- No new dependency, no new configuration key, no placeholder, stub or `todo!()`. Remove what your
  change orphans (`hunks`, `lost_assertions` in `edits.rs` become dead).
- Doc comments state current behaviour. No dated history, no "was", no "used to".

## Tests

- In-crate tests sit in a `*_tests.rs` file beside the module (`#[cfg(test)] #[path =
  "x_tests.rs"] mod tests;`).
- Git fixtures: `git init -b main` in a `TempDir`, local identity, and the isolation of
  `src/verify/integrity/tests.rs::Repo` (it calls `crate::git::runner::run_git_checked`).
- Tests that set process environment variables are `#[serial_test::serial]` and restore them.
- No test needs tmux, the network or the real `HOME`.
- Frozen contract files (`tests/integrity_noise_contracts.rs`, `tests/reviewer_brief_contracts.rs`,
  `tests/reviewer_brief_guard_contracts.rs`) are never edited. `loom stage contracts show
  gate-noise-and-review` lists them.

## Checks

You run at most ONE narrowly scoped check over the files you wrote (for example
`cargo test --lib verify::integrity::statements::`), once. The orchestrator runs
`loom check gate-noise-and-review`.

## Memory

Record mistakes, decisions and surprises immediately with `loom memory note` / `loom memory
decision`, including every symbol or path that differed from the brief. Record
`stale-knowledge:` notes for knowledge sections that describe the old rules (for example
`architecture/verification-v2-gates.md`, "TI-edit matches removed assertion lines against added
lines"). Never `loom knowledge`; never Claude Code auto-memory.
