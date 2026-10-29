---
kind: document
of: doc/plans/DONE-PLAN-feat-planner-02-server.md
file: sha256:7b7268763debb9c99de5894916fd6172b19820de4444df8a4021f7fe0277075a
cites:
  "doc/plans/DONE-PLAN-feat-planner-02-server.md#decisions": "sha256:066a6e71e1d0636be6afb0cd3e5daa18a1029ced2d37502ba67854add11ffbbd"
  "doc/plans/DONE-PLAN-feat-planner-02-server.md#environment-inventory": "sha256:8400ce854ae9916a41c618ba3dfe431add185a603d92a8a11d98bb5bffee865b"
  "doc/plans/DONE-PLAN-feat-planner-02-server.md#goals": "sha256:54e45c20c966cf4628f8a4ea5355d3fcc36c06aab6c13f383bcc6be133793817"
  "doc/plans/DONE-PLAN-feat-planner-02-server.md#interfaces-later-plans-consume": "sha256:12afcb18098622a42c6da56b01c0681a5030a4856a84b50c2d7a0317f15e94fe"
  "doc/plans/DONE-PLAN-feat-planner-02-server.md#overview": "sha256:679c76a9c626fda5af464ef6595478051004796037f545a266c0a734fa1c7cfc"
  "doc/plans/DONE-PLAN-feat-planner-02-server.md#stages": "sha256:35a8b17032de80800e474f143f7226ae150a6d913db948da0bb47e43d7eb68e1"
---

# Plan feat-planner-02-server: server foundation and access

## Overview

Bare `loom plan` starts an authenticated planning server with the seven-step header tracker, presence, Share and the 401 screen. [overview](../../../plans/DONE-PLAN-feat-planner-02-server.md#overview)
The dashboard's server parts move into a shared `loom::web_server` module whose accept loop takes a router. [overview](../../../plans/DONE-PLAN-feat-planner-02-server.md#overview)
The plan adds section 02 (root, port, lock, runtime directory, opener, lifetime, read-only mode) and the section 12 access model. [overview](../../../plans/DONE-PLAN-feat-planner-02-server.md#overview)

## Goals

The dashboard keeps its behavior, with edits only to use paths, and its startup lines stay pinned because plan 11 parses them. [goals](../../../plans/DONE-PLAN-feat-planner-02-server.md#goals)
The planning app is a second entry page in the one web Vite build, and its code stays out of the dashboard bundle. [goals](../../../plans/DONE-PLAN-feat-planner-02-server.md#goals)
Document, chat, session and summary routes are out of scope, and the plan adds no new dependency. [goals](../../../plans/DONE-PLAN-feat-planner-02-server.md#goals)

## Decisions

The accept loop takes only the router, because the shared hub is reached from each router's WebSocket upgrade and never from the loop. [decisions](../../../plans/DONE-PLAN-feat-planner-02-server.md#decisions)
Plan 2 owns the 10-minute authoring grace timer and frees authoring at expiry. [decisions](../../../plans/DONE-PLAN-feat-planner-02-server.md#decisions)
The API prefix is `/api/` and the WebSocket path is `/ws`, with one typed WebSocket per tab. [decisions](../../../plans/DONE-PLAN-feat-planner-02-server.md#decisions)
The planning Origin rule is exact even on loopback, and the checklist arrives in `hello` and `checklist` deltas rather than a route. [decisions](../../../plans/DONE-PLAN-feat-planner-02-server.md#decisions)

## Interfaces later plans consume

Every wire field holding a document path is repo-relative and starts with `doc/`, and plan 3's request parameters are the only exception. [interfaces](../../../plans/DONE-PLAN-feat-planner-02-server.md#interfaces-later-plans-consume)
Stage `plan-server-core` exposes `loom::plan_server` with `start`, `run`, `PlanContext`, route modules and the WebSocket hub. [interfaces](../../../plans/DONE-PLAN-feat-planner-02-server.md#interfaces-later-plans-consume)

## Stages

Stage `web-server-extract` moves the dashboard server into `loom::web_server` behind a `Router` trait and leaves the dashboard's behavior unchanged. [stages](../../../plans/DONE-PLAN-feat-planner-02-server.md#stages)
Stage `plan-server-core` builds the lock, runtime directory, read-only mode, access gate, authoring holder, WebSocket hub and shutdown hooks on that layer. [stages](../../../plans/DONE-PLAN-feat-planner-02-server.md#stages)

## Environment inventory

No new dependency is expected, and the criteria bind loopback sockets inside the host job without credentials. [environment](../../../plans/DONE-PLAN-feat-planner-02-server.md#environment-inventory)
