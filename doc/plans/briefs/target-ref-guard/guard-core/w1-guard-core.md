# W1: the target guard core and its `merge_stage` integration

Stage `guard-core`, wave 1 (alone), tier opus. Read `../common.md` first; its "Shared interface"
section is the exact API you build.

You own `loom/src/git/target_guard/` (new: `mod.rs`, `record.rs`, `evaluate.rs`,
`attestation.rs`, `text.rs`, and test files `tests.rs`, `evaluate_tests.rs`,
`attestation_tests.rs`), `loom/src/git/mod.rs`, `loom/src/git/runner.rs`,
`loom/src/git/merge/mod.rs`, `loom/src/git/merge/tree.rs`, `loom/src/git/merge/tree/tests.rs`,
`loom/src/git/merge/lock.rs`, `loom/src/git/merge/control_paths.rs` and
`loom/src/git/merge/target_guard_tests.rs` (new).

Frozen, read-only: `loom/tests/target_guard_contracts.rs` (the contract session wrote it from
the plan). Read it before you start; your code must make every test in it pass.

Your one check, run once at the end, from `loom/`: `cargo test --lib git::`. The contract files
are integration tests that also call W2's `install_reference_transaction_hook`, so they compile
only after W2 (who runs after you); the main agent runs them then. Do not touch them.

## Why

A stage agent can move `refs/heads/<target>` itself (Claude Code grants every linked-worktree
session the whole git common directory), so a change can reach the target without the merge
gate. The plan (sections 1-3) measured why a filesystem deny cannot stop it and settles on
detection: loom records the last target tip it accepted, in a file no session can write, and
every move it did not make is evaluated. W2 adds a git `reference-transaction` hook that
appends host-side moves to a ledger; you read that ledger.

## 1. Module layout

`pub mod target_guard;` in `loom/src/git/mod.rs`. Inside `loom/src/git/target_guard/`:

- `mod.rs`: the public API of `../common.md` (re-export from the submodules), the constants,
  `check`, `check_locked`, `pending_hold`, `accept`, `merged_into_accepted`.
- `record.rs`: the serde record (`GuardRecord { targets: BTreeMap<String, TargetEntry> }`,
  `TargetEntry { accepted: String, hold: Option<Hold>, attestation: bool }`, both
  `#[serde(deny_unknown_fields)]`), read and write, `accepted_tip`, `recorded_hold`,
  `recorded_holds`, `attestation_latched`, `record_advance`, `guarded_refs`, and the refs-file
  writer.
- `attestation.rs`: ledger parsing, `append_attestation`, `attestation_mode`.
- `evaluate.rs`: the evaluation of a move (section 3).
- `text.rs`: `hold_alert`, `accept_command`, `restore_commands`, and a `Display` for
  `HoldReason`.

Every file stays under 400 lines, every function under 50.

## 2. Record, refs file, ledger

- Paths: `work_dir.join(RECORD_FILE)` etc. Write the record with
  `crate::fs::locking::locked_write(path, &json)` (temp + rename under a directory lock); read it
  with `crate::fs::locking::locked_read`. `locked_read` returns `Err` for a missing file
  (`fs/locking.rs`): check `path.exists()` first (or match `NotFound` in the error chain) so a
  missing record file is an empty record, and any other read error stays an error.
- Record keys are short branch names: every public function strips a leading `refs/heads/` from
  `target` first (`git/merge/stage_tests.rs` calls `merge_stage(.., "refs/heads/main", ..)`).
- Every record write also writes `REFS_FILE` (same helper), exactly:

      # written by loom; read by .git/hooks/reference-transaction
      ref <branch_ref(key)>          (one line per key of the record, in key order)
      allow doc/loom/knowledge/

  Derive the allow prefix from `crate::sandbox::KNOWLEDGE_WRITE_GLOB` (`"doc/loom/knowledge/**"`,
  strip the trailing `**`; the `config` module is private, the constant is re-exported from
  `sandbox/mod.rs`): no second literal.
