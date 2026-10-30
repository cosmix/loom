---
kind: document
of: doc/product/feat-planner/section_09.md
file: sha256:05451da978de5d52685a4cba350ad5ed04db35b877aa99990f773f8088d8040d
cites:
  "doc/product/feat-planner/section_09.md#delivery": "sha256:a69079968adba7a69efd9ba592da5d447864043ff36be05e4af0f3b2936122f9"
  "doc/product/feat-planner/section_09.md#models-and-effort": "sha256:5127a3e8a37702ac8a5be6ff4bcc0e41abfb9e11b48856a87405e3de2e617b96"
  "doc/product/feat-planner/section_09.md#steps-and-their-skills": "sha256:262a0359c049425e57024ec6818a4ee401fc6aa829a5eecf63bbe9e3bddf0cb1"
  "doc/product/feat-planner/section_09.md#summaries-after-a-turn": "sha256:8b05d359f9c926fee85bb8a63a1186fedcb850fcafff4eab390ffe2f5c460bfe"
  "doc/product/feat-planner/section_09.md#summary-sessions": "sha256:890150c266cb74c402b7cc4d8d7af8cd2a8aed198baf35dcd8d18f9d518c7d2a"
  "doc/product/feat-planner/section_09.md#the-common-brief": "sha256:e2bf500fde4e4bbcd1ad85090b4eaaa336490fc7c15f34a3fc9b5443843fcc5b"
  "doc/product/feat-planner/section_09.md#the-model-in-one-paragraph": "sha256:5ee492fdced3a3bd05d2cb8f91fa96d71c30095f1a9a8d19653927bb0530f0d5"
  "doc/product/feat-planner/section_09.md#the-plugin": "sha256:57e2c6aab6b96aef20db0c3d5e9a57f3ba54c3a7699e4db1f908a8a962c9b6f0"
  "doc/product/feat-planner/section_09.md#the-users-claudemd-files-and-other-ambient-guidance": "sha256:3eb57413fb6c67af2074edfcba315993f9d4939c445edf7f9fe35a74dcf3924c"
  "doc/product/feat-planner/section_09.md#what-each-session-kind-gets": "sha256:c810e28bcb0c19668d164e6889b4d5a2ebee68ceead50756c7861afaa10d0406"
  "doc/product/feat-planner/section_09.md#write-limits": "sha256:d2dc6674717d8b9e9a9bb00224a1d8686cb58d8e8e352c7db60e5f626fe51a69"
---

# 09 — Session Skills and Guidance

## The model in one paragraph

Every planning session gets a brief appended to its system prompt, an opening message naming its step and target, and a plugin directory of planning skills and summary agents, all generated at launch. [The model in one paragraph](../../../../product/feat-planner/section_09.md#the-model-in-one-paragraph)

## What each session kind gets

Authoring sessions get the common brief with a starting point, summary sessions get the summary brief, and refinement subagents get their agent definition. [What each session kind gets](../../../../product/feat-planner/section_09.md#what-each-session-kind-gets)

## Delivery

The brief travels through `--append-system-prompt-file`, the step and target through the positional prompt, and the plugin through `--plugin-dir` for that session only. [Delivery](../../../../product/feat-planner/section_09.md#delivery)

Installing into `~/.claude` and editing any `CLAUDE.md` or settings file are channels the section rejects. [Delivery](../../../../product/feat-planner/section_09.md#delivery)

## The plugin

The plugin is generated per session from sources in `plan-plugin/`, embedded in the binary and written to the session directory at launch. [The plugin](../../../../product/feat-planner/section_09.md#the-plugin)

Plugin skills are namespaced as `loom-plan`, so they cannot collide with the user's own skills. [The plugin](../../../../product/feat-planner/section_09.md#the-plugin)

## The common brief

Every authoring brief states what the session is, that the documents are the truth, the conventions, the write limits, the turn-end duty and the closing message. [The common brief](../../../../product/feat-planner/section_09.md#the-common-brief)

## Steps and their skills

The brainstorming skill is the shared discussion procedure, loaded by the vision, section and metaplan steps rather than offered as a step of its own. [Steps and their skills](../../../../product/feat-planner/section_09.md#steps-and-their-skills)

## Write limits

Writes are limited to `doc/`, and locked `IN_PROGRESS-`, `DONE-` and `RETIRED-` plans and their archives are read-only. [Write limits](../../../../product/feat-planner/section_09.md#write-limits)

A planning session never stages, commits, branches, or runs `loom init`, `loom run` or `loom pressure`. [Write limits](../../../../product/feat-planner/section_09.md#write-limits)

## Summaries after a turn

After a turn that edits documents, the session runs `loom plan check` and rebuilds the affected summaries in two waves, document summaries first and then overviews. [Summaries after a turn](../../../../product/feat-planner/section_09.md#summaries-after-a-turn)

## Summary sessions

A background summary session never asks the user anything, rebuilds only the ids it is given, and runs on the overview model. [Summary sessions](../../../../product/feat-planner/section_09.md#summary-sessions)

## Models and effort

Spec steps default to medium effort and plan steps to high effort, and each `[plan]` key pairs a model key with an effort key for its role. [Models and effort](../../../../product/feat-planner/section_09.md#models-and-effort)

## The user's `CLAUDE.md` files and other ambient guidance

Planning sessions load the user's and the project's `CLAUDE.md` files as any Claude Code session does, and the session follows the brief's paths where they disagree. [The user's `CLAUDE.md` files and other ambient guidance](../../../../product/feat-planner/section_09.md#the-users-claudemd-files-and-other-ambient-guidance)
