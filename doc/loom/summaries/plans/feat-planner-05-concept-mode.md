---
kind: document
of: doc/plans/feat-planner/archive/DONE-PLAN-feat-planner-05-concept-mode.md
file: sha256:688c7651c0c6579894bced32a4ee88c9336f56ded10c121118c2960a08b14647
cites:
  "doc/plans/feat-planner/archive/DONE-PLAN-feat-planner-05-concept-mode.md#cross-plan-interfaces": "sha256:432d7c2e0b58eb91bec54e34d69ee10865579cb02658d86e06d5eca2b18b4ecb"
  "doc/plans/feat-planner/archive/DONE-PLAN-feat-planner-05-concept-mode.md#decisions": "sha256:a750926414a96d864d90bab0894e9fa82cee9b1f47b41a3f67bbc8ca1dba81b3"
  "doc/plans/feat-planner/archive/DONE-PLAN-feat-planner-05-concept-mode.md#execution-diagram": "sha256:e389c298b8af57480d19c4df370325c6125fda5e309ffcd883831d9481217946"
  "doc/plans/feat-planner/archive/DONE-PLAN-feat-planner-05-concept-mode.md#goals": "sha256:8c6e36bf1f31c91dff4556d75fba6323d7511708c1982c6617f4df5cc550386b"
  "doc/plans/feat-planner/archive/DONE-PLAN-feat-planner-05-concept-mode.md#overview": "sha256:c0eaa11313cbc88cf3c34fc840c2c95b50ab8a62d87eca3cca3138f488599f50"
---

# Plan: `loom plan` concept mode

## Scope

The server side is one route module that drives plan 4's summary engine from the watcher and pushes a delta when a batch changes what a page shows. [Overview](../../../plans/feat-planner/archive/DONE-PLAN-feat-planner-05-concept-mode.md#overview)

The plan adds the Product and Engineering tabs, the summary pages under the `/product/` and `/engineering/` routes, and the document key with its colour chips. [Overview](../../../plans/feat-planner/archive/DONE-PLAN-feat-planner-05-concept-mode.md#overview)

Readers open an overview sentence to the summary sentences it cites and a summary sentence to its source passages, all from files under `doc/`. [Goals](../../../plans/feat-planner/archive/DONE-PLAN-feat-planner-05-concept-mode.md#goals)

Out-of-date marks turn dashed at once, arrivals flash, and open runs survive a rebuild when their source overlap holds. [Goals](../../../plans/feat-planner/archive/DONE-PLAN-feat-planner-05-concept-mode.md#goals)

Every rule is reachable by keyboard and screen reader, and colour is never the only signal. [Goals](../../../plans/feat-planner/archive/DONE-PLAN-feat-planner-05-concept-mode.md#goals)

The plan needs no session: until plan 10 lands, summaries come from a hand-run `loom plan summary seal`, and the empty-state offers render disabled. [Overview](../../../plans/feat-planner/archive/DONE-PLAN-feat-planner-05-concept-mode.md#overview)

## Decisions

The concept module's `install` creates the summary engine, keeps the latest `Evaluation`, and broadcasts one delta per batch that changed something. [Decisions](../../../plans/feat-planner/archive/DONE-PLAN-feat-planner-05-concept-mode.md#decisions)

The route returns each page fully resolved, so the browser computes nothing from summary files and the open-state rule runs on `rests_on` alone. [Decisions](../../../plans/feat-planner/archive/DONE-PLAN-feat-planner-05-concept-mode.md#decisions)

Each delta names the pages whose content changed, the documents whose summaries went out of date, and the source paths that changed, and an overview is only ever replaced by a whole page. [Decisions](../../../plans/feat-planner/archive/DONE-PLAN-feat-planner-05-concept-mode.md#decisions)

Passages come from plan 3's document route, so no second document-reading route exists. [Decisions](../../../plans/feat-planner/archive/DONE-PLAN-feat-planner-05-concept-mode.md#decisions)

Colours are assigned in overview scope order as `index % 8 + 1`, and the vision and metaplans have no chip, so their marks use colour 0. [Decisions](../../../plans/feat-planner/archive/DONE-PLAN-feat-planner-05-concept-mode.md#decisions)

View state lives in a jotai atom per page key, mirrored to `sessionStorage`, so the URL carries only the page. [Decisions](../../../plans/feat-planner/archive/DONE-PLAN-feat-planner-05-concept-mode.md#decisions)

The repository has no browser runner, so verification is a vitest test over recorded route fixtures plus a `curl` smoke test against a real server. [Decisions](../../../plans/feat-planner/archive/DONE-PLAN-feat-planner-05-concept-mode.md#decisions)

## Interfaces

The plan consumes plans 1 to 4 and publishes `concept::page_keys_for_summary`, `ConceptState`, and extension points that render nothing until plans 9 and 10 fill them. [Cross-plan interfaces](../../../plans/feat-planner/archive/DONE-PLAN-feat-planner-05-concept-mode.md#cross-plan-interfaces)

Stages run concept-data, then concept-web, then integration-verify, then knowledge-distill. [Execution Diagram](../../../plans/feat-planner/archive/DONE-PLAN-feat-planner-05-concept-mode.md#execution-diagram)
