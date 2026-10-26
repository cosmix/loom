# W3: the shared path predicate

Tier: opus (`loom-senior-software-engineer`). Read `doc/plans/briefs/feat-planner-01-documents/common.md`
first. You start after W1 (slugs, discovery) and W2 (sections) have returned; read their public
items in `loom/src/planning/slug.rs` and `discover.rs`.

## Files you own

`loom/src/planning/paths.rs`, `loom/src/planning/paths/tests.rs`.

## Why this exists

Plan 3's save route and plan 6's `loom plan hook PreToolUse` both decide "may this path be
written" with this module, so the editor and the sessions never disagree on what a locked plan is
(section 12 "Three layers enforce the table", layer 3; "Saves"). It must be correct on the current
tree at every call: `LockedSet::scan` is cheap enough to run per hook invocation (one walk of
`doc/plans/`, plus `collect_inputs` for each locked plan).

## API

Signatures in common.md ("Shared path predicate"). Rules:

- `LockedSet::scan(root)`: list `doc/plans/` and `doc/plans/archive/` with the no-follow listing
  (reuse `discover::discover(root)` and filter its `plans`; strays count too: a stray
  `IN_PROGRESS-` copy is locked by name even when a `DONE-` copy wins). For each plan file whose own
  `plan_state` is Running, Done or Retired, record it with `LockReason::PlanState(state)`; every
  path under `doc/plans/archive/` is `Archive` (files and not-yet-existing paths alike, by prefix);
  `doc/plans/REVIEW-*.md` is `RunReview`.
- Briefs of a locked plan (section 03 "Briefs" and section 12's "brief-to-plan rule"): for each
  locked plan, (a) the bundle prefix `doc/plans/briefs/<slug>/` locks every path under it, existing
  or not; (b) `crate::plan::inputs::collect_inputs(root, &plan_path)` (`plan/inputs.rs:89`,
  `pub(crate)`) lists the plan's inputs; every listed path other than the plan itself is locked
  (`BriefOfLockedPlan { slug }`). `collect_inputs` errors propagate (fail closed: the caller denies).
  A written plan's bundle is not locked unless a locked plan also names its files.
- `lock_reason(rel)`: exact path match first, then the prefixes. `rel` is repo-relative
  (`doc/...`), already syntax-checked.
- `locked_paths()`: the existing locked files (plans, reviews, archive directory itself, existing
  briefs), sorted, for plan 6's spawn-time `denyWrite`/`Edit` deny lists.
- `classify_write(&locked, rel)`: `Locked(reason)` first; then `doc/loom/knowledge/...` ->
  `Knowledge`; `doc/loom/summaries/...` -> `Summaries`; other `doc/...` -> `Doc`; anything else
  `OutsideDoc`.
- `check_relpath_syntax(raw)` (section 12 "Paths in requests"): `raw` is relative to `doc/`. Refuse,
  with a `PathSyntaxError` variant per rule and a message naming it: empty string, absolute (leading
  `/`), a backslash anywhere, any char `< 0x20` or `0x7f` or in `0x80..=0x9f` (NUL included), more
  than 1024 bytes, any empty component (`a//b`, trailing `/`), any `.` or `..` component. Return
  `PathBuf::from("doc").join(raw)`.
- `symlink_component(root, rel)`: `symlink_metadata` on each prefix of `rel` below `root`, first
  one that is a symlink returned; stops at the first missing component (`Ok(None)`: the rest cannot
  be a link yet).

`PathSyntaxError` implements `std::error::Error` and `Display`. `LockReason` derives `Serialize`
(`#[serde(tag = "reason", rename_all = "snake_case")]`).

## Tests (yours, `paths/tests.rs`)

Each `LockReason` variant; a stray `IN_PROGRESS-` copy beside a winning `DONE-`; a brief named by a
locked plan through a glob token (`collect_inputs` expands patterns); a plan locked between two
`scan` calls; `collect_inputs` failing on an unreadable directory makes `scan` return `Err`;
every `check_relpath_syntax` rule including a C1 char (`\u{85}`) and a 1024-byte path that passes;
`symlink_component` on a link in the middle component and on a missing tail.

## Proof

`cargo test --lib planning::paths::`, `cargo clippy --all-targets -- -D warnings`.
