# Validation: verification, host jobs, contracts, confinement, stage cache, test-runner adapters

Revision `62be2a3d2`. Paths are repo-relative. Every cited `file:line` in the reviews was checked; drift is noted per finding.

## E1 — A build stopped before the compiler, or a missing exit code, freezes as a red contract

**Verdict:** CONFIRMED
**Evidence:**
- `loom/src/testrun/adapters/cargo_test.rs:171-195`: `parse` sets `build_failed` for any `^error(\[E\d+\])?: ` line (regex at `:84`) when no `running N tests` line exists, so `error: failed to run rustc to learn about target-specific information` is `build_failed`. `compile_rejected` (`:193`) additionally requires `could not compile`.
- `loom/src/verify/contracts/freeze.rs:86-129` (`judge_run`): `RunOutcome::BuildFailed` maps to `BUILD_FAILED` and is accepted. `refuse_environment_red` is skipped for it (`:121`). `Unparsed | None if exit != Some(0)` (`:114`) accepts `exit == None` (signal death) as `UNVERIFIED`.
- `loom/src/verify/contracts/freeze/prepared.rs:94-139`: `judge_prepared_result` computes `compile_rejected` after `judge_run` and uses it only to choose `warn_if_build_stopped` (`:171-189`), which emits a warning after acceptance.
- `prepared.rs:252-266` (`a_build_stopped_before_the_compiler_is_warned_with_its_output`) asserts `outcome == BUILD_FAILED` for the rustc-unavailable output, so the acceptance is a tested behaviour.
- Guards checked: `timed_out` and `!output_complete` are refused first (`prepared.rs:100-117`). Neither covers a toolchain failure or a signal kill with complete capture.

**Fix seam:**
- `judge_run` (`loom/src/verify/contracts/freeze.rs:86`). Add a `compile_rejected: bool` parameter. `BuildFailed` is accepted only when it is true. Otherwise return `Err` naming an infrastructure failure: "the build stopped before the compiler judged the code (toolchain or boundary failure)", quoting the output tail.
- Reuse `tail_to_byte_limit` / `run_output_line` / `STOPPED_BUILD_TAIL_BYTES` from `prepared.rs:164-189` for the quote. Then delete `warn_if_build_stopped`, which this change leaves unused.
- Change `Some(RunOutcome::Unparsed) | None if exit != Some(0)` to `if matches!(exit, Some(code) if code != 0)`. Add an arm that refuses `exit == None` ("the contract ended without an exit code").
- In `judge_prepared_result` (`prepared.rs:94`), compute `compile_failed` before `judge_run` and pass it. With no adapter, pass `false`. This is unreachable for `BuildFailed`, because `classify` runs only with an adapter.
- Adapters without an override keep `compile_rejected == build_failed` (`loom/src/testrun/mod.rs:210`), so only cargo-test and cargo-nextest change behaviour.
- Callers to update: `prepared.rs:125`, `loom/src/verify/contracts/freeze_environment_tests.rs:29`, `loom/src/testrun/adapters/bun_test.rs:233` (test), `loom/src/commands/stage/contracts/freeze_tests.rs:58-77`.

**Regression test:**
- In the `prepared.rs` tests module, use the `judged_lines(stderr)` helper (`prepared.rs:~236`). Invert `a_build_stopped_before_the_compiler_is_warned_with_its_output` so it requires `judged.judged.is_err()` with the message naming the toolchain. It fails today because the outcome is `BUILD_FAILED`.
- Keep `a_compile_rejection_is_not_warned` as the control. It still accepts with `compile_failed == true`.
- In `freeze_tests.rs`, add `judge_run(&spec(), None, RunSummary::default(), None, None).is_err()`. It fails today because the result is `UNVERIFIED`.

**Files a fix touches:** `loom/src/verify/contracts/freeze.rs`, `loom/src/verify/contracts/freeze/prepared.rs`, `loom/src/verify/contracts/freeze_environment_tests.rs`, `loom/src/commands/stage/contracts/freeze_tests.rs`, `loom/src/testrun/adapters/bun_test.rs`.
**Size / tier:** M; judgment (it decides what evidence admits a frozen contract).
**Overlap:** CR-47 (adapter `build_failed` heuristics feed the same `classify`).

## E2 — A replacement dispute can cite a trial run on a different dirty tree, or a trial not yet settled

