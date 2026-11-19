# Worker D: the kickoff scope's size and the confined delete

Read `doc/plans/briefs/planner-kickoff-fixes/kickoff-fixes/common.md` first.

Territory brief for four codex units: `scope-size` (section 1), `safe-remove` (section 2),
`draft-store` and `draft-delete` (section 3). This territory deletes files under the repository
on a browser's command: it is security-sensitive, and integration-verify gives it a dedicated
reviewer. Each unit owns only the files its unit brief lists.

## 1. Size in the kickoff scope (`loom/src/plan_session/scope.rs`)

- `KickoffScope` gains `pub size: KickoffSize` (serde field `size`, required).
- `SessionScope::kickoff(project_root: &Path, size: KickoffSize) -> Result<SessionScope>`.
- New `SessionScope::kickoff_resumed(project_root: &Path, size: KickoffSize, name: &str)
  -> Result<SessionScope>`: the scope of a continued, already confirmed kickoff. It sets
  `grant: Some(KickoffGrant { size, name })` at once, so `allows_write` permits that target and
  bundle although they exist. It refuses a name `is_feature_name` (`loom/src/planning/kinds.rs:82`)
  rejects. It skips `refuse_existing` and `refuse_live` for its own target only: those checks
  exist so a kickoff does not overwrite someone else's document, and this document is the
  draft's own.
- `arm_kickoff` refuses a `size` other than `kickoff.size` with a new
  `ArmRefusal::WrongSize(KickoffSize)` whose message reads
  `This kickoff was started as a feature.` / `… as a standalone plan.` (the stored size).
- Make the private `target(size, name)` (`scope.rs:216`) public as
  `pub fn kickoff_target(size: KickoffSize, name: &str) -> ArmedTarget` and use it at its
  existing call sites. Worker S calls it; the signature is fixed.

`scope.rs` is 380 lines. Move the arming code (`arm_kickoff`, `refuse_existing`, `refuse_live`,
`ArmRefusal` and its `Display`) into a new `loom/src/plan_session/scope/arm.rs` re-exported from
`scope.rs`, so both files stay under 400 lines. Update `loom/src/plan_session/scope_tests.rs` for
the new constructor arguments and add tests for `WrongSize` and `kickoff_resumed`.

## 2. Removing a directory tree without following links (`loom/src/fs/safe_fs/write.rs`)

Add, re-exported from `loom/src/fs/safe_fs.rs` beside `safe_remove_in_workdir`:

```rust
/// Remove the directory `relpath` under `dirfd` and everything in it. Every component is
/// opened with O_NOFOLLOW; a symlink anywhere (the path, an entry) is unlinked as a link and
/// never followed. Refuses `..` and absolute paths. At most `MAX_TREE_ENTRIES` (4096) entries.
pub fn safe_remove_tree_in_workdir(dirfd: RawFd, relpath: &Path) -> Result<()>
```

Walk with `openat(O_DIRECTORY|O_NOFOLLOW)` and `fdopendir`/`readdir` (or `std::fs::read_dir` on
`/dev/fd/<n>`: pick the one that keeps the no-follow guarantee on both macOS and Linux and say
which in a comment), unlink files and links with `unlinkat(.., 0)` and directories with
`unlinkat(.., AT_REMOVEDIR)` after emptying them. Also add
`pub fn safe_remove_empty_dir_in_workdir(dirfd: RawFd, relpath: &Path) -> Result<bool>` (true when
removed, false when not empty). Tests go in `loom/src/fs/safe_fs_write_tests.rs`: a tree with a
symlink entry pointing outside the TempDir is removed and the link target survives; a `relpath`
whose parent is a symlink is refused and nothing outside is touched.


## 3. The draft store (`loom/src/plan_server/chat/drafts.rs` and `drafts/*`)

Territory D owns the whole draft store (`draft-store` and `draft-delete`). Worker S calls it through the surface below and owns the
commands in `draft_cmd.rs`; neither of you edits the other's files.

### Stored draft

- `Draft` gains `pub(super) size: KickoffSize` (required) and
  `#[serde(default)] pub(super) finished: bool`. `Seed` gains `pub(super) size: KickoffSize`
  (no `Default` derive any more: every caller states the size). `Draft::started` copies it.
