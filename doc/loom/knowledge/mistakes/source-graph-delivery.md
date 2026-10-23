# Source Graph Delivery

> Resolver, cache, wiring

Mistakes made while building the evidence-classed, dialect-aware source graph (resolver, resolved view, retrieval
routing, edge-quality evaluator). Current behaviour is in [Source Graph](../architecture/source-graph.md),
[Resolution](../architecture/source-graph-resolution.md) and [Resolved View](../architecture/source-graph-view.md).

## A Hand-Built Resolver Fixture Named a File After the Symbol Under Test

**What happened:** a `tests_rules` lone-candidate fixture put the caller in `src/run.ts` while calling `run`. The test
failed because a `File` node answers to its stem, so the file itself became a second candidate and the lone-candidate
case was no longer lone.

**Why:** `record.rs` computes the keys a node answers to, and file nodes answer to their file name and stem.

**Prevention:** in hand-built resolver fixtures never name a file after a symbol under test, and assert that the
expected node exists before asserting a bind.

**Fix:** the caller moved to `src/main.ts`.

## Narrowing Candidates Across Files Turned Ambiguity Into a False Bind

**What happened:** twice in one stage (the implementer, then the fixer), a constructor-to-type pairing keyed by
`(family, scope)` let one constructor make namesake types in other files yield, so an ambiguous call became a
`UniqueName` bind to the wrong type. Keying by file instead broke C++ out-of-line constructors (class in `.h`,
constructor in `.cpp`).

**Why:** the index grouped types across files, so the pairing either over- or under-matched.

**Prevention:** any "X yields to Y" candidate narrowing must pair only when exactly one type in the family has that
scope, and a test must pin both the cross-file namesake case (Java) and the header/implementation case (C++).