**Verdict:** CONFIRMED
**Evidence:**
- `loom/src/host_verify/child.rs:316-337` (`differences`): the child compares HEAD and the tree digest against `record.tree_digest` during the trial.
- `loom/src/host_verify/child_contracts.rs:259-269` (`save_receipt`): the receipt stores `head` and `files` only (`TrialReceipt`, `loom/src/verify/contracts/trial.rs:37-40`). The digest is not retained.
- `loom/src/stage_service/dispute_admission.rs:230-252` (`check_trial`): filing compares only the current HEAD and the replacement hashes (`receipt_covers`, `trial.rs:255-272`). An uncommitted edit to non-frozen code after the trial keeps both, and the old pass is reused.
- `trial.rs:345-358` (`passed_trial`): a record in `JobState::Running` with a passing result file counts. That window covers the time between the child writing its result and the runner's deadline/overrun settlement (`loom/src/host_verify/runner.rs:146-150`).
- `loom/src/orchestrator/adjudication/prompt/contract_replacement.rs:70-71` gives the judge `load_receipt` + `receipt_files_cover` with no HEAD or digest check. That is the "historical evidence" path.
- Mitigation confirmed: completion reruns the contracts (`loom/src/verify/contracts/completion.rs:159-200`).

**Fix seam:**
- Add `pub tree_digest: String` to `TrialReceipt` (`trial.rs:37`). Set it from `record.tree_digest` in `save_receipt` (`child_contracts.rs:259`); `differences` has just confirmed that value.
- Extend `receipt_covers(receipt, head, tree_digest, replacements)` (`trial.rs:255`) to refuse a digest mismatch with the existing `RUN` hint.
- In `check_trial` (`dispute_admission.rs:235`), compute the digest through the same pinned `git`. For `stage.stage_type == StageType::Knowledge`, use `digest::knowledge_tree_digest_in(&git)`, the selection made at `child.rs:280`. For any other stage, use `digest::tree_digest_in(&git)` (`loom/src/host_verify/digest.rs:60-67`).
- In `passed_trial` (`trial.rs:348`), require `JobState::Passed` and drop `Running`. `loom check` returns only on a terminal state (`loom/src/host_verify/wait.rs:357-369`), so normal filings are unaffected. Update the doc comment at `trial.rs:345-347`.
- In `contract_replacement.rs:70`, pass the stage's current HEAD and digest and use `receipt_covers`, so the judge never sees a stale pass described as current.
- Old receipts fail `deny_unknown_fields`/missing-field deserialization and are skipped with the existing warning (`trial.rs:336-339`). No migration is needed.

**Regression test:**
- In `loom/src/stage_service/dispute_trial_tests.rs`, `record_trial` (`:21`) currently writes `tree_digest: "digest"`. Make it write the real digest. Add a test that records a passing trial, edits an untracked or non-frozen file in the worktree, and files the dispute. Expect rejection; today the filing is accepted.
- In `loom/src/verify/contracts/trial_receipt_tests.rs`, add a test: a `Running` record with a passing result yields `load_receipt == None`. Today it returns the receipt.

**Files a fix touches:** `loom/src/verify/contracts/trial.rs`, `loom/src/host_verify/child_contracts.rs`, `loom/src/stage_service/dispute_admission.rs`, `loom/src/orchestrator/adjudication/prompt/contract_replacement.rs`, tests in `loom/src/stage_service/dispute_trial_tests.rs`, `loom/src/verify/contracts/trial_receipt_tests.rs`, `loom/src/verify/contracts/trial_contract_tests.rs`, `loom/src/orchestrator/adjudication/prompt/contract_replacement_tests.rs`, `loom/src/host_verify/tests_child_contracts.rs`.
**Size / tier:** M; judgment (admission evidence binding).
**Overlap:** E4 (host job history reads), CR-37 (`host_verify` gating).

## E3 — A failed spawn followed by a failed `Failed` write strands a `Running` job

**Verdict:** CONFIRMED
**Evidence:**
- `loom/src/host_verify/runner.rs:205-238` (`start`): the Queued-to-Running CAS happens before `spawn_child`. On spawn error it calls `fail_to_start(..., JobState::Running, ...)` (`:240-254`), whose `compare_and_set` error propagates with `?`. `start_queued` (`:190-202`) only logs it.
- The job is not in `self.running`, so `settle_held` (`:111-121`) never sees it. `queued_records` (`:301-315`) selects only `Queued`.
- `pending::promote_locked` (`loom/src/host_verify/pending.rs:33-38`) returns early while current is non-terminal, so the waiting job is never promoted. The stage stays stalled until a daemon restart, when `recover_after_restart` / `interrupt` (`loom/src/host_verify/signals.rs:87, 131-152`) marks it Interrupted, or until an operator cancels it.

**Fix seam:**
- Options:
  - (A) Keep `failed_starts: HashMap<job_id, (stage_id, detail)>` in `HostJobRunner` and retry `fail_to_start` at the top of `settle_held`.
  - (B) In `queued_records`, or a sibling scan run from `start_queued`, fail every current record that is `Running`, has `pid == None` and is not held by `self.running`.
- Recommend B. The runner is the only writer of `Running`. A held job whose pid write failed is in `self.running` and is skipped. A previous daemon's `Running` record is marked Interrupted by startup recovery before the first tick. B also needs no fault injection to test.
- Implement B with `compare_and_set(work_dir, stage, job, JobState::Running, |r| end_record(r, JobState::Failed, Some("verify-host never started".into())))`. Return the record in `ended` so waiters and application see it.

