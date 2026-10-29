---
kind: document
of: doc/plans/PLAN-live-test-lane.md
file: sha256:0a0431e057c1f98aeefc02f6bf17dd1b61ab06216cc11bca983e46f68a8bc6c8
cites:
  "doc/plans/PLAN-live-test-lane.md#decisions": "sha256:bd765b4d210efae0dae50d5ced417c323ddaed0a0c17eb2769f7fdb3b9f25796"
  "doc/plans/PLAN-live-test-lane.md#execution-diagram": "sha256:b14f84fc325f1877c6f914b44acb1c3ee6180c383a754521a258a261dfe5dba4"
  "doc/plans/PLAN-live-test-lane.md#goals": "sha256:400221aea089f7f5e662120a72a6a40f9cd537824ba8c8123f8e7c0cfc5256d4"
  "doc/plans/PLAN-live-test-lane.md#overview": "sha256:4e3c90a3eecb8e2ef7e7209e730a8bb01bb2df932638218297b4309f8c9637c2"
  "doc/plans/PLAN-live-test-lane.md#residual-risks": "sha256:a756f171e78099a11fd95c2ecac2550336bb659fbcb6ba9bfd64e7b4b407e1b3"
---

# Live test lane

## Problem and profile

Several of loom's own tests drive a real Claude Code and need outbound network and a credential that neither the confined runner nor the session sandbox provides [Overview](../../../plans/PLAN-live-test-lane.md#overview)

The live profile keeps the confined runner and its filesystem policy, adds a private writable HOME, and opens one gated outbound path [Overview](../../../plans/PLAN-live-test-lane.md#overview)

On Linux the runner's empty network namespace gains exactly one read-only bind for the gate's Unix socket directory, and on macOS the Seatbelt profile allows outbound access to the gate's loopback port only [Overview](../../../plans/PLAN-live-test-lane.md#overview)

The host-side gate requires a per-run secret, terminates TLS for allowlisted hosts with a per-run CA, injects the real credential and refuses everything else, so the credential never enters the sandbox [Decisions](../../../plans/PLAN-live-test-lane.md#decisions)

## Running and receipts

A stage declares live entries that run only through loom check --live, which writes a receipt bound to the content of the tree [Decisions](../../../plans/PLAN-live-test-lane.md#decisions)

Loom stage complete never runs live entries: a required entry needs a passing receipt for the current content, and an advisory entry is only reported [Decisions](../../../plans/PLAN-live-test-lane.md#decisions)

A tree that changes during a live run voids its receipt binding, and each entry's outcome carries its own definition hash [Decisions](../../../plans/PLAN-live-test-lane.md#decisions)

A missing prerequisite yields an unavailable outcome, never a pass, a skip or a fallback to the host [Decisions](../../../plans/PLAN-live-test-lane.md#decisions)

## Escape hatches and guards

The plan closes escape hatches in loom's own sessions: display and session-bus variables in confined commands, command_confinement inherit, the allow_unsandboxed_escape setting and writable codex trust surfaces [Overview](../../../plans/PLAN-live-test-lane.md#overview)

Loom plan verify refuses escape flags in plan-authored commands, and per-facility and live-profile guards replace the LOOM_TEST_REQUIRE_SANDBOX_FREE switch [Overview](../../../plans/PLAN-live-test-lane.md#overview)

The goals keep live tests counted in every ordinary run and make a probe for an advertised facility fail rather than skip [Goals](../../../plans/PLAN-live-test-lane.md#goals)

## Stages and risks

The stages run confinement-hardening and test-escape-policy first, then live-gate-and-boundary, live-profile, live-checks, integration-verify and knowledge-distill [Execution Diagram](../../../plans/PLAN-live-test-lane.md#execution-diagram)

A stage session controls the code a live entry runs, so it can spend the credential through loom check --live, and nothing bounds the number of runs [Residual risks](../../../plans/PLAN-live-test-lane.md#residual-risks)
