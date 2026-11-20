# PLAN: Contracts Run Where Their Package Lives, and One Dispute Can Fix Them All

Status: design, owner decisions below. Not yet a loom plan; convert with `/loom-plan-writer` when
approved.

Includes an unrelated small fix in the same delivery: the Claude quota meters missing from the web
footer (section F).

## Incident (loom 1.2.0, plan product-blog-kit)

The repository had no root Python project. Each package (`post`, `tests`, `fixtures/web`,
`fixtures/tui`) had its own `pyproject.toml` and `uv.lock`. Every contract set `runner: pytest` and
omitted `cwd`.

1. **Resolution.** pytest has no `run_root_markers` (`testrun/mod.rs:94` default `&[]`), so each run
   dir fell back to the stage working dir `.` (`verify/contracts/run_dir.rs:63-68`). `.` holds no
   `uv.lock`, so the command became `python3 -m pytest <file>::<test> -q` (`testrun/adapters/pytest.rs:68-75`)
   on a system python without pytest.
2. **Freeze.** Every run exited 1 with `No module named pytest`. The pytest parser read nothing, the
   outcome was `Unparsed`, and `judge_run` (`commands/stage/contracts/freeze.rs:75-113`) accepted the
   non-zero exit as `exit_nonzero_unverified`. Freeze prints each command but not the run's output, so
   the writer never saw the error.
3. **Plan verify** said nothing about a marker-less runner resolving to the root of a repository with
   four packages.
4. **Repair.** The implementers ran `cd <pkg> && uv run pytest`, saw green, and committed. Completion
   failed all 7 contracts. A contract dispute names one contract (`cli/types_stage_disputes.rs:56-70`),
   a filing ends the session, and the budget is 3 (`models/stage/dispute_budgets.rs:13`). Both stages
   escalated to `NeedsHumanReview` with correct work.
5. **Display.** The operator's `loom stage amend --field contracts` fixed the plan and stage file.
   `loom stage contracts show` prints only `freeze.json` (`commands/stage/contracts/show.rs:9-46`), which
   amend never touches, so it kept printing `cwd=.`. Completion ran correctly: the plan `cwd` wins
   over the recorded one.

## Owner Decisions (Binding)

1. Goal: contracts must not be written in a way that blocks stages. At minimum, a stage can dispute
   wrong contracts and have them adjudicated and fixed.
2. No new loom commands or subcommands, and no new flags where an existing one can carry the need.
   No `loom stage contracts run`: the freeze already runs every contract through `resolve_run`, and
   the signals print its command. No freeze override flag.
3. No backwards compatibility or migration (project rule): the dispute request, verdict and
   amendment formats change in place.

## A. Recovery: One Dispute Covers Every Contract With the Same Cause

1. **Filing.** `DisputeContractArgs.contract` becomes `Vec<String>` with `num_args = 1..`, as findings
   and integrity disputes already are (`cli/types_stage_disputes.rs:56-70`). `DisputeTarget::Contract`
   and `DisputeKind::Contract { contract_ids: Vec<String> }` (renamed from `contract_id`) carry the
   list. `check_frozen` (`daemon/server/dispute_kinds.rs:137-147`) requires every id frozen, and refuses
   an empty list or a duplicate.
2. **Budget.** One filing spends one unit of `tally.contract_disputes`, whatever the number of ids.
3. **Verdict.** For a contract dispute, `plan_patch` (`orchestrator/adjudication/verdict_kinds.rs:47,77-97`,
   `plan_patch.rs::decode_patch`) is one patch object or an array of them, each `field: "contracts"`.
   With more than one patch, each must be a `replace` at a distinct index whose current contract is one
   of the disputed ids, so no patch shifts another's index. `insert` and `delete` stay single-patch.
4. **Amendment.** `AmendmentRequest.patch` becomes `patches: Vec<AmendmentPatch>`
   (`plan/amendment.rs:118`). `apply_amendment` (`plan/amendment.rs:434-660`) applies them in order to
   one clone, validates once, and writes one snapshot, one audit row and one plan version. That counts
   as one `amendments_applied`, under one idempotency key (the dispute id). `loom stage amend` builds a
   one-element list. `plan/amendment.rs` is in `loom/maintainability-baseline.txt` at 1031 lines (the
   ledger is exact in both directions), so a growing file is split, or the entry is updated in the same
   change.
5. **Apply.** `apply_contract::accept` (`orchestrator/adjudication/apply_contract.rs:24-67`) collects the
   old files of every disputed contract. `refrozen_contracts` (`:117-139`) takes every disputed id plus
   its existing mismatch rule, and the whole set goes through one `store::refreeze`. Feedback and the
   reject notice name every contract.
