# Loom blockages

## Plan 05: test-integrity completion gate

- Symptom: concept-web passed host checks but completion rejected changes to assertions in `file-mode.test.tsx` and `tabs.test.ts` (fingerprint `bae5247c132c`).
- Recovery: the stage handled the integrity rejection and subsequently completed and merged. No operator override was applied by this monitor.
- Follow-up: inspect the integrity event and dispute records before changing detection; passing tests alone do not establish assertion integrity.

## Plan 05: adjudicator startup failure

- Symptom: integration-verify reported `SandboxSetupFailure` while spawning an adjudicator, naming the installed loom hook scripts. The visible status truncated the underlying error.
- Recovery: a later adjudication session started successfully and the stage resumed. Other runtime maintenance was in progress; the exact repair is not established by this monitor.
- Follow-up: preserve the complete startup error in operator status and keep resolved issues out of current attention.

## Plan 05: generated asset merge conflict

- Symptom: integration-verify could not merge `web/dist/assets/store.css` after the target advanced concurrently.
- Recovery: Loom's merge resolver regenerated dist from merged sources. Its resolution commit `56bf9bf0` reached the target. The stage later appeared completed with an assumed merge after operator recovery.
- Follow-up: inspect why pending-merges output still reports the conflict while the completed stage permits knowledge-distill to run.

## Plan 05: orchestrator stalled in sync

- Symptom: status reported no tick for 187 seconds, stuck in sync, during merge resolution.
- Recovery: the monitor stopped the daemon through `loom stop`. Restart initially refused because another agent had uncommitted tracked work. The operator later resumed orchestration.
- Follow-up: identify the blocking sync operation; make recovery account for a dirty operator checkout without committing another agent's unfinished work.

## Plan 05: target guard after external landing

- Symptom: target guard held `feat-planner-ui` after external commits carried integration-verify's work (`ec0f59e0` to `99c9894f`).
- Recovery: reviewed the commit list and changed paths, then accepted the current target with `loom target accept --to 99c9894f2ffe19f36d90f24f378c38a4c7c5acd5`. A later status showed the target in sync; the stage's earlier merge-blocked explanation remained visible.
- Follow-up: ensure accepting a target reliably reconciles affected merge state and current attention without requiring an unsafe completion override.