- `Draft::view` takes `size` from `self.size`, never from the shape.
  `KickoffDraftView.size` becomes `KickoffSize` (not `Option`) in
  `loom/src/plan_server/wire/kickoff.rs`; worker S owns that file and makes exactly this
  change plus the detail fields below, so pin them:

  ```rust
  pub struct KickoffDraftDetail {
      pub draft: KickoffDraftView,
      pub shape: Option<KickoffShape>,
      pub messages: Vec<String>,
      /// Repository-relative files the confirmed kickoff wrote that exist, document first.
      pub written: Vec<String>,
      /// Whether HEAD holds any of `written`.
      pub committed: bool,
  }
  ```

- `drafts/handlers.rs` `detail` fills `written`/`committed` from `delete::written` when the draft
  has a target (empty and `false` otherwise, and also when `written` returns Err).

### Landing (`drafts/store.rs` `Files::landed`)

A draft has landed when a locked spelling of its target exists (`IN_PROGRESS-`, `DONE-`,
`RETIRED-`, or any of them under `archive/`: the existing `spellings` list minus the bare
target), or when `draft.finished` is true and the bare target exists. A draft whose bare target
exists but `finished` is false has NOT landed: it stays listed so it can be continued or deleted.
Update the module comment of `drafts.rs` ("A draft whose confirmed document exists has landed…")
to this rule.

### Delete (`drafts/delete.rs`, new; tests in `drafts/delete_tests.rs`)

```rust
/// The files a confirmed kickoff's grant created that exist now, document first, and whether
/// HEAD holds any of them.
pub(in crate::plan_server::chat) struct Written {
    pub paths: Vec<String>,
    pub committed: bool,
}

/// Derive the grant's files from `target` (the draft's stored confirmed document) and report
/// those that exist. Err when `target` is not exactly a kickoff target.
pub(in crate::plan_server::chat) fn written(root: &Path, target: &str) -> Result<Written, String>;

/// Delete exactly the grant's files of `target`; returns the removed paths.
pub(in crate::plan_server::chat) fn remove_written(root: &Path, target: &str)
    -> Result<Vec<String>, String>;
```

Re-export both from `drafts.rs` as `drafts::written` and `drafts::remove_written`. Rules, each
one a test:

- `target` must be exactly `doc/plans/PLAN-<slug>.md` or `doc/product/<name>/index.md` with
  `is_feature_name` true for the slug or name, and equal byte for byte to
  `kickoff_target(size, name).document`. Anything else (`doc/loom/knowledge/INDEX.md`,
  a path with a `..` component, a trailing slash after the plan file name, uppercase, a unicode look-alike) is refused and
  nothing is touched. The stored draft file is data someone with access to the runtime
  directory could edit; never widen this check.
- The files: for a plan, the document and `doc/plans/briefs/<slug>/` (removed with
  `safe_remove_tree_in_workdir`); for a feature, the document and then `doc/product/<name>/`
  only when it is then empty (`safe_remove_empty_dir_in_workdir`). Nothing else.
- Refuse when a locked spelling exists, with
  `"<path> has gone into execution; it cannot be deleted here."`.
- All filesystem access goes through `safe_open_dirfd(root)` and the `safe_fs` helpers: a
  symlinked `doc/`, `doc/plans/`, `doc/plans/briefs/` or bundle directory is refused, or
  unlinked as a link, and never followed.
- `committed`: true when HEAD holds any existing path, checked with
  `crate::git::runner::run_git(&["cat-file", "-e", &format!("HEAD:./{path}")], root)` as
  `loom/src/plan_server/trace/settle_head.rs` `unheld` does. A repository without HEAD or a git
  that fails reads as `false`; it never refuses the delete.

Run from `loom/`: `cargo test --lib plan_session::scope`, `cargo test --lib plan_server::chat::drafts`,
`cargo test --lib fs::`.

## Files

`loom/src/plan_session/scope.rs`, `loom/src/plan_session/scope/arm.rs`,
`loom/src/plan_session/scope_tests.rs`, `loom/src/fs/safe_fs.rs`,
`loom/src/fs/safe_fs/write.rs`, `loom/src/fs/safe_fs_write_tests.rs`,
`loom/src/plan_server/chat/drafts.rs`, `loom/src/plan_server/chat/drafts/store.rs`,
`loom/src/plan_server/chat/drafts/handlers.rs`, `loom/src/plan_server/chat/drafts/encode.rs`,
`loom/src/plan_server/chat/drafts/delete.rs`, `loom/src/plan_server/chat/drafts/delete_tests.rs`,
`loom/src/plan_server/chat/tests_drafts.rs`.
