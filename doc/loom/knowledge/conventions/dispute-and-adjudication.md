---
verified: 5546d3c47ddc1f8890b40157134f057393b8b90e
---

# Dispute And Adjudication

>

## Dispute File Ownership Convention

`.loom/work/disputes/<stage>/<n>/` — always split by authority:

| File                                             | Authority            | Notes                                                                                  |
| ------------------------------------------------ | -------------------- | -------------------------------------------------------------------------------------- |
| `.loom/work/disputes/<stage>/<n>/request.md`     | agent-attestable     | written by the daemon on behalf of the agent's RPC, or drained from the worktree spool |
| `.loom/work/disputes/<stage>/<n>/verdict.json`   | adjudication session | the session's DRAFT, not the record — it has no authority until recorded               |
| `.loom/work/disputes/<stage>/<n>/verdict.md`     | daemon-only          | the record `apply_pending_verdicts` reads                                              |
| `.loom/work/disputes/<stage>/<n>/applied.marker` | daemon-only          | zero-byte idempotency sentinel                                                         |
| `.loom/work/disputes/<stage>/<n>/attempts`       | daemon-only          | respawn budget, spent when an adjudication job is handed out                           |

Never collapse these into one file. The rule behind the split is unchanged: **if the party under
dispute can write the verdict, it can self-approve.**

What changed on 2026-08-31 is WHO writes the verdict, so the guard moved rather than disappearing.
The adjudicating session is a different session from the disputing stage agent, and it records
through `loom stage adjudicate`, which refuses:

- when `LOOM_WORKTREE_PATH` is set — a stage worktree session can never record a verdict, which is
  precisely the self-approval case;
- when the stage is not in `NeedsAdjudication`;
- when the named dispute was never filed;
- when a `.loom/work/disputes/<stage>/<n>/verdict.md` already exists — a recorded verdict is not replaceable.

A degenerate verdict escalates rather than being recorded. The verdict file is written by the adjudication session itself; there is no worker thread and no API call.

## Adjudicator Scope Convention

The adjudicator amends ONLY:

- `acceptance: Vec<AcceptanceCriterion>` (plan/schema/types.rs::StageDefinition)
- `wiring: Vec<WiringCheck>` (plan/schema/types.rs::StageDefinition)
- `contracts: Vec<ContractSpec>` on a v2 contract dispute (`AmendmentField::Contracts`)

Never amends: `before_stage`, `after_stage`, `artifacts`, `dependencies`, `id`, `working_dir`, `model`, `sandbox`, `execution`. Use `AmendmentField` enum to enforce this at the type level.

**A stage session cannot amend.** `loom stage amend` is an operator command: `commands/stage/amend.rs` refuses a process whose `LOOM_WORKTREE_PATH` names a stage worktree (`refuse_stage_session`) and one with `LOOM_STAGE_ID` or `LOOM_SESSION_ID` set (`refuse_session_identity`). Both are guards against a session running the command by mistake; the boundary is the session's write denies on `.loom`, which stop the amendment from being written whatever the environment says.

**Narrowing needs base evidence.** An acceptance `delete` or narrowing `replace` must declare `plan_patch.leaves_out: [{index, tests, runs_at, reason}]`, and each entry needs a recorded run of that criterion at the stage's base that failed with tests executed (`verdict_narrowing.rs::check_narrowing`). A criterion that passes at the base, does not build there or selects no test is the stage's own failure and is refused; so is a `replace` equal to the current entry. A stage's own new test is never narrowed. Each accepted entry becomes a hand-off ([Criterion Narrowing](../architecture/contract-collection.md)).

## Dispute Budget Limits Convention

Per-stage caps to bound the autonomy loop:

