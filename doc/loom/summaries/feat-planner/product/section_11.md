---
kind: document
of: doc/product/feat-planner/section_11.md
file: sha256:176ed688a6a9d30e6047681aec07e805eed011cf1c20cc7d8d072122dcdc69fd
cites:
  "doc/product/feat-planner/section_11.md#abandoning-an-unfinished-run": "sha256:1001cddce7273e75f364cd685285b5b1bbbfc1b618be79fb7e66cc0d3b7919c8"
  "doc/product/feat-planner/section_11.md#handing-off-to-the-dashboard": "sha256:9337721bd654140bd736ecc692b36e5e61aa5348f4148d41a2970216f5e56cf3"
  "doc/product/feat-planner/section_11.md#how-the-server-runs-it": "sha256:972dd1e58f39753bd45ce525191bcda51c5e7c542cc549e1790ddbcb511b924d"
  "doc/product/feat-planner/section_11.md#starting-a-run": "sha256:6d4463a578869a401ad4259fff938f52193cd0dcd20860e122bb6476c0d511a4"
  "doc/product/feat-planner/section_11.md#stopping-failure-and-server-stops": "sha256:f7cba4499041c1235563877f41f0c48e92489e4017811357fa46e17a2502a9a9"
  "doc/product/feat-planner/section_11.md#the-dialog": "sha256:70c9b47aa37a1f67b8f34c0f8cdf8203caf61dd0f5abe99bdc0670640dd1bca1"
  "doc/product/feat-planner/section_11.md#the-model-in-one-paragraph": "sha256:40284a3f463bc1cadd901c4404c677d98fc69f290529746c5ec2b719706dc368"
  "doc/product/feat-planner/section_11.md#the-pressure-record-a-change-to-loom-pressure": "sha256:5d4affa6985376bac471edf0c95282f35586e843c96ee4710dba2a428703e27a"
  "doc/product/feat-planner/section_11.md#the-run-dialog": "sha256:0d86a7cde3e8aa194a6a2c959cb0de3bae0af29f2f4d8f3a408db432222546f1"
  "doc/product/feat-planner/section_11.md#the-workspaces-state": "sha256:1f9fedc0bf27346660a54213181c7d2d37a3d21c29d3360c1a59d0b94c22d10f"
  "doc/product/feat-planner/section_11.md#what-loom-requires": "sha256:5f5dddfa6efa5a86fd45dab8f35ca8905974a1759d27454ee0259ede4abc2dad"
  "doc/product/feat-planner/section_11.md#what-the-chat-shows": "sha256:baa22fe6eea0402eba19522ae1575f43502ffe76ddc8029b846a5589b23738f1"
  "doc/product/feat-planner/section_11.md#where-it-is-offered": "sha256:2b83c81e909b2574c853f73ed26748b50ef0b0ddc595851d0802bcd31415f09b"
  "doc/product/feat-planner/section_11.md#while-plans-run": "sha256:f61a6ceda05d38a1bcb8029f2534621bd7183fb1174bff09dddadb68f3b9cc08"
---

# 11 — Handoff to Execution

## The model in one paragraph

Pressure testing and starting a run are buttons the user presses, never automatic. [model](../../../../product/feat-planner/section_11.md#the-model-in-one-paragraph)
A pressure test runs the same pipeline as `loom pressure`, with the server as the driver, whose Claude steps are tmux sessions that the chat shows and whose Codex step is a background process. [pipeline](../../../../product/feat-planner/section_11.md#how-the-server-runs-it)
Starting a run executes `loom init` and `loom run` as child processes after the app shows what they require. [start](../../../../product/feat-planner/section_11.md#starting-a-run)

## Pressure testing from the app

The app offers a pressure test only for written plans, and disables it when the Codex CLI or its pressure skill is missing, the server is read-only, or the browser does not hold authoring, or another browser does. [offer](../../../../product/feat-planner/section_11.md#where-it-is-offered)
The dialog sets the rounds, default 2, and each step's model and effort for the run. [dialog](../../../../product/feat-planner/section_11.md#the-dialog)
A pressure run takes the project's authoring slot and holds it until the run ends. [slot](../../../../product/feat-planner/section_11.md#the-dialog)
Each round deletes the Codex report, runs Claude `/pressure` and Codex `$pressure` at once, then Claude `/address`, with each Claude step a pressure session through the session bridge. [rounds](../../../../product/feat-planner/section_11.md#how-the-server-runs-it)
The chat shows each step's turns, the Codex status, and the plan edits, and its input box stays open to the browser holding authoring. [chat](../../../../product/feat-planner/section_11.md#what-the-chat-shows)
Stopping the run, a failed step, or a lost holder ends it the same way, with sessions torn down and no record written. [stops](../../../../product/feat-planner/section_11.md#stopping-failure-and-server-stops)
When a run completes, the driver appends one line to the plan's `## Pressure tests` section with the date, models, round count, and outcome. [record](../../../../product/feat-planner/section_11.md#the-pressure-record-a-change-to-loom-pressure)

## Starting a run

`loom init` and `loom run` refuse unless the plan and its briefs are committed and no tracked file is modified or staged, so the app shows these requirements first. [requirements](../../../../product/feat-planner/section_11.md#what-loom-requires)
The offer that appears depends on the state of `.loom/work` and the plan's file name. [offer state](../../../../product/feat-planner/section_11.md#the-workspaces-state)
Abandoning an unfinished run renames the plan to a `RETIRED-PLAN-` file in its `archive/` folder only after `loom clean` has removed the workspace and worktrees. [abandon](../../../../product/feat-planner/section_11.md#abandoning-an-unfinished-run)
The run dialog commits only changes under `doc/` when the user chooses to, and blocks the run on modified or staged files elsewhere. [run dialog](../../../../product/feat-planner/section_11.md#the-run-dialog)
After a successful start the app offers "Open dashboard", and the first open starts `loom status --web` as the server's own child process, while the planning app shows plan states from file names and a one-line run summary. [dashboard](../../../../product/feat-planner/section_11.md#handing-off-to-the-dashboard) [states](../../../../product/feat-planner/section_11.md#while-plans-run)