- `guarded_refs(work_dir)`: the `ref` lines of `REFS_FILE` (empty when absent).
- The ledger is append-only. `append_attestation(work_dir, reference, from, to)` opens
  `LEDGER_FILE` with `OpenOptions::new().append(true).create(true)` and writes one
  `attest <from> <to> <ref>\n` with a single `write_all`. Parse lines `attest|abort <from> <to>
  <ref>` whose two ids are exactly 40 or 64 lowercase hex characters; skip every other line (git
  sends `ref:refs/heads/x` values for symref updates, and the file is input even though only the
  host writes it). An `abort` line cancels the most recent earlier `attest` line with the same
  three fields. Create the ledger (empty) when the record entry is first created.
- A record that does not parse is not overwritten by `check`: `check_locked` returns
  `GuardState::Held(Hold { accepted: String::new(), observed: tip, reasons:
  vec![HoldReason::Unevaluable { error }], since: now })` without writing. `accept` replaces an
  unparseable record with a fresh one (it is the operator's repair).

## 3. `check_locked(repo_root, work_dir, target, tip)` (caller holds `MergeLock`)

1. No entry for `target`: create `{ accepted: tip, hold: None, attestation: attestation_mode(..)
   == Active }`, create the ledger, write, `tracing::info!("target guard: recorded {target} at
   {tip12}")`, return `Clear`.
2. `tip == accepted`: if a hold is recorded, clear it and write. Return `Clear`. No git.
3. A recorded hold whose `observed == tip` and whose reasons hold no `Unevaluable`: return it.
   No git.
4. Otherwise evaluate (`evaluate.rs`), with `mode = attestation_mode(repo_root, work_dir)` and
   `required = mode == Active || entry.attestation` (the latch: plan section 3.2; `.git/config`
   is only a file deny, so a session can rewrite it once the operator's `git config` has
   replaced it, and re-reading the mode alone would let it switch the ledger walk off). Every
   record write of this step sets `entry.attestation = required`, so the latch rises when the
   mode turns `Active` and never falls here:
   - Not a fast-forward (`crate::git::branch::is_ancestor_of(accepted, tip, ..)` is `Ok(false)`;
     an `Err` is a git error, see the last bullet; never `run_git_bool`, which swallows errors):
     reasons `[NotFastForward]` plus `ControlPaths` when the next bullet finds any; skip the rest.
   - Control paths: make `control_paths::changed_paths` `pub(crate)` and call it with
     `(accepted, tip)`; keep the paths for which `control_paths::is_control_path(path,
     hooks_dir_prefix(repo_root).as_deref())` holds. Non-empty: `ControlPaths { paths }`.
   - Attestation, only when `required`: the chain walk below. An offending gap:
     `Unattested { from, to, paths }` with the gap's paths outside the allow prefix (at most 20).
   - Stage work: list `git for-each-ref --format=%(refname:short) refs/heads/loom/`; for each
     branch `B`, run `git merge-base --all <tip> <B>` through `run_git` (exit 1 with empty output
     means `B` shares no history with the target: no stage work for `B`; only other failures are
     errors; `merge_handler_attempt_tests.rs` already creates an orphan `loom/*` branch); if any
     base is NOT an ancestor of `accepted`, the move carries commits of `B` loom never accepted:
     `StageWork { branches }`.
     A branch whose commits are all in `accepted` (merged, cleanup pending) never trips this.
     A partial fast-forward (the target moved to an early commit of `B`, `B` has later commits)
     must trip it: never use `--merged`/branch-tip ancestry for this test.
   - No reasons: accept silently: `accepted = tip`, clear any hold, write, and
     `tracing::info!("target guard: {target} advanced outside loom {a12}..{t12}; accepted")`.
   - Reasons: record `Hold { accepted, observed: tip, reasons, since: Utc::now() }`, write,
     return `Held`.
   - A git error anywhere in the evaluation: `Held` with `Unevaluable { error }` (recorded, so
     status shows it; step 3 re-evaluates it next time).

Chain walk (attested mode). Entries: the ledger's live `attest` lines for
`branch_ref(target)` with non-zero `from` and `to` and `from != to`, in ledger order (host
`git gc`/`pack-refs` makes the hook write `attest X X` and `attest X 0…`; both drop out here).

    P = accepted; visited = {P}
    repeat at most entries.len() + 2 times:
        if P == tip: return no gap
        next = entries with from == P, to not in visited, and (to == tip or to is an
               ancestor of tip); prefer the one with to == tip, else the last in ledger order
        if next exists: P = next.to; visited += P; continue
        candidates = distinct `from` values Q of entries with Q not in visited, P an ancestor
                     of Q, Q an ancestor of tip (or equal)
        Q = the candidate that is an ancestor of every other candidate; none: Q = tip
        paths = changed_paths(P, Q) not starting with the allow prefix
        if paths non-empty: return gap Unattested { from: P, to: Q, paths }
        P = Q; visited += P
    did not converge: Unevaluable { error: "attestation chain did not converge" }

Every ancestry test whose operand comes from the ledger ("to is an ancestor of tip", the
candidate tests) treats any outcome other than `Ok(true)` as "not an ancestor", never as an
error: an attested commit a reset abandoned and `gc` pruned makes `merge-base --is-ancestor`
exit 128, and an error there would turn every later evaluation into `Unevaluable`. Do not
restrict `next` to forward steps: reset to `A~1`, commit `B`, merge `A` into it (`M`) is a fully
attested history that only a backward first step walks. The visited set is what guarantees
termination: an operator history commit `e`, commit `f`, `reset --hard e`, commit `g`,
`reset --hard f`, then a knowledge commit cycles `E→F→E` without it.

The knowledge-only exception exists because knowledge stages commit to the target from inside
a sandbox (the hook cannot attest them) and touch only `doc/loom/knowledge/**`.

`attestation_mode(repo_root, work_dir)` returns `Off { reason }` when any of these holds, else
`Active`:

- `crate::git::hooks::configured_hooks_path(repo_root)` is `Some(p)`: "core.hooksPath is set
  to <p>, so git does not run hooks from .git/hooks".
- The common git dir's `hooks/reference-transaction` (common dir from
  `git rev-parse --path-format=absolute --git-common-dir` in `repo_root`) is missing or does not
  contain `HOOK_MARKER`: "loom's reference-transaction hook is not installed".
