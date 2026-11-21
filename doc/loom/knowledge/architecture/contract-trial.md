# Contract Trial

> loom check --contracts, replacement trial receipt, overlay undo, env-red refusal

## Contract Trial

`loom check <stage> --contracts [--replacement <frozen-file>=<path> ...]` runs a stage's frozen contracts where completion runs: a Check host job, confined, calling `verify::contracts::completion::check`. A full `loom check <stage>` of a standard stage with a freeze runs them after the goal-backward checks. The selection rides as `JobRecord.contracts` plus a `replacements.json` job file, and `host_verify::submit::submit_contracts(work_dir, request, replacements, now)` is the entry point; `SubmitRequest` gained no field. The point is that a contract unable to run under the host job (a confined command launched from inside it) is found by the agent before completion, not after six disputes ([the incident](../mistakes/completion-runtime.md#confined-contract-tests-looped-through-six-accepted-disputes-2026-11-20)).

### Replacement trial

With `--replacement`, `verify/contracts/trial.rs::with_admitted_overlay` lays each replacement over its frozen file in the worktree, under the host-job stage lock taken by `child_contracts::admit_overlay` (the snapshot of the original bytes is taken after admission). The job runs the freeze's format and static gates on those files, then the contracts, with `CachePolicy::Bypass` (`completion::run`), and puts the frozen files back.

- **Receipt.** A pass writes `trial.json` (`TrialReceipt`: HEAD, plus a `TrialFile` of path and sha256 per replacement). `dispute_admission::check_trial` refuses a filing whose replacements are not covered by a receipt at the worktree's current HEAD (`trial::receipt_covers`); the adjudicator prompt reuses `receipt_files_cover` (`prompt/contract_replacement.rs`). A receipt is not tied to the freeze generation or to uncommitted implementation edits, and is lost once newer applied jobs push its job directory out of `KEPT_JOB_DIRS` (3).
- **Restore.** The final restore takes the stage lock and writes a file back only while it still holds the overlay bytes and the freeze does not record those bytes: an accepted ruling usually writes the very bytes the trial ran, so "the file still holds the overlay" alone would revert a legitimate accept. `frozen_restore.rs::undo_trial_overlay` writes the frozen copy, not the pre-trial bytes, which is safe because every gate refuses drift.
- **A killed trial.** A SIGKILLed trial child is undone daemon-side in `host_verify/signals.rs::kill_groups` through `child_contracts::undo_stopped_overlay`, and `recover_after_restart` undoes the overlay of a trial that was Running at restart. The restore is not spared from the watchdog.
- **Known leaks.** A SIGKILL mid-publish leaves a `.<name>.<rand>.tmp` beside the file (`fs/safe_replace.rs`); a superseded trial killed before its first confined spawn leaves its overlay until completion's drift check names it.

### Environment-red refusal

`verify/contracts/freeze.rs::judge_run` refuses a red whose output carries an environment marker (`verify/environment.rs::environment_marker`: `LAUNCHER_MARKERS` `bwrap:` and `operation not permitted`, then `HOST_MARKERS`: name-resolution and network failures, `connection refused`, `read-only file system`, `no space left on device`; the denial markers `eperm`, `sandbox_apply`, `deny(1)` and `couldntreadcurrentdirectory` are read only by `sandbox_denial` and `run_denial`, so the freeze and `dry_run_classify.rs` miss them), because such a red proves nothing about the implementation. `BuildFailed` stays accepted. `judge_output` (beside `judge_run`) carries the output text to the check. The marker applies even when the runner's summary parsed, so a contract whose expected failure message contains `connection refused` cannot be frozen; `plan verify --dry-run` (`dry_run_classify.rs`) treats the same markers as assertion text once a summary parsed. The writer's rules state the run environment: no network, no nested OS sandbox, writes only in the tree and TMPDIR. A contract about confinement asserts on the generated policy (`verify::criteria::confined_policy`) and launches nothing.

A trial with `--replacement` re-queues a new host job on every call and each submit cancels the previous one, so a second trial within 120 s returns a result only while its payload stays identical.

Limit: when the red is a build failure the freeze cannot tell the contract cannot run where completion runs; the recourse is the trial and `--replacement`.
