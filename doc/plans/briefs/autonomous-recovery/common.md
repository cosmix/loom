# autonomous-recovery: rules every worker follows

Read this file in full, then your own brief. The plan is `doc/plans/PLAN-autonomous-recovery.md`
(`IN_PROGRESS-PLAN-autonomous-recovery.md` once it runs). Its "Decisions" section is
authoritative where a brief and the plan differ; the stage description's PUBLIC SURFACE is
authoritative for every signature the frozen contracts call.

## Where you are

- Every stage runs with `working_dir: loom`. Paths in briefs are relative to `loom/` unless they
  start with `../` (the repository root: `../skills/`, `../loom-hooks/`).
- The plan runs after the feat-planner plans 08-11 and after the live-test-lane plan
  (`doc/plans/PLAN-live-test-lane.md`) have merged. That plan removes
  `CommandConfinement::Inherit` (replaced by a `#[cfg(test)]` `TestUnconfined` variant), deletes
  `allow_unsandboxed_escape`, rebuilds the confined environment through
  `crate::process::apply_confined_host_environment`, and bumps the criteria containment revision
  to `host-os-boundary-v10`. Never reintroduce any of those. It also leaves, and this plan keeps:
  the `confine_host.rs` split (`confine_bwrap.rs`, `confine_seatbelt.rs`, `wrap(.., live)`);
  `Live` in `CriterionField`, `AmendmentField`, `DisputeField` and `AmendField`; the `live`
  check in `run_checks`; `crate::live::completion::block_refusal`; `JobKind::Live`;
  `HostJobRequest.live`. The plan's "Run order and sibling plans" table lists every file both
  plans edit and the rule for each.
- Line numbers in briefs are advisory. Locate every seam by symbol first:
  `loom map --outline <file>`, `loom map --find-all <name>`. A named file that no longer exists,
  or a symbol that moved: record `loom memory note "found: <what moved> (<file:line>)"` and use
  the merged name.

## Ownership

- You write only the files your row of the stage's worker table owns. A compile error in a file
  no row owns goes to the row whose change caused it; record the file with `loom memory note`.
- Two workers never write one file. If you need a change in another worker's file, stop and
  report the exact change instead of making it.
- Never run git, never write under `.loom/`, never spawn subagents.

## Code rules

- Files at most 400 lines, functions at most 50 lines. Split by concern into a sibling module
  when a file would cross 400. `cargo test --test maintainability` enforces both and is in every
  stage's acceptance; `maintainability-baseline.txt` is a ratchet file and is never edited.
- Every existing-stage mutation goes through `crate::verify::transitions::update_stage(id,
  work_dir, |stage| { ...; Ok(()) })`. Never `load_stage` + mutate + `save_stage`
  (`save_stage` creates and refuses to overwrite). Slow work (git, processes, verification) runs
  outside the closure.
- A value that arrives from an agent, a model's verdict or the wire is untrusted: validate ids
  with `crate::validation::validate_id` before any path join, strip control characters with
  `crate::models::dispute::strip_control`, cut text with
  `crate::models::dispute::truncate_to_byte_limit` (never a byte slice).
- A refusal the daemon returns to an agent is `Response::Error { message }`, never an `Err`:
  the spool drain retries an `Err` forever.
- Errors: `anyhow` with `.context(...)` at the boundary, typed errors (`thiserror` or a struct
  implementing `std::error::Error`) where a caller classifies by type. Never classify by
  matching message text when a typed error can carry the fact.
- No new dependency. No new configuration key. No placeholder, stub or `todo!()`.
- Doc comments state current behaviour. No dated history, no "was", no "used to".

## Tests

- In-crate tests live in a `*_tests.rs` file beside the module (`#[cfg(test)] #[path =
  "x_tests.rs"] mod tests;` as the module already does), or in `tests/` for public API.
- Tests that set process environment variables are `#[serial_test::serial]` and restore them.
- Git fixtures: `git init -b main` in a `TempDir`, local `user.email`/`user.name`, and the
  environment isolation `tests/completion_gates_contracts.rs::git` uses
  (`GIT_CONFIG_GLOBAL`/`GIT_CONFIG_SYSTEM` pointed at absent files, `GIT_CONFIG_NOSYSTEM=1`).
- No test needs tmux, a real Claude Code, the network or the real `HOME`. A test that would
  needs a different design.
- A new test that needs a Unix socket, loopback TCP, the process tree, bubblewrap or tmux
  guards itself with `crate::process::sandbox_probe::require_facility(Facility::<name>, <probe>,
  <test name>, <why>)`: it skips in your session sandbox, which denies Unix sockets, and fails
  where loom's confined runner advertises the facility in `LOOM_TEST_FACILITIES`. Compile-check
  such a test; `loom check` runs it.
- Frozen contract files are never edited. `loom stage contracts show <stage-id>` lists them.

## Checks

You run at most ONE narrowly scoped check over the files you wrote (for example
`cargo test --lib <your module>::`), once. The orchestrator runs `loom check <stage-id>`.

## Memory

Record mistakes, decisions and surprises immediately with `loom memory note` /
`loom memory decision`, including every symbol or path that differed from the brief. Never
`loom knowledge`; never Claude Code auto-memory.
