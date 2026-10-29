---
kind: document
of: doc/plans/DONE-PLAN-feat-planner-05-concept-mode.md
file: sha256:688c7651c0c6579894bced32a4ee88c9336f56ded10c121118c2960a08b14647
cites:
  "doc/plans/DONE-PLAN-feat-planner-05-concept-mode.md#1-concept-data-working_dir-loom": "sha256:c6fffc0524b34fff2fd75d352fd758113f27d0503fb7bd42ccc94d7954983566"
  "doc/plans/DONE-PLAN-feat-planner-05-concept-mode.md#2-concept-web-working_dir-web": "sha256:04c904c32a9a38ae158ad955ac0ca27a6a6b8e9cb675f8d36d5e3ff2a93dd0df"
  "doc/plans/DONE-PLAN-feat-planner-05-concept-mode.md#3-integration-verify": "sha256:fc92c390467668ab8e54b221e027a3d6aa864001203d53b02503593f680d748c"
  "doc/plans/DONE-PLAN-feat-planner-05-concept-mode.md#4-knowledge-distill": "sha256:28235efb85dfd3fcf9a208b2a19db5c33c4c530a95c7ce91979aba8184b194ff"
  "doc/plans/DONE-PLAN-feat-planner-05-concept-mode.md#decisions": "sha256:a750926414a96d864d90bab0894e9fa82cee9b1f47b41a3f67bbc8ca1dba81b3"
  "doc/plans/DONE-PLAN-feat-planner-05-concept-mode.md#execution-diagram": "sha256:e389c298b8af57480d19c4df370325c6125fda5e309ffcd883831d9481217946"
  "doc/plans/DONE-PLAN-feat-planner-05-concept-mode.md#goals": "sha256:8c6e36bf1f31c91dff4556d75fba6323d7511708c1982c6617f4df5cc550386b"
  "doc/plans/DONE-PLAN-feat-planner-05-concept-mode.md#overview": "sha256:c0eaa11313cbc88cf3c34fc840c2c95b50ab8a62d87eca3cca3138f488599f50"
---

# Concept mode plan

## Scope

The plan builds the reviewer's view of `loom plan`, covering the Product and Engineering tabs, document summary pages, sentence marks, opening and folding, keyboard reading, out-of-date marks and empty states. [Overview](../../../plans/DONE-PLAN-feat-planner-05-concept-mode.md#overview)

It depends on plans 1 to 4 and needs no session, so summaries come from a hand-run `loom plan summary seal` until plan 10 lands. [Overview](../../../plans/DONE-PLAN-feat-planner-05-concept-mode.md#overview)

A reader follows citations from overview to summary to source passages, and the view updates in place when summaries or sources change. [Goals](../../../plans/DONE-PLAN-feat-planner-05-concept-mode.md#goals)

Producing summaries, the chat side of "Add to chat" and any new dependency are non-goals. [Goals](../../../plans/DONE-PLAN-feat-planner-05-concept-mode.md#goals)

## Design

The server resolves each page fully, so the browser renders key chips, marks and citations and computes nothing from summary files. [Decisions](../../../plans/DONE-PLAN-feat-planner-05-concept-mode.md#decisions)

The route module drives plan 4's summary engine from the watcher and broadcasts one delta per batch that changed something. [Decisions](../../../plans/DONE-PLAN-feat-planner-05-concept-mode.md#decisions)

Each page is refetched on a delta, and an overview is only ever replaced by a whole page. [Decisions](../../../plans/DONE-PLAN-feat-planner-05-concept-mode.md#decisions)

Passages are heading sections of source files read through plan 3's document route, so no second document-reading route exists. [Decisions](../../../plans/DONE-PLAN-feat-planner-05-concept-mode.md#decisions)

Colours follow overview scope order as index modulo 8 plus 1, and the vision and metaplans have no chip and use colour 0. [Decisions](../../../plans/DONE-PLAN-feat-planner-05-concept-mode.md#decisions)

The web package has no browser harness, so the browser test is a vitest test against recorded route fixtures. [Decisions](../../../plans/DONE-PLAN-feat-planner-05-concept-mode.md#decisions)

## Stages

The stages run in the order concept-data, concept-web, integration-verify and knowledge-distill. [Execution Diagram](../../../plans/DONE-PLAN-feat-planner-05-concept-mode.md#execution-diagram)

Stage concept-data builds the route module, wire types, page building and engine driver in `loom/`. [1. concept-data](../../../plans/DONE-PLAN-feat-planner-05-concept-mode.md#1-concept-data-working_dir-loom)

Stage concept-web builds the planning app's concept components and the embedded bundle in `web/`. [2. concept-web](../../../plans/DONE-PLAN-feat-planner-05-concept-mode.md#2-concept-web-working_dir-web)

Stage integration-verify runs the full Rust and web suites, a code review and a smoke test of a real `loom plan` server. [3. integration-verify](../../../plans/DONE-PLAN-feat-planner-05-concept-mode.md#3-integration-verify)

Stage knowledge-distill records the concept route, engine driver and open-state rule in the knowledge base and adds a README paragraph. [4. knowledge-distill](../../../plans/DONE-PLAN-feat-planner-05-concept-mode.md#4-knowledge-distill)
