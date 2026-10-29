---
kind: document
of: doc/plans/optim-token/archive/DONE-PLAN-01-doctrine-truth-and-plan-guards.md
file: sha256:bb5d07267678827c23dd849d7850c343fedb62d46ea5cae0d0a47e0a196f7c9f
cites:
  "doc/plans/optim-token/archive/DONE-PLAN-01-doctrine-truth-and-plan-guards.md#1-doctrine-truth-and-plan-guards-doctrine-and-guards": "sha256:1d25dd61761717f49871e7558579014a326257936ebeb46d74f981eb88c5bad8"
  "doc/plans/optim-token/archive/DONE-PLAN-01-doctrine-truth-and-plan-guards.md#2-integration-verification-integration-verify": "sha256:ed8bf50e6bb2c4a55e69cafdd894bbec00828e51fd88813ce052db320d615da9"
  "doc/plans/optim-token/archive/DONE-PLAN-01-doctrine-truth-and-plan-guards.md#3-knowledge-distillation-knowledge-distill": "sha256:d131d952aa91bb0fa2007547c36d5a70851773f86f59bf3e953de828906d66ae"
  "doc/plans/optim-token/archive/DONE-PLAN-01-doctrine-truth-and-plan-guards.md#baselines": "sha256:8fb6314fb9e40567849e92da347bb9e3835d879a15b6e269d668a9af1989d932"
  "doc/plans/optim-token/archive/DONE-PLAN-01-doctrine-truth-and-plan-guards.md#coverage-of-the-metaplan": "sha256:cb30f95f9814ef17c8043032b96d9ce4103445b24347bd54f136b033bea674a3"
  "doc/plans/optim-token/archive/DONE-PLAN-01-doctrine-truth-and-plan-guards.md#deviations-from-the-metaplan-the-tree-wins": "sha256:d7b87e93891a9c7c3eb7aea0e8baf6e2f70d4e5f1c46d19a64200853f8743625"
  "doc/plans/optim-token/archive/DONE-PLAN-01-doctrine-truth-and-plan-guards.md#goals": "sha256:99faa8a6ed6ab65ee06fc88d2fd881491f8d54dd4d3859c4bacb4b6c2492bb6e"
  "doc/plans/optim-token/archive/DONE-PLAN-01-doctrine-truth-and-plan-guards.md#overview": "sha256:8d68e7f98605d3557bbe74c8aa0b6f3801d6667f435f9cc35adae2615d4e9a33"
---

# Doctrine Truth and Plan Guards

## Overview

One implementation stage corrects the doctrine in the signal, hooks, template, README and two skills, deletes the dead memory-summary path, and adds four plan guards. [Overview](../../../plans/optim-token/archive/DONE-PLAN-01-doctrine-truth-and-plan-guards.md#overview)

The guards cover the bootstrap warning, two version-2 bookend lints, the stage model allowlist and the archive abort. [Overview](../../../plans/optim-token/archive/DONE-PLAN-01-doctrine-truth-and-plan-guards.md#overview)

## Goals

Integration-verify signals stop telling the agent to run loom knowledge update, and hooks stop telling implementation stages to curate knowledge. [Goals](../../../plans/optim-token/archive/DONE-PLAN-01-doctrine-truth-and-plan-guards.md#goals)

The found, gotcha and found/gotcha note prefixes become one class. [Goals](../../../plans/optim-token/archive/DONE-PLAN-01-doctrine-truth-and-plan-guards.md#goals)

A section annotated deprecated, superseded or historical is proven to leave default retrieval. [Goals](../../../plans/optim-token/archive/DONE-PLAN-01-doctrine-truth-and-plan-guards.md#goals)

An archive failure while journals exist stops plan completion, clean and init clean before they delete the state directory. [Goals](../../../plans/optim-token/archive/DONE-PLAN-01-doctrine-truth-and-plan-guards.md#goals)

No new command, flag, config key or telemetry event is added, and effort, ceilings and delegation do not change. [Goals](../../../plans/optim-token/archive/DONE-PLAN-01-doctrine-truth-and-plan-guards.md#goals)

## Deviations from the metaplan (the tree wins)

The stage model is shell-escaped, so the defect is a typo that fails at spawn and not an injection. [Deviations from the metaplan (the tree wins)](../../../plans/optim-token/archive/DONE-PLAN-01-doctrine-truth-and-plan-guards.md#deviations-from-the-metaplan-the-tree-wins)

The daemon still reports completion after an archive failure, because only the foreground run propagates the error. [Deviations from the metaplan (the tree wins)](../../../plans/optim-token/archive/DONE-PLAN-01-doctrine-truth-and-plan-guards.md#deviations-from-the-metaplan-the-tree-wins)

## Baselines

The hook suite registers 102 tests, and the knowledge check fails on three issues at the recorded revision. [Baselines](../../../plans/optim-token/archive/DONE-PLAN-01-doctrine-truth-and-plan-guards.md#baselines)

## Stages

The single standard stage doctrine-and-guards carries every code and doctrine change through wave-ordered workers. [1. Doctrine truth and plan guards (`doctrine-and-guards`)](../../../plans/optim-token/archive/DONE-PLAN-01-doctrine-truth-and-plan-guards.md#1-doctrine-truth-and-plan-guards-doctrine-and-guards)

Integration-verify runs the full gate, the hook suites, the review and the functional checks. [2. Integration verification (`integration-verify`)](../../../plans/optim-token/archive/DONE-PLAN-01-doctrine-truth-and-plan-guards.md#2-integration-verification-integration-verify)

Knowledge-distill is a single agent that makes the knowledge check green and rewrites the sections this plan made false. [3. Knowledge distillation (`knowledge-distill`)](../../../plans/optim-token/archive/DONE-PLAN-01-doctrine-truth-and-plan-guards.md#3-knowledge-distillation-knowledge-distill)

## Coverage of the metaplan

Each metaplan item maps to a worker, a contract or a distill step. [Coverage of the metaplan](../../../plans/optim-token/archive/DONE-PLAN-01-doctrine-truth-and-plan-guards.md#coverage-of-the-metaplan)
