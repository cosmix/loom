# Pressure review: fewer disputes

The plan needs revision before execution. Nine findings remain after re-reading the plan and its stage contracts. The most consequential are an internally contradictory frozen contract, an insufficient move-scanner interface, and a completion precondition that excludes the impact-test failures the feature promises to resolve.

`loom plan verify --strict doc/plans/PLAN-fewer-disputes.md` passed with **0 errors and 0 warnings**. It also reported 21 uncommitted plan/brief inputs; those must be committed before execution. No build or test suite was run for this review. Findings below describe the specified implementation, grounded in the current source; they are not claims that these proposed features already exist.

## 1. Blocking: the dry-run public contract requires two incompatible zero-test outcomes

**Anchor:** `criteria-authoring`, PUBLIC SURFACE `severity`, Decision 3, contract `dry-run-severity-follows-the-plan`.

The authoritative public surface says `Environment, TimedOut, ZeroTests give Error`. Decision 3, the W4 brief and the frozen contract instead require zero tests to produce a note and leave `report.errors` empty. A writer following the public surface and an implementer following the brief cannot both satisfy the specification. The distinction is observable: the existing runner explicitly emits `selected zero tests (...)` even for an exit-zero command (`loom/src/verify/criteria/zero_tests.rs:16-24`).

**Change:** make the public surface say `ZeroTests -> Some(Note)` and retain a direct assertion of that result for both dependency states, alongside the report-level assertion. Resolve this before the contract session freezes its tests.

## 2. Blocking: `cancel_moves` has no input from which to verify collection

**Anchor:** `gate-noise-and-review`, Decision 9, W1 `moves.rs` interface and `moved-test-cancels-only-into-a-collected-file`.

W1 specifies `cancel_moves(deltas: &mut [FileDelta])`; each delta contains a relative path and the changed file's contents. The same function must check Rust collection by reading `Cargo.toml`, crate roots and intervening modules. Those files need not change and therefore need not be present in `deltas`. The real `chain_problems` consumes a `FileSource` reader (`loom/src/verify/contracts/collection/mod.rs:34`, `loom/src/verify/contracts/collection/rust/mod.rs:28-33`), and immediately reads package/target information before walking modules (`loom/src/verify/contracts/collection/rust/chain.rs:51-64`). The caller already has the correct pinned repository in `scan_changes` (`loom/src/verify/integrity/mod.rs:129-136`), but the specified call drops it.

**Change:** pass the pinned repository or an explicit head-file reader into `cancel_moves`, or compute collection eligibility before constructing the deltas and include it in their type. Update the pinned signature and its callers together. Add a case with unchanged manifest/root files, and invoke the scan while the process cwd is outside the fixture repository; reading ambient cwd must not satisfy the test.

## 3. High: accepted impact-test disputes cannot take the promised completion path

**Anchor:** Decision 11; `verdict-completion` eligibility; W2 submission step 3; contract `failed-agent-completion-is-not-rerun`.

The proposed submission rejects every newest `Complete` job whose state is not `Passed`, assuming that only daemon-applied integrity/review gates could have refused an otherwise eligible run. Impact tests run **inside the child**, even with `daemon_gates: true`: `run_v2` sends their outcome to the sink at `loom/src/verify/completion/complete_verification_v2.rs:44-45`, before the daemon-gate return at lines 50-54. The child records that outcome as failed (`loom/src/host_verify/child_complete.rs:153-156,168-176`), and any failed outcome makes the job result fail (`loom/src/host_verify/child.rs:213`). An accepted impact dispute therefore holds the stage, then immediately hands it back to a fresh session. The headline impact-test capability can never work for the failure that prompted the dispute.

The current contracts only prove that an acceptance failure is ineligible and that an integrity verdict holds. They do not exercise impact failure → accepted verdict → submitted job → completion.

**Change:** distinguish a failed result whose only failing check is the now-accepted impact result from an acceptance/contract/postcondition/infrastructure failure. Preserve rejection of the latter. Add a contract driving the real failed-impact result through verdict application and submission, plus a mixed acceptance-and-impact failure that must requeue.

## 4. High: moving a test into an existing uncollected file silently removes coverage

**Anchor:** Decision 9; `gate-noise-and-review` PUBLIC SURFACE; common brief “Function identity and collection”.

Collection is checked only when the destination did not exist at base. The brief explicitly assumes an existing file “is collected as it was.” Existing integrity discovery establishes only a tracked regular file and a matching language profile, not runner collection (`loom/src/verify/integrity/count.rs:53-69`). Counts also include matching changed files without a collection check (`count.rs:126-133`).

