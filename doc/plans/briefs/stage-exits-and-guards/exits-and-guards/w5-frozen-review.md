# exits-and-guards / w5-frozen-review: an accepted contract replacement does not stale the review round

Tier: codex sol (`gpt-6.2-sol`, effort xhigh)
Read first: `doc/plans/briefs/stage-exits-and-guards/common.md`.

## Finding

After an accepted contract dispute the daemon writes the replacement into the stage worktree and
re-freezes (`write_and_refreeze` in `loom/src/orchestrator/adjudication/apply_contract_rulings.rs`).
The review fingerprint hashes the working-tree content of every path changed since the merge
base (`compute_local` in `loom/src/verify/review/fingerprint.rs`), frozen files included. The
daemon's own write therefore made the latest review round stale: the next session had to run a
third review round and the full gate again for a file an adjudicator had ruled on and that no
reviewer has authority over.

## Decision

A review round still covers the tree when the base is unchanged and every path that differs
from what the round saw is a frozen file of the stage whose current content is exactly what the
stage's freeze record holds. Such a file can only have been written by the daemon after an
accepted ruling: the contract check fails any other content, and an agent's own edit to a
frozen file does not match the record.

## Files

Own (write):
- `loom/src/verify/review/frozen_cover.rs` (new)
- `loom/src/verify/review/mod.rs`
- `loom/src/verify/review/gate.rs`
- `loom/src/verify/review/gate_tests.rs`
- `loom/src/commands/stage/review_status.rs`

Read only: `loom/src/verify/review/fingerprint.rs` (`ChangeFingerprint`, `changed_since`,
`sha256_hex`), `loom/src/verify/review/prose.rs`, `loom/src/verify/contracts/store.rs`
(`load_freeze`, `FreezeRecord`, `FrozenFile`), `loom/src/host_verify/child_contracts.rs`
(`relock_frozen`: how a frozen path, which is relative to the stage's working directory,
becomes a path under the worktree root), `loom/src/daemon/server/observer.rs`
(`check_completion_gates`, the daemon-side caller of the same gate).
Frozen (never edit): `loom/src/verify/review/frozen_cover_contract_tests.rs`.

## Interface (pinned; the frozen contract calls it)

In `loom/src/verify/review/frozen_cover.rs`:

```rust
use std::collections::BTreeMap;

/// Whether every path in `changed` is a frozen file whose current digest is
/// the frozen one. `current` maps worktree-relative paths to the digests of a
/// `ChangeFingerprint`; `frozen` maps worktree-relative paths to the sha256
/// the freeze record holds. An empty `changed` is not covered.
pub(crate) fn only_intact_frozen(
    changed: &[String],
    current: &BTreeMap<String, String>,
    frozen: &BTreeMap<String, String>,
) -> bool

/// The stage's freeze record as worktree-relative path to sha256, or `None`
/// when the stage has no freeze record or it cannot be read.
pub(crate) fn frozen_digests(stage: &Stage, work_dir: &Path) -> Option<BTreeMap<String, String>>
```

`ChangeFingerprint.files` holds `sha256_hex(bytes)` for a present file and the literal
`deleted` for an absent one; `FrozenFile.sha256` is the sha256 of the frozen bytes. Confirm both
are the same lowercase hex form before comparing; if they differ, normalise inside
`frozen_digests` and say so in your report.

## Changes

### 1. `stale_review` (`gate.rs`)

After `covers_review(latest, current)` returned false, and only when the base did not move
(`!base_move::moved(latest, current)`): compute `fingerprint::changed_since(&latest.files,
&current.files)`; when `frozen_digests(stage, work_dir)` is `Some(frozen)` and
`only_intact_frozen(&changed, &current.files, &frozen)` holds, return `None` (the round
covers). Every other path through the function is unchanged. `round_covers` calls
`stale_review` and inherits the rule.

`frozen_digests` maps each `FrozenFile.path` to the worktree-relative path the fingerprint
uses, the way `relock_frozen` joins the working-directory prefix; a stage whose `working_dir`
is `.` has no prefix.

### 2. `loom stage review status` (`review_status.rs`)

`freshness_report` prints `Latest round (N) matches the current fingerprint: yes|no` and the
list under `changed since last round:`. Three functions of this file call `covers_review`
directly (`freshness_report`, `append_exempt_changes`, `print_base_move_cover`). Make the yes/no
of `freshness_report` come from `gate::round_covers`, so the status and the completion gate
cannot disagree; leave the other two as they are unless they print the same verdict. When the round covers only through this rule,
print `yes (the only change is an accepted replacement of a frozen file)` and keep the path
list.

## Tests (write them; the orchestrator runs them)

- `frozen_cover.rs` test module: `only_intact_frozen` for an intact frozen file; for an intact
  frozen file plus a source file; for a frozen file with other content; for a deleted frozen
  file; for an empty `changed`. `frozen_digests` with a prefix and without.
- `gate_tests.rs`, following the file's round and fingerprint fixtures: a round, then a frozen
  file rewritten to the freeze record's content, is covered by `round_covers`; the same with a
  second, unfrozen path changed is stale; the same with the frozen file at other content is
  stale; a base move with only the frozen file changed still goes through the base-move path.
- Contract to turn green: `frozen_cover_contract_tests.rs`.

## Proof

From `loom/`: `cargo check --all-targets`.

## Traps

- The exemption needs equality with the CURRENT freeze record. Exempting every path the record
  names, whatever its content, would let an agent's edit of a frozen file ride a stale review.
- An unreadable or missing freeze record means "not covered". Never treat "could not read" as
  "nothing frozen changed".
- Do not change `covers_review` or the fingerprint itself: the recorded rounds and the daemon's
  observer compare stored values, and both callers already pass through `stale_review`.
- `gate.rs` is 280 lines and `gate_tests.rs` is a separate file; keep the new logic in
  `frozen_cover.rs`.
