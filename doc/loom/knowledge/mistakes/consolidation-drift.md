# Consolidation Drift

>

## Copies of One Helper Drifted Into Bugs (2026-11-15)

**What happened:** the dry-consolidation plan found one cluster of copies after another that had
diverged from its siblings, and the divergence was the bug. Each row names the cluster, the wrong
behaviour and the canonical helper now in
[canonical-helpers](../patterns/canonical-helpers.md).

- **Tag-ambiguous branch reads.** `git rev-parse --abbrev-ref HEAD` and `symbolic-ref --short`
  print `heads/loom/<id>` when a tag shares the branch name. Stage detection and completion
  then resolved the wrong stage or none: with branch `loom/smoke` and tag `loom/smoke`,
  `loom stage merge` without an id reported `Stage file not found for: smoke`, never
  `heads/loom/smoke`. `branch --show-current` was a third spelling.
- **A block left the scheduler graph stale.** Six `let _ = persist_blocked_stage(..)` sites persisted
  `Blocked`, discarded the error and never marked the graph, so the scheduler's graph stayed out
  of date.
- **Spool and log appends followed a planted symlink.** `jsonl_spool::append`, hook events,
  telemetry, adjudication feedback, the memory journal and forward receipts opened with plain
  `OpenOptions`, so a symlink at the file name redirected the write.
- **A saturating path normalizer exempted files.** Copies that popped `..` and stopped at the root
  turned a declaration such as `../outside` into `outside`, so a declaration climbing out of the
  repository exempted the in-repo file of that name from completion checks.
- **Trimmed NUL listings.** A listing read through a trimming runner lost leading and trailing
  whitespace on the first or last NUL-delimited name, so a path with edge whitespace named a
  different file.

**Why:** each copy was written for one caller, with no shared test pinning the edge case, and a
later copy repeated the first one's gap instead of reading its siblings.

**Prevention:**

1. Before adding a copy of a git query, a block, an append, a path normalizer or a listing parser,
   look the cluster up in [canonical-helpers](../patterns/canonical-helpers.md) and route to it.
2. Read a branch from the full ref (`checked_out_branch_with`), never from an abbreviated name.
3. Block a stage through `Orchestrator::block_stage` only; it persists, marks the graph and logs.
4. Append through `safe_open_append`; a new log writer gets a refuse-a-symlink test.
5. Normalize a path lexically through `normalize_lexically` and treat `None` as an escape.
6. Parse NUL-delimited git output from the untrimmed bytes (`parse_porcelain_v1_z`,
   `changed_names`).

**Fix:** routed every site to its canonical helper with a regression test per behaviour change
(`block_contract_tests::block_syncs_graph`, `spool-append-refuses-symlink`, `listing-is-untrimmed`).

## A Stricter Opener Broke Callers Whose Anchor Was Not Ready (2026-11-15)

**What happened:** two failures came from routing a writer through `safe_open_append`, which opens
its anchor directory with `O_NOFOLLOW` and requires it to exist.

- `jsonl_spool::append` gained the precondition, and nine `spool_drain_tests.rs` tests built spools
  under a worktree root they never created; they failed in the integration-verify full suite
  with `Failed to open dirfd`. The fs-safety gate had run filtered module tests, not
  `orchestrator::core::spool_drain`.
- Routing the target-guard attestation ledger to `safe_open_append(work_dir, ..)` failed
  `tests/merge_off_checkout_cli.rs` with `ENOTDIR`: inside a stage worktree `work_dir` is the
  `.loom/work` symlink, and `O_NOFOLLOW` refuses a symlink in the final component of the anchor.

**Why:** `WorkDir::root` in a worktree is the unresolved `<wt>/.loom/work` symlink. The anchor's
last component must stay no-follow, so the owner resolves the trusted root first.

**Prevention:** when an opener gains a precondition, `rg` every test helper that calls its callers
and run those modules. Anchor `safe_open_append` one level below the `.loom/work` symlink (hook
events, telemetry, memory, feedback and receipts anchor at their own subdirectory) or canonicalize
the directory first (`stage_events`, `tick.rs`, `ticket_io.rs`).

**Fix:** anchors moved below the symlink; the spool tests create the worktree root first.

## A Brief Deleted a Helper Without Listing Its Callers or the Absence Check Names (2026-11-15)

**What happened:** three consolidation briefs broke a build or an acceptance check.

- T0b's brief covered deleting `relay/ticket.rs` `sha256_hex` but not its re-export in
  `relay/mod.rs` or about 15 importers through `crate::relay::sha256_hex` (host_verify,
  verify/contracts, stage_service, daemon tests, `tests/*`); a haiku fix worker repointed them.
- P1b said to rebuild `split_front` while acceptance forbade `fn split_front(`, and P2 kept a
  wrapper named `normalize` while acceptance forbade `fn normalize(`; the wrapper was renamed
  `absolute_lexical`.
- F2 and F3 kept `write_atomic`, `regular_file` and `read_capped` wrappers because callers sat in
  files no row owned, so `rg` absence checks for those names kept matching.
- P1a deleted `scan::parse_yaml_frontmatter` and broke two tests no row owned
  (`adjudication/tests.rs`, `session_tests.rs`), and the `user_git::head` and
  `contracts::changed_names` deletions broke callers in `plan_server/run/commit.rs`,
  `plan_server/trace/settle_head.rs`, `verify/impact_tests.rs` and `verify/review/base_move.rs`.

**Why:** the brief was written from the definition site, and the absence criteria were not read
against the names the brief told the worker to introduce.

**Prevention:** before briefing a deletion, `rg` the name's callers and re-exports and list every
caller file under Files owned; read the stage's `rg` absence criteria and name the forbidden
identifiers in the brief. A deleted import that only a `use super::*` test module needed shows
only under `cargo check --tests`.

**Fix:** the main agent or a fix worker repointed the unowned callers, recorded as plan decisions.

## A Frontmatter Split Shadowed the YAML Start Offset

**What happened:** While changing the frontmatter splitter to return both YAML and body, a newly named body offset shadowed the YAML start offset and briefly made the returned YAML slice empty.

**Why:** Two distinct offsets were represented by the same local name during the first edit.

**Prevention:** Keep the YAML start and post-delimiter body start as separate named offsets; test both returned slices with indented delimiters and CRLF.

**Fix:** Renamed the post-delimiter offset to `remainder_start`, retained the original `body_start` for YAML, and added wrapper boundary tests.