**Regression test:** in `loom/src/host_verify/tests_runner.rs`, use the helpers `state_root()`, `shell()`, `queue()` and `tick_until()` (`:36-87`). Queue a job, CAS it to `Running` with `pid: None` through `store::save_current`, build a fresh `HostJobRunner` and tick once. Assert the record is `Failed`, and that a pending job then promotes and starts. Today the record stays `Running` forever.
**Files a fix touches:** `loom/src/host_verify/runner.rs` (400 lines already; put the scan in a new `runner_orphans.rs` or move `compare_and_set`/`end_record` out), `loom/src/host_verify/tests_runner.rs`.
**Size / tier:** S-M; judgment (job-state concurrency).
**Overlap:** E4 (the same store).

## E4 — An older passing completion can outrank a newer failure when history is damaged

**Verdict:** CONFIRMED (needs an unreadable newer per-job record, as the review says)
**Evidence:**
- `loom/src/host_verify/verdict_reuse.rs:56-89` (`latest_of` / `readable_record`): an unreadable record, or one whose identity does not match its directory, is logged and skipped. The max is then taken over the remaining records.
- `verdict_reuse.rs:25-32` (`decides`): uses the same `latest_of`, so the same skip makes the older pass "newest" a second time.
- `loom/src/host_verify/verdict_complete.rs:90` (`start`): picks `latest` through it.
- `verdict_complete.rs:103-113` (`deciding_job`): reads the current slot but returns early unless its kind is `VerdictComplete`. A current slot naming a newer failed `Complete` is never consulted.
- Remaining bindings (review, policy hash, clean tree at `latest.head`, `counted_result` at `:120-143`) still apply. A missing result file goes to `counted_result`'s refusal, as the review says.

**Fix seam:**
- Change `latest_of` (`verdict_reuse.rs:56`) to fail closed. Return `Result<std::result::Result<Option<JobRecord>, String>>`, the `Ok(Err(reason))` style `counted_result` already uses. Any unreadable or mismatched record yields `Err("the stage's host job history cannot be ordered: job <id> is unreadable")`.
- Also load `store::load_current` (`loom/src/host_verify/store.rs`). When its kind is in `kinds`, treat it as a candidate. If it outranks the per-job maximum by `(submission_order, queued_at)`, or its per-job copy is missing, it wins.
- Callers: `verdict_complete.rs:90` maps the reason to `VerdictStart::Ineligible(reason)`; `verdict_reuse.rs:30` maps it to `Ok(false)`, so a fresh run is queued; `verdict_complete.rs:238` (test-only `latest_completion_head`) adapts.

**Regression test:** in `loom/src/host_verify/verdict_reuse_tests.rs`, use the fixtures `held()`, `submit()` and `current()` (`:18-40`). Record an older passing `Complete`, then a newer same-head `Failed` `Complete`, and overwrite the newer job's `record.json` with garbage. Assert `submit()` is not `VerdictStart::Reused`. Today it reuses the older pass.
**Files a fix touches:** `loom/src/host_verify/verdict_reuse.rs`, `loom/src/host_verify/verdict_complete.rs`, `loom/src/host_verify/verdict_reuse_tests.rs`.
**Size / tier:** S-M; judgment (completion authority).
**Overlap:** E2, E3, CR-37 (`verify_bound_run` is shared by verdict reuse and `apply.rs`).

## CR-24 — `unlinkat` inside an active `readdir` loop, and an unchecked `errno`

**Verdict:** CONFIRMED (impact narrower than stated)
**Evidence:**
- `loom/src/verify/contracts/store_generation_gc.rs:42-66`: `remove_contents` iterates `libc::readdir` on a `DIR*` and calls `remove_entry` (`unlinkat`, `:94`) on the same directory mid-iteration. A NULL return is treated as EOF without clearing or checking `errno`.
- POSIX leaves readdir behaviour on a changing directory unspecified.
- A skipped entry makes the parent's `AT_REMOVEDIR` fail with `ENOTEMPTY`, which surfaces as an error rather than silently. A readdir I/O error at the top level silently leaves entries.
- The only production caller, `loom/src/verify/contracts/store.rs:285-287`, logs a warning, so the effect is leftover staging trees plus a warning. No data loss.
- The project already documents and solves this in `loom/src/fs/safe_fs/write/tree.rs:118-145`, which lists names in full before removing any.

**Fix seam:**
- Replace the body of `gc::cleanup` (`store_generation_gc.rs:9`) with `crate::fs::safe_fs::safe_remove_tree_in_workdir(safe_open_dirfd(dir)?.as_raw_fd(), Path::new("staging"))` (`tree.rs:43`, `pub fn safe_remove_tree_in_workdir(dirfd: RawFd, relpath: &Path) -> Result<()>`). Delete `Directory`, `remove_contents` and `remove_entry`.
- `staging/` is recreated by `safe_create_dir_all_in_workdir` in `store_generation.rs:95-96`, so removing the directory itself is safe. A missing `staging` is already `Ok`.
- Limit: `MAX_TREE_ENTRIES` (4096) per call. A staged generation holds the frozen files of one stage, far below it. Revisit if freeze sets grow past a few thousand files.

