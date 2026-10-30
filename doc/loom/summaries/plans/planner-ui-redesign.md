---
kind: document
of: doc/plans/DONE-PLAN-planner-ui-redesign.md
file: sha256:3b1ce8864049dbb0755debcefc5df52eef09bd1bca6ae86ce1f97f21caa5395d
cites:
  "doc/plans/DONE-PLAN-planner-ui-redesign.md#1-features-the-document-model": "sha256:a8d38e06802cd2c4a995ecd969fd3084054dd6d0de5760885f8bf24a7172b444"
  "doc/plans/DONE-PLAN-planner-ui-redesign.md#2-screens-and-navigation": "sha256:3ebaa79bdc54fbf6fdadab8bb2243bd0bd50871e438498fd0df3778913a3cfa1"
  "doc/plans/DONE-PLAN-planner-ui-redesign.md#4-typography-locked": "sha256:13d74093216fa47dd4080fa5f2f5312c0d310dcd0a5f9825218ae5097e9fa615"
  "doc/plans/DONE-PLAN-planner-ui-redesign.md#6-reading-interactions-locked": "sha256:d20acab901361d0792f4e804df23c54d248cfe8800b03c8004cd7d6ad731e939"
  "doc/plans/DONE-PLAN-planner-ui-redesign.md#8-new-feature-the-kickoff-conversation-d10-d11": "sha256:57ac7db9ba55827cd8da1cebf32b8b94fdbd53f8f622c73a5d355b91117900de"
  "doc/plans/DONE-PLAN-planner-ui-redesign.md#draft-status": "sha256:4af8317f55bb658e46a01bb39576e350e2571c2e2bb1b16050874fcaf6344e66"
  "doc/plans/DONE-PLAN-planner-ui-redesign.md#goals": "sha256:14ecaa5696522c0263fe3ec917400e4610342e19eded8bcdb8e74e2320e06ba8"
  "doc/plans/DONE-PLAN-planner-ui-redesign.md#open-decisions": "sha256:04b0e86f81f7c81a7e1f236defdbae97e28e4bb280bb498e6e384d071c48c47f"
  "doc/plans/DONE-PLAN-planner-ui-redesign.md#problem": "sha256:7d03ddd3504bd4aa195682f8c99b012d7afac1ea30640726bc9b29c3d8d32d21"
  "doc/plans/DONE-PLAN-planner-ui-redesign.md#spec-changes-before-running": "sha256:63ef5109b4da1f86cb1c265c1b729763a624d7f466c2ab8670cdfa6dbcdd40b1"
  "doc/plans/DONE-PLAN-planner-ui-redesign.md#stages": "sha256:0d9039081fb5e99610e8452889d2a07efd1be6a61d3d6a9496e4e3bbac28ca0c"
---

# Planner UI redesign and features

## Problem

Plans 1-11 deliver every planner capability, but the app does not read as one product with the dashboard and two system rules fail on a real repository. [Problem](../../../plans/DONE-PLAN-planner-ui-redesign.md#problem)

Every unlisted plan becomes its own offer, so the home screen of this repository shows about 40 identical buttons and the engineering overview mixes two programmes. [Problem](../../../plans/DONE-PLAN-planner-ui-redesign.md#problem)

## Goals

The planner and the dashboard should share one header, icon set, theme controls, settings and help, and a repository should hold several features, each with its own vision, spec, metaplan, plans and overviews. [Goals](../../../plans/DONE-PLAN-planner-ui-redesign.md#goals)

A user should be able to start a feature from the browser and choose guided or draft-all spec writing. [Goals](../../../plans/DONE-PLAN-planner-ui-redesign.md#goals)

## Design

A feature is a name shared by a product directory and a plans directory, and the top level of the plans directory holds only standalone plans. [Features](../../../plans/DONE-PLAN-planner-ui-redesign.md#1-features-the-document-model)

The routes cover home, feature, product, engineering, files and kickoff screens, and a repository with exactly one feature opens directly on that feature. [Screens](../../../plans/DONE-PLAN-planner-ui-redesign.md#2-screens-and-navigation)

Chrome and titles use Inter, reading text uses Source Serif 4, and code uses Commit Mono Cosmix, with Literata and Piazzolla offered as alternative reading faces. [Typography](../../../plans/DONE-PLAN-planner-ui-redesign.md#4-typography-locked)

Sentence hover becomes an ink wash, and folding uses inline caps at both ends of each opened run. [Reading](../../../plans/DONE-PLAN-planner-ui-redesign.md#6-reading-interactions-locked)

Every new piece of work starts with a kickoff conversation that writes nothing under the docs until the user confirms the size and name. [Kickoff](../../../plans/DONE-PLAN-planner-ui-redesign.md#8-new-feature-the-kickoff-conversation-d10-d11)

Draft-all runs one authoring session that leads an agent team of up to five teammates, which coordinates while drafting and is followed by a lead consistency pass. [Kickoff](../../../plans/DONE-PLAN-planner-ui-redesign.md#8-new-feature-the-kickoff-conversation-d10-d11)

## Stages

The stages run feature-model with design-foundation first, then kickoff-session with reading-experience, then feature-ui with migrate-repo-docs, then document-ui and help, and finish with integration-verify and knowledge-distill. [Stages](../../../plans/DONE-PLAN-planner-ui-redesign.md#stages)

## Open decisions

The open decisions section lists none, since every decision was settled with the user, but the product spec files must be updated before the plan runs. [Open decisions](../../../plans/DONE-PLAN-planner-ui-redesign.md#open-decisions) [Spec changes](../../../plans/DONE-PLAN-planner-ui-redesign.md#spec-changes-before-running)

Before the plan is initialised, its metadata must be promoted to version 2 and plan 11 must finish first. [Draft status](../../../plans/DONE-PLAN-planner-ui-redesign.md#draft-status)
