# W1: kinds, slugs, discovery, runtime paths, lifecycle prefix

Tier: sonnet (`loom-software-engineer`). Read `doc/plans/briefs/feat-planner-01-documents/common.md` first.

## Files you own

`loom/src/lib.rs`, `loom/src/planning/mod.rs`, `loom/src/planning/discover.rs`,
`loom/src/planning/tests/mod.rs`, `loom/src/planning/tests/discover_tests.rs`,
`loom/src/fs/plan_lifecycle.rs`, `loom/src/fs/mod.rs`, `loom/src/plan/inputs.rs`,
`loom/src/commands/usage/discovery.rs`.

Sections 2 (slugs), 3 (kinds) and 5 (runtime) of this brief are the rules codex units C2, C1 and
C3 implemented in wave 1 (`slug.rs`, `kinds.rs`, `runtime.rs` and their test files); read their
code, do not edit it. You build section 1 (the lifecycle prefix) and section 4 (discovery). When
you add `RETIRED_PREFIX` to `fs/plan_lifecycle.rs`, leave C2's local prefix consts in slug.rs as
they are (a later cleanup is not in scope).

## 1. Lifecycle prefix (`fs/plan_lifecycle.rs`, `fs/mod.rs`)

- Add `pub const RETIRED_PREFIX: &str = "RETIRED-";` beside `IN_PROGRESS_PREFIX` and `DONE_PREFIX`
  (`plan_lifecycle.rs:23-24`) with a doc comment: an abandoned plan, only ever at
  `doc/plans/archive/RETIRED-PLAN-<slug>.md` (section 03 "Plans and their lifecycle"). Nothing in
  `loom run` writes it in this plan (plan 11 does).
- Re-export it from `fs/mod.rs` next to the other two (`fs/mod.rs:47-50`).
- `plan/inputs.rs::bundle_dir` (`:136-145`): add `.trim_start_matches("RETIRED-")` after the
  `DONE-` strip, using `crate::fs::plan_lifecycle::RETIRED_PREFIX` (and the other two constants,
  replacing the string literals). The frozen contract `plan::inputs::retired_tests::retired_plan_bundle_is_an_input`
  pins this.
- `commands/usage/discovery.rs::normalize_plan` (`:311-325`): add `"RETIRED-"` to the status-prefix
  array and its doc comment.

## 2. `slug.rs`

Signatures in common.md ("Plan slugs"). Rules:

- `plan_slug(file_name)`: the name must end `.md`; strip at most ONE of `IN_PROGRESS-`, `DONE-`,
  `RETIRED-`; the rest must start `PLAN-` and leave a non-empty stem. `REVIEW-...` is never a plan.
  `DONE-PLAN-DONE-x.md` has slug `DONE-x`.
- `plan_state(rel)`: only files directly in `doc/plans/` or `doc/plans/archive/`. In `doc/plans/`:
  `PLAN-` written, `IN_PROGRESS-PLAN-` running, `DONE-PLAN-` done, `RETIRED-PLAN-` retired. Under
  `archive/`: `RETIRED-PLAN-` retired, any other plan spelling done (section 01: "or under
  `doc/plans/archive/`").
- `PlanState` derives `Ord` in the order Written < Running < Done < Retired, plus `Serialize`
  (`snake_case`) and `Hash`.
- A resolver `pub fn resolve_plans(paths: impl Iterator<Item = PathBuf>) -> BTreeMap<String, PlanEntry>`:
  the highest state wins; on a tie (two done copies) the lexicographically smaller path wins; every
  other path of the slug goes to `strays` (sorted).

## 3. `kinds.rs`

`classify(rel)` per section 03's kinds table, from the path alone:

| Path | Kind |
| --- | --- |
| `doc/product/index.md` | `Vision` |
| `doc/product/section_<digits>.md` | `ProductSection { number }` |
| any other file under `doc/product/` | `ProductNote` |
| `doc/plans/index.md`, `doc/plans/<name containing "metaplan">.md` (directly in `doc/plans/`) | `Metaplan` |
| `plan_state(rel)` is `Some` | `Plan { slug, state }` |
| under `doc/plans/briefs/<slug>/` | `Brief { slug }` |
| `REVIEW-<plan file stem>.md` directly in `doc/plans/` or `doc/plans/archive/` | `RunReview { slug }` (slug via `plan_slug` of the name without `REVIEW-`) |
| under `doc/loom/knowledge/` | `Knowledge` |
| anything else under `doc/` | `Other` |

Briefs a plan names outside its bundle are discovered as `Other` here; plan-level brief
attachment is `briefs::plan_briefs` in stage `checklist-check`. `doc_id` per common.md.
`DocKind` derives `Serialize` with `#[serde(tag = "kind", rename_all = "snake_case")]`.

## 4. `discover.rs`

`discover(root)` walks `doc/` from a dirfd (`crate::fs::safe_fs::safe_open_dirfd(root)`, then
`crate::fs::safe_read::list_dir_no_follow(&fd, relative)` recursively; `EntryKind` tells
directories, files, symlinks and specials apart without following).

- `doc` itself: `symlink_metadata(root/doc)`: missing -> `DocDirState::Missing` and an empty tree;
  a symlink -> `Symlinked` and an empty tree (section 12: "A symlinked `doc` is treated as missing").
- Skip: names starting with `.`; editor temporaries (`*~`, `*.swp`, `.#*`); the directory
  `doc/loom/summaries`; specials; symlinked directories.
- A symlink entry is kept only when `std::fs::canonicalize` of it is a regular file
  (`symlink_metadata` of the canonical path) that `starts_with(canonical doc/)` and does not
  `starts_with(canonical doc/loom/summaries)`. Mark it `via_symlink: true`. Everything else is
  skipped silently.
- Git-ignored files are read like any other (no git calls here).
- Bounds: refuse to descend more than 16 directory levels or list more than 20,000 entries
  (return an error naming the limit); `doc/` holds hundreds of files.
- `size` from the entry's own metadata (lstat for a real file; the target's for an allowed link).
- Build `docs` sorted by path, `plans` with `resolve_plans` over every `Plan` document.
- Expose the walk itself for W4's watcher: `pub(crate) enum WalkScope { Documents, IncludeSummaries }`,
  `pub(crate) struct WalkEntry { pub rel: PathBuf, pub size: u64, pub mtime_ns: i128, pub inode: u64, pub via_symlink: bool }`,
  `pub(crate) fn walk_doc(root: &Path, scope: WalkScope) -> Result<(DocDirState, Vec<WalkEntry>)>`.
  `discover` calls it with `Documents` (skips `doc/loom/summaries`); the watcher with
  `IncludeSummaries` (every other skip rule unchanged). For an allowed symlink the metadata is the
  target's.