**Regression test:** in `loom/src/verify/contracts/store_tests.rs` (`generation::cleanup(&dir)` at `:107`), stage nested directories of 200+ files plus a symlink pointing outside, then run cleanup. Assert `staging/` is gone and the symlink target survives. The skip itself is filesystem-dependent and cannot be made to fail reliably today. The test guards the rewrite.
**Files a fix touches:** `loom/src/verify/contracts/store_generation_gc.rs` (or inline into `store_generation.rs` and delete the file), `loom/src/verify/contracts/store_tests.rs`.
**Size / tier:** S; mechanical.
**Overlap:** none found.

## CR-26 — Knowledge-stage `Protected` leaves the `.loom` parent renameable on Linux

**Verdict:** CONFIRMED (bubblewrap; Seatbelt unmeasured)
**Evidence:**
- `loom/src/verify/criteria/confine_protected.rs:62-91` (`protected`): `read_only` holds the state root `<tree>/.loom/work` and the direct children `.git`, `.worktrees` and `.claude`.
- `confine_protected.rs:97-108` (`bind`): only `--ro-bind p p` is emitted, after the writable tree bind at `loom/src/verify/criteria/confine_host.rs:235-237`. The call is at `confine_host.rs:252`.
- The kernel refuses to rename a mount point (`EBUSY`) but not an ancestor of one. The direct children are mount points themselves and safe. `.loom` is not a mount point, so `mv .loom .loomx && mkdir -p .loom/work` moves the main checkout's state root on the host and plants a writable one.
- The repository's knowledge (`concerns/sandbox-and-confinement-gaps.md`, "A Read-Only Bind Inside a Writable Tree Escapes Through a Renamed Parent") records this measured with bwrap 0.11.1, and records that self-binding the intermediate directory makes the rename fail.
- The legacy `.work` state root is a direct child and is safe.

**Fix seam:**
- Add `pinned: Vec<PathBuf>` to `Protected` (`confine_protected.rs:47`). Fill it in `protected()` with every directory strictly between `tree` and each `read_only` path: for `.loom/work`, `<tree>/.loom`. Use `path.ancestors()` up to `tree`, keeping only non-symlink directories.
- In `bind()`, emit `--bind p p` for each pinned path before the `--ro-bind` loop. The pins must follow the tree bind, which holds because `bind()` is called after it.
- `seatbelt_rules` (`:113`) ignores the new field. Seatbelt denies by path, so a rename of `.loom` is a write under the denied subpath's ancestor, which is not denied. Record it as unmeasured, matching the knowledge entry.
- Update `confine_protected_tests.rs:42-60`, which compares `Protected` exactly.

**Regression test:** `loom/src/verify/criteria/confine_protected_tests.rs:132` (`a_knowledge_criterion_cannot_change_the_state_root_or_read_its_secrets`, using `checkout()` at `:23` and `canary()`). Add a criterion running `mv .loom .loomx`. Assert it fails and that `.loom/work` on the host is unchanged. Today the rename succeeds.
**Files a fix touches:** `loom/src/verify/criteria/confine_protected.rs`, `loom/src/verify/criteria/confine_protected_tests.rs`.
**Size / tier:** S-M; judgment (security boundary).
**Overlap:** CR-35 (the same confinement layer), CR-34.

## CR-34 — Proxy URL credentials reach confined commands

**Verdict:** NARROWED (proxy credentials: confirmed; DBUS/XAUTHORITY: no new exposure)
**Evidence:**
- `loom/src/process/environment.rs:95-102`: `HTTP_PROXY`, `HTTPS_PROXY`, `ALL_PROXY` and their lowercase forms are in `STAGE_HOST_ENV_ALLOWLIST` and are forwarded verbatim by `apply_stage_environment_from` (`:123-136`).
- Confined criteria get them through `apply_confined_environment` (`loom/src/verify/criteria/confine.rs:333-334`).
- Agent sessions do not get them: `AGENT_SESSION_ENV_NAMES` (`environment.rs:12-37`) has no proxy names. So a test the agent wrote can print `user:pass@` from a criterion's environment into output the agent reads. That is a real escalation over the session.
- The stage cache already drops `GOPROXY` with userinfo (`loom/src/stage_cache/environment.rs:46-70`).
- `DBUS_SESSION_BUS_ADDRESS`, `XAUTHORITY` and `DISPLAY` are in `AGENT_SESSION_ENV_NAMES` too, so forwarding them to criteria gives nothing the session lacks. Whether the bus socket is reachable inside bwrap depends on binds and is not statically settled.
- The same allowlist also feeds the terminal spawners (`loom/src/orchestrator/terminal/native/spawner.rs:61`, `loom/src/orchestrator/terminal/tmux/mod.rs:134,151`) and the daemon (`loom/src/daemon/server/environment.rs:248`).