Concrete counterexample: base contains a collected `src/calc_tests.rs` and an unreferenced `src/unused_tests.rs`. Move a complete test, unchanged, into the latter. The specified cancellation removes the source's lost statement; declaration/assertion counts remain equal; the test no longer runs. The same gap applies when a previously collected destination loses its module declaration in this change. The proposed contracts only cover a **new** uncollected destination.

**Change:** validate collection at head for every cancelling destination, regardless of base existence. Add both existing-uncollected and newly-disconnected destination cases. If collection cannot be established, retain the event. Reuse the collection reader from finding 2 rather than assuming that a filename is execution evidence.

## 5. High: a final eligible verdict can erase an earlier sibling verdict's fresh-session requirement

**Anchor:** Decision 11; `verdict-completion` “Held is derived, never stored”; W1 `hold_for_completion_or_remaining_disputes`.

The decision is based on the last verdict alone. A criteria-accept verdict first amends the plan, then calls the requeue helper (`loom/src/orchestrator/adjudication/apply_accept.rs:34-41`). If another dispute remains, that helper deliberately leaves the stage in `NeedsAdjudication` (`loom/src/orchestrator/adjudication/requeue.rs:54-87`). If the final sibling is accepted integrity, the new hold helper will turn this into an eligible hold. HEAD can still equal the previous completion's HEAD because the amendment did not edit the stage worktree.

The proposed submission checks only the previous job's state and HEAD; it never compares that run's policy with the amended policy. The new run captures the new policy and can pass, completing without the fresh session the plan explicitly requires after criteria amendments. The repository already has the necessary distinction: `effective_policy_hash` includes the stage definition, source plan, frozen contracts and amendment revision (`loom/src/handoff/completion/policy.rs:18-33`).

**Change:** require that the prior completion's relevant policy/frozen-contract identity still matches, or derive a “requires fresh session” decision from all verdicts in the adjudication batch. Add a criteria amendment followed by an accepted integrity verdict, and the reverse ordering, proving both requeue. Do not let the order in which sibling verdicts arrive decide this boundary.

## 6. High: formatter checks in `after_stage` are omitted by both freeze paths

**Anchor:** Decision 2; `criteria-authoring`; W2 `static_problems` and `check_formatting`.

Decision 2 promises pure checks from both acceptance and `after_stage`. The brief preserves `format_problems` unchanged, then tells `static_problems` to exclude every format check because `format_problems` handles them. But `format_problems` only receives the acceptance list (`loom/src/verify/contracts/format_gate.rs:76-87`); its current caller passes the stage's acceptance (`loom/src/verify/contracts/freeze.rs:158` onward). A formatter specified only in `after_stage` reaches neither path. A writer can freeze an unformatted contract and leave the implementation stage with an immutable failing postcondition.

The frozen contracts check command classification and attribution, not a freeze whose sole formatter is an `after_stage` entry.

**Change:** explicitly route `after_stage` formatters through the formatter gate, preserving their 30-second default and their assertions. Add an executable freeze-gate contract with no formatter in acceptance and an `after_stage` formatter that names the writer's unformatted file.

## 7. Medium: the dry-run executor discards postcondition assertions and the plan confinement default

**Anchor:** Decision 3; `criteria-authoring`, W4 real `run_dry_run` executor; `DryCommand`.

The brief runs `after_stage` and `wiring_tests` with `run_spec_in_optional_tree` and returns “no failures.” Completion additionally evaluates expected exit code and output assertions: `loom/src/verify/goal_backward/truths.rs:28-35` and `loom/src/verify/goal_backward/wiring_tests.rs:39-46`. Thus `echo wrong` with `stdout_contains: expected` is reported passing in the dry run while completion rejects it. `DryCommand` does not carry these assertion specifications, and no contract exercises them. Raw `result.success` also cannot represent a successful criterion that intentionally expects a nonzero exit.

For acceptance, the specified `CriteriaConfig::default().with_tree_root(...).with_only(...)` omits `with_plan_confinement`. The real runner resolves confinement from the stage override and that config field (`loom/src/verify/criteria/runner.rs:34-35`); absent a stage override, `None` means `Confined` (`loom/src/verify/criteria/config.rs:29-34`). Computing `DryCommand.confinement` does not propagate it into this call. This changes behavior for a current-tree plan with an explicit plan-level confinement setting.

**Change:** carry or look up each full check definition, use the same assertion evaluation as completion, and explicitly pass the resolved plan confinement. Add parity tests for output assertions, expected nonzero exit, and a plan-level confinement default without a stage override. Keep the public classifier's raw-command role separate from the final criterion verdict.