- `dispute_count`: max 3 criterion disputes per stage (`MAX_DISPUTES_PER_KIND` via `Stage::max_disputes_per_stage`, `models/stage/dispute_budgets.rs`); one filing may name several `--criterion-index` values and spends one; further dispute requests are refused
- `evidence_rounds` (NeedsMoreEvidence iterations): max 5 (`MAX_EVIDENCE_ROUNDS`, `orchestrator/adjudication/mod.rs`) before escalation to NeedsHumanReview
- `amendments_applied`: bounded by the plan-level `adjudication.max_amendments_per_stage`, default 10 (`default_max_amendments_per_stage`, `plan/schema/types.rs`); the cap counts verdicts (distinct base dispute ids in the audit rows, `plan/amendment/apply.rs::count_verdicts`) and is checked at a verdict's first patch, so a multi-patch verdict applies wholly or not at all; past it the stage escalates to NeedsHumanReview
- `adjudicator_attempt_count` (worker crash retries): max 3
- `finding_disputes`, `contract_disputes`, `integrity_disputes`: max 3 each (`MAX_DISPUTES_PER_KIND`), kept beside `evidence_rounds` and `amendments_applied` in `stage.tally`; see `architecture/adjudication-lifecycle.md`
- contract writer respawns: max 3 (`MAX_CONTRACT_RESPAWNS`, `.loom/work/contracts/<stage>/attempts`), spent when a replacement writer is handed out; see `architecture/contract-phase.md`

## Adjudication Attempt Budget Convention

Respawn of an adjudication session is bounded by an on-disk `attempts` file in the dispute
directory, incremented when a job is handed out — the same shape as the merge-resolver attempt
counter. `MAX_ADJUDICATION_ATTEMPTS` is 3.

**No marker file tracks an in-flight adjudication.** An adjudication session's liveness is read from the session registry
(`live_sessions_for_stage` filtered to `SessionType::Adjudication`, PID identity with start-time
verification), so there is no marker to go stale and no freshness window to tune. That
also survives a daemon restart.

**Counting rule:** a counter derived from an artifact that the counted operation has not produced yet is not a counter. Spend the budget when the work is HANDED OUT, not when it succeeds.

## Daemon-as-Filesystem-Writer Convention

The intent: agent data that needs authority separation is persisted to the state directory by the daemon, not by the agent. `loom memory note` does not send an RPC for the daemon to write the journal:

- `loom memory note` writes `.loom/work/memory/<stage>.md` directly from the calling process. Only when that write is denied (EROFS/EACCES, `is_write_denied`) does it append to the per-worktree spool `<worktree>/.loom/memory-spool.jsonl`, which the daemon drains (`commands/memory/handlers/record.rs` → `record_via_spool`; `orchestrator/core/spool_drain.rs`). Off a worktree the spool refuses, so a denied main-checkout session fails.
- `loom stage dispute-criteria` never writes the state directory: it appends to `<worktree>/.loom/stage-request-spool.jsonl`, and the daemon writes `.loom/work/disputes/<stage>/<n>/request.md` (`fs/stage_request/`).

Moving every session-originated write onto a daemon-drained channel is the pending `.loom` confinement work; see [Live State Pollution](../mistakes/live-state-pollution.md).

## Adjudicator Transport Convention

Adjudication runs in a **spawned loom session**, the same way merge conflict resolution does. There
is no API key, no HTTP call, and no headless subprocess anywhere in the path.

**Corrected twice on 2026-08-31, so read the history before reintroducing either shape.** This
section first described an `ANTHROPIC_API_KEY` gate: absent key at daemon startup disabled
adjudication for the whole run and every dispute escalated to `NeedsHumanReview` — which killed the
feature for anyone on a subscription rather than an API key. It was then briefly a headless
`claude -p` subprocess driven from a worker thread. Both are gone. The owner's call: a dispute is
judged by a real session with the full tool surface, because that is what lets it check the tree and
run things.

- **Spawn:** `adjudication::session::start_pending_adjudications` builds a `SessionType::Adjudication`
  session and spawns it through the same `TerminalBackend` every other session goes through. It runs
  in the stage's worktree when that exists, else in the main repo (`judge_cwd`, `adjudication/session.rs`),
  and never gets a worktree of its own.
- **Model:** `resolve_model` reads `.loom/work/config.toml::[adjudication].model`, defaulting to `opus`.
- **Briefing:** `orchestrator/signals/adjudication.rs` writes a signal whose body comes from
  `adjudication::prompt`, carrying the dispute, the stage's criteria, the evidence commit diff and
  the failure output.