6. **Scope.** The adjudicator may patch only the disputed contracts. The implementer signal (section B)
   tells the implementer to name every affected contract in the one filing.

## B. Every Actor Sees the Command Completion Runs

Every command below comes from `run_dir::resolve_run`, with the same inputs completion uses
(`verify/contracts/completion.rs:55-57`: stage spec, `resolve_adapter`, `store::recorded_cwd`). Then
equality holds by construction.

1. **Implementer signal** (`orchestrator/signals/v2_section.rs:31-72`, `append_frozen_contracts`). The
   table gains `Run dir` and `Command` columns, and the section gains two rules:
   - Before the gate and before committing, run each contract's command from its run dir exactly as
     listed, not your own test command, and report its output.
   - A contract that fails under this command while the implementation is right is a contract-metadata
     defect. Examples: the wrong run dir or runner, a runner or dependency missing in that dir, the test
     not selected. File ONE `loom stage dispute-contract <stage> --contract <a> --contract <b> ...`
     naming every affected contract and the shared cause, never one dispute per contract.
   - The section also lists every contract the freeze recorded as `exit_nonzero_unverified`: its red was
     never proven, so run it first.
2. **`loom stage contracts show`** loads the stage and resolves each contract as completion does. It
   prints `runs in <cwd> (<rule>): <command>` next to the freeze record's outcome, exit code and
   `frozen cwd=`. After an amend, it prints the new run. With no worktree (merged or removed), it prints
   the record alone and says so.
3. **Contract signal** (`orchestrator/signals/contract.rs`, `append_contracts`/`append_rules`):
   - A red counts only when the test ran and failed for its `rejects` reason.
   - The freeze refuses environment reds (section C2) and prints each run's exit code and output tail.
   - A collection error is valid only when it names a module or symbol this stage has not written yet.
     A missing third-party dependency is an environment red.
   - The writer cannot change contract metadata (`cwd`, `runner`). When a refusal shows a wrong run dir
     or runner, it states that in its final message and stops. The refusal parks the stage for the
     operator (section D).
4. **Adjudication prompt** (`orchestrator/adjudication/prompt/contract.rs`):
   - It covers every disputed contract: its spec block and its current run block (`first_step`,
     `:57-93`).
   - For each contract it adds a candidate table: every detected package
     (`ProjectProfile::discover(worktree).package_details()`) whose dir contains the file, plus the
     worktree root, each with the command loom would run there
     (`adapter.single_test_command(relative_file, test, dir)`). The judge can then run a proposed `cwd`
     exactly as completion will.
   - Rules: reproduce with the listed commands before ruling, and cite each exit code and output. When
     one metadata fix applies to several disputed contracts, patch all of them in this verdict.
   - The prompt byte cap (`prompt/tests_kinds.rs:160-175`) still holds: candidate tables are dropped
     before spec or run blocks.

## C. Prevention

### C1. Run-dir resolution

- pytest `run_root_markers`: `pyproject.toml`, `pytest.ini`, `tox.ini`, `setup.cfg`. unittest:
  `pyproject.toml`, `setup.py`, `setup.cfg`. pytest finds its config from its arguments, but the Python
  environment (`uv run`, a project venv) comes from the cwd. That is why these runners need a run root.
- `uv run` is chosen when `uv.lock` is in the run dir or an ancestor, through the first ancestor
  holding `.git`, as `testrun/command.rs::package_runner` (`:100-117`) does for bun. A uv workspace
  member then runs `uv run` in the member dir. A shared `lockfile_above(dir, names)` replaces both
  searches.
- `ContractRun` gains `rule: RunDirRule` (`PlanCwd`, `Recorded`, `Marker(<name>)`, `WorkingDir`).
  `FrozenContract` stores it as `cwd_rule`, and freeze output, `show` and the signals print it.
- Test: an explicit `runner: pytest` with no `cwd` resolves to the nearest marker dir.

### C2. Freeze runs contracts the way completion does

- Freeze runs each contract through the criteria probe (`verify/criteria/probe.rs`, as
  `completion::check` builds it), replacing `run_spec_with_timeout`. That gives it the same
  `stage.setup` prefix, confinement precedence and timeout as completion. A red is never stored in the
  certified cache (the cache stores passes only; verify this in the implementation).