**Fix seam:**
- In `apply_confined_environment` (`confine.rs:333`), after `apply_stage_environment`, call `command.env_remove(name)` for each of the eight proxy names whose value has URL userinfo.
- Move `goproxy_has_userinfo` (`stage_cache/environment.rs:46`, `pub fn goproxy_has_userinfo(value: &OsStr) -> bool`) to `loom/src/process/environment.rs` as `url_list_has_userinfo`. Have `host_env_from` and the new call site share it. It already handles single URLs and lists.
- Drop the variable instead of stripping its userinfo: a stripped URL still fails with 407, and dropping matches the `GOPROXY` policy.
- Leave the terminal-spawn allowlist unchanged: the tmux server and the agent wrapper need the proxy for the session's own network.
- Do not change DBUS/XAUTHORITY in this fix. Open question: a separate confined-only deny list (`XAUTHORITY`, `DBUS_SESSION_BUS_ADDRESS`, `TMUX`, `TMUX_PANE`) would harden criteria, but GUI or Electron test suites may need `DISPLAY` and the bus. Recommend deferring it until a stage needs it.

**Regression test:** in `loom/src/verify/criteria/confine_host_tests.rs`, or a unit test beside `apply_confined_environment`, run `/usr/bin/env` confined with `HTTPS_PROXY=http://u:p@proxy:8080` and `HTTP_PROXY=http://proxy:8080` in the source. Assert the first is absent and the second present. Today both are present. Keep `environment.rs` test `ambient_secret_canary_is_excluded_but_terminal_basics_survive` (`:186`) as it is, since the terminal allowlist is unchanged.
**Files a fix touches:** `loom/src/verify/criteria/confine.rs`, `loom/src/process/environment.rs`, `loom/src/stage_cache/environment.rs`, one confine test file.
**Size / tier:** S; needs-exploration (the test environment source is process-wide).
**Overlap:** CR-35 (`stage_cache/environment.rs`).

## CR-35 — The host job's sccache socket `h.sock` sits in the stage-writable cache root

**Verdict:** NARROWED
**Evidence:**
- `loom/src/stage_cache/environment.rs:218-233` (`sccache_socket`): `Consumer::HostJob` uses `self.root.join("h.sock")`. Session and worker use `socket_dir`, set by `with_socket_dir` (`loom/src/orchestrator/terminal/native/launch/host.rs:159`, `loom/src/native_worker/boundary/environment.rs:171`).
- The cache root is shared and writable by the stage session and its workers. The doc comment at `:218-221` says so, and `ROOT_DIRS` are set from it at `:188-190`.
- Seatbelt grants exactly that path (`loom/src/verify/criteria/confine_seatbelt.rs:167-183`). Bubblewrap binds the root writable (`confine_host.rs:214-221`).
- What narrows it: the cache contents themselves (sccache dir, cargo, go, m2) are already shared and writable, and the code accepts "no integrity guarantee between them" (`environment.rs:219`), so result poisoning is an accepted residual. The socket adds two things. A session can pre-bind `h.sock` and serve the host job's compiles from its own server. A session can connect to the live host-job server and run compilers inside the host job's boundary. Both are real on Linux. On macOS the session's own Seatbelt profile decides whether it can bind there, which cannot be settled statically.

**Fix seam:**
- Give `Consumer::HostJob` a private socket directory the session cannot write.
- Options:
  - (a) The per-command `CargoScratch` directory (`loom/src/verify/criteria/confine_scratch.rs:291`). It is fresh and randomly named, but sits under `TMPDIR` roots a Linux session may also write.
  - (b) The host job's own directory under the state root (`verification/<stage>/<job>/`), which session capsules deny for writes.
- Recommend (b), with the existing `SUN_LEN` fallback that omits `SCCACHE_SERVER_UDS` when the path is too long (`environment.rs:232`). Plumb it the way sessions do: `StageCache::with_socket_dir(job_dir)` (`loom/src/stage_cache/mod.rs:142`) at the host-job call site (`confine_host.rs:194-196`, `cache.environment(Consumer::HostJob, ...)`). Make `sccache_socket` use `socket_dir` for `HostJob` too (`h.sock`).
- `sccache_socket_rule(root)` (`confine_seatbelt.rs:173`) must then take the socket path instead of the root. The bwrap side needs a `--bind` of that directory.
- The job directory path must reach `confine_host`. Check how `Grants` is built (`confine_host.rs`); this is the exploration part.

**Regression test:**
- `loom/src/verify/criteria/confine_stage_cache_tests.rs:207-221` and `:362` currently assert `granted.join("h.sock")` under the root. Change them to assert the socket is outside `cache.root()`.
- `loom/src/stage_cache/tests.rs:119` asserts the root path. Update it the same way. Each fails today.