**Fix:** the pairing rule in [Resolution](../architecture/source-graph-resolution.md#cross-file-resolution-rules).

## Cache Mutation Outside `GraphStore` and a Currency Check That Skipped a Variant

**What happened:** clearing a stale or corrupt base with `path.exists()` then `fs::remove_file()?` in `ensure_base`
broke the read-only-cache memory fallback and raced concurrent snapshots. Separately, `Oversized` file nodes carry
`LEXICAL_PARSER_VERSION` but the currency checks compared them to the extractor identity, so a layer holding a file over
512 KiB was never current.

**Why:** the briefs covered `Gap` and `Unknown` but not `Oversized`, and did not mention the fallback path.

**Prevention:** keep the currency predicate in ONE function (`entries_are_current`) that covers every coverage variant
stamped with `LEXICAL_PARSER_VERSION`, and route any cache mutation through `GraphStore` so `fall_back_to_memory`
applies. A test for a new degraded path must assert `parser_version` and currency across `ensure_snapshot` and
`build_worktree_graph`.

**Fix:** `GraphStore::replace_base` and `entries_are_current`.

## A Worker Ran `git mv` Despite the Subagent Git Restriction

**What happened:** a worker turning the single-file coverage module into a `coverage/` directory ran `git mv`, a git write, and then reset the index.

**Why:** a file move looked like a rename task, not a git operation.

**Prevention:** briefs that move a file say "create the new file and delete the old one with file tools; never `git mv`".
See [Subagent Briefing](subagent-briefing.md).

## Stage Wiring Patterns Were Not Quoted in the Brief (Twice)

**What happened:** in `map-api-freshness` a worker wrote `print_freshness_line` through a local `state` binding, so the stage
wiring pattern `state\(\)\.as_str\(\)` in `knowledge/context.rs` missed. In `resolved-view` a fix brief did not name the
wiring pattern `graph_store\.view\(` in `refresh/snapshot.rs`; the fixer replaced the call with `materialize_view` and the
stage failed goal-backward verification after the commit.

**Why:** the wiring patterns were not quoted in the worker briefs; two stages repeated the miss.

**Prevention:** paste each stage wiring pattern into the brief of the worker that owns that file, in every fix brief that
touches a wired file, and run `loom check <stage> --suggest` before committing. A recurrence: the proposal for a check that
warns when a brief for a wired file omits the pattern is in [Review Backlog](../concerns/source-graph-review-backlog.md#smaller-items).
See also [Pinned Literals, Ledgers and Wiring](pinned-literals-ledgers-and-wiring.md).

## A Formatter Pass Broke a Size Cap and a Frozen Contract

**What happened:** running `cargo fmt` after a worker added `graph_cache_budget_bytes` reflowed a match arm in
`RetrievalConfig::apply` to 52 lines and failed `tests/maintainability` (function cap 50). Separately,
`tests/retrieval_delivery_contracts.rs` was frozen without rustfmt, so the stage criterion `cargo fmt --all -- --check` and
the frozen hash could not both hold.

**Why:** the worker left the arm one line short of the cap unformatted; the contract session froze unformatted code.

**Prevention:** run `cargo test --test maintainability` after any fmt pass. Contract sessions run rustfmt on contract files
BEFORE freezing; orchestrators do not run `cargo fmt --all` over frozen files without checking the hash
(`loom stage contracts show`).

**Fix:** split the graph-cache and reconcile keys into `apply_graph_key`.

## An Eval Case Without `expect`, `forbid` or `abstain` Fails Three Tests

**What happened:** a worker added the eval case `relationship-callers-of-reconcile-source-graph` with only `relevant` ids;
`load_cases_file` rejects a case lacking `expect`, `forbid` and `abstain`, failing three `tests_abstention` tests.

**Why:** the worker could not run the eval and did not run the `commands::knowledge::eval::` tests.

**Prevention:** after editing `loom/eval/retrieval-cases.yaml`, run `cargo test --lib commands::knowledge::eval::`.

## A Helper That Returns a Borrow Must Be a `fn`, Not a Closure

**What happened:** a closure `|file: &str|` returning a sub-slice of its argument (`bindings.rs` `names_module`) failed to
compile: closure signatures do not elide the output lifetime to the input.

**Prevention:** a helper that returns a borrow of its `&str` parameter is a `fn`. **Fix:** `module_of`.

## An Edit That Ended Mid-Line Corrupted a Shell Block

**What happened:** an `Edit` whose `old_string` ended mid-line (`for p in`) and whose `new_string` ended with a line-continuation
backslash left the rest of the original line after it, producing `\ "$R"`, an escaped space glued onto the next word; a probe
script silently tested `/path` as absent, and a review round caught it.

**Prevention:** after editing a fenced shell block, extract it and run `bash -n` and the script itself before handing it on.

## A Command Was Described From a Stale Doc Comment

**What happened:** the user was told `loom stage complete --no-verify` marks a stage `CompletedWithFailures`, from the doc
comment at `complete.rs:311`. The code (`complete.rs:694-742`) calls `try_complete`, and the daemon merges and triggers
dependents.

**Why:** the answer came from a comment without reading the branch.

**Prevention:** read the code path before stating what a command does; treat a doc comment as a claim. The stale comment
is listed in [Review Backlog](../concerns/source-graph-review-backlog.md#smaller-items).

## After an Extractor Change Alters Ids, Checked-In Ids Go Stale

**What happened:** after the generic-impl fix, Rust methods in `impl<'a> Foo<'a>` and `impl<T> Foo<T>` got ids
`path#function:Foo::method`. `loom/eval/retrieval-cases.yaml` still named `merge_lifecycle.rs#function:reconcile` (now
`MergeLifecycle::reconcile`) and `admission.rs#function:remaining` (now `DeadlineReader::remaining`); the second was a
`forbid` id that could silently never fire.

**Why:** hand-written ids in eval cases and labelled corpora are not tied to the extractor that produced them.

**Prevention:** after any extractor change that alters ids, grep the checked-in ids (`loom/eval/retrieval-cases.yaml`, the
labelled corpora) and confirm each against the new binary's graph. A `forbid` case that names a nonexistent id proves
nothing.

## Stage Sandbox and Tooling Gotchas From the Graph Plan

- **Contract sessions and size caps.** A frozen contract function over 50 lines fails `cargo test --test maintainability`,
  and the ratchet baseline is a plan `ratchet_file`; the stage's own criterion becomes unsatisfiable without editing a
  frozen file. Run the maintainability test over contract files before freezing.
- **Worker checks.** `subagent-verify-guard` blocks even a brief's single `cargo build --all-targets`, so name a narrower
  check (`cargo test --lib <module>::`) or none; with parallel engineers a worker's one `cargo check` can be aborted by
  another engineer's half-written files, so run it last and report it inconclusive when the errors lie outside the
  worker's files.
- **`git mv`** by a worker leaves the rename staged and the pre-commit hook then refuses any earlier partial commit
  (`staged paths also have unstaged changes`): `git restore --staged` both paths first.
- **`cargo add` in a stage sandbox** fails with `failed to persist temporary file: Device or resource busy` because
  `loom/Cargo.toml` and `loom/Cargo.lock` are individual bind mounts; run it in a `$TMPDIR` copy of the package (those two
  files copied, every other entry symlinked) and copy the two files back in place. The crates.io sparse index IS reachable
  through the sandbox proxy even when the signal says "No network access".
- **`clippy::needless_update`.** A plan amendment that rewrites full struct literals to `..Default::default()` trips it
  wherever every field is set; drop fields equal to the default, else `#[allow(clippy::needless_update)]` (never
  `expect`, which fails once a field is added).
- **Waiting.** `loom subagents watch` exits 5 (`worker set does not resolve to one Claude parent UUID`) for named
  Agent-tool spawns, and exited 6 (hung) for worker sets that had all handed back; wait on the Agent tool's completion
  notifications instead. A reviewer spawned with the Agent tool's `name` parameter ran as a teammate and went idle, so no
  review round was recorded; spawn reviewers unnamed.
- **Tests run from a copy** under `/tmp/claude-*` fail ten tests for location reasons only (`hooks_subagent_verify_guard`
  refuses a path containing `claude`, `relay_e2e` refuses a scratch root under `/tmp`, `version::derive` needs a git build
  commit). Measure the suite baseline in the main checkout or a worktree.
- **Fixtures for `core.fsmonitor`.** `git ls-files` and `git check-attr` run the monitor only once the index carries the
  fsmonitor extension, which the first `git status` after setting the key writes; prime with a plain `git status` before
  asserting the hook fires (`git/runner/pinned/tests.rs`).
- **clap.** `requires = "census"` on a bool flag is satisfied by its default; enforce `--root needs --census` in
  `require_view` (`commands/map.rs`).
- **Markdown lint.** The pre-commit markdown lint (`markdownlint-cli2` via `bunx`) cannot reach `registry.npmjs.org` from a
  stage sandbox, so stage commits print `markdown was NOT linted`; lint before pushing.
- **Grammar quirks.** `tree-sitter-c-sharp` emits `this` and `base` as anonymous tokens, so `(_)` misses them as
  `@call.receiver`; the query uses `[(_) "this" "base"]`. Java `this`/`super` are named nodes. `tree-sitter-cpp 0.23.4`
  reports an ERROR on the common `extern "C"` header guard.
