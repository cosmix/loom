# W3: `loom knowledge bootstrap` command, session prompts, CLI, integration test

Plan: `doc/plans/PLAN-knowledge-bootstrap-command.md`. Its "Design decisions (settled)" table is
binding; read it first. The crate is `loom/`; run cargo from `loom/`.

Wave 1 is finished:

- W1 added `crate::claude::{classify_exit, run_foreground, ClaudeOutcome, ExitAction,
  AGENT_TEAMS_ENV}` in `loom/src/claude/session.rs`. `classify_code` and `send_sigterm` are
  private to `session.rs`. `run_foreground` returns `Completed` whenever the marker exists,
  even if the child already exited.
- W2 added `bootstrap/clusters.rs` (`FileFacts`, `Cluster`, `file_facts`, `fan_in`, `partition`,
  `MAX_CLUSTER_FILES`, `MIN_CHILD_CLUSTER_FILES`, `KNOWLEDGE_PREFIX`) and `bootstrap/receipt.rs`
  (`Receipt`, `ReceiptCluster`, `ClusterStatus`, `RefreshPlan`, `refresh_plan`,
  `RECEIPT_FILENAME`). `file_facts` drops every path under `doc/loom/knowledge/`.

Read the real signatures with `loom map --outline` on those files before writing a call. The main
agent's report on wave 1 may note deviations from the planned API.

## Files you own (exclusive)

- `loom/src/commands/knowledge/bootstrap/mod.rs` (grow it; keep the existing `mod` lines)
- `loom/src/commands/knowledge/bootstrap/prompt.rs` (new)
- `loom/src/commands/knowledge/bootstrap/graph.rs` (new; register it in `mod.rs`)
- `loom/src/commands/knowledge/bootstrap/tests.rs` (new; register it in `mod.rs`)
- `loom/src/commands/knowledge/mod.rs` (only if needed beyond the existing `pub mod bootstrap;`)
- `loom/src/commands/knowledge/sync.rs` (visibility only)
- `loom/src/cli/mod.rs` (one export line), `loom/src/cli/types_memory.rs`, `loom/src/cli/dispatch.rs`
- `loom/src/context/retrieve.rs` (one message constant)
- `loom/tests/integration/mod.rs`, `loom/tests/integration/knowledge_bootstrap.rs` (new),
  `loom/tests/integration/knowledge_bootstrap_support.rs` (new, only if needed; see Tests)
- `loom/src/completions/dynamic/tests/tests_commands.rs` (one list edit, below)

Also remove W2's temporary `#![cfg_attr(not(test), allow(dead_code))]` from `clusters.rs` and
`receipt.rs` once your code calls them. That one-line removal is the only edit you make to W2's files.

## CLI

In `KnowledgeCommands` (`loom/src/cli/types_memory.rs:25`; line 24 is its derive), add a variant next to `Sync` (118).
Mirror the doc-comment style of its neighbours:

```rust
/// Build the knowledge base for this repository: scaffold, index the source graph,
/// plan exploration by directory cluster, then run an interactive Claude session
/// that writes knowledge through the loom knowledge CLI
Bootstrap(BootstrapArgs),
```

Define `BootstrapArgs` (clap `Args`) in `types_memory.rs` next to `AnnotateArgs` (line ~149):

- `--structural-only`: no model; build indexes and print coverage and the plan.
  `conflicts_with_all = ["dry_run", "model", "effort"]`.
- `--refresh`: explore only clusters changed since the committed receipt, plus
  template-only tier-1 files.
- `--dry-run`: print the plan, write the brief, and print the exact claude command without
  running it.
- `--model <MODEL>`: validate against `crate::claude::CLAUDE_MODELS`. Copy the attribute at
  `loom/src/cli/types_pressure.rs:36`:
  `value_parser = clap::builder::PossibleValuesParser::new(crate::claude::CLAUDE_MODELS)`.
- `--effort <EFFORT>`: validate against `crate::models::stage::ALLOWED_REASONING_EFFORTS`. Copy
  the attribute at `types_pressure.rs:40`.