**Files a fix touches:** `loom/src/stage_cache/environment.rs`, `loom/src/stage_cache/mod.rs`, `loom/src/verify/criteria/confine_host.rs`, `loom/src/verify/criteria/confine_seatbelt.rs`, `loom/src/verify/criteria/confine_stage_cache_tests.rs`, `loom/src/stage_cache/tests.rs`, `loom/src/verify/criteria/confine_seatbelt_network_tests.rs`.
**Size / tier:** M; judgment (security boundary, cross-module).
**Overlap:** CR-26, CR-34.

## CR-36 — `completion-attestation.key` ignores write bits and owner, and builds the key from two UUIDs

**Verdict:** NARROWED
**Evidence:**
- `loom/src/handoff/completion/attest.rs:142-151` (`refuse_unsafe_mode`): it checks only `mode & 0o044`. A group- or world-writable key (`0o622`) is accepted, and its writer chooses the key and can forge attestations. That part is confirmed.
- No owner check. That is redundant for a non-root daemon: a foreign-owned file the daemon can read must carry group or other read bits, which are already refused. It matters only when loom runs as root.
- `random_key` (`:153-158`) concatenates two `Uuid::new_v4()` values. uuid's `v4` feature uses the OS CSPRNG (`getrandom`), so the key has 244 random bits. That is adequate; replacing it is hygiene, not a vulnerability.
- Creation goes through `safe_create_new` (`:28`).

**Fix seam:**
- In `refuse_unsafe_mode`, reject `mode & 0o077 != 0` and `metadata.uid() != geteuid()`, following the existing pattern in `loom/src/verify/criteria/confine_scratch/reclaim.rs:80` (`metadata.uid() == uid && mode & 0o022 == 0`).
- Keep `random_key` unless a CSPRNG crate is already a dependency. `Cargo.toml` has only `uuid` with `v4`, so do not add `getrandom` just for 12 bits.
- Keep the error text distinct per cause.

**Regression test:** in `loom/src/handoff/completion/attest_tests.rs`, next to `attestation_key_refuses_group_readable_file` (`:96`), add a `0o620` (group-writable) case. Assert `attestation_key` errs; today it returns the key.
**Files a fix touches:** `loom/src/handoff/completion/attest.rs`, `loom/src/handoff/completion/attest_tests.rs`.
**Size / tier:** S; mechanical.
**Overlap:** none.

## CR-37 — `host_verify/apply.rs` evaluates completion gates outside the sessions lock

**Verdict:** NARROWED
**Evidence:**
- `loom/src/host_verify/apply.rs:183-202` (`apply_passed`) calls `check_outside_lock` (`:221-228`, which runs `gates::refusal` at `:227`) before `complete_stage` (`:264-280`) takes `locked_dir_update(&sessions_dir, ...)`.
- Inside the lock, `verify_bound_run` (`:306-332`) re-checks the disposition blocker, the head, that the committed content equals `result.head` (it refuses uncommitted content), and the policy hash. It does not re-run the gates.
- What narrows it: the tree-derived gate inputs (the fingerprint the integrity and review gates compare, `loom/src/daemon/server/observer.rs:78-88`) are pinned by the commit binding under the lock.
- What stays open: the review rounds and `integrity.json` (`loom/src/verify/review/store.rs:168-191`, `loom/src/verify/review/verdict_records.rs:96`) change under the reviews-directory lock, not the sessions lock. A round with open findings recorded between `check_outside_lock` and the transition is missed. Moving `gates::refusal` inside the sessions lock alone would not close that, because review writers do not take the sessions lock.

**Fix seam:**
- Inside the `complete_stage` closure (`apply.rs:268`), after `validate_active_identity`, take the stage's reviews-directory lock and re-run `gates::refusal(work_dir, &record.stage_id)`, mapping `Some(reason)` to `binding_failed(reason)`.
- Lock order is sessions, then reviews. Confirm no path takes reviews then sessions (`rg locked_dir_update` over `loom/src/verify/review` and `loom/src/stage_service`).
- Cost: the gates take seconds (`loom/src/host_verify/gates.rs:3-6`) while the sessions lock is held, which blocks session writers for that long. Alternative: re-read only the review and integrity record generations under the reviews lock and compare them with what the outside check saw. That is cheaper but needs a generation stamp the stores do not have today.
- Recommend the full re-run. Completions are rare and the lock wait is bounded.
- Apply the same change to the verdict path, which shares `verify_bound_run` (`loom/src/host_verify/verdict_complete.rs:196`, through `complete_held`).

**Regression test:** in `loom/src/host_verify/tests_apply.rs`, apply a passed record after recording a review round with an open finding. That needs a hook between `check_outside_lock` and the lock, so use a `#[cfg(test)]` fault point as in `generation::faults` (`loom/src/verify/contracts/store.rs:281`). Assert `Applied::Rejected`/rejection. Today the stage completes.
**Files a fix touches:** `loom/src/host_verify/apply.rs`, `loom/src/host_verify/verdict_complete.rs`, `loom/src/host_verify/tests_apply.rs`.
**Size / tier:** M; judgment (lock ordering).
**Overlap:** E4 (verdict completion), E2.