- The parent of the common git dir joined with `.loom/work`, canonicalized, differs from
  `work_dir` canonicalized: "the hook cannot find this state directory".

## 4. The rest of the API

- `check(repo_root, work_dir, target)`: resolve `tip` (`rev-parse --verify
  refs/heads/<target>^{commit}` through `branch_ref`). Fast path without the lock: the record
  has an entry, `accepted == tip`, no hold: `Ok(Some(Clear))`. Otherwise take
  `MergeLock::try_acquire_in(work_dir)?`; `None` (contended): `Ok(None)`; `Some(lock)`: re-read
  `tip`, `check_locked`, release.
- `pending_hold`: the same evaluation (the same `required`), writing nothing (no TOFU write, no
  record update): `None` when the entry is missing or the move would be accepted.
- `attestation_latched(work_dir, target)`: the entry's `attestation`; `false` when missing.
- `record_advance(work_dir, target, from, to)`: when the entry's `accepted == from`, set it to
  `to`, clear any hold, write. Otherwise leave the record unchanged and return Ok (the next
  evaluation judges the move).
- `merged_into_accepted(repo_root, work_dir, target, commit)`: entry present:
  `is_ancestor_of(commit, accepted)`; entry missing: `crate::git::merge::verify_merge_succeeded`.
- `accept(repo_root, work_dir, target, expected)`: `MergeLock::acquire(work_dir, 30 s)`;
  `expected` resolved with `rev-parse --verify <expected>^{commit}` (accepts an abbreviation);
  refuse unless it equals the current tip, with
  "the target moved since you reviewed it: {target} is now at {tip}; review again"; set
  `accepted = tip`, clear the hold, set `attestation = attestation_mode(..) == Active` (the only
  place the latch falls: the operator's explicit decision), write; return
  `Accepted { from: old accepted, to: tip }`.
- `hold_alert(target, hold)`: one line, e.g. `Target main held: moved outside loom
  1a2b3c4→5d6e7f8 (<reasons>). Merges into main wait. Review: loom target status`.
  Reasons render briefly: "not a fast-forward", "touches .claude/settings.json",
  "unattested move 1a2b3c4..9f8e7d6 touching src/x.rs", "carries work of loom/s", "could not
  evaluate: <error>"; list at most 3 paths then "and N more".
- `accept_command(target, hold)`: `loom target accept --to <observed full id>`.
- `restore_commands(repo_root, target, hold)`: always
  `git update-ref refs/heads/<target> <accepted> <observed>`; when the target is checked out in
  `repo_root` (`git symbolic-ref -q HEAD` there equals `refs/heads/<target>`), also
  `git read-tree -m -u <observed> <accepted>` with a note that it runs in that checkout.

## 5. `MergeBlock::TargetHeld` and `merge_stage`

- `tree.rs`: add `TargetHeld { target: String, accepted: String, observed: String }` to
  `MergeBlock`; `Display`: "target {target} moved outside loom ({a12} → {o12}); merges into it
  wait until the operator accepts or restores it (loom target status)". Add a serde round-trip
  test beside the existing ones in `tree/tests.rs`. `rg -n 'MergeBlock::' src` and confirm
  no other exhaustive match needs an arm (at `066e7667` only `Display` matches exhaustively;
  `blocked_retry.rs` and `models/stage/merge_block.rs` use specific arms or `_`). Abbreviate ids
  with `get(..12).unwrap_or(..)` (common.md).
- `lock.rs`: `pub(crate) fn try_acquire_in(work_dir: &Path) -> Result<Option<MergeLock>>`
  wrapping the private `try_acquire(&work_dir.join("merge.lock"))`.
- `mod.rs` `merge_stage` (about lines 85-131, 47 lines now; it must stay at or under 50):
  right after `old` and `branch_tip` are resolved and BEFORE the `is_ancestor_of(&branch_tip,
  &old)` `AlreadyUpToDate` return, call `target_guard::check_locked(repo_root, work_dir,
  target_branch, &old)?`; `Held(h)` returns `MergeResult::Blocked(MergeBlock::TargetHeld {
  target, accepted: h.accepted, observed: h.observed })`. This runs for `MergeGate::Bypass` too:
  the bypass covers the stage's own control paths, not a moved target. Move steps into helpers
  to stay under the limit.
- `merge_and_advance`: after `Advance::Advanced`, the merge commit is `pending.commit()?`
  (cached, no second commit object). Call `target_guard::record_advance(work_dir,
  target_branch, old, &merge_commit)`; on `Err`, `tracing::warn!` and still return `Success`.
  Thread `work_dir` through as needed.

## 5b. Replace refs and grafts (`runner.rs`)

A stage session can write `<common>/refs/replace/*` and `<common>/info/grafts` (the capsule
denies only `.git/hooks`, `.git/config` and `config.worktree`: `sandbox/control_surfaces/
session_denies.rs`), and git honors both in `diff`, `merge-base` and every ancestry test.
Measured on git 2.53: `git replace X D` makes `git diff -z --name-only --no-renames A X` print
D's paths; a graft line `<X> <A>` makes `merge-base --is-ancestor A X` succeed for an orphan
`X`; `GIT_NO_REPLACE_OBJECTS=1` disables replace refs but NOT grafts; `GIT_GRAFT_FILE=/dev/null`
disables grafts.

In `runner.rs::git_command` add `.env("GIT_NO_REPLACE_OBJECTS", "1")` and
`.env("GIT_GRAFT_FILE", "/dev/null")` beside `LC_ALL`/`LANG` (before `.envs(env)`, so a caller's
explicit env still wins), with a doc comment naming the session-writable paths. The pinned
runner (`runner/pinned.rs`) builds its command through `git_command` too. Loom uses neither
mechanism, so nothing else changes. Tests in `runner.rs`'s existing test module (or
`target_guard/tests.rs`): with a planted `info/grafts` line `is_ancestor_of` stays `false`;
with a planted `git replace`, `changed_paths(A, X)` still names X's real path.

