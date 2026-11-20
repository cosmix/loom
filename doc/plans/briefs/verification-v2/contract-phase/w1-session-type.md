# contract-phase / W1 — `SessionType::Contract` plumbing

Read first: `doc/plans/briefs/verification-v2/DESIGN.md` D0, D8 (first bullet). Knowledge:
`mistakes/adjudication-autonomy-deadlock.md` "Adoption Matched Sessions by stage_id Alone";
`architecture/terminal-backends.md` (the native and tmux sections).

Parallel workers depend on the names you add; they are pinned here and in their briefs:
`SessionType::Contract`, `Session::new_contract(stage_id: &str) -> Session`,
`SessionBackend::spawn_contract_session(&self, stage: &Stage, worktree: &Worktree, session: &Session, signal_path: &Path) -> Result<...>`
(same return type as `spawn_session`). W4 owns `relay/matrix.rs` and adds the `Contract` rows
there, so leave it alone.

## Files you own

`loom/src/models/session/types.rs`, `loom/src/models/session/methods.rs`,
`loom/src/orchestrator/terminal/backend.rs`, `loom/src/orchestrator/terminal/native/launch.rs`,
`loom/src/orchestrator/terminal/native/session_settings/contents.rs`,
`loom/src/orchestrator/session_registry.rs`, `loom/src/relay/emit/session_type.rs`,
`loom-hooks/commit-guard.sh`, `loom-hooks/tests/commit-guard-contract-session.sh` (new),
`loom-hooks/tests/run-all.sh`.

## Tasks

1. `models/session/types.rs:8-24`: add `Contract`; update the exhaustive `Display` (L26-36).
2. `models/session/methods.rs`: `new_contract(stage_id)` next to `new_knowledge` (L31-66);
   `derive_tracking_key` (L76-84) gives `loom-contract-<stage_id>`.
3. `terminal/native/launch.rs`:
   - `remote_control_session_name` (L32-46): a contract prefix;
   - `model_and_effort` (L110-126): `Contract` resolves exactly as `Stage`;
   - `initial_prompt` (L129-167): a contract line pointing at the signal file ("You are the
     contract test writer for stage <id>. Read your signal ...").
4. `terminal/backend.rs`: `spawn_contract_session`, built on the worktree-taking `spawn_session`
   shape (L144-156) through `dispatch_spawn` (L119-142), never `spawn_main_repo_session`. Check
   the tmux backend's spawn path accepts the new kind (find it via `loom map --find-all spawn_session`)
   and extend any per-kind naming there. If that means a tmux file, report it; the main agent
   decides ownership.
5. `session_settings/contents.rs` L121: `Contract` is brokered like `Stage` and `Knowledge`.
6. `session_registry.rs` L41-46: add `Contract` to `SESSION_KINDS`. While it runs, a contract
   session is the stage's agent, so orphan adoption must find it by its tracking key.
7. `relay/emit/session_type.rs`: parse `"contract"` ↔ `SessionType::Contract`. Confirm that the
   session wrapper exports `LOOM_SESSION_TYPE=contract` for a contract session. Find where the
   other kinds' values are exported (`rg -n LOOM_SESSION_TYPE loom/src`) and extend that site
   (report it if it is outside your files).
7a. `loom-hooks/commit-guard.sh` (the advisory Stop hook): when `LOOM_SESSION_TYPE` is
   `contract`, print the DESIGN D8 contract reminder instead of the commit-and-complete advice,
   and exit 0. Add `loom-hooks/tests/commit-guard-contract-session.sh` and register it in
   `run-all.sh`; follow an existing hook test's shape.
8. Grep every other `SessionType::` comparison listed below and decide each explicitly. Leave
   code unchanged where the default is right, and list each decision in your report:
   `stage_takedown.rs:373`, `commands/stage/state.rs:147`,
   `commands/stage/state/loop_recovery/mod.rs:108,143`, `orphan_adoption.rs:28` (all
   `!= Adjudication`: a Contract agent is treated like a stage agent, which is right);
   `merge_gate.rs:141` (`!= Stage`); `daemon/server/control_complete.rs:18`
   (`COMPLETING_SESSION_TYPES`: Contract must NOT complete a stage);
   `daemon/server/self_service.rs:89,296`.

## Tests

Unit tests beside each change: Display, tracking key, `model_and_effort` parity with `Stage`,
the session-type parse round trip. No named test is assigned to you.

## Proof (one command, once)

`cargo test --manifest-path loom/Cargo.toml --lib models::session::`

## Report

Files changed; the decision for each comparison site in task 8; any tmux file that needs
editing; the proof result.