- **The session RUNS the disputed criterion.** This is the point of using a session rather than a
  one-shot call. `prompt::execution_site` resolves the stage's worktree root joined with its
  `working_dir` — the same `EXECUTION_PATH` acceptance criteria resolve against — and the briefing
  tells the session to execute the criterion there and rank the observed exit code above the
  disputing agent's account of it. Running it from the wrong directory makes a sound criterion look
  broken and inverts the verdict, which is why the site is computed rather than guessed.
- **It judges, it does not fix:** no edits, no commits, never `loom stage complete`. The single
  authorized execution is the disputed criterion itself — the same command the stage already runs,
  under the same sandbox, so it is not new exposure.
- **Return path:** the session writes its verdict JSON to `.loom/work/disputes/<stage>/<n>/verdict.json` and
  runs `loom stage adjudicate --stage <id> --dispute <n> --verdict-file <path>`. The daemon's
  existing `apply_pending_verdicts` applies it on the next tick. Nothing waits on the session; the
  daemon observes the state change, exactly as it does for a merge resolution session.
- **Liveness and budget:** a live adjudication session is detected through
  `session_registry::live_sessions_for_stage` filtered to `SessionType::Adjudication` (PID identity
  with start-time verification), so it is correct across a daemon restart with no in-memory state.
  Respawn is bounded by an on-disk `attempts` counter in the dispute directory, spent when a job is
  handed out.
- `sandbox/settings.rs`'s `SENSITIVE_ENV_KEYS` still filters `ANTHROPIC_API_KEY` from agent sandbox
  environments. That remains correct hygiene and has nothing to do with adjudication.

## A Dispute Answers a Wrong Check, Never a Planned Step

