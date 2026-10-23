# Contract Phase Human Blocking

> Refusal waited on operator

## A Contract Freeze Refusal Waited on the Operator

**What happened:** In liquidmon (plan custom-fan-curve, stage fix-instance-lock), the plan harnessed `src/main.rs` but not `src/device_lock/mod.rs`. The contract writer added `mod device_lock;`, the freeze's format gate (`cargo fmt --all -- --check`) failed on the missing module file, and the stage could never freeze. The refusal parked the stage `WaitingForInput`. The web UI showed only 'awaiting input'. The writer then asked the operator to choose between two fixes, one naming a command that does not exist (`loom stage amend` has no harness field) and one the relay refuses (a Contract session cannot dispute). Unblocking took several operator commands in a strict order.
**Why:** The contract phase had no automatic path out of a plan defect. A Contract session may not dispute (`relay/matrix.rs`), a refused freeze parks the stage for a human, and harness was the only way to let the writer touch module wiring, but harness freezes what it matches.
**Prevention:** A stage never waits on a human for something loom or an agent can resolve. Deterministic checks guide the agent. A refused freeze is disputed and adjudicated, never parked: the adjudicator either sends a fresh writer back with what to fix or amends the plan's contract setup. A human is reached only after the dispute budget is spent, through one named command. Any new park or block state must name the automatic path that leaves it. Genuine human input stays: an agent's `AskUserQuestion`, `loom stage block`, and an adjudicator rejecting a criterion dispute all mean a person is needed, and loom does not suppress them.

**Fix:** doc/plans/PLAN-contract-phase-autonomy.md.
