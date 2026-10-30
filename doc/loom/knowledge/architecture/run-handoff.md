# Run Handoff

> Run lane

## Scope

The planning server drives a written plan to execution without a terminal: `loom/src/plan_server/run/` (routes, jobs, state, strip, dashboard) beside the pressure routes ([pressure](pressure.md)), and `web/src/plan/run/` for the dialogs. Spec: `doc/product/feat-planner/section_11.md`.

| Route | Role | Job |
| --- | --- | --- |
| `GET /api/run/state` | Viewer | workspace state (`WorkspaceState`: `Stopped`, `Initialized`, `Done`, ...), the `RunOffer` (`Start`, `StartInitialized`, `NoneRunning`, `Resume`, `StartNext`) and the uncommitted-change list |
| `POST /api/run/commit` | Holder | commit the ticked documentation changes |
| `POST /api/run/start` | Holder | `loom init <plan> [--clean]`, then `loom run`, with streamed output (`Steps::RunOnly` skips the init) |
| `POST /api/run/dashboard` | Viewer | start or reuse the one `loom status --web` child, answer its URL |
| `POST /api/run/abandon`, `/finish-abandon` | Holder | the abandon sequence |

## Jobs And Output

`child.rs` owns one `RunJobs` slot per planning server (`install_jobs` must run before `jobs(ctx)`, which panics otherwise; `commit::install` and `start::install` call it and store a `Weak<PlanContext>` for their worker threads). `run_streamed` spawns a child in its own process group with null stdin, reads stdout and stderr on separate threads through a bounded channel, caps a line at 4096 bytes, enforces `CHILD_TIMEOUT` (30 min) on the caller and kills the group with SIGKILL on expiry. Each line is broadcast as a `RunOutput` frame; every job ends with exactly one `done` frame carrying the exit code (abandon routes emit it from the outcome, `emit_done_for`). The server never broadcasts `PlanMessage::RunState`, so the browser refetches `/api/run/state` after a job ends or an accepted POST.

## Start Admission

`admit(state, request)` decides before anything spawns. A `Stopped` run of the same plan with a clean tree resumes with `loom run` alone; any other `Stopped` start is 409 `stopped`. When tracked changes and the run-state entry both block, the 409 names the run-state remedy first. A workspace without a plan source reports `Initialized` with the Start offer and a clean notice (never a 500); a detached HEAD reports the branch as `HEAD` (`current_branch` returns `Ok("HEAD")` on a `symbolic-ref` exit 1).

## The Commit Lane

`commit_doc_changes` commits only ticked paths. `valid_paths` requires a non-empty list (an empty pathspec would commit unrelated staged work), each under `doc/`, not absolute, free of NUL and of the substring `..`, with no empty/`.`/`..` component, and present in the current change list. `git add` and `git commit` run through a plain `Command::new("git")`, NEVER `crate::git::run_git` or the `NO_HOOKS_ARGS` runner: the user's pre-commit hooks and signing must apply. Contract `commit_runs_the_users_pre_commit_hook` pins it (a mutant passing `NO_HOOKS_ARGS` fails it). A staged deletion or a rename's source is not addable (`git add` exits 128): it stays on the commit's pathspec only. `changes.rs` lists a rename's non-doc source as blocking and keeps a copy's out.

## The Abandon Sequence

`abandon/sequence.rs`: refuse without an `IN_PROGRESS-` plan; re-check the `loom/*` branches against the list the dialog confirmed (`BranchesChanged` returns the fresh list as a 409; a git listing error is `Refused`, since any branch may hold unmerged work); refuse a rename that could never happen; write the marker `<runtime>/abandon/<slug>.json`; `loom stop`; re-check the branches again (a stage may have committed while the daemon stopped); `loom clean --all` with `LOOM_CLEAN_YES=1` set only on that step; confirm ON DISK that the run's worktrees and state are gone (`leftover`; `loom clean` exiting 0 is not proof, see `CLEAN_LEFT_STATE`); only then rename `doc/plans/IN_PROGRESS-PLAN-<slug>.md` into `doc/plans/archive/RETIRED-PLAN-<slug>.md` through the no-follow primitives. The marker is removed when nothing changed the run and kept after `loom clean` ran, so a server that died between clean and rename offers "Finish abandoning". Both routes run on the request's connection thread and answer with the outcome.

