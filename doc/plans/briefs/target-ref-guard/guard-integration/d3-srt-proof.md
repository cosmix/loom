# D3: the sandboxed proof under srt

Stage `guard-integration`, tier opus, in parallel with D1 and D2 (disjoint files). Read
`../common.md` and `doc/loom/knowledge/architecture/execution-containment.md` sections "The Test
Pattern That Makes A Boundary Test Able To Fail" and "Confinement E2E Lives Outside the Sandbox".

You own `loom/src/orchestrator/terminal/native/tests_confinement_target.rs` (new), and
visibility-only edits (no line added or removed) in
`loom/src/orchestrator/terminal/native/tests_confinement_e2e.rs` (exactly 400 lines: it may not
grow) and `loom/src/orchestrator/terminal/native/tests_confinement_srt.rs`, plus the one module
line in `loom/src/orchestrator/terminal/native/launch.rs`:
`#[cfg(test)] #[path = "tests_confinement_target.rs"] mod tests_confinement_target;` beside the
existing `tests_confinement_e2e` declaration (about lines 350-351).

Visibility, exactly: `tests_confinement_target` is a sibling of `tests_confinement_e2e` under
`launch`, and `srt` is a child of `tests_confinement_e2e`, so `pub(super)` items inside `srt`
are visible only to `tests_confinement_e2e`, and a `pub(super) use` re-export of them is a
compile error. In `tests_confinement_e2e.rs`: `mod srt;` becomes `pub(super) mod srt;`, and
`Fixture` with its fields, `fixture`, `confine`, `stage` and `STAGE_ID` become `pub(super)`. In
`tests_confinement_srt.rs`: every item your test uses becomes `pub(crate)` (`skip`, `Confined`
and its fields, `run_alive`, `write`, `refusals_missed`, `writes_missed`, `diagnostics`, and the
private `run` and `write_rc` they call, if your test calls them directly).

Run no check: the library's test build does not compile until D1 adds
`Orchestrator::check_target_guard` (the frozen daemon contracts call it); the main agent compiles
and runs your test afterwards (inside a session sandbox `srt` is unavailable and it self-skips).
The operator runs it for real after the plan, outside any sandbox:

    mkdir -p <scratch>/bin && printf '#!/bin/sh\nexec bunx --bun @anthropic-ai/sandbox-runtime "$@"\n' > <scratch>/bin/srt && chmod +x <scratch>/bin/srt
    cd loom && env -u LOOM_WORK_DIR PATH=<scratch>/bin:$PATH LOOM_TEST_REQUIRE_SANDBOX_FREE=1 cargo test --lib tests_confinement

## What the test proves

Under a stage capsule with the emulated Claude Code linked-worktree grant (the harness already
adds the common git directory to `allowWrite`), one `#[test] #[serial]`
`a_sandboxed_target_move_is_refused_or_left_unattested_and_held`:

1. Fixture (host side, before confining): the e2e file's `fixture()` repository and stage
   worktree. That fixture is NOT a working git setup: `repo_with_worktree` runs only `git init
   -q` (no commit; the branch name follows the ambient `init.defaultBranch`), and the worktree's
   admin dir `.git/worktrees/stage-1` holds only `config.worktree` and `gitdir`, without the
   `HEAD` and `commondir` git needs. Complete it in YOUR file (the e2e file cannot grow), with
   ambient config shut out (common.md): `git symbolic-ref HEAD refs/heads/main` and a seed commit
   in the repo; `git branch loom/stage-1`; write `ref: refs/heads/loom/stage-1` to the admin
   dir's `HEAD` and `../..` to its `commondir`; `git reset -q` in the worktree; then confirm
   `git -C <worktree> rev-parse --abbrev-ref HEAD` prints `loom/stage-1` (assert it, so a broken
   fixture fails loudly instead of making every probe fail for the wrong reason).
   `.loom/work/` exists under the repo (the capsule denies `.loom/`); a commit on the
   stage branch in the worktree; `loom::git::hooks::install_reference_transaction_hook(repo)`;
   `loom::git::target_guard::check(repo, work, "main")` (records the tip, writes the refs file and
   the ledger). Remember `main`'s tip.
2. Confined (`confine(&f, SessionType::Stage, &stage)`), one run per probe, each with the
   `Confined` sentinel discipline (a probe counts only when the harness started):
   - `LOOM_SESSION_ID=probe git update-ref refs/heads/main <stage commit>` in the worktree:
     exits non-zero; `main` is unchanged afterwards. In the SAME run, a write inside the
     worktree succeeds (positive control: the harness ran and the worktree is writable).
   - `git -c core.hooksPath=/dev/null update-ref refs/heads/main <stage commit>`: exits 0 and
     `main` now equals the stage commit (the gap is real under the grant; this is the matched
     negative control for the refusal above).
   - Writes to `<work>/target-guard.json`, `<work>/target-guard.ledger` and
     `<work>/target-guard.refs` are refused (`Confined::refusals_missed` empty), with the
     worktree write as the positive control.
3. Back on the host: the ledger holds no line for the move, and `target_guard::check` returns
   `Held` with an `Unattested` reason (and a `StageWork` reason naming the stage branch).

Self-skip exactly as the existing srt tests do (`skip(test_name)`); with
`LOOM_TEST_REQUIRE_SANDBOX_FREE=1` a skip must fail. The file stays under 400 lines.

## Traps

- A must-fail probe that counts any non-zero exit passes when the harness never started: check
  the sentinel (`mistakes/verification-harness.md`, "A Must-Fail Probe That Counts Any Non-Zero
  Exit Passes When the Harness Never Started").
- `srt` mounts a denied absent path from `/dev/null`, so a write to it exits 0 and lands
  nowhere: create the three state files before confining, and judge refusals by content on the
  host, not by exit status alone.
- The hook resolves the work dir through `git rev-parse --git-common-dir`; keep `.loom/work`
  directly under the fixture repository root.
