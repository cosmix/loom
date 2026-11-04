# Review Final Wave

> Final wave

## A hook lock depended on a loom subcommand the PATH binary lacks

**What happened:** The heartbeat lock called a `loom` helper subcommand. The binary on `PATH` predated it, so hooks lost the lock while tests against a fresh build passed.

**Why:** Hook scripts and the binary install separately, and the tests ran with the new binary on `PATH`.

**Prevention:** Keep hook primitives to Bash and coreutils, and test them under a restricted `PATH`.

**Fix:** `loom-hooks/_heartbeat.sh` uses `mkdir` and `rmdir` alone; `heartbeat-reclaimer-crash.sh` runs the contended case under a restricted `PATH`.

## A doc comment claimed GIT_COMMON_DIR makes git ignore commondir

**What happened:** The `pinned.rs` documentation said setting `GIT_COMMON_DIR` made git ignore the `commondir` file. A private git directory without that file wrote refs to itself and exited 0.

**Why:** The claim is false for git's ref store, which reads the file, and exit 0 hid it.

**Prevention:** Verify a claim about git internals with a separate git process that reads back what the command wrote, not with the exit status.

**Fix:** `ScratchAdmin::prepare` writes `commondir`, and the module documentation states the behaviour.

## TempDir::new_in follows the umask

**What happened:** A private git directory meant to be owner-only got the mode the umask allowed.

**Why:** `tempfile`'s default mode for that call is not 0700 on its own.

**Prevention:** Set the mode explicitly for any directory whose privacy matters, and assert it in a test.

**Fix:** `owner_only` in `git/worktree/scratch_admin_files.rs` uses `tempfile::Builder::permissions` with 0700.

## A bare `bash -c 'sleep 60'` loses the crafted argv

**What happened:** A process fixture meant to show a crafted argv to `direct_worker` showed plain `sleep 60`.

**Why:** Bash execs the last command of a `-c` body in place, which replaces the argv.

**Prevention:** End such a fixture's body with `; :` so bash stays the process.

**Fix:** The live direct-worker fixtures end their bodies with `; :`.

## Widening a shared init_repo broke a test relying on absence

**What happened:** Adding state to the shared `init_repo` fixture failed an unrelated test that needed that state absent.

**Why:** The change was checked against tests that use the state, not against tests that assert it is missing.

**Prevention:** Before widening a shared fixture, search its users for absence assertions.

**Fix:** The fixture and that test were reconciled in the same repair.

## Hand-made stage frontmatter hid behind `failure_count > 0`

**What happened:** A status test wrote stage frontmatter by hand. The file failed to parse, the parse failure counted as a failure, and `failure_count > 0` passed.

**Why:** A lower-bound assertion cannot tell the intended failure from any other.

**Prevention:** Build stage files with `serialize_stage_to_markdown`, and assert exact counts.

**Fix:** The fixture serializes a real stage and asserts exact counts.

## A redirect fixture held the commit in both repositories

**What happened:** The test for `git show` under an inherited repository redirect passed before the fix.

**Why:** Both fixture repositories held the same commit object, so the redirected read succeeded too.

**Prevention:** A redirect fixture must keep the object out of the repository the redirect names, and the test must be seen to fail before the fix.

**Fix:** `git_show_ignores_an_inherited_repository_redirect` keeps the commit in one repository.

## A relative core.hooksPath hid a GIT_DIR redirect

**What happened:** A `git hook run` test with a relative `core.hooksPath` could not detect a redirect through `GIT_DIR`.

**Why:** Git resolves a relative hooks path against the working directory, so that directory's hook ran whichever repository the variable named.

**Prevention:** Give each repository in a redirect fixture an absolute `core.hooksPath` and its own marker.

**Fix:** `hooks_ignore_an_inherited_repository_redirect` sets absolute paths.

## A swallowed identity_for failure skipped a comparison

**What happened:** The provision gate's listing around a stage worktree treated a failed pin as no finding, so the comparison silently did not run.

**Why:** `identity_for` needs the main checkout's identity. A fixture without one made the pin fail, and an `Err` taken as empty looks like a clean result.

**Prevention:** Propagate a pin error, and have the test assert that the listing ran as well as what it found.

**Fix:** `status_entries` in `orchestrator/core/provision_gate.rs` runs pinned and reports the error; `the_listing_around_a_provision_does_not_follow_a_forged_git_file` covers it.

## The first bwrap harness passed when bwrap refused to start

**What happened:** A confinement test asserted that a write was refused, and passed on a host where bubblewrap never started.

**Why:** A sandbox that does not start also leaves the file unwritten, so the negative result proved nothing.

**Prevention:** A confinement test must assert that its body ran, beside what it was refused.

**Fix:** The harness skips where bubblewrap cannot start, and the tests assert what the body reached.

## Mutation checks in a shared tree broke other workers