## Dashboard Child And Strip

`dashboard.rs` keeps one `loom status --web` child per server (own process group, killed with SIGKILL on shutdown); `alive()` checks `try_wait` and `url()` restarts a dead child. `dashboard_parse.rs` reads the startup lines for the URL and `strip_token` removes the bearer token from anything broadcast. `strip.rs` builds a `RunStrip` (plan, daemon running, merged/total stages, attention count, dashboard URL without credentials) every 5 s on a polling thread started after the job, dashboard, commit and start handles are installed (`run/install.rs`).

## Web Side

Socket schemas: `planMessageSchema` (`web/src/plan/api/schema.ts`) must list `pressure_run`, `run_state`, `run_output` and `run_strip`, or `socketMessageSchema.safeParse` drops the frames. Holder-only controls (Abandon, Finish abandoning, Stop) are hidden unless the viewer holds authoring and the server is writable; confirmations keep the exact displayed branch snapshot and re-confirm fresh conflict lists. Server-supplied paths, branches and models render inside `Untrusted` spans wrapping the whole phrase. `ConfigBlockNotice` reads `pressureOptionsAtom.config_block`. Web contract tests use one top-level `it()` carrying the full name: vitest 5 joins `describe` and `it` with ` > `, which loom's space-joined `-t` filter never matches.

## Contract Tests

The frozen contracts live in `loom/tests/plan_handoff_contracts.rs` with `plan_handoff_contracts/support.rs` (a fixture git repository whose hooks are isolated through a local `core.hooksPath` with signing off, a fake `loom` shell executable, the runtime dir and abandon marker, `EnvGuard`) and `drive.rs` (surface calls and outcome unpacking). Every process-spawning contract is `#[serial]`: `finish_abandon_renames_a_stranded_plan` sets `LOOM_PLAN_STATE`. Contracts import `loom::process::{ProcessIdentity, process_start_time}` because `process::identity` is private, and pin the startup lines of `loom status --web` as literals (`REMOTE_STARTUP`) because `commands::status::web::startup` is private. `AbandonOutcome` and `CodexAction` need `Debug`; `WorkspaceState`, `RunOffer` and `ChangedPath` need `Debug + PartialEq`. The operator tests (`tests/plan_handoff_operator.rs`) are `#[ignore]`; nothing live ran in the plan that added them (no real Codex Stop/kill, no tmux probe, no browser render at 390 px and 1280 px, no Radix focus timing across the two abandon steps), so those behaviours rest on fake-driven tests and static review.

A test binary that spawns `CARGO_BIN_EXE_loom` needs a `SANCTIONED_SPAWNERS` entry for `binary_spawn_guard` and a scrub of `LOOM_*` from the child environment (`support.rs::run_loom` does).

## Smaller Rules

- `AbandonView` (`wire/run.rs`) has no refusal field: a `loom/*` listing error is logged by `view()` and surfaces only when `POST /api/run/abandon` refuses with 409 `refused` (`UnmergedBranches.listing_error`). `loom clean` on the CLI treats a listing error as no unmerged branches.
- `PressureRunView.step` is `codex` only while Codex runs alone (`view.rs` `codex_alone`); the steps list holds a running Codex beside `/pressure`.
- Adding a `PlanMessage` variant needs new arms in `tests_schema.rs`'s `assert_exhaustive` match, in the web `planMessageSchema`, and a producer: the server never builds `RunState` today.
- The web refreshes the latest pressure run through the server's `on_open` `pressure_run` replay, keeps the last view while waiting and refetches options over HTTP on resync.