`read_doc(root, rel)`: refuse `rel` not starting with `doc/` and any `..` component; for a real file
`crate::fs::safe_read::read_to_string_bounded(root, rel, MAX_DOC_BYTES)`; for a symlink, re-run the
target check, then read the canonical target's path relative to the canonical root with the same
no-follow read (a swap between check and open fails the open). `MAX_DOC_BYTES = 8 * 1024 * 1024`.
Invalid UTF-8 is an error naming the path.

## 5. `runtime.rs`

Signatures in common.md ("Runtime paths"). `runtime_paths(root)` =
`runtime_paths_from(std::env::var_os("LOOM_PLAN_STATE").filter(non-empty), env LOOM_PLAN_SESSION, loom_dir, root)` with
`loom_dir = crate::user_config::config_path()?.parent()` (`user_config/mod.rs:154`, honours
`LOOM_HOME`). `runtime_paths_from(Some(state), session, ..)`: refuse unless `state` is absolute,
`symlink_metadata` says directory (not symlink), `MetadataExt::uid()` equals `nix::unistd::getuid()`,
its file name equals `project_key(root)`, `session` is `Some` and matches `[0-9a-fA-F-]{8,64}`, and
`nix::unistd::access(state.parent(), AccessFlags::W_OK)` FAILS (provenance: plan 6's sandbox makes
the real planning root read-only to the session, so a writable parent means a directory the caller
made, e.g. under Claude Code's tmp dir, which would bypass the server's lock and claims); each refusal
is an error naming `LOOM_PLAN_STATE` and the rule. `server_runtime_paths(root)`: always
`runtime_paths_from(None, None, loom_dir, root)`, returning `std::env::var_os("LOOM_PLAN_STATE")` as
the ignored value (plan 2 warns; a server started from a shell that exported a session's variable must
not move its runtime root). Then `dir = state`, `planning_root = state.parent()`. `None`: `dir = loom_dir/plan/<key>`,
`planning_root = loom_dir/plan`. `project_key`: canonicalise `root`, take its final component, replace every
char outside `[A-Za-z0-9._-]` by `_`, append `-` and the first 12 hex digits of SHA-256 of the
canonical path's bytes (`crate::context::source_graph::body_hash`, strip `sha256:`). File names:
`lock`, `drafts`, `claims`, `summaries.lock`, `summary-cost.jsonl`, `sessions`. Creates nothing.

## Tests (yours)

`planning/tests/discover_tests.rs`: a
symlinked `doc`, a FIFO under `doc/` skipped (create with `nix::unistd::mkfifo`), the depth bound,
`read_doc` of an allowed link and refusal of `../` paths. Extend `plan/inputs/tests.rs` only if a
case is not already pinned by the frozen contract.

## Proof

`cargo test --lib planning::` (your modules), `cargo test --lib plan::inputs::`,
`cargo test --lib commands::usage::`, `cargo clippy --all-targets -- -D warnings`. Report the public
items you added and any deviation from common.md.
