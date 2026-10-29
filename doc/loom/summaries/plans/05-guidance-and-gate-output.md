---
kind: document
of: doc/plans/archive/DONE-PLAN-05-guidance-and-gate-output.md
file: sha256:5f871a79c53179046c386e46ff05f0473fdf994e3e63ff9436667db9098ca024
cites:
  "doc/plans/archive/DONE-PLAN-05-guidance-and-gate-output.md#1-guidance-and-gate-output-standard": "sha256:2eb5b417374c52bfc8b83b2bdd6cfa2d57e6da0dfc392d903611dc6ea876f06a"
  "doc/plans/archive/DONE-PLAN-05-guidance-and-gate-output.md#2-integration-verify": "sha256:6934f3772b5ea5cc1ca31436067756d539db688a18cbd1fee28944f9e9bda408"
  "doc/plans/archive/DONE-PLAN-05-guidance-and-gate-output.md#3-knowledge-distill": "sha256:5a5ec7f5180034779a1a23b9aeff88c957f498088b0180bb912b5b0534ef7c23"
  "doc/plans/archive/DONE-PLAN-05-guidance-and-gate-output.md#baselines-history-2026-10-18-6fee4ecd-re-measured-by-the-stage-at-its-base": "sha256:64ee91c0734ca5de1af1653d90c93b6a03022592cf36fcb3f7f13a20d6d0050a"
  "doc/plans/archive/DONE-PLAN-05-guidance-and-gate-output.md#before-loom-run-operator": "sha256:0e0aea676fe33788138299ba312c95f3f6269df4212e6047374fc2e01f49d317"
  "doc/plans/archive/DONE-PLAN-05-guidance-and-gate-output.md#deviations-from-the-metaplan-the-tree-wins": "sha256:319d0e5a93fe7f4ae75427eb152171dcd9f33b4b5865844f5786c97cb409ff74"
  "doc/plans/archive/DONE-PLAN-05-guidance-and-gate-output.md#goals": "sha256:a63ede25593b1cd61dafe0dd68dcebf033b92c23b2e2fb5cb05089641b20ccc2"
  "doc/plans/archive/DONE-PLAN-05-guidance-and-gate-output.md#overview": "sha256:d6d42c17d8ce58f427e646a1b12785c958b80bb394d285621ca686dc2caca65d"
  "doc/plans/archive/DONE-PLAN-05-guidance-and-gate-output.md#shared-files-and-what-earlier-plans-change": "sha256:95054f5584e21fa631057531fde1f0138e5e47c8e72c7c0da646712bfa15d9ff"
---

# Guidance and gate output

## Scope

Plan 5 of the token-optimization programme changes bytes only, with evidence class 1 and no paired runs. [Overview](../../../plans/archive/DONE-PLAN-05-guidance-and-gate-output.md#overview)
Template Rule 14 tells a session to log verbose output and print the command's own exit code, never to pipe the command itself, and never to redirect a loom command. [Goals](../../../plans/archive/DONE-PLAN-05-guidance-and-gate-output.md#goals)
Template Rules 3 and 3b shrink to one line each because the hooks already print both procedures. [Overview](../../../plans/archive/DONE-PLAN-05-guidance-and-gate-output.md#overview)
The orchestration skill drops the paragraphs that the template or the stage signal's stable prefix already carry, and it gains a size ceiling. [Overview](../../../plans/archive/DONE-PLAN-05-guidance-and-gate-output.md#overview)
A stage main session that changes more than 20 lines or more than 2 files itself gets one advisory through additionalContext, and the Go skill stops qualifying on Rust prompts. [Goals](../../../plans/archive/DONE-PLAN-05-guidance-and-gate-output.md#goals)
The plan adds no command, flag, hook script, hook registration, config key or telemetry event, and it changes no effort, ceiling or delegation rule. [Goals](../../../plans/archive/DONE-PLAN-05-guidance-and-gate-output.md#goals)

## Dependencies and shared files

The canonical run order places this plan after plans 3, 1, 2, B, gate-dedup and 4, and plan B must be installed before its loom run. [Overview](../../../plans/archive/DONE-PLAN-05-guidance-and-gate-output.md#overview)
The manifest freezes template headings and bullets as they stand at this plan's base, because earlier plans edit the shared files first. [Shared files and what earlier plans change](../../../plans/archive/DONE-PLAN-05-guidance-and-gate-output.md#shared-files-and-what-earlier-plans-change)

## Baselines

The 6fee4ecd byte counts are history, and the stage re-measures every number at its own base before spawning. [Baselines: history (2026-10-18, 6fee4ecd), re-measured by the stage at its base](../../../plans/archive/DONE-PLAN-05-guidance-and-gate-output.md#baselines-history-2026-10-18-6fee4ecd-re-measured-by-the-stage-at-its-base)

## Operator steps and stages

The operator merges and installs the prerequisite plans, commits the plan and its briefs, and confirms the codex lane before loom run. [Before `loom run` (operator)](../../../plans/archive/DONE-PLAN-05-guidance-and-gate-output.md#before-loom-run-operator)
The single standard stage guidance-and-gate-output runs two waves, with the two codex units forwarded first in the foreground and three Claude workers after them. [1. guidance-and-gate-output (standard)](../../../plans/archive/DONE-PLAN-05-guidance-and-gate-output.md#1-guidance-and-gate-output-standard)
The integration-verify stage runs the full gate with the hook suites and records the signal replay and the template and skill byte sizes against this plan's base. [2. integration-verify](../../../plans/archive/DONE-PLAN-05-guidance-and-gate-output.md#2-integration-verify)
The knowledge-distill stage records the six hard stops' enforcement matrix and updates the README's delegation paragraph. [3. knowledge-distill](../../../plans/archive/DONE-PLAN-05-guidance-and-gate-output.md#3-knowledge-distill)

## Deviations

The Go trigger list adds mutex and echo as phrases, and no verifier recipe is built because gate-dedup removed the verifier. [Deviations from the metaplan (the tree wins)](../../../plans/archive/DONE-PLAN-05-guidance-and-gate-output.md#deviations-from-the-metaplan-the-tree-wins)
The unfiltered test suite runs only in integration-verify, and the stage acceptance keeps every other pre-push step except cargo audit. [Deviations from the metaplan (the tree wins)](../../../plans/archive/DONE-PLAN-05-guidance-and-gate-output.md#deviations-from-the-metaplan-the-tree-wins)