- Freeze prints each run's exit code and the last 20 lines of its output.
- `TestRunnerAdapter` gains `environment_failure(&RunOutput) -> Option<String>`. Its default catches
  exit 126/127 and `command not found`. pytest and unittest add `No module named pytest`,
  `ModuleNotFoundError: No module named 'pytest'`, and `uv` spawn errors (`error: Failed to spawn`).
  `judge_run` refuses any run it matches, whatever the parsed outcome, naming the cwd, the command and
  the matched line.
- pytest exit 4 `ERROR: file or directory not found:` is classified as `NotSelected`, alongside the
  `ERROR: not found:` node-id form. Captured fixtures for it and for `No module named pytest` go under
  `testrun/fixtures/pytest/`.
- An adapter-backed `Unparsed` red with no environment match is still accepted as
  `exit_nonzero_unverified`, now with its output printed. Section B1 lists it for the implementer.

### C3. Plan verify

In `plan/schema/validation/v2_lints/contracts.rs`, run from `commands/plan/verify.rs:386-405` with
`repo_root`:

- **Error** when a contract has no `cwd`, the repository holds more than one detected package, and
  `resolve_run` against `repo_root/working_dir` (no recorded cwd) resolves a dir other than the
  innermost package owning the file. The message names the package and the line to add:
  `cwd: <package>`.
- **Warning** when no detected package owns the contract's file.
- **Warning** (skip) when the package dir does not exist yet, because the plan creates it.
- `verify/contracts/mod.rs:92` `owning_package` moves to where both callers can reach it.
- `commands/plan/verify.rs` is ledgered at 555 lines; put the new lint in the lints module.
- `skills/loom-plan-writer/references/v2-contracts.md`: in a repository with more than one package,
  set `cwd` on every contract to the dir holding the test's manifest and lockfile (list them with
  `loom project detect`). `loom plan verify` rejects a contract that resolves elsewhere.

### C4. Amend and the freeze record

`show` resolves through the stage file (B2), so it matches completion after an amend without
rewriting `freeze.json`. The record keeps what the freeze ran. `show` labels it `frozen`.

## D. Operator Guidance

- The refused-freeze reason (`verify/contracts/refusal.rs`, `review_reason = "contract freeze
  refused; ..."`) and the contract-dispute budget escalation (`daemon/server/dispute_kinds.rs:84-91`)
  end with: `run loom stage contracts show <stage> for each contract's run dir and command; fix
  metadata with loom stage amend <stage> --field contracts --op replace --index <n> --value '<contract>'`.
- `loom status` already renders `review_reason` (`commands/status/render/attention.rs:199-201`), so it
  needs no change. Keep the reason within the bound in `status/data/sanitize.rs:71`.

## E. Stages

| Stage | Covers | Main files | Depends on |
| --- | --- | --- | --- |
| `contract-run-resolution` | C1, C2 | `testrun/adapters/{pytest,unittest}.rs`, `testrun/mod.rs`, `testrun/command.rs`, `testrun/fixtures/pytest/`, `verify/contracts/{run_dir,store}.rs`, `commands/stage/contracts/freeze.rs`, `daemon/server/contracts.rs` | — |
| `contract-dispute-many` | A | `cli/types_stage_disputes.rs`, `commands/stage/dispute_kinds.rs`, `models/stage/dispute_budgets.rs`, `daemon/server/dispute_kinds.rs`, `orchestrator/adjudication/{verdict_kinds,plan_patch,apply_contract,apply}.rs`, `plan/amendment.rs`, `commands/stage/amend.rs`, ledger | — |
| `contract-visibility` | B, C4, D | `orchestrator/signals/{v2_section,contract}.rs`, `commands/stage/contracts/show.rs`, `orchestrator/adjudication/prompt/contract.rs`, `verify/contracts/refusal.rs`, `daemon/server/dispute_kinds.rs` (message only) | both above |
| `plan-verify-contract-run-dir` | C3 | `plan/schema/validation/v2_lints/contracts.rs`, `verify/contracts/mod.rs`, `skills/loom-plan-writer/references/v2-contracts.md` | `contract-run-resolution` |
| `quota-keychain-account` | F | `quota/credentials.rs`, `quota/poller.rs`, `quota/shared.rs`, `web/src/lib/quota.ts` (if needed) | — |
| `integration-verify` | acceptance | fixture test, knowledge | all |

## F. Claude Quota Meters Missing From the Web Footer

**Diagnosis** (this machine, 2026-10-15):

- `quota/credentials.rs:22-31` looks up `security find-generic-password -s "Claude Code-credentials" -w`
  with no account. The keychain holds two items under that service:
  - account `dkaponis`, which Claude Code writes, last modified 2026-10-15;
  - account `unknown`, last modified 2026-09-07, whose token expired 2026-09-08 08:19 local time.

  `security` returns the `unknown` item.