## 6. Tests (yours, beside the frozen contracts)

In-crate, with real git in `tempfile::TempDir` repos and ambient config shut out (common.md):

- `target_guard/tests.rs`: TOFU creates the record, refs file and ledger; refs file content
  exact; `record_advance` with a stale `from` changes nothing; `accept` with an abbreviated
  current tip works; a corrupt record yields `Unevaluable` and is left unchanged; `check`
  returns `Ok(None)` while another `MergeLock` is held; `pending_hold` writes nothing;
  `restore_commands` gives one command when the target is not checked out and two when it is.
- `target_guard/evaluate_tests.rs`: each reason alone; a gap that is knowledge-only passes; an
  attested step then an unattested non-knowledge step then an attested step holds naming the
  middle range; an `abort` line cancels its `attest`; a stale merged `loom/*` branch is not
  stage work; a partial fast-forward into `loom/s` is stage work; an orphan `loom/x` branch is
  not stage work and not `Unevaluable`; ledger lines `attest X X`, `attest X <zero>`, a `ref:`
  value and an attested id that no longer exists are skipped or treated as non-ancestors (the
  next knowledge-only move is still `Clear`); the e/f/g reset cycle of section 3 converges; a
  held target that moves again is re-evaluated (`observed` updates); a full-ref target
  (`"refs/heads/main"`) writes `ref refs/heads/main` and keys the record `main`.
