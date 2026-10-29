---
kind: document
of: doc/plans/PLAN-autonomous-recovery.md
file: sha256:2b3860353ea8fd6dadc9594625df08a3a40527452521cd0fe64827357403ce56
cites:
  "doc/plans/PLAN-autonomous-recovery.md#decisions": "sha256:8441c3a50077349e20a8af7425b8a3a364d46b1c48c1a9b1f872f5a07a7b0653"
  "doc/plans/PLAN-autonomous-recovery.md#g2-the-block-guard-and-genuine-human-needs": "sha256:6a59dbbfdd990c70c37e27cc22206740bc13968ad1e055763bf0616f1c7d2678"
  "doc/plans/PLAN-autonomous-recovery.md#g5-a-failed-adjudicator-spawn": "sha256:f711697b8d61e4c060d70fd74973587591783943090303deb3165ce4ad894186"
  "doc/plans/PLAN-autonomous-recovery.md#g7-the-scheduler-stalls-in-its-sync-phase": "sha256:db8e7f9aff6386fecdd0cbba919a2a07ad150498bd1580542158ba8619a4b150"
  "doc/plans/PLAN-autonomous-recovery.md#goals": "sha256:655f1d79e81bfbd37d620dd1f34d652e51ba9206fc8cd973cd2890c62987e8d1"
  "doc/plans/PLAN-autonomous-recovery.md#open-decisions": "sha256:1238cb02dff68558cd497c584a76381eef01254b29ad58048f54afb605188d7f"
  "doc/plans/PLAN-autonomous-recovery.md#overview": "sha256:fdd86937c27b74e4d7e96e00045d634a3f7932c303a50de28073c71fac180b4a"
  "doc/plans/PLAN-autonomous-recovery.md#residual-risks": "sha256:34162d29e0f23a0512fa30fee17ba772165f6d1ac83b59a2a47270ad4aaa4427"
  "doc/plans/PLAN-autonomous-recovery.md#run-order-and-sibling-plans": "sha256:4c5ca57151f272fb84c98e912a8fa00ddc03bf25cfc489c269a59a664a81ce6e"
---

# Autonomous recovery plan

## Stops and routes

The plan covers five stop groups: gate checks with no route (G1), the block guard (G2), tools hidden by confinement (G4), failed judge spawns (G5), and a stalled scheduler (G7). [overview](../../../plans/PLAN-autonomous-recovery.md#overview)

Every stop ends either in an autonomous route or, for a credential, a host install, or a decision only a person can make, in a block that names what is needed. [goals](../../../plans/PLAN-autonomous-recovery.md#goals)

Each route has a budget and a terminal state, so no route loops. [goals](../../../plans/PLAN-autonomous-recovery.md#goals)

Plan-authored checks gain addresses such as `[reachable 1]`, so every gap can be disputed. [decisions](../../../plans/PLAN-autonomous-recovery.md#decisions)

Integration-verify can file against a completed stage's wiring or reachable check, and an accepted verdict amends the owner's definition. [decisions](../../../plans/PLAN-autonomous-recovery.md#decisions)

Loom's own gates (`unwired`, `duplicates`, `impact-tests`) can be waived by an accepted verdict, and every waiver is listed for reviewers. [decisions](../../../plans/PLAN-autonomous-recovery.md#decisions)

## Human needs

A new `needs-person` verdict blocks a stage with a reason that starts with `needs a host install` and names the missing item. [decisions](../../../plans/PLAN-autonomous-recovery.md#decisions) [g2](../../../plans/PLAN-autonomous-recovery.md#g2-the-block-guard-and-genuine-human-needs)

The block guard refuses a block while a failing check has a dispute route, and never refuses a label with no route or an unreadable record. [g2](../../../plans/PLAN-autonomous-recovery.md#g2-the-block-guard-and-genuine-human-needs)

Tool roots are computed per command, and a root that is `/`, the home directory, or a credential path is never exposed. [decisions](../../../plans/PLAN-autonomous-recovery.md#decisions)

## Runtime and scheduling

Spawn failures are classified by type: permanent ones escalate, repairable ones (hook drift, a stale daemon) are repaired, and transient ones retry at 30 and 60 seconds. [g5](../../../plans/PLAN-autonomous-recovery.md#g5-a-failed-adjudicator-spawn) [decisions](../../../plans/PLAN-autonomous-recovery.md#decisions)

A daemon whose binary was replaced restarts itself onto the installed binary, at most three times an hour. [decisions](../../../plans/PLAN-autonomous-recovery.md#decisions)

Resolved-merge verification moves into a host job, and every scheduler-thread wait is bounded and named, with a watchdog restart at 900 seconds. [g7](../../../plans/PLAN-autonomous-recovery.md#g7-the-scheduler-stalls-in-its-sync-phase) [decisions](../../../plans/PLAN-autonomous-recovery.md#decisions)

## Open items

The plan runs after feat-planner plans 08-11 and the live-test-lane plan have merged, where plan 08 is merged, plan 09 is executing and plans 10 and 11 are queued, and it changes no web or planning-server file. [run order](../../../plans/PLAN-autonomous-recovery.md#run-order-and-sibling-plans)

The open decisions include lanes not yet confirmed with the user and a failing `before_stage` check that still blocks for a person. [open decisions](../../../plans/PLAN-autonomous-recovery.md#open-decisions)

Residual risks include heuristic tool discovery, a successor that dies after taking the lock, and waivers that persist for the stage's life. [residual risks](../../../plans/PLAN-autonomous-recovery.md#residual-risks)