## CR-38 — Fragmented stage-ID validation

**Verdict:** NARROWED (no path traversal; inconsistency and maintenance only)
**Evidence:** `validate_id` (`loom/src/validation.rs:61-105`) allows only `[A-Za-z0-9_-]`, at most 128 bytes, no reserved names, and no `NN-` prefix. Plans enforce it on every stage id (`loom/src/plan/schema/validation.rs:305`), so no real stage id falls outside it. The six validators:

| Validator | Accepts that `validate_id` rejects | Path use | Callers a unification would break |
| --- | --- | --- | --- |
| `valid_stage_path_component` (`loom/src/validation.rs:25-32`) | `.` inside or leading (`.hidden`, `a.b`), any length, `NN-foo`, reserved names (`con`) | single path component; rejects `/`, `\`, `..` | its tests expecting `stage-1.a_b` valid |
| memory `validate_stage_id` (`loom/src/commands/memory/handlers/work_dir.rs:60-65`) | same (delegates) | journal file name | test at `work_dir.rs:72-85` (`stage-1.a_b` valid); callers `read.rs:64,162,304` unaffected in practice |
| status `valid_stage_id` (`loom/src/status/data/sanitize.rs:148`; the review's `status/data/sanitize.rs` is under `loom/src/status/`, not `commands/status/`) | same (re-export) | `codex_jobs.rs:74`, `resolver.rs:94`, `rounds.rs:66`, `execution_models.rs` | test at `sanitize.rs:302` |
| `StageCache::resolve` (`loom/src/stage_cache/mod.rs:94-100`) | anything without `/`, NUL or `..` | none: the id is hashed into the root name (`:106-111`) | none; the `"provision"` pseudo-id (`confine_seatbelt.rs:192`) passes `validate_id` |
| native `validate_stage_id` (`loom/src/native_worker/ids.rs:75-89`) | nothing: it ends in `validate_id` and only adds typed `IdError`s | index/store paths | none |
| tick `valid_stage_id` (`loom/src/orchestrator/tick.rs:161-167`) | `.`, `..`, dots anywhere, `NN-foo` | none: busy-line display and a `debug_assert` (`:120`) | none in the tree |

**Fix seam:**
- Make `valid_stage_path_component` return `validate_id(id).is_ok()` and keep the name; it has the three callers above. Flip the `stage-1.a_b` cases in the tests at `work_dir.rs:72-85` and `sanitize.rs:302`.
- Replace tick's private `valid_stage_id` with `crate::validation::validate_id(id).is_ok()` and delete `MAX_STAGE_ID_BYTES` if it is otherwise unused.
- Replace the `ensure!` in `StageCache::resolve` with `validate_id(stage_id).context(...)`.
- Leave `native_worker::ids::validate_stage_id` as it is; it already delegates. Its pre-checks exist for typed errors.
- Do not add a new helper.

**Regression test:** extend `work_dir.rs` `stage_ids_require_safe_path_components` and the `sanitize.rs:302` test with `".hidden"`, `"01-foo"` and `"con"` as invalid. Today each is accepted.
**Files a fix touches:** `loom/src/validation.rs`, `loom/src/commands/memory/handlers/work_dir.rs`, `loom/src/status/data/sanitize.rs`, `loom/src/orchestrator/tick.rs`, `loom/src/stage_cache/mod.rs`.
**Size / tier:** S; mechanical.
**Overlap:** CR-29 (`wrapper.rs` `pid_key` without `validate_id`, outside this group).

## CR-47 — Test-runner adapter parsing and filter bugs

Each sub-claim has its own verdict.

**swift_test pre-suite `error:` overrides parsed counts — Verdict: NARROWED**
**Evidence:**
- `loom/src/testrun/adapters/swift_test.rs:68-82` sets `build_failed` when an `error:` line precedes the first `Test Suite ... started`, even when `Executed N tests` counts parsed. `classify` then returns `BuildFailed` first (`loom/src/testrun/outcome.rs:16`).
- I found no standard SwiftPM output where counts parse and a genuine `error:` precedes the suite start: a pre-suite compiler error stops the run.
- The ordering is fragile, though: `combined_output` concatenates stdout then stderr (`loom/src/testrun/recognize.rs:315-317`) instead of interleaving them, and the fixtures are hand-written, not captured (`loom/src/testrun/fixtures/swift-test/PROVENANCE.md`).

**Fix seam:** in `parse`, set `build_failed` only when the counts did not parse: `summary.executed.is_none() && build_failed`. That matches `dotnet_test.rs:114-120`, where a result wins.
**Regression test:** add a scenario to `swift_fixture_outcomes_and_counts` (`swift_test.rs:~138`; fixtures in `loom/src/testrun/fixtures/swift-test/`) with a pre-suite `error:` line in stdout and then one failing suite. Expect `Failed`. It is `BuildFailed` today. Update the `names.len() == 5` count.

**swift_test omits Swift Testing (`@Test`) — Verdict: CONFIRMED**
**Evidence:**
- Only the XCTest `Executed N tests` line is parsed (`swift_test.rs:21-24`).
- A Swift Testing-only package prints XCTest's `Executed 0 tests` and Swift Testing's own `Test run with N tests passed/failed` line. The adapter reads `executed: Some(0)`, so `NotSelected` (`outcome.rs:19`), and a Swift Testing contract cannot freeze.

**Fix seam:** add a `SWIFT_TESTING` regex for `Test run with (\d+) tests? (passed|failed)` and the issue count. Sum it with the XCTest totals. This needs captured output from a Swift 6 toolchain (needs-exploration).

**dotnet_test misses `-t` — Verdict: CONFIRMED**
**Evidence:** `loom/src/testrun/adapters/dotnet_test.rs:87-96` checks `has_flag(&args, "--list-tests")` only. `dotnet test -t` counts as a full run.
**Fix seam:** `|| has_flag(&args, "-t")`.
**Regression test:** the recognition/full-run tests in the same file. Assert `!is_full_run("dotnet test -t")`.

**dotnet_test unescaped VSTest filter characters — Verdict: CONFIRMED**
**Evidence:** `dotnet_test.rs:98-103` builds `FullyQualifiedName={test}` raw. VSTest filter syntax treats `\ ( ) & | = ! ~ ,` as operators unless backslash-escaped, so a name with `(`, `,` or `=` (parameterized display names) misfilters or errors.
**Fix seam:** escape each of `\()&|=!~` with `\` before `shell_quote`. Open question: for xUnit theories, `FullyQualifiedName` excludes arguments, so the contract's `test` should be the method name. Document that in the adapter comment.
**Regression test:** assert the command for `Ns.C.M(x: 1)` contains `M\(x: 1\)`.

**maven `-q` hides the Surefire summary — Verdict: CONFIRMED (citation drift)**
**Evidence:**
- The `-q` is in `single_test_command` at `loom/src/testrun/adapters/maven.rs:118-120`, not in the cited `:57-76`, which are the summary regexes.
- `-q` suppresses `[INFO]` lines, so a passing single test prints no `[INFO] Tests run:` aggregate. It parses as `Unparsed` with exit 0 instead of `Passed`.
- Failures (`[ERROR] Tests run:`) and no-match still parse. The fixtures were authored without `-q` (`loom/src/testrun/fixtures/maven/one-pass.meta`), so they never exercise it.

**Fix seam:** drop `-q` in `single_test_command`.
**Regression test:** assert `single_test_command` has no `-q`.

**gradle synthetic `executed: Some(1)` — Verdict: CONFIRMED (documented lower bound)**
**Evidence:**
- `loom/src/testrun/adapters/gradle.rs:99-118`: `BUILD SUCCESSFUL` plus a plain `> Task :test` yields `executed: Some(1), failed: Some(0)`, so `Passed`.
- A `--tests` selection whose only test is skipped (`@Disabled`) or never executed still reads as `Passed`, so a completion contract check can pass without running its test.

**Fix seam:**
- Options:
  - (a) Read JUnit XML under `build/test-results/test/*.xml` for exact counts.
  - (b) Add an init script or `-i` logging to the selection command.
- Recommend (a), read only when the parse falls back to the lower bound. That needs a file read from `parse`, which today sees only `RunOutput`, so it is a trait change (judgment).

**Regression test:** none cheap before the design is chosen.

**minitest missing `bundle exec` — Verdict: CONFIRMED**
**Evidence:** `loom/src/testrun/adapters/minitest.rs:115-128`: `single_test_command` and `select_command` emit bare `ruby -Itest`. In a Bundler project that loads system gems, or fails with `LoadError`.
**Fix seam:** reuse `marker_choice(package_dir, &["Gemfile"], "bundle exec ruby", "ruby")` (`loom/src/testrun/command.rs:96`), exactly as `loom/src/testrun/adapters/rspec.rs:72` does. `select_command` needs its `_package_dir` parameter.
**Regression test:** a `tempfile` dir with a `Gemfile` yields `bundle exec ruby -Itest ...`, following `command.rs:242`.

**Files a fix touches:** `loom/src/testrun/adapters/swift_test.rs`, `loom/src/testrun/adapters/dotnet_test.rs`, `loom/src/testrun/adapters/maven.rs`, `loom/src/testrun/adapters/gradle.rs`, `loom/src/testrun/adapters/minitest.rs`, and fixtures under `loom/src/testrun/fixtures/swift-test/`.
**Size / tier:**
- swift pre-suite, dotnet `-t`, dotnet escaping, maven `-q`, minitest: S each; mechanical.
- Swift Testing: M; needs-exploration (captured output).
- gradle: M-L; judgment (trait change).
**Overlap:** E1 (`build_failed` feeds freeze acceptance).