## 8. Medium: the sibling inventory omits three overlapping executable plans

**Anchor:** “Run order and sibling plans”; `criteria-authoring` and `gate-noise-and-review` ownership.

The current inventory covers only autonomous-recovery and live-test-lane. The following siblings also own shared composition roots in their executable metadata:

| Sibling and stage                                                                                                                | Shared ownership / seam                                                                                                                        |
| -------------------------------------------------------------------------------------------------------------------------------- | ---------------------------------------------------------------------------------------------------------------------------------------------- |
| `PLAN-autonomous-recovery.md`: `check-routes`, `gate-waivers`, `spawn-recovery`, `stall-recovery`                                | Adjudication, doctrine, judge watchdog and `src/host_verify/**`; acknowledged in this plan                                                     |
| `PLAN-live-test-lane.md`: `confinement-hardening`, `test-escape-policy`, `live-gate-and-boundary`, `live-profile`, `live-checks` | Criteria runner, schema/model fields, CLI, host jobs and doctrine; acknowledged in this plan                                                   |
| `PLAN-loom-ui-mods.md`: `loom-ui`                                                                                                | `src/commands/hook/mod.rs`, `src/cli/types_ops.rs`, `src/cli/dispatch*.rs`; also intersects this plan's broad `src/commands/hook/**` ownership |
| `PLAN-web-host-graft-followthrough.md`: `implement-host-and-context`                                                             | `loom/src/cli/**` and `loom/src/orchestrator/signals/cache/**`; serial worker C splits dispatch and removes resolved ledger rows               |
| `PLAN-model-router-hooks.md`: `router-core`                                                                                      | `loom/src/cli/dispatch.rs`; its wiring entry pins the router dispatch there                                                                    |

These are actual shared consumers: current `dispatch_plan` calls `plan::verify::execute` at `loom/src/cli/dispatch.rs:86-94`, and `dispatch_hook` starts at `dispatch.rs:120`. Finding their symbols after a sibling merges does not itself grant ownership of a newly extracted file. The fewer-disputes worker tables still grant the exact `dispatch.rs` path and assume fixed existing ledger shapes.

**Change:** add these siblings and an explicit serialization rule. If any CLI-refactoring sibling runs first, amend the stage `files:` lists and worker territories to cover the resulting dispatch owner before execution; preserve the sibling's pinned wiring. Treat that reconciliation as a blocking prerequisite in that run order, rather than relying on compiler errors. The new feature modules themselves have distinct names; no missing upstream feature from these siblings needs to be invented.

## 9. Medium: producing stages omit canonical rustdoc and Markdown gates

**Anchor:** all three code stages' `acceptance`; `integration-verify` and `knowledge-distill`.

All code stages run build, Clippy, format and filtered tests, but none runs the repository's warnings-as-errors rustdoc gate. The canonical hook explicitly explains that build/Clippy/test do not cover rustdoc lints, then runs `RUSTDOCFLAGS="-D warnings" cargo doc --workspace --all-features --no-deps` (`loom/.githooks/pre-push:135-143`). Only the final integration stage runs it here, so a producing stage can merge broken public documentation.

The stages also modify skills, agent documentation and README without a Markdown acceptance gate. The canonical hook checks tracked Markdown (`loom/.githooks/pre-push:66-86`); this plan's only Markdown gate occurs in knowledge-distill and names README alone, leaving the changed skills and reviewer agent unchecked.

**Change:** add the canonical rustdoc command to each Rust-producing stage and a non-mutating Markdown check covering each stage's changed documentation, including proof the linter actually ran. Keep the unfiltered test suite in integration-verify as the repository's verification-rules prescribe; duplicating that suite is not needed to close these omissions.

## Coverage notes

The main capabilities have producing stages and YAML contracts/wiring/reachability entries. The strongest remaining delivery gaps are behavioral: findings 3 and 6 allow the current named contracts to pass while promised behavior is absent. Several modified capability owners are omitted from `artifacts:` itself—especially the static gate, timeout configuration and `consumer_deferred` model/schema owners in `criteria-authoring`; add those paths if the required capability-to-artifact mapping is to be literal rather than inferred from `files:` and tests.

No new package, registry version or external API is needed for the reviewed implementation. The code stages use `working_dir: loom`, bookends use the repository root, and their listed shell commands use matching relative paths. The retirement boundary, stage-bound receipt and restart handling have explicit owners; the missing cross-verdict condition is finding 5. The active trace-screen plan shares only final documentation/knowledge territory with this feature's core work. The ECC proposal has no executable stage contract supplying a dependency here.