- At daemon start (19:09) the poll got `HTTP 401` (`.loom/work/orchestrator.log`). The retry at 19:15
  got a 429 with `Retry-After: 3600`. `~/.loom/quota/claude-<key>.json` holds `quota: null`,
  `consecutive_rate_limits: 1`, next poll at 20:15.
- An unconfirmed 429 hides the error (`quota/shared.rs:109-114`). With no reading, `project_account`
  (`quota/poller.rs:175`) writes no `claude.json`, and `record_success` logs a false
  `quota: claude: recovered`. No `claude.json` means no Claude row in the footer. Codex is unaffected.
- No quota code changed in 1.2.0 beyond `046213f0` (FIFO-safe reads). The failure follows the stale
  keychain item, not the release.

**Fix:**

1. Look up `-a <login name>` first, the account Claude Code writes. Resolve the name with `getpwuid`,
   not `$USER`, which the daemon's scrubbed environment may lack. Then fall back to the unqualified
   lookup, then `~/.claude/.credentials.json`. Skip a token whose `claudeAiOauth.expiresAt`
   (milliseconds) has passed. Keep the argv builders exact-tested.
2. When every token found is expired, record `claude.ai token expired` without calling the endpoint.
   A dead token then cannot earn an hour-long 429.
3. A logged-in account that has no reading yet still writes `claude.json`, with empty windows and the
   error (including an unconfirmed 429). The footer then shows a Claude row with the reason instead of
   nothing. `record_success` does not log `recovered` while the account holds no reading.

**Operator workaround until then:** delete the stale item with
`security delete-generic-password -s "Claude Code-credentials" -a unknown`. The next poll uses the
live token, which has a new key, so it polls at once.

## Acceptance

- **Fixture repo** (test guarded by `which uv`, modeled on `bun_missing()` in
  `commands/stage/contracts/freeze_run_dir_tests.rs`): two uv packages `pkg-a/` and `pkg-b/`, each with
  `pyproject.toml`, `uv.lock` and `tests/`, and no root project.
  - With pytest markers, contracts without `cwd` resolve to `pkg-a` and `pkg-b` with `uv run pytest`.
  - A contract forced to `cwd: .` is refused at freeze, naming `No module named pytest` (or
    `command not found`), the cwd and the command.
  - With correct dirs, freeze records `failed` reds. After the implementation is written, completion
    passes. Freeze output, `contracts show` and a completion failure message all print the same cwd and
    command.
- **Plan verify** on that fixture with `.` as the resolved dir (markers removed from the package
  dirs, or `cwd: .`) reports an error per contract naming its package.
- **Amend:** after `loom stage amend --field contracts` changes a contract's `cwd`, `contracts show`
  prints the new run dir and command.
- **Multi-contract dispute:**
  - A dispute naming 3 contracts spends one unit of `contract_disputes`.
  - A verdict with 3 `replace` patches raises `amendments_applied` by one and the plan version by one,
    and re-freezes all 3.
  - A verdict patching a non-disputed contract is refused.
- **Signal tests** (string-contains, as the existing ones are):
  - Contract signal: the environment-red rule and the metadata stop rule.
  - Implementer signal: each contract's run dir and command, and the one-dispute rule.
  - Adjudication prompt: the candidate commands, reproduce-before-ruling, and patch-all-disputed.
- **Quota:**
  - Lookup order and expiry skipping are unit-tested.
  - A 429 with no prior reading writes `claude.json` with an error and empty windows.
  - The web footer test renders a provider with no windows and an error.
- **Gates:** `cargo build`, `cargo test`, `cargo clippy -- -D warnings`, `cargo fmt --check`, and the
  web tests and typecheck when `web/` changes.

## Knowledge Updates

- `architecture/contract-phase.md`, "Where a Contract Runs": Python run roots, the lockfile climb,
  `cwd_rule`, freeze through the probe, environment refusals.
- `architecture/test-runner-adapters.md`, "Run-Root Markers": now states that pytest and unittest have
  none; correct it.
- `architecture/adjudication-lifecycle.md` and `conventions/dispute-and-adjudication.md`: one contract
  dispute names several contracts, multi-patch verdicts, and the budget unit.
- `architecture/quota-poller.md`: lookup order, expiry check, and the no-reading row.
- `mistakes/verification-v2-delivery.md`: this incident (an environment red frozen as real, and a
  per-contract dispute budget unable to repair a shared cause).
- A mistakes entry for the stale keychain item.