`types_memory` is a private module (`cli/mod.rs:9`), and `cli/types.rs:7` re-exports only the
command enums, so `BootstrapArgs` is not nameable from `commands::knowledge` until you change
`cli/mod.rs:16` to exactly `pub(crate) use types_memory::{AnnotateArgs, BootstrapArgs};`. In
`bootstrap/mod.rs` write `use crate::cli::BootstrapArgs;` (precedent
`commands/knowledge/annotate.rs:3`). Plan wiring checks grep both lines.

Dispatch in `dispatch_knowledge` (`loom/src/cli/dispatch.rs:27`):
`KnowledgeCommands::Bootstrap(args) => knowledge::bootstrap::execute(args),`. The plan wiring
check greps `KnowledgeCommands::Bootstrap` in `dispatch.rs`. If `dispatch_knowledge` is no
longer in `dispatch.rs` (a sibling plan may have moved it), stop and report; the main agent
re-points the plan's checks first.

In `loom/src/completions/dynamic/tests/tests_commands.rs`, move `"bootstrap"` from the "gone"
knowledge-subcommand list (:62-75, the entry at :69) to the present list in that test (:56). Completions
are built from `Cli::command()` (`completions/dynamic/commands.rs:46`), so the new subcommand
makes the old assertion fail.

## `bootstrap/mod.rs`: `pub fn execute(args: BootstrapArgs) -> Result<()>`

Keep every function under 50 lines and the file under 400. Split helpers into `prompt.rs`
(all prompt and brief text) and keep orchestration in `mod.rs`. Steps, in order:

1. **Guards.** If `LOOM_STAGE_ID` is set and non-empty (read it the way
   `commands/hook/target.rs:35` does, with `non_empty_env`), bail with
   `loom knowledge bootstrap is an operator command; inside a stage use loom knowledge update (unset LOOM_STAGE_ID if this shell is not a stage session)`.
   Resolve `repo_root` with `git rev-parse --show-toplevel` (mirror
   `commands/pressure/paths.rs:23-39`, including `crate::git::runner::NO_HOOKS_ARGS`, but
   WITHOUT the cwd fallback). Outside a git repo, bail with
   `loom knowledge bootstrap must run inside a git repository`. When
   `git rev-parse --verify HEAD` fails, bail with
   `loom knowledge bootstrap needs at least one commit` (the enumerator runs
   `git ls-tree -r -z HEAD`, `enumerate.rs:33`, and a repo with no commits degrades to an empty
   graph).
2. **`.loom/` hygiene** (`ensure_loom_ignored(&repo_root)`): write `repo_root/.loom/.gitignore`
   containing `*\n` only when ALL of these hold: `git ls-files -z .loom` prints nothing, the file
   does not exist, and (`git check-ignore -q .loom/cache` exits non-zero OR
   `git check-ignore -q .loom/work` exits non-zero). Observed exit codes: fresh repo 1; `.loom/`
   ignored 0; only `.loom/work` ignored gives 1 for the `.loom/cache` check. Print one line when
   you write it. Never touch the repo's own `.gitignore`.
   Then take the run lock in every mode: `acquire_run_lock(&repo_root) -> Result<File>` creates
   `repo_root/.loom/work/bootstrap/`, opens `.lock` there and calls
   `fs2::FileExt::try_lock_exclusive` (precedent `git/merge/lock.rs:4,53`). On failure bail
   `another loom knowledge bootstrap is already running in <repo_root>`. Bind the returned
   `File` to `_run_lock` in `execute` so it is held until `execute` returns (a wiring check
   greps `acquire_run_lock(` in `mod.rs`). Never hold the
   knowledge-directory lock across the session: the child writes knowledge through
   `loom knowledge update`.
3. **Scaffold.** `let knowledge = KnowledgeDir::new(&repo_root); let fresh = !knowledge.exists();
   knowledge.initialize()?;` (`fs/knowledge/dir.rs:22,44,86`). ALWAYS call `initialize()`. It is safe on an existing dir: `dir.rs:96` uses
   `create_new(true)`, and `INDEX.md` is written only when fresh (:111-113). Skipping it would
   make the gap check error on a missing tier-1 file in a partially populated dir. Print
   `scaffolded doc/loom/knowledge/` only when `fresh`. Then `let (knowledge_root, store) =
   crate::context::retrieve::resolve_roots(&repo_root)?;` (`context/retrieve.rs:147`,
   `pub(crate)`).
