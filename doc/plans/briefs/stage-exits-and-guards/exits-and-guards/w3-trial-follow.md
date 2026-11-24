# exits-and-guards / w3-trial-follow: a rerun of a replacement trial follows its own job

Tier: codex sol (`gpt-6.2-sol`, effort xhigh)
Read first: `doc/plans/briefs/stage-exits-and-guards/common.md`.

## Finding

`loom check <stage> --contracts --replacement <frozen-file>=<path>` prints "Run the same command
again in your next Bash call to wait for it". A session did so seven times in 50 seconds. Every
call relayed a NEW request, and the daemon answered each with `host job <new> queued; <previous>
cancelled`. The trial never finished until the session stopped rerunning and polled the result
files by hand.

Cause, read from the code:

- `drive` in `loom/src/host_verify/wait.rs` follows an earlier request only when `state_matches`
  holds, and `state_matches` compares `state.tree_digest` with the live worktree digest
  (`host_verify/digest.rs`: `git status --porcelain=v2` plus the content of every dirty file).
- A trial lays its replacement over the frozen file in the live stage worktree for the whole run
  (`trial` in `host_verify/child_contracts.rs`, through `with_admitted_overlay`). While the job
  runs, the worktree digest differs from the digest the request recorded.
- The rerun therefore misses its own state and relays a new request. On the daemon,
  `submit_selection` in `host_verify/submit.rs` never joins a contracts trial (`covers` returns
  false for `own`), so it supersedes the running job, whose overlay is then undone.

The request payloads confirm it: every rerun carried the same byte count and a different hash,
which is what a changed fixed-width `tree_digest` gives.

## Decision

A trial's replacement bytes are already part of the state file's name (`selection_key` in
`wait_state.rs` hashes each replacement). A rerun of a trial with the same HEAD and the same
replacements follows the recorded request whatever the worktree digest reads. The digest keeps
deciding for every other job: a plain check of an edited tree still relays a new request.

This is safe: the job itself compares the tree with its record after it restores the overlay
(`tree.differences(record)` in `trial`), reports a change, and writes no receipt for a tree that
moved; and `check_trial` in `stage_service/dispute_admission.rs` admits a filing only against a
receipt for the worktree's current HEAD and digest.

## Files

Own (write):
- `loom/src/host_verify/wait.rs`
- `loom/src/host_verify/wait_state.rs`
- `loom/src/host_verify/wait_state_tests.rs`
- `loom/src/host_verify/tests_wait.rs`
- `loom/src/relay/emit/stderr_text.rs`

Read only: `loom/src/host_verify/submit.rs`, `loom/src/host_verify/child_contracts.rs`,
`loom/src/host_verify/digest.rs`, `loom/src/models/dispute_replacement.rs` (`FileReplacement`).
Frozen (never edit): `loom/src/host_verify/trial_follow_contract_tests.rs`.

## Changes

### 1. `state_matches` (`wait.rs`)

A site is a trial when `site.contracts` is `Some` and the slice is non-empty. For a trial the
`state.tree_digest == digest` term is skipped; every other term stays. `pending_request_id` calls
the same function and so follows the same rule. `wait.rs` is 373 lines: put the predicate
(`fn is_trial(site: &WaitSite<'_>) -> bool` or a method) in `wait_state.rs` if `wait.rs` would
pass 395 after formatting. Update the doc comments of `state_matches` and `pending_request_id`
to state the rule and the reason (the daemon's own overlay changes the digest while the trial
runs).

A plain `--contracts` check (an empty replacement slice) lays nothing over the tree and keeps
the digest comparison.

### 2. The advice text (`stderr_text.rs`)

The sentence "A rerun within 120 s follows the same request and prints its `Host job:` line
instead of a new `received` line; follow that line." states a rule the code does not have: a
rerun follows for as long as the tree and the selection are unchanged, and 120 s
(`REQUEST_GRACE` in `wait.rs`) is only how long a request may go unseen by the daemon. Replace
"A rerun within 120 s follows the same request" with "A rerun on an unchanged tree follows the
same request". Find every test that pins the old sentence (`rg -n 'within 120 s' loom/src
loom/tests loom-hooks`) and report each one you cannot edit because you do not own it; a pinned
copy in a file you own is updated.

## Tests (write them; the orchestrator runs them)

In `tests_wait.rs`, using its `fixture()` and a site built like `Fx::site` with `contracts` set:

- `a_trial_rerun_follows_after_the_overlay_changed_the_tree`: request a trial replacing `a.txt`;
  write the replacement bytes to `a.txt` in the checkout; call again; the relay closure ran
  once in total.
- `a_trial_with_other_replacement_bytes_is_a_new_request`: second call with different
  replacement content relays again (the state path differs).
- `a_plain_check_of_an_edited_tree_is_a_new_request`: `contracts: None`, edit `a.txt` between
  the calls, the relay closure ran twice. This guards the rule's limit.
- `a_plain_contracts_check_keeps_the_digest_rule`: `contracts: Some(&[])`, edit between calls,
  relayed twice.

Contract to turn green: `trial_follow_contract_tests.rs`.

## Proof

From `loom/`: `cargo check --all-targets`.

## Traps

- Do not change `covers` or `submit_selection` in `submit.rs`. A daemon-side join would have to
  tell the overlay's digest change from a real edit, and it cannot.
- Do not drop the digest from the state file or from the request: the daemon binds the job to
  it, and the receipt check depends on the job's record carrying the digest of the tree before
  the overlay.
- `state.contracts == site.contracts.map(trial_files)` stays: it is what distinguishes a trial
  from a plain check that hashed to the same file name by accident of an empty selection.
