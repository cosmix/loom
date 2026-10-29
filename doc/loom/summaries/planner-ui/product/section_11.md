---
kind: document
of: doc/product/planner-ui/section_11.md
file: sha256:f7fc74de258cedfae19a63c7a034722665aa4ba4e0043c8357b6fe6d7a132fc2
cites:
  "doc/product/planner-ui/section_11.md#abandoning-an-unfinished-run": "sha256:9af0980f9b5b6a283060f07d869e74f2cef46615c4ab069e47b589a0facc5030"
  "doc/product/planner-ui/section_11.md#after-plans-complete": "sha256:82dbce054871f0c6a5afbee0d4e3eedf541b0f83362ba9a42dcf70762f205581"
  "doc/product/planner-ui/section_11.md#handing-off-to-the-dashboard": "sha256:9337721bd654140bd736ecc692b36e5e61aa5348f4148d41a2970216f5e56cf3"
  "doc/product/planner-ui/section_11.md#how-the-server-runs-it": "sha256:73c6af31ded16a77fdffcd24cb1af44aaf768ece9ad1b80c2701201963b0427b"
  "doc/product/planner-ui/section_11.md#stopping-failure-and-server-stops": "sha256:f7cba4499041c1235563877f41f0c48e92489e4017811357fa46e17a2502a9a9"
  "doc/product/planner-ui/section_11.md#the-dialog": "sha256:e23905a801ed2986e7c8455eadb59c8179a0a7ae335d575d2e6eae289aa9f47f"
  "doc/product/planner-ui/section_11.md#the-model-in-one-paragraph": "sha256:40284a3f463bc1cadd901c4404c677d98fc69f290529746c5ec2b719706dc368"
  "doc/product/planner-ui/section_11.md#the-pressure-record-a-change-to-loom-pressure": "sha256:4d1c7091f70d69a5b26df8846b159508e4c128f77bbee0815636bd396d20ec3b"
  "doc/product/planner-ui/section_11.md#the-run-dialog": "sha256:0d86a7cde3e8aa194a6a2c959cb0de3bae0af29f2f4d8f3a408db432222546f1"
  "doc/product/planner-ui/section_11.md#the-workspaces-state": "sha256:8a08daa89a09c274c68dc08e8838babd133c3da002e1a5348f63ce9c9092bdd3"
  "doc/product/planner-ui/section_11.md#what-loom-requires": "sha256:c96a0ecf6eda09b87c8d6e4640067de2fb86bac942d3b4421f3256483826e992"
  "doc/product/planner-ui/section_11.md#where-it-is-offered": "sha256:3b247da04d57b618fba25fa90fe07cbe8688ea18e29abbb9138936e33a400b74"
  "doc/product/planner-ui/section_11.md#while-plans-run": "sha256:a3172fb0c7f8e9acbee03cf39f269e8c2cd339d4d0de0bb9271a688d406a8753"
---

# Handoff to Execution

## Pressure testing from the app

The app runs a pressure test with the same pipeline as `loom pressure`, with the server as driver, and the driver appends one record line to the plan's `## Pressure tests` section [The model in one paragraph](../../../../product/planner-ui/section_11.md#the-model-in-one-paragraph)

A pressure test is offered only for plans in the written state, and only one pressure run may be active per project [Where it is offered](../../../../product/planner-ui/section_11.md#where-it-is-offered)

The offer is disabled when the `codex` CLI or Codex's `pressure` skill is missing, when the server is read-only, or when the browser does not hold authoring [Where it is offered](../../../../product/planner-ui/section_11.md#where-it-is-offered)

The dialog sets the plan, the round count with a default of 2, and the three models, and it says that the run edits the plan and writes `planning-omissions.md` [The dialog](../../../../product/planner-ui/section_11.md#the-dialog)

A pressure run holds the project's authoring slot until the run ends, and starting one first ends any open authoring chat [The dialog](../../../../product/planner-ui/section_11.md#the-dialog)

The server runs the Claude `/pressure` and `/address` steps as pressure sessions in tmux and runs Codex as a background child process [How the server runs it](../../../../product/planner-ui/section_11.md#how-the-server-runs-it)

A stop, a holder's absence, a failed step, or Ctrl-C ends the run without a record, and the plan keeps the edits its steps made [Stopping, failure, and server stops](../../../../product/planner-ui/section_11.md#stopping-failure-and-server-stops)

The pressure record is one line with the date, the models, the round count and the outcome, appended after the metadata end marker [The pressure record](../../../../product/planner-ui/section_11.md#the-pressure-record-a-change-to-loom-pressure)

## Starting a run

Starting a run checks the requirements that `loom init` and `loom run` enforce, so the user sees every problem at once [What loom requires](../../../../product/planner-ui/section_11.md#what-loom-requires)

The run offer depends on the state of `.loom/work` and on the plan's file name prefix [The workspace's state](../../../../product/planner-ui/section_11.md#the-workspaces-state)

Abandoning a run takes two confirmations when unmerged branches exist, and the plan is renamed only after the workspace and worktrees are gone [Abandoning an unfinished run](../../../../product/planner-ui/section_11.md#abandoning-an-unfinished-run)

The Start button runs `loom init` and then `loom run` as child processes, and the first non-zero exit stops the sequence [The run dialog](../../../../product/planner-ui/section_11.md#the-run-dialog)

## Dashboard and completion

After a run starts, the app offers "Open dashboard", which runs `loom status --web` as a child process of `loom plan` [Handing off to the dashboard](../../../../product/planner-ui/section_11.md#handing-off-to-the-dashboard)

While plans run, the app shows plan states and a run strip, and leaves stage monitoring to the dashboard [While plans run](../../../../product/planner-ui/section_11.md#while-plans-run)

After plans complete, the app offers a run review, a new metaplan round, and the next plan [After plans complete](../../../../product/planner-ui/section_11.md#after-plans-complete)