4. **Catalog and graph refresh.** Make `upgrade_flat_layout`, `refresh_index_best_effort` and
   `print_human` in `commands/knowledge/sync.rs` `pub(super)` (visibility only; no behaviour
   change). Call the same sequence `sync()` runs (`sync.rs:50-56`), with `structural_only = false`
   ALWAYS. Bootstrap's `--structural-only` means "no model", not "skip the source graph", and
   the graph is needed for clusters. Then `print_human(&outcome, upgraded)`.
5. **Load the graph** in `bootstrap/graph.rs`. Do NOT call or edit `commands/map.rs`:
   its `load_graph` (:217-236) prints an `Unavailable` snapshot and resolves whatever layers
   exist, so a failed overlay yields the stale base and a missing base an empty graph. Write
   `pub(super) fn load_current_graph(repo_root: &Path) -> Result<ResolvedGraph>` that mirrors
   map.rs:217-236 with the same imports (`ContextStore::open(&WorkDir::new(repo_root)?)`,
   `store.ensure()`, `GraphStore::new`, `ensure_snapshot(.., SnapshotPolicy::LocalCurrent)`,
   `graph_store.resolved(..)`, `resolve_graph`), except that it calls
   `require_snapshot(&snapshot)?` before resolving. `pub(super) fn
   require_snapshot(outcome: &SnapshotOutcome) -> Result<()>` is pure: it bails with
   `source graph unavailable: <outcome.reason>; nothing was spawned and no receipt was written`
   when `outcome.action == SnapshotAction::Unavailable` and is `Ok` otherwise (a wiring check
   greps `SnapshotAction::Unavailable` in `graph.rs`). `WorkDir::new` creates nothing
   (`fs/work_dir.rs:123-165`). The snapshot is reused (cheap) after step 4. Print
   `crate::context::CoverageReport::of(&graph)` (its `Display` is the one-line coverage summary).
6. **Plan.** `let facts = clusters::file_facts(&graph); let clusters = clusters::partition(&facts,
   &clusters::fan_in(&graph));`. The wiring check greps `clusters::partition(` in `mod.rs`.
   Compute tier-1 gaps: every `KnowledgeFile::all()` (`fs/knowledge/types.rs:113`) whose file
   content, trimmed, equals `templates::default_content(file)` trimmed
   (`fs/knowledge/templates.rs:30`). Find the file path accessor with
   `loom map --outline loom/src/fs/knowledge/dir.rs`.
   `let receipt = Receipt::load(&knowledge_root)?;`. Work set: without `--refresh`, every cluster.
   With `--refresh` and a receipt, clusters whose status is New or Changed. With `--refresh` and
   no receipt, print `no receipt; running a full bootstrap` and use every cluster. Compute it in
   a pure `work_set(clusters, plan, refresh: bool)` so it is unit-testable.
7. **Report.** Print a table: cluster id, files, symbols, status (`new`/`changed`/`unchanged`,
   shown only with `--refresh`), and hot files. Then removed cluster ids, tier-1 gaps, and the
   estimate line: `work: N clusters, F files, S symbols, G template-only tier-1 files`. Do not
   print a dollar estimate.
8. **Early exits.** `--structural-only`: print `structural-only: no model run` and return `Ok`,
   writing no brief and no receipt. If the work set is empty, there are no gaps, AND no clusters
   were removed, print `knowledge is current (receipt <completed_at>, revision <source_revision>)`
   and return `Ok` without spawning. A change that only deletes a directory must still brief the
   session; otherwise the receipt keeps the removed id forever.
9. **Brief.** Write `repo_root/.loom/work/bootstrap/brief-<pid>.md` (create the dir) from
   `prompt::render_brief(...)`. The marker is `repo_root/.loom/work/bootstrap/claude-<pid>.done`,
   with `<pid> = std::process::id()`. The integration test's fake claude relies on this exact
   marker path. Resolve model and effort: flag, else
   `let user = UserConfig::load();` then `user.stage_model(StageType::Knowledge).to_string()` /
   `user.stage_reasoning_effort(StageType::Knowledge).to_string()`. `UserConfig::load()` returns
   `UserConfig`, not a `Result` (`user_config/mod.rs:207`), and both accessors return `&str`
   borrowed from it (`user_config/models.rs:89,98`).
