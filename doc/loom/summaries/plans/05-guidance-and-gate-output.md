---
kind: document
of: doc/plans/optim-token/archive/DONE-PLAN-05-guidance-and-gate-output.md
file: sha256:5f871a79c53179046c386e46ff05f0473fdf994e3e63ff9436667db9098ca024
cites:
  "doc/plans/optim-token/archive/DONE-PLAN-05-guidance-and-gate-output.md#before-loom-run-operator": "sha256:0e0aea676fe33788138299ba312c95f3f6269df4212e6047374fc2e01f49d317"
  "doc/plans/optim-token/archive/DONE-PLAN-05-guidance-and-gate-output.md#deviations-from-the-metaplan-the-tree-wins": "sha256:319d0e5a93fe7f4ae75427eb152171dcd9f33b4b5865844f5786c97cb409ff74"
  "doc/plans/optim-token/archive/DONE-PLAN-05-guidance-and-gate-output.md#goals": "sha256:a63ede25593b1cd61dafe0dd68dcebf033b92c23b2e2fb5cb05089641b20ccc2"
  "doc/plans/optim-token/archive/DONE-PLAN-05-guidance-and-gate-output.md#overview": "sha256:d6d42c17d8ce58f427e646a1b12785c958b80bb394d285621ca686dc2caca65d"
---

# plans/05-guidance-and-gate-output

The plan is plan 5 of the token-optimization programme and changes bytes only, with no paired runs. [Overview](../../../plans/optim-token/archive/DONE-PLAN-05-guidance-and-gate-output.md#overview)
Template Rule 14 tells a session to log verbose output and print the command's own exit code, and never to pipe or redirect a `loom` command. [Goals](../../../plans/optim-token/archive/DONE-PLAN-05-guidance-and-gate-output.md#goals)
Template Rules 3 and 3b shrink to one line each, and the orchestration skill drops paragraphs that the template or the stage's stable prefix already carry. [Overview](../../../plans/optim-token/archive/DONE-PLAN-05-guidance-and-gate-output.md#overview)
A test fails if the template or the skill regrows past its byte ceiling, or if a frozen template heading or rule bullet disappears without a recorded new home. [Goals](../../../plans/optim-token/archive/DONE-PLAN-05-guidance-and-gate-output.md#goals)
The Go skill stops qualifying on Rust prompts that use words such as "struct" and "map". [Goals](../../../plans/optim-token/archive/DONE-PLAN-05-guidance-and-gate-output.md#goals)
A stage main session that changes more than 20 lines or more than 2 files itself gets one advisory through `additionalContext`. [Goals](../../../plans/optim-token/archive/DONE-PLAN-05-guidance-and-gate-output.md#goals)
The plan adds no command, flag, hook script, hook registration, config key or telemetry event, and leaves effort, ceilings and the delegation rule unchanged. [Goals](../../../plans/optim-token/archive/DONE-PLAN-05-guidance-and-gate-output.md#goals)
The stage runs in two lanes, Claude and codex, with two single-file test units sent to codex terra. [Overview](../../../plans/optim-token/archive/DONE-PLAN-05-guidance-and-gate-output.md#overview)
Plans 3, 1, 2, plan B and gate-dedup must merge and be installed before this plan runs, and its before_stage checks fail fast when they have not. [Overview](../../../plans/optim-token/archive/DONE-PLAN-05-guidance-and-gate-output.md#overview)
The orchestrator runs `loom check` once itself and reads each failing criterion's `full log:` path rather than rerunning the criterion in a session shell. [Deviations](../../../plans/optim-token/archive/DONE-PLAN-05-guidance-and-gate-output.md#deviations-from-the-metaplan-the-tree-wins)
The Go triggers `mutex` and `echo` also become phrases, because they collide with Rust and the shell. [Deviations](../../../plans/optim-token/archive/DONE-PLAN-05-guidance-and-gate-output.md#deviations-from-the-metaplan-the-tree-wins)
No verifier recipe or gate-block edit is built, because gate-dedup already removed the verifier subagent. [Deviations](../../../plans/optim-token/archive/DONE-PLAN-05-guidance-and-gate-output.md#deviations-from-the-metaplan-the-tree-wins)
The advisory uses stdout `additionalContext`, since Claude Code passes a hook's stderr to the model only on exit 2. [Deviations](../../../plans/optim-token/archive/DONE-PLAN-05-guidance-and-gate-output.md#deviations-from-the-metaplan-the-tree-wins)
The unfiltered cargo test suite runs only in integration-verify, as a host job on the merged tree. [Deviations](../../../plans/optim-token/archive/DONE-PLAN-05-guidance-and-gate-output.md#deviations-from-the-metaplan-the-tree-wins)
Before `loom run`, the operator commits the plan and its briefs, runs `./dev-install.sh` after the merge, and checks that the codex lane is ready. [Before loom run](../../../plans/optim-token/archive/DONE-PLAN-05-guidance-and-gate-output.md#before-loom-run-operator)
