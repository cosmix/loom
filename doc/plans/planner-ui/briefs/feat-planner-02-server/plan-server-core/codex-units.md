# plan-server-core: codex units

Lane: `loom-codex-forwarder`, `--model gpt-6.1-terra --effort high`, foreground only, at most six
at once over disjoint files, each spawn with an explicit Bash timeout of 600000 ms. Every unit is
one file, or one file plus its test (fixture JSON counts as that file's test data), at most three
steps, and must finish inside the wrapper's 540 s deadline. A unit that times out is re-split
against the partial tree, never re-forwarded as is; work that cannot be cut this small goes to a
Claude worker.

## Rules every unit follows (codex reads them from this file; the prompt is only the fixed line)

- Read `doc/plans/briefs/feat-planner-02-server/common.md` and the spec section the unit names, then write
  only the files the unit owns. Do not run git. Do not open or write any path under the hidden
  loom state or work directories. Run at most `cargo build --manifest-path loom/Cargo.toml` (or
  `cargo check`); the orchestrator runs the tests.
- `loom map` answers from the base layer: it cannot see files other units of this stage wrote.
  Every shape a unit uses from another unit is pinned below or in the named spec section.
- Size: leave 10% under the limits (360 lines per file, 45 per function); `cargo fmt` grows code.
- Rust traps: double literal braces inside `format!`; put CLI placeholders in backticks in rustdoc.

## Orchestrator duties

**Module seeding (every stage):** whenever the orchestrator adds a `mod` line, it creates the
module's file at the same moment holding only a `//!` purpose line, so the crate always compiles
and every unit's proof selects its tests as soon as the unit writes them. The unit or worker that
owns the file replaces the seed; the seed is not ownership.


Before wave 0: create `loom/src/plan_server/mod.rs` with `pub const DEFAULT_PORT: u16 = 7272;` and
`pub mod ids; pub mod wire;`, `loom/src/plan_server/wire/mod.rs` with `pub mod server; pub mod
presence; pub mod checklist; pub mod paths; pub mod message;`, and `pub mod plan_server;` in
`loom/src/lib.rs` (small change). Before wave 1: add the wave-1 module lines (authoring, presence,
ws, shutdown, root, lock, prereq, browser, startup, lifecycle and their cfg(test) test modules,
including `ids_tests`, `prereq_tests`, `lifecycle_tests`, and `wire/paths_tests` in `wire/mod.rs` before wave 0), and
widen `sandbox_prerequisite_problem` in `loom/src/commands/run/sandbox_preflight.rs:56` from
`pub(super)` to `pub(crate)` (one word). After EVERY unit: `git status --short` (only the unit's
files may appear) and the unit's proof command below. Then hand `loom/src/plan_server/mod.rs` to
the integrator.

## Units

| Unit | Wave | Owns | Reads | Entry points | Spec | Proof (orchestrator) |
| ---- | ---- | ---- | ----- | ------------ | ---- | -------------------- |
| C1 | 0 | `loom/src/plan_server/ids.rs`, `loom/src/plan_server/ids_tests.rs` | `loom/src/web_server/token.rs` (`mint`) | `BrowserId`, `PublicId`, `ConnId`, `ConnCtx` | foundation.md "ids.rs"; must pass the frozen contract `browser_ids_are_random_128_bit_and_never_printed` (`loom/tests/plan_server_contracts.rs`) | `cargo test --manifest-path loom/Cargo.toml --lib plan_server::ids` and `--test plan_server_contracts browser_ids_are_random_128_bit_and_never_printed` |
| C2 | 0 | `loom/src/plan_server/wire/server.rs` (inline parity tests), fixtures `server-info.json`, `share.json`, `refused.json` | `loom/src/commands/status/web/model_tests.rs` (`fixture_matches_serde_output`) | `ServerInfo`, `MissingPrereq`, `PrereqKind`, `You`, `ShareLink`, `BindKind`, `Refused` | foundation.md "Wire model" and "Fixtures and parity tests" | `--lib plan_server::wire::server` |
| C3 | 0 | `loom/src/plan_server/wire/presence.rs` (inline parity tests), fixtures `presence.json`, `authoring.json` | as C2 | `PresenceList`, `PresenceEntry`, `AuthoringView`, `HolderView` | foundation.md "Wire model" | `--lib plan_server::wire::presence` |
| C4 | 0 | `loom/src/plan_server/wire/checklist.rs` (inline tests), fixture `checklist.json` | plan 1 `loom/src/planning/checklist/` (`Checklist`, `Step`, `StepMark`, `Offer`, `ChecklistItem`) | `ChecklistView`, `StepView`, `OfferView`, `OfferKind`, `ItemView`, `CurrentView`, `From<&Checklist>` | foundation.md "Wire model" (checklist) | `--lib plan_server::wire::checklist` |
| C5 | 0 | `loom/src/plan_server/wire/paths.rs`, `loom/src/plan_server/wire/paths_tests.rs` | none | `to_doc_rel`, `to_repo_rel` | foundation.md "Path rule" | `--lib plan_server::wire::paths` |
| C6 | 0 | `loom/src/plan_server/wire/message.rs` (inline parity tests), fixtures `hello.json`, `client-set-name.json`, `client-reading.json` | the pinned types of C2, C3, C4 (foundation.md) | `PlanMessage`, `ClientMessage`, `Hello`, `to_frame` | foundation.md "Wire model" (message.rs) | `--lib plan_server::wire::message` |
| C7 | 1 | `loom/src/plan_server/lock.rs`, `loom/src/plan_server/lock_tests.rs` | `loom/src/fs/daemon_state.rs` (`acquire_lock`, `inspect_lock`) | `acquire`, `Acquired`, `ServerLock::publish`, `LockRecord` | runtime-startup.md "lock.rs"; must pass the frozen contracts `symlinked_lock_path_is_refused` and `second_lock_reports_url_and_token` | `--lib plan_server::lock` and `--test plan_server_contracts symlinked_lock_path_is_refused`, `second_lock_reports_url_and_token` |
| C8 | 1 | `loom/src/plan_server/browser.rs`, `loom/src/plan_server/browser_tests.rs` | none | `EnvView`, `decide`, `OpenDecision`, `SkipReason`, `open` | runtime-startup.md "browser.rs" | `--lib plan_server::browser` |
| C9 | 1 | `loom/src/plan_server/startup.rs`, `loom/src/plan_server/startup_tests.rs` | C7's `LockRecord` (pinned in runtime-startup.md), `loom/src/context/untrusted.rs:123` | `Startup`, `startup_lines`, `already_running_lines`, `tunnel_hint` | runtime-startup.md "startup.rs" | `--lib plan_server::startup` |
| C10 | 1 | `loom/src/plan_server/root.rs`, `loom/src/plan_server/root_tests.rs` | `loom/src/git/runner.rs` (`run_git`) | `resolve_project` | runtime-startup.md "root.rs" | `--lib plan_server::root` |
| C11 | 1 | `loom/src/plan_server/prereq.rs`, `loom/src/plan_server/prereq_tests.rs` | `loom/src/claude/auth.rs` (`stage_auth_status`, `AuthProbe`), `loom/src/commands/run/sandbox_preflight.rs` (`sandbox_prerequisite_problem`, widened by the orchestrator) | `ReadOnly::{detect, from_problems, is_read_only, problems}` | runtime-startup.md "prereq.rs" | `--lib plan_server::prereq` |
| C12 | 1 | `loom/src/plan_server/lifecycle.rs`, `loom/src/plan_server/lifecycle_tests.rs` | `loom/src/context/untrusted.rs` | `line`, `safe` | runtime-startup.md "lifecycle.rs" | `--lib plan_server::lifecycle` |
| C13 | 1 | `loom/src/completions/dynamic/mod.rs`, `loom/src/completions/dynamic/tests/tests_commands.rs` | `completions/dynamic/commands.rs` | `("plan", ...)` arms | runtime-startup.md "Docs and completions" | `--lib completions::` |
| C14 | 1 | `README.md` (`### Plan Commands` only), `skills/loom-usage/SKILL.md` (plan commands passage only) | doc/product/section_02.md | the bare `loom plan` passage in each | runtime-startup.md "Docs and completions" | `rg -qF -- "--no-open" README.md` and the same for the skill |
| C17 | 3 | `loom/src/plan_server/server_info/mod.rs`, `loom/src/plan_server/server_info/tests.rs` (route-level tests through the integrator's `tests/support.rs`: `/api/server` fields, `/api/share` loopback tunnel and remote bind; C17 declares `#[cfg(test)] mod tests;` itself) | `plan_server/routes.rs`, `request.rs`, `context.rs`, `tests/support.rs`, which exist when wave 3 starts (the Claude integrator wrote them in wave 2; `loom map` cannot see them, so read the files): `RouteSpec`, `ApiRequest`, `ApiResponse`, `PlanContext::{bind, read_only, presence, is_stopping, root}` | `routes()` for `GET /api/server` and `GET /api/share` | integrator.md "server_info/mod.rs" | `--lib plan_server::server_info` |
| C18 | 3 | `loom/tests/plan_server_cli.rs` | `loom/tests/plan_server_contracts.rs` (the frozen binary contract, as the pattern); when wave 3 starts the integrator has wired bare `loom plan` and `loom::plan_server::start` is public | the binary scenarios | integrator.md "`loom/tests/plan_server_cli.rs`" | `--test plan_server_cli` |

Units in one wave over disjoint files run at most six at a time: wave 0 is C1-C6; wave 1 runs
C7-C12 and then C13-C14 while the Claude `authoring` and `ws-hub` workers run in the background;
wave 3 (C17, C18) starts after the Claude `integrator` returns.