10. **Argv** (`prompt::claude_args(brief, marker, model, effort) -> Vec<String>`), in exactly this
    order:
    `--permission-mode auto --model <m> --effort <e> --disallowedTools Edit,Write,NotebookEdit
    --append-system-prompt <prompt::system_prompt(marker)> <prompt::initial_prompt(brief)>`.
    `--disallowedTools` is variadic, so its value must be ONE comma-joined string, and a
    non-variadic flag (`--append-system-prompt`) must sit between it and the positional prompt.
    Unit-test the order.
11. **`--dry-run`.** Print `brief: <path>` and `claude <argv joined by spaces>` (mirror
    `pressure/mod.rs:149-160`), then return `Ok`.
12. **Spawn.** `crate::claude::find_claude_path()`, bailing with `claude not found; install Claude
    Code or use --structural-only` on error. Then
    `crate::claude::run_foreground(&claude_path, &repo_root, &args, &marker)`, written fully
    qualified (wiring check).
13. **Finalize** (always, even after a non-zero exit): run step 4's refresh again (index and
    catalog pick up the session's writes), then print a one-line issue count from
    `crate::fs::knowledge::catalog::build(&knowledge_root)` (`catalog.rs:153`; find the issues
    field with `loom map --outline`) with the hint `run loom knowledge check for details`.
    Delete the brief on EVERY exit path after the spawn (Completed, Continue, Abort, Warn); only
    `--dry-run` keeps it. Then branch on the outcome:
    - `ClaudeOutcome::Completed`: save `Receipt::from_clusters(&clusters, &graph.base_revision,
      model, effort)` (ALL current clusters, because unchanged ones keep their digest) and print
      `receipt: doc/loom/knowledge/.bootstrap-receipt.json (commit it with the knowledge)`.
    - `Exited(status)` with `classify_exit` giving `Continue`: warn `session ended without
      signalling completion; receipt not written`, then `Ok`.
    - `Abort`/`Warn`: bail `claude exited with <code or signal>; receipt not written`.

In `context/retrieve.rs:84`, set `NO_KNOWLEDGE_DIR` to
`"Knowledge directory not found. Run 'loom knowledge bootstrap' (or 'loom init <plan>') to create it."`.
Before changing it, find tests that pin the old text with
`rg -n "Run 'loom init' to create it" loom` and update them.

## `prompt.rs`: text content (settled)

Write in plain, direct English without filler. `system_prompt(marker)` (appended; keep it short):

- You are bootstrapping this repository's loom knowledge base (`doc/loom/knowledge/`) for future
  coding agents. Read the brief file named in the first message in full before anything else.
- Write knowledge ONLY with `loom knowledge update|replace-section|annotate` run from the repo
  root. File-editing tools are disabled. Never modify source files, commit, or create branches.
- Before writing, read `doc/loom/knowledge/INDEX.md` and the existing sections you would touch;
  keep human-written entries; correct stale claims with `replace-section` and name the wrong
  claim in the replacement.
- Tier routing: a finding of about 40 lines or fewer goes into its tier-1 file
  (architecture, entry-points, patterns, conventions, mistakes, stack, concerns). A larger one
  goes to `loom knowledge update <category>/<slug>`, plus
  `loom knowledge annotate <category>/<slug> --blurb "<at most 80 chars>"`, plus a 2-4 line summary
  and link in the tier-1 file.
- Every claim cites `path:line` evidence that you or a subagent read. Record only durable facts:
  architecture, entry points, patterns, conventions, stack, real concerns. Leave out anything
  git history or a quick grep answers.
- Completion: reuse the wording of `commands/pressure/spawn.rs::completion_instruction` (lines
  49-59), adapted to say the session was launched by `loom knowledge bootstrap`. Its FINAL action
  is `touch <marker>`, run only after `loom knowledge check` shows no new structural issue it
  caused.

`initial_prompt(brief)`: `Read <brief path> in full, then bootstrap the knowledge base as it
instructs.`

`render_brief(...)` (markdown written to the brief file, so there is no argv size limit). It
contains the repo root, `source revision`, mode (`full` or `refresh`), the coverage line, the
work-set cluster table (id, files, symbols, hot files, and for refresh the status), removed
clusters (one line per id, exactly `- removed: <id>`, under the note "no longer a cluster
(deleted, or re-partitioned into the clusters above); verify knowledge about these paths"; the
integration test greps that line), the tier-1 gap
list, and this procedure:

