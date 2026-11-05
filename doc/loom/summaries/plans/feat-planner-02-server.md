---
kind: document
of: doc/plans/feat-planner/archive/DONE-PLAN-feat-planner-02-server.md
file: sha256:7b7268763debb9c99de5894916fd6172b19820de4444df8a4021f7fe0277075a
cites:
  "doc/plans/feat-planner/archive/DONE-PLAN-feat-planner-02-server.md#1-web-server-extract-the-shared-http-layer": "sha256:cf70cfaf264ae8d682028f1e34c9f86953d713da7832b939ea80f9c026756171"
  "doc/plans/feat-planner/archive/DONE-PLAN-feat-planner-02-server.md#2-plan-server-core-the-planning-server": "sha256:d3f71679cd592c148a14c71f6a27cacc82422eb2aaedaf20d2b5b4cd735608d1"
  "doc/plans/feat-planner/archive/DONE-PLAN-feat-planner-02-server.md#3-plan-web-shell-the-planning-apps-second-entry": "sha256:f51a92d1382f7f9bbb252f8ec8c6501c0d31e97943c5a1fca779584e0b19291a"
  "doc/plans/feat-planner/archive/DONE-PLAN-feat-planner-02-server.md#decisions": "sha256:066a6e71e1d0636be6afb0cd3e5daa18a1029ced2d37502ba67854add11ffbbd"
  "doc/plans/feat-planner/archive/DONE-PLAN-feat-planner-02-server.md#execution-diagram": "sha256:784138229540c7b11105e0ae7b5409261b61fbc8a6bdb8a6f6a639f6002d3b87"
  "doc/plans/feat-planner/archive/DONE-PLAN-feat-planner-02-server.md#goals": "sha256:54e45c20c966cf4628f8a4ea5355d3fcc36c06aab6c13f383bcc6be133793817"
  "doc/plans/feat-planner/archive/DONE-PLAN-feat-planner-02-server.md#interfaces-later-plans-consume": "sha256:12afcb18098622a42c6da56b01c0681a5030a4856a84b50c2d7a0317f15e94fe"
  "doc/plans/feat-planner/archive/DONE-PLAN-feat-planner-02-server.md#overview": "sha256:679c76a9c626fda5af464ef6595478051004796037f545a266c0a734fa1c7cfc"
---

# Server foundation and access plan (plan 02)

## Scope

Bare `loom plan` starts a server that serves an authenticated planning app with a header, presence, Share and a 401 screen. [Overview](../../../plans/feat-planner/archive/DONE-PLAN-feat-planner-02-server.md#overview)
The dashboard's server parts move into a shared `loom::web_server` module whose accept loop takes a router, and the dashboard and the planning server each become one router. [Overview](../../../plans/feat-planner/archive/DONE-PLAN-feat-planner-02-server.md#overview)
The plan adds section 02's project root, port, lock, runtime directory, opener, lifetime and read-only mode, and section 12's access model. [Overview](../../../plans/feat-planner/archive/DONE-PLAN-feat-planner-02-server.md#overview)
Its goals are the planning shell behind the access checks, a dashboard that behaves as before, and a second entry page whose code stays out of the dashboard bundle. [Goals](../../../plans/feat-planner/archive/DONE-PLAN-feat-planner-02-server.md#goals)
Non-goals are any document, chat, session or summary route, which plans 3 to 11 add, and any new dependency. [Goals](../../../plans/feat-planner/archive/DONE-PLAN-feat-planner-02-server.md#goals)

## Decisions

The accept loop takes only the router, because it never calls a publisher and the shared hub is reached from each router's WebSocket upgrade. [Decisions](../../../plans/feat-planner/archive/DONE-PLAN-feat-planner-02-server.md#decisions)
Plan 2 owns the 10-minute authoring grace timer and frees authoring at expiry, so a holder who closed the tab does not hold authoring forever. [Decisions](../../../plans/feat-planner/archive/DONE-PLAN-feat-planner-02-server.md#decisions)
The API prefix is `/api/`, the WebSocket path is `/ws`, and the planning `Origin` rule is exact on loopback too. [Decisions](../../../plans/feat-planner/archive/DONE-PLAN-feat-planner-02-server.md#decisions)
Bundle separation is checked from Vite's manifest, following static imports only, so a lazy chunk never counts against the initial bundle. [Decisions](../../../plans/feat-planner/archive/DONE-PLAN-feat-planner-02-server.md#decisions)

## Interfaces

Every wire field that holds a document path is repo-relative and starts with `doc/`. [Interfaces later plans consume](../../../plans/feat-planner/archive/DONE-PLAN-feat-planner-02-server.md#interfaces-later-plans-consume)
The dashboard's startup lines stay unchanged because plan 11 parses them. [Interfaces later plans consume](../../../plans/feat-planner/archive/DONE-PLAN-feat-planner-02-server.md#interfaces-later-plans-consume)
Wire types are Rust structs with parity tests against zod schemas and the fixtures under `web/src/plan/api/fixtures/`. [Interfaces later plans consume](../../../plans/feat-planner/archive/DONE-PLAN-feat-planner-02-server.md#interfaces-later-plans-consume)

## Stages

Stages run in the order web-server-extract, plan-server-core, plan-web-shell, integration verification and knowledge distillation. [Execution Diagram](../../../plans/feat-planner/archive/DONE-PLAN-feat-planner-02-server.md#execution-diagram)
Stage 1 moves the dashboard's server parts behind a Router trait, and the Host check must run before any router. [Stage 1](../../../plans/feat-planner/archive/DONE-PLAN-feat-planner-02-server.md#1-web-server-extract-the-shared-http-layer)
Stage 2 builds the planning server core, and stage 3 adds the second Vite entry, the shell, the 401 screen and the bundle check. [Stage 2](../../../plans/feat-planner/archive/DONE-PLAN-feat-planner-02-server.md#2-plan-server-core-the-planning-server) [Stage 3](../../../plans/feat-planner/archive/DONE-PLAN-feat-planner-02-server.md#3-plan-web-shell-the-planning-apps-second-entry)