**What happened:** Mutation checks left five files broken during other workers' test runs. Restoring with `copy2` kept the old mtimes, so Cargo reused the mutated binary.

**Why:** The tree was shared, and Cargo's freshness check reads mtimes that the restore preserved.

**Prevention:** Run a mutation in a private copy of the tree with a private target directory, and never restore a source file with its old mtime.

**Fix:** The files were restored and rebuilt.

## Re-exec tests fail with NotFound when a shared test binary is rebuilt

**What happened:** Three tests failed with `current_exe` `NotFound` and passed on a rerun.

**Why:** Tests that re-execute their own binary lose it when another worker's Cargo run replaces it in a shared target directory.

**Prevention:** Read a `NotFound` from `current_exe` as a shared-target collision. Give concurrent workers their own `CARGO_TARGET_DIR`.

**Fix:** Rerun with no concurrent build.

## Workers piped Cargo against the brief

**What happened:** Most workers in the wave piped or filtered Cargo despite the brief's rule, some repeatedly. One redirect lost the name of a failing test, leaving a possible flake unidentified. One `until cargo check` loop spun on the build lock.

**Why:** A rule in a brief does not change the habit, and nothing enforced it.

**Prevention:** Log Cargo's output to a file, read the exit status, then search the file for failures. Enforce this in a hook if it recurs, since repeating the rule has not worked.

**Fix:** None applied; the suspect run is reported for the full suite to settle.

## rev_parse appends ^{commit}

**What happened:** `git/merge/tree.rs::rev_parse` failed when asked for a non-commit revision such as a tree.

**Why:** It appends `^{commit}` to every revision, which its name does not say.

**Prevention:** Read a helper's argument handling before reusing it for another object type.

**Fix:** `checkout_stash.rs` calls `run_git_checked` with `rev-parse --verify` for the tree.

## `pkill -f <path>` killed the worker's own shell

**What happened:** Two workers ran `pkill -f` on a path to stop a stuck process and ended their own shell.

**Why:** The shell's command line holds the pattern, so `-f` matches it.

**Prevention:** Stop a process by verified PID, never by a pattern the current command line contains.

**Fix:** None needed beyond rerunning the step.

## Each review wave found defects in the previous wave's code

**What happened:** The review of the repair wave's new code reported critical and major defects (a lock inversion, a gitlink hole behind a global option, reflog links), and the fixes for those drew further findings. Open-ended rounds kept widening.

**Why:** Each wave wrote new code in the most sensitive paths, and every review of it was a fresh full pass with no stopping rule.

**Prevention:** Cap review rounds up front. Later rounds review only the new diff and report critical and major findings. Past the cap, findings go to the tracker as residuals for the owner to decide.

**Fix:** Open-ended review rounds are capped.

## Reflog deny added without measuring half-applied commands

**What happened:** The capsule denied `<common>/logs` on the claim that blocked Git commands fail cleanly. A session's `git reset <commit>` and a bare `git merge` moved the index and tree first, then failed on the reflog, so a later commit could reverse the stage's work. A review caught it before commit.

**Why:** The tests checked only commands that fail before touching anything, and checked exit codes rather than resulting state.

**Prevention:** Pin any deny on Git state with `status` after each refused command, across commands that move the index before the ref. Do not set Git config through `GIT_CONFIG_COUNT` where a tool scrubs environment names, because `GIT_CONFIG_KEY_n` matches `*KEY*` and git then refuses every command.

**Fix:** Stage branches keep no per-ref reflog: the capsule removes the branch log at spawn, pinned commands pass `core.logAllRefUpdates=false`, and the wrapper exports it through `GIT_CONFIG_PARAMETERS`.

## Tally composition was stated from memory

**What happened:** Asked what "75 awaiting gates" meant, I said 28 came from the original 43 active findings and 47 had been reopened. The tracker's lists give 38 and 37.

**Why:** I answered from recollection of moved counts without recounting the IDs.

**Prevention:** Derive any breakdown of a tally by counting individual IDs from the tracker before stating it.

**Fix:** The handoff section lists both groups by ID, and the user was told of the correction.

## Worker Done Claims Hid Two Unmet Acceptance Clauses

**What happened:** RBR-17 and INT-04 stood as implemented with green suites while a worker's report named a residual that was an acceptance clause: the writer fold stayed unauthenticated, and deprecated validators stayed supported only by self-tests. A final check of every pending finding against its acceptance text found both.
**Why:** suites prove the tests that exist, and a worker's residual line reads as a limit even when it contradicts the finding. Deprecation was accepted with no check against the no-compatibility policy.
**Prevention:** before moving a finding to done, compare each reported residual with the finding's acceptance text, and resolve any "compatibility" clause against the project policy.
**Fix:** `fold_completion` folds authenticated components only; `hooks/validators` and `HooksConfig` deleted.