1. Read INDEX.md and the tier-1 files.
2. Group the work-set clusters into at most 6 assignments of at most about 120 files each,
   keeping neighbouring directories together. If more work remains, run another wave after the
   first returns.
3. Spawn one `Explore` subagent per assignment, all in one message. Each explores its clusters
   with `loom map --outline <file>`, `loom map --impact <symbol|path>` and
   `loom map --find-all <symbol>` before opening files. Each RETURNS proposed entries (target
   file, heading, body, evidence `path:line`) and does NOT write knowledge itself.
4. Merge the proposals: drop duplicates, add cross-cluster links, decide tier routing, then write
   with the loom knowledge CLI.
5. Fill every tier-1 gap listed.
6. Run `loom knowledge check`; fix any issue you introduced.
7. Touch the marker (the system prompt names it).

`render_brief` must be a pure function of its inputs, so it can be unit-tested without a graph.

## Tests

`bootstrap/tests.rs` (module path contains `knowledge::bootstrap::`):

- `claude_args` order: `--disallowedTools` is immediately followed by
  `"Edit,Write,NotebookEdit"`, `--append-system-prompt` comes after it, and the last element is
  the initial prompt.
- `system_prompt` contains the marker path and `touch`. `render_brief` includes every work-set
  cluster id, the removed list, and the gaps, and marks statuses only in refresh mode.
- `ensure_loom_ignored`: in a fresh `git init` temp repo it writes `.loom/.gitignore`. When
  `.loom/cache` is already ignored by the repo `.gitignore`, it writes nothing. When only
  `.loom/work` is ignored, it writes the file. Give each case its own `TempDir` and pass the dir
  explicitly (no cwd changes, no `set_current_dir`).
- Tier-1 gaps: a freshly initialized `KnowledgeDir` reports 7 gaps, and 6 after one entry is
  appended to one tier-1 file through the code path `loom knowledge update` uses.
- Partial dir: `doc/loom/knowledge/` exists holding only a human-written `architecture.md`.
  After step 3's `initialize()`, `architecture.md` is byte-identical, the other six tier-1 files
  exist with template content, and the gap count is 6.
- `require_snapshot`: an outcome with `action: SnapshotAction::Unavailable` gives `Err` naming
  the reason; `Reused` gives `Ok`. Build `SnapshotOutcome` with a struct literal
  (`SourceGraphCounters` derives `Default`, `refresh/source_graph.rs:35-36`).
- Run lock: `acquire_run_lock` twice on one temp repo root while the first `File` is alive; the
  second returns `Err` containing `already running`. After dropping the first, a third succeeds.
- `work_set(clusters, plan, refresh: bool)` omits `Unchanged` clusters when `refresh` is true and
  keeps every cluster when it is false.
- clap: `Cli::try_parse_from` rejects `knowledge bootstrap --structural-only --model opus`.

`loom/tests/integration/knowledge_bootstrap.rs` (register `pub mod knowledge_bootstrap;` in
`tests/integration/mod.rs`) drives the real binary with `.current_dir(repo)` on each `Command`.
Never mutate the test process's env or cwd.

Build every loom invocation with `super::helpers::loom_cmd()` (`tests/integration/helpers.rs:203-249`;
it scrubs every `LOOM_*` variable in `RELAY_ENV_VARS_TO_CLEAR`, `LOOM_STAGE_ID` included, and sets
a scratch `LOOM_HOME` with `[update] check = false`). Pass the binary to the fake as
`.env("LOOM_BIN", super::helpers::loom_bin_path())` (`helpers.rs:176`) AFTER `loom_cmd()`. The
fake inherits `LOOM_HOME`, so its `knowledge update` calls stay off `~`. NEVER write
`Command::new(env!("CARGO_BIN_EXE_loom"))`: `tests/integration/binary_spawn_guard.rs:24-28,44-51`
fails the suite for any file outside the sanctioned helpers that contains it, and the scoped
filter never runs that guard. Start each fixture from `super::helpers::init_test_repo()`
(`helpers.rs:13`: git init, local user config, a README.md commit) and add the source files on
top.

