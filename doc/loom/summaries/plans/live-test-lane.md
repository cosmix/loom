---
kind: document
of: doc/plans/PLAN-live-test-lane.md
file: sha256:027c4b5ba0116b2e64c056624056de5a1deac40f060ff3175ddb6c95d5d678d0
cites:
  "doc/plans/PLAN-live-test-lane.md#decisions": "sha256:1adb00de5fbaef7752f374fdc2fdcfcb5e3b90d7bb7668d3bc00e34ff6a4fc3a"
  "doc/plans/PLAN-live-test-lane.md#execution-diagram": "sha256:b14f84fc325f1877c6f914b44acb1c3ee6180c383a754521a258a261dfe5dba4"
  "doc/plans/PLAN-live-test-lane.md#goals": "sha256:c0d66758ce5c91f973637e014b8de5b7c9c7f16e9097a671c92a7636479fb15c"
  "doc/plans/PLAN-live-test-lane.md#overview": "sha256:4e3c90a3eecb8e2ef7e7209e730a8bb01bb2df932638218297b4309f8c9637c2"
  "doc/plans/PLAN-live-test-lane.md#residual-risks": "sha256:86fe3dc5a671bb6fde2d13bb1031043fe1e93522c9c686fe24eaae28b6da29af"
---

# Live test lane

## Problem and profile

[keep] Several of loom's own tests drive a real Claude Code and need outbound network and a credential that neither the confined runner nor the session sandbox provides [Overview](../../../plans/PLAN-live-test-lane.md#overview)

[keep] The live profile keeps the confined runner and its filesystem policy, adds a private writable HOME, and opens one gated outbound path [Overview](../../../plans/PLAN-live-test-lane.md#overview)

[keep] On Linux the runner's empty network namespace gains exactly one read-only bind for the gate's Unix socket directory, and on macOS the Seatbelt profile allows outbound access to the gate's loopback port only [Overview](../../../plans/PLAN-live-test-lane.md#overview)

[keep] The host-side gate requires a per-run secret, terminates TLS for allowlisted hosts with a per-run CA, injects the real credential and refuses everything else, so the credential never enters the sandbox [Decisions](../../../plans/PLAN-live-test-lane.md#decisions)

## Running and receipts

[keep] A stage declares live entries that run only through loom check --live, which writes a receipt bound to the content of the tree [Decisions](../../../plans/PLAN-live-test-lane.md#decisions)

[keep] Loom stage complete never runs live entries: a required entry needs a passing receipt for the current content, and an advisory entry is only reported [Decisions](../../../plans/PLAN-live-test-lane.md#decisions)

[keep] A tree that changes during a live run voids its receipt binding, and each entry's outcome carries its own definition hash [Decisions](../../../plans/PLAN-live-test-lane.md#decisions)

[keep] A missing prerequisite yields an unavailable outcome, never a pass, a skip or a fallback to the host [Decisions](../../../plans/PLAN-live-test-lane.md#decisions)

## Escape hatches and guards

[keep] The plan closes escape hatches in loom's own sessions: display and session-bus variables in confined commands, command_confinement inherit, the allow_unsandboxed_escape setting and writable codex trust surfaces [Overview](../../../plans/PLAN-live-test-lane.md#overview)

[keep] Loom plan verify refuses escape flags in plan-authored commands, and per-facility and live-profile guards replace the LOOM_TEST_REQUIRE_SANDBOX_FREE switch [Overview](../../../plans/PLAN-live-test-lane.md#overview)

[keep] The goals keep live tests counted in every ordinary run and make a probe for an advertised facility fail rather than skip [Goals](../../../plans/PLAN-live-test-lane.md#goals)

## Stages and risks

[keep] The stages run confinement-hardening and test-escape-policy first, then live-gate-and-boundary, live-profile, live-checks, integration-verify and knowledge-distill [Execution Diagram](../../../plans/PLAN-live-test-lane.md#execution-diagram)

[keep] A stage session controls the code a live entry runs, so it can spend the credential through loom check --live, and nothing bounds the number of runs [Residual risks](../../../plans/PLAN-live-test-lane.md#residual-risks)
