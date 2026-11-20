# test-guards / W2 — impact-selected tests

Read first: `doc/plans/briefs/verification-v2/DESIGN.md` D0, D5 (`select_command`), D6, D7, D14,
D17. Knowledge: `architecture/source-graph.md` (what edges exist, confidence);
`architecture/token-accounting-and-receipts.md` (the criterion cache). Code:
`context/worktree_graph.rs::build_for_worktree` (returns the graph and the changed paths),
`context/resolve/impact.rs::impact_with`, `testrun::registry`, `skills::project` detection
(`ProjectProfile::package_details`), `verify/criteria/runner.rs::run_with_cache`.

## Files you own

`loom/src/verify/impact_tests.rs` (new), `loom/src/verify/impact_tests_tests.rs` (new).

## API you publish (pinned for W3)

```rust
pub struct ImpactOutcome { pub ran: Vec<String>, pub notes: Vec<String> }  // commands run; skipped/degraded notes
pub fn run(stage: &Stage, working_dir: &Path, criteria_config: &CriteriaConfig) -> Result<ImpactOutcome>;
```

Use the criteria runner's config type as `run_with_cache` takes it; read its signature first.

## Tasks

1. `build_for_worktree(working_dir)`. Take every node in a changed file. `impact_with` each
   (kinds and limits as D11, no depth limit). Keep hits whose file matches its language
   profile's `test_file_globs` (`languages::for_path`). Map each to a
   `TestTarget { file, name }`: the name is the hit node's qualified test path where the
   adapter selects by name (cargo), and `None` elsewhere.
2. Group targets by the adapter detected for their package. Drop the stage's own contract test
   files, which the contract check already ran.
3. For each group, `adapter.select_command(targets, package_dir)`. `None` ⇒ a note ("<adapter>
   cannot select tests by file; the full suite runs in integration-verify"). Otherwise run it
   through `run_with_cache` (300 s) and classify: `Failed`/`BuildFailed` ⇒ an error naming the
   command and the failing tests; a timeout ⇒ a note, not an error.
4. For cargo, derive the libtest path of a Rust test node from the graph's module structure (the
   node's containing modules up to the crate root). When a hit's path cannot be derived, leave
   it out and add a note. Never guess a path.

## Named tests (binding), in `impact_tests_tests.rs`

- `impact_selection_runs_reached_tests`: a temp Rust crate with `src/lib.rs` (`pub fn add`) and a
  test in `tests/add_test.rs` calling `add`, plus an unrelated test file. Change `add` in the
  worktree → the selection's command names the `add` test and not the unrelated one. Use an
  injected executor so the test runs no cargo.
- `impact_selection_skips_unsupported_adapters`: the changed file is reached only from a test in
  a package whose adapter returns `None` from `select_command` → no command, one note.

## Proof (one command, once)

`cargo test --manifest-path loom/Cargo.toml --lib verify::impact_tests`

## Report

Files changed; the public API as written; the proof result.