A stage description does not order a dispute for an event the detector does not raise. The integrity detector compares assertion statements, cancels tests that moved to a collected file and raises a ratchet event only when a ledger loosens (see [verification-v2-gates](../architecture/verification-v2-gates.md)), so a reflow, a moved test or a tightened ratchet raises nothing. A move counts only when the destination is collected: for Rust, a test root that `verify/contracts/collection/rust/targets.rs::test_roots` offers for the destination reaches it through a `#[path]`-free `mod` chain (`verify/integrity/moves_collect.rs::rust_collected`), so a move into a file included only through `#[path]` raises `TI-edit` on the source file. When a stage still expects an event (a test deleted because the behaviour it pinned is gone, a ledger raised on purpose), its description names the event and the reason, and the stage reviewer brief tells the final reviewer to ask for a finding only when the changed statement no longer asserts what that reason describes. Naming the event does not accept it: only an accepted `loom stage dispute-integrity` writes `reviews/<stage>/integrity.json` (`orchestrator/adjudication/apply_kinds.rs` via `verify/review/verdict_records.rs::append_integrity_acceptance`), and the gate (`verify/integrity/gate.rs::check_changes`) fails on every event that file does not accept. So the agent commits, runs `loom stage complete`, then files ONE `loom stage dispute-integrity <stage> --event <id> ... --reason "..."` for every raised event, named or not. After a filed dispute the stage agent stops; an accepted integrity or impact-tests dispute, or a findings ruling that upholds nothing, lets the daemon finish a passing committed tree without a new session (see [adjudication-lifecycle](../architecture/adjudication-lifecycle.md#retirement-and-fresh-session-boundary)), and any other outcome costs a fresh stage session. An accepted event stays accepted while it gets no worse, and its pin covers the cancelling partner files.

## Frozen Contracts Change Only Through `--replacement`

A stage agent never edits a frozen contract file; the signal says so and the Claude Code permission classifier enforces it. A contract that must change is disputed with complete replacements, trialled first:

```bash
loom check <stage> --contracts --replacement <frozen-file>=<path> [--replacement ...]
loom stage dispute-contract <stage> --contract <id> [--contract <id> ...] \
  [--replacement <frozen-file>=<path> ...] --reason "..."
```

- **Where the file lives.** The agent copies the frozen file to `$LOOM_SCRATCH_DIR`, edits the copy, and runs the stage's formatter check on it. The CLI (`commands/stage/dispute_kinds.rs`) refuses a path that canonicalizes into the stage worktree, reads it without following symlinks, and requires UTF-8; it resolves the stage and worktree once for all replacements.
- **One filing, several contracts.** `DisputeKind::Contract { contract_ids, replacements }` (`models/dispute.rs`) spends one unit of `contract_disputes` however many contracts it names. A replacement names any file of the freeze record, a harness file included, and carries that file's own base hash. A replacement for a contract file needs every contract of that file named.
- **Size.** `MAX_REPLACEMENT_BYTES` (32 KiB, `models/dispute_replacement.rs`) bounds each file; the whole filing travels in one `file-dispute` ticket of at most `MAX_TICKET_BYTES` (64 KiB), checked before anything is written. A filing that does not fit is split by file.
- **Trial.** `loom check <stage> --contracts` runs the stage's frozen contracts in a Check host job, the code completion runs (`verify/contracts/completion.rs`); a full `loom check` of a frozen standard stage runs them too. With `--replacement` the job lays each replacement over its frozen file (`verify/contracts/trial.rs::with_admitted_overlay`, under the host-job stage lock), runs the freeze's format and static gates on those files and the contracts with `CachePolicy::Bypass`, and puts the frozen files back whatever the outcome. A pass records a receipt (HEAD, file, sha256) in `trial.json`. A `kill_groups` retirement or a restart undoes a live overlay daemon-side (`verify/contracts/frozen_restore.rs::undo_trial_overlay`); the restore never overwrites a file that holds other bytes or that the freeze now records at the overlay bytes.
- **Admission** (`stage_service/dispute_admission.rs`) refuses, before the budget is spent: an empty or repeated id, an id the stage no longer lists or the record does not freeze, a replacement for a file the record lacks, a replacement over the size limit or identical to the frozen content, any recorded frozen file changed or missing in the worktree (restore it with `loom stage contracts restore <stage>` and file the change as a replacement), and a replacement without a passing trial receipt for the same bytes at the worktree's HEAD (`trial::receipt_covers`). It overwrites any client-supplied base hash with the frozen file's.
- **Judging.** The verdict is `contract-rulings`: one `accept` or `reject` per disputed contract, plus an optional `plan_patch` on `contracts`. The prompt (`prompt/contract_replacement.rs`) shows the trial result and every assertion statement the replacement loses or changes (`verify::integrity::lost_statements`); the judge cites what covers each or rejects. The judge runs no code. Every contract prompt forbids asking the agent to edit a frozen file.
- **Recording.** `verdict_contract.rs` refuses an `accept` that changes nothing: its file needs a replacement the rulings let the daemon write, or a patch must name it (`files_to_write`, `apply_contract_rulings.rs`). The refusal returns to the judge as an `Err`, so no evidence round is spent.
- **Accept** writes each replacement the rulings allow, then re-freezes ([Accepted Dispute Re-Freeze](../architecture/contract-phase.md#accepted-dispute-re-freeze)). The writes are uncommitted: the notice tells the agent to `git add` the files and commit through `loom stage commit`. A reject leaves the worktree unchanged and discards its replacement.
- **Needs more evidence.** The feedback says never to edit a frozen file and to re-file with complete replacements; an earlier replacement is not carried over. `apply.rs::apply_needs_more_evidence` owns the evidence-round cap.

## Verification Design Priorities

Every change to verification, completion, disputes or adjudication is judged against three priorities, in this order. A lower one never buys an exception to a higher one.

1. **No stage code runs outside a sandbox.** System integrity comes first. An unwrapped run, `sandbox.command_confinement: inherit` used as a fix, and a broker or job that applies a profile the stage authored are all rejected, whatever autonomy they would buy.
2. **A person is the last resort.** `NeedsHumanReview` and `--force-unsafe` are for needs only a person can meet. A design that sends a class of stage there routinely is rejected: an environment-caused failure needs an exit the stage and the adjudicator can take alone.
3. **A stage agent cannot game its own checks.** A contract is never skipped or exempted. The adjudicator may narrow, rewrite or delete a criterion or a contract that is wrong, meaning it cannot pass whatever the implementation; it never does so as the easy way out, and the stage agent never does so at all.