On every test `Command` (loom and git alike) set `GIT_CONFIG_GLOBAL=/dev/null` and
`XDG_CONFIG_HOME=<a TempDir>`. `git check-ignore` reads the global excludes file, and a machine
that globally ignores `.loom` would otherwise fail test 1. Set `PATH` to
`<fake dir>:<dirname of the real git>:/usr/bin:/bin`, never the inherited `PATH`: `claude.rs:29-47`
falls back to `~/.claude/local/claude`, so a fake that is not found would start a REAL billed
session on the inherited TTY. Find the real git's directory once by searching the test
process's `PATH` entries for an executable `git`.

Test functions obey the 50-line limit and the file stays under 400 lines. Factor test 3 into
helpers (fixture builder taking a file list, fake-claude writer, receipt reader, a `bootstrap(repo,
args)` runner) and reuse them in the other tests; if the file still passes 400 lines, move the
helpers to `tests/integration/knowledge_bootstrap_support.rs` (register it in
`tests/integration/mod.rs`; W3 owns it).

The fake claude is a `#!/bin/sh` script. Its cwd is the repo root and `$PPID` is the loom
process, which gives the exact marker path `.loom/work/bootstrap/claude-$PPID.done`. The
production driver retries ETXTBSY (W1's `spawn_retrying_text_busy`); the test just writes the
script, sets mode 0o755 and drops the handle. Every fake that touches the marker ends with
`exec sleep 30`, so loom's SIGTERM kills the real process and nothing outlives the test
(`mistakes/detached-spawn-in-tests.md`).

Fixture for tests 2 and 3 (47 files): `init_test_repo()`'s `README.md`, plus `src/lib.rs`,
`src/core/c00.rs` through `src/core/c34.rs` (35 files), and `src/util/a.rs` plus
`src/util/u01.rs` through `src/util/u09.rs` (10 files), each with one small function, committed.
The expected clusters are exactly `.` (residual: `README.md`), `src` (residual: `lib.rs`),
`src/core` and `src/util`.

1. `structural_only_scaffolds_and_reports`: `knowledge bootstrap --structural-only` exits 0;
   stdout contains `coverage:` and `structural-only`; `doc/loom/knowledge/INDEX.md` and
   `.loom/.gitignore` exist; `git status --porcelain` has no line containing `.loom`; there is no
   receipt.
2. `dry_run_prints_command_without_spawning`: a fake `claude` on `PATH` appends to `$FAKE_LOG`
   when invoked; pass `FAKE_LOG` (otherwise "log absent" is trivially true). `--dry-run` exits 0;
   stdout contains `--disallowedTools Edit,Write,NotebookEdit`; parse the `brief: <path>` line
   from stdout and assert that file exists (the test cannot know the child's pid); the cluster
   table lists the four expected ids and never a `doc/loom/knowledge` path; `$FAKE_LOG` does not
   exist.
3. `session_completion_writes_receipt_and_refresh_is_idempotent`: the fake appends `invoked` to
   `$FAKE_LOG`, runs `printf '%s\n' "$@" > "$FAKE_ARGS"`, then writes one short entry into EVERY
   tier-1 file so that no template-only gap remains:
   `for f in architecture entry-points patterns conventions mistakes stack concerns; do
   "$LOOM_BIN" knowledge update "$f" "## Fake $f Entry $PPID" || exit 1; done` (the `$PPID`
   suffix keeps headings unique when a test runs the fake twice). When `FAKE_BRIEF` is set it
   also runs `cat ".loom/work/bootstrap/brief-$PPID.md" >> "$FAKE_BRIEF"` (the brief is deleted
   after the session). After that it runs `touch ".loom/work/bootstrap/claude-$PPID.done"` and
   `exec sleep 30`. Tests 8 and 10 reuse this completing fake. Pass `FAKE_LOG` and
   `FAKE_ARGS` through `Command::env`. If `knowledge update` rejects a heading-only body, add one
   line of body text. Assertions:
   - `knowledge bootstrap --model sonnet --effort low` exits 0; `$FAKE_LOG` has exactly 1 line;
     in `$FAKE_ARGS`, `--disallowedTools` is immediately followed by `Edit,Write,NotebookEdit`
     and the last argument names the brief path; the receipt exists and parses as JSON with a
     non-empty `clusters` array; `architecture.md` contains `Fake architecture Entry`.
   - `knowledge bootstrap --refresh` exits 0, prints `knowledge is current`, and `$FAKE_LOG` still
     has exactly 1 line (no second spawn).
   - After editing and committing `src/util/a.rs`, `knowledge bootstrap --refresh --dry-run`
     shows `src/util` as `changed` and `.`, `src` and `src/core` as `unchanged`, and lists no
     `doc/loom/knowledge` path.
4. `exit_without_marker_writes_no_receipt`: a fake that runs `exit 0` without touching the marker
   makes bootstrap exit 0, print `receipt not written`, and leave no receipt file. In a second
   `TempDir`, a fake that runs `exit 1` makes bootstrap exit non-zero with no receipt.
5. `refuses_inside_stage`: `.env("LOOM_STAGE_ID", "s")` after `loom_cmd()`; exit is non-zero and
   stdout or stderr contains `operator command`.
6. `refuses_outside_git`: a plain `TempDir` (set `GIT_CEILING_DIRECTORIES` to its parent so a
   git repo above `TMPDIR` is not found); exit is non-zero.
7. `refuses_without_commits`: `git init` only, no commit; exit is non-zero and the output contains
   `needs at least one commit`.
8. `removal_only_refresh_briefs_removed_cluster`: fixture `init_test_repo()`'s `README.md` plus
   `a/a00.rs`..`a/a19.rs` (20), `b/b00.rs`..`b/b20.rs` (21) and `c/c00.rs`..`c/c07.rs` (8),
   committed: 50 files, clusters exactly `.`, `a`, `b`, `c`. After `c/` is deleted the root
   still holds 42 > 40 files, so `.`, `a` and `b` stay unchanged and the only difference is the
   removed id. With the completing fake and `FAKE_BRIEF`: `knowledge bootstrap` exits 0
   (`$FAKE_LOG` 1 line); `git rm -rq c` and commit; `knowledge bootstrap --refresh` exits 0,
   `$FAKE_LOG` has 2 lines, `$FAKE_BRIEF` contains `- removed: c`, and the receipt's cluster
   ids are exactly `.`, `a`, `b`; a second `--refresh` prints `knowledge is current` and
   `$FAKE_LOG` still has 2 lines.
9. `marker_then_exit_writes_receipt`: a fake that runs `touch ".loom/work/bootstrap/claude-$PPID.done"; exit 0`
   (no sleep) makes bootstrap exit 0 and write the receipt. This is the race W1's marker
   precedence fixes; `exec sleep 30` fakes cannot catch it.
10. `refresh_is_current_after_commit_and_in_clone`: 47-file fixture and the completing fake.
    After `knowledge bootstrap` exits 0, `git add -A && git commit -qm k` (commits the knowledge
    and the receipt; `.loom/.gitignore` holds `*` and ignores itself). `knowledge bootstrap
    --refresh` prints `knowledge is current`. Then `git clone -q <repo> <second TempDir>/clone`
    and run `knowledge bootstrap --refresh` in the clone with the same `PATH` and env: it prints
    `knowledge is current`, and `$FAKE_LOG` still has exactly 1 line after both refreshes.

Keep `FAKE_LOG`, `FAKE_ARGS` and `FAKE_BRIEF` in a `TempDir` outside the fixture repo, so
`git add -A` in test 10 never commits them. The fake must NOT write anything under `HOME`. Bootstrap reads `config.toml` from the scratch
`LOOM_HOME` that `loom_cmd()` sets, via `UserConfig::load`, and the explicit `--model`/`--effort`
flags keep the result independent of any config.

## Requirements the main agent verifies

`cargo build`, `cargo test --lib`, `cargo test --test maintainability`,
`cargo clippy --all-targets -- -D warnings` and `cargo fmt --check` are clean, and
`cargo run --quiet -- knowledge bootstrap --help` lists `--structural-only`.

## Proof (run from `loom/`, ONE command)

```bash
cargo test --test integration knowledge_bootstrap
```

If a compile error is in a file you do not own, report it; do not edit it. The main agent runs
the full gate (build, clippy, fmt, tests) after each wave.

Report the final CLI surface, any deviation from this brief with the reason, and every
`pub(super)`/`pub(crate)` visibility you widened. Do not commit.