- `target_guard/attestation_tests.rs`: each `Off` reason of `attestation_mode`, and `Active`.
  The latch: an entry created while `Active` is latched; after `git config core.hooksPath
  /dev/null` an unattested non-knowledge move still holds `Unattested`; `accept` while `Off`
  lowers the latch, and the next unattested move with no control path or stage work is `Clear`;
  an entry created while `Off` is unlatched and the first evaluation after the mode turns
  `Active` raises it; `pending_hold` uses the latch too.
- `merge/target_guard_tests.rs` (declare `#[cfg(test)] mod target_guard_tests;` in
  `merge/mod.rs`): `merge_stage` blocks with `TargetHeld` and writes no commit object;
  `MergeGate::Bypass` is blocked too; a merge through `update-ref` and one through the checkout
  fast-forward both leave `accepted` at the new tip.

To write a ledger line in a test without W2's hook, call `append_attestation`; to make
`attestation_mode` `Active`, write `.git/hooks/reference-transaction` containing
`HOOK_MARKER` in the temp repo and keep `.loom/work` under the repo root.

## Traps

- `rev-parse` of a branch name must go through `branch_ref`; a tag `main` must not win.
- `git diff --name-only` must use `-z` and `--no-renames` (reuse `changed_paths`).
- Never write the record from `pending_hold`.
- `check_locked`'s step 3 memo must not hide a change of tip: compare `observed` to `tip`.
- Do not hold the `MergeLock` inside `check_locked`; the caller does.
- `merge_stage` is 47 lines at `066e7667` (`git/merge/mod.rs` about 85-131) and the guard step
  adds about 9: move the block into a helper. `operator_operation` stays the first check; the
  guard step follows it and precedes the `AlreadyUpToDate` return.
