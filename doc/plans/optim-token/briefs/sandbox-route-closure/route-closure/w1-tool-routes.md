# W1: tool-route denies in every session capsule

Tier: sonnet (`loom-software-engineer`). Read `doc/plans/token-optim/briefs/sandbox-route-closure/common.md` first.

## Goal

Every session capsule's `permissions.deny` names the tools that run in the Claude Code process outside the Bash sandbox: `LSP`, `RemoteTrigger`, `CronCreate`, `PushNotification`, and `Workflow`. The one exception: the Stage session of a stage with `ultracode: true` keeps `Workflow`, because ultracode is a shipped feature that licenses that tool through the session's prompt.

## Files you own

- `src/orchestrator/terminal/native/session_settings/contents.rs`
- `src/orchestrator/terminal/native/session_settings.rs`
- `src/orchestrator/terminal/native/launch.rs`
- `src/orchestrator/terminal/native/tests_session_settings.rs` (one field line and three visibility edits only)
- `src/orchestrator/terminal/native/tests_capsule_contents.rs` (one field line only)
- `src/orchestrator/terminal/native/tests_capsule_cwd_grants.rs` (one field line only)
- `src/orchestrator/terminal/native/tests_capsule_interpreters.rs` (one field line only)
- `src/orchestrator/terminal/native/tests_capsule.rs`
- `src/orchestrator/terminal/native/tests_capsule_routes.rs` (new)
- `src/orchestrator/terminal/native/tests_launch_capsule.rs`
- `../skills/loom-refactoring/SKILL.md` (one sentence)

Read only: `src/orchestrator/terminal/native/tests_capsule_denies.rs` (the import pattern), `src/orchestrator/terminal/native/wrapper.rs` (`#[allow(clippy::too_many_arguments)]` precedent near line 67).

## Steps

1. **`contents.rs`.** Beside `RELAY_SCRIPT`, add:
   - `const SESSION_TOOL_DENIES: [&str; 4] = ["LSP", "RemoteTrigger", "CronCreate", "PushNotification"];`
   - `const WORKFLOW_TOOL: &str = "Workflow";`

   One doc comment above them states: these tools run in the Claude Code process, not under the Bash sandbox (the LSP plugins' servers index the worktree and can run its build scripts and proc macros; RemoteTrigger starts work off the machine; CronCreate and PushNotification act outside the session); they are bare tool names in `permissions.deny` because `--disallowedTools` is variadic and would consume the positional prompt (`build_claude_command`); `SendMessage`, `ListAgents`, `WebFetch` and `WebSearch` stay allowed (agent teams; the engineer agents list the web tools); `Workflow` is denied unless `CapsuleInputs::workflow_licensed`.

   Add the field `pub workflow_licensed: bool` to `CapsuleInputs` (after `hidden_sockets`), doc: "Whether the session may use the Workflow tool: true only for the Stage session of an `ultracode: true` stage (`launch::workflow_licensed`)."

   Add `fn capsule_deny_rules(inputs: &CapsuleInputs<'_>) -> Vec<String>`: `inputs.denies.edit` cloned, then every `SESSION_TOOL_DENIES` entry, then `WORKFLOW_TOOL` unless `inputs.workflow_licensed`. Inside `capsule_settings` replace exactly the line `extend_strings(&mut settings, &["permissions", "deny"], &denies.edit);` with `extend_strings(&mut settings, &["permissions", "deny"], &capsule_deny_rules(inputs));`. `capsule_settings` is at most 50 lines (42 after host verification removed its macOS daemon-socket block) and must not grow; `denies` is still used by the `deny_write` line above it. Mention the tool denies in `capsule_settings`'s doc comment.
2. **`session_settings.rs`.** Add `pub workflow_licensed: bool` to `CapsuleRequest` (doc as above) and pass `workflow_licensed: request.workflow_licensed,` into the `contents::CapsuleInputs` literal in `write_session_capsule`. Register the new test file beside the others at the bottom:

   ```rust
   #[cfg(test)]
   #[path = "tests_capsule_routes.rs"]
   mod tests_capsule_routes;
   ```

3. **`launch.rs`.** Add `fn workflow_licensed(kind: SessionType, stage: &Stage) -> bool { kind == SessionType::Stage && stage.ultracode }` with a doc comment saying it is the one predicate both the prompt keyword and the capsule's Workflow exception read, so they cannot disagree. In `initial_prompt`'s `SessionType::Stage` arm, replace `if stage.ultracode {` with `if workflow_licensed(kind, stage) {`. `write_capsule` gains a `stage: &Stage` parameter (eight parameters: put `#[allow(clippy::too_many_arguments)]` on it, as `wrapper.rs` does for `create_session_wrapper_script`) and sets `workflow_licensed: workflow_licensed(kind, stage),` in its `CapsuleRequest` literal. Pass `stage` at the call in `prepare_session_launch_with`.
4. **Fixture literals.** Add `workflow_licensed: false,` to the `CapsuleRequest` literal in `tests_session_settings.rs::write_capsule` and to the `CapsuleInputs` literals in `tests_capsule_contents.rs::try_build`, `tests_capsule_cwd_grants.rs::capsule_in` and `tests_capsule_interpreters.rs::build_with_interpreters` (`rg -n 'CapsuleInputs \{|CapsuleRequest \{' src` lists these four plus the two production literals you edit in steps 2 and 3; re-run it and cover every hit). In `tests_session_settings.rs` also change the three `Checkout` field lines `work_dir: PathBuf,`, `scratch_root: PathBuf,` and `tmp_root: PathBuf,` to `pub(super) work_dir: PathBuf,` and so on, in place: step 6 builds its own request from them, and they are private today. Nothing else changes in those files.
5. **`tests_capsule.rs`.** `every_kind_gets_exactly_the_section_10_write_denies_for_its_location` compares `/permissions/deny` for equality, so it must learn the tool denies. Add `const TOOL_DENIES: &str = "LSP RemoteTrigger CronCreate PushNotification Workflow";` beside `SHARED_EDIT_DENIES` and change only the line `let mut edit = expand(&[SHARED_EDIT_DENIES, own_edit], &checkout);` (about line 344, followed by `edit.extend(skill_edits);`) to `let mut edit = expand(&[SHARED_EDIT_DENIES, own_edit, TOOL_DENIES], &checkout);`. `expand` splits each template on whitespace and substitutes only `{R}`, `{T}` and `{H}`, and both sides of the assertion are `BTreeSet`s, so bare tool names work and order does not matter. The file is 390 lines; your two added lines keep it under 400. Leave every `assert` line as it is.
6. **`tests_capsule_routes.rs` (new).** Module doc: the tool-route denies of every session kind's capsule. Import from `crate::orchestrator::terminal::native::tests_session_settings` the items `checkout`, `read_capsule`, `sandbox_with`, `strings`, `write_capsule`, `ALL_KINDS` (the pattern at the top of `tests_capsule_denies.rs`). Tests:
   - `every_kind_denies_the_tool_routes_outside_the_bash_sandbox`: for every kind in `ALL_KINDS` plus `SessionType::Contract` (that const lists five kinds and omits Contract; chain the sixth in the test, never edit the const) at both the checkout and the worktree, `/permissions/deny` holds each of `LSP`, `RemoteTrigger`, `CronCreate`, `PushNotification`, `Workflow`.
   - `the_agent_team_web_and_core_tools_stay_allowed`: for the same capsules, no `/permissions/deny` entry equals `SendMessage`, `ListAgents`, `WebFetch`, `WebSearch`, `Bash`, `Read`, `Edit` or `Agent`, and none starts with `Read(`. This is the control that fails an over-broad deny.
   - `a_licensed_stage_session_keeps_the_workflow_tool`: build one Stage capsule in the worktree through `super::write_session_capsule` with `workflow_licensed: true` (this file is a child of `session_settings`, so `super::{write_session_capsule, CapsuleRequest}` are in reach; write the `CapsuleRequest` literal from `tests_session_settings.rs::write_capsule`'s fields, with `checkout.work_dir`, `checkout.scratch_root.join(<session id>)` and `checkout.tmp_root` made visible in step 4; build the surfaces with `ControlSurfaces::new(&checkout.work_dir, Some(&checkout.scratch_root), &[], Some(&checkout.home))`, as `surfaces_for` does); assert no `/permissions/deny` entry equals `Workflow` and the four other names are present.
7. **`tests_launch_capsule.rs`.** Split `try_launch` so the new test reuses it without copying: `try_launch(fixture, kind)` keeps its signature and doc, builds its `stage-1` `Stage` as now, and returns `try_launch_stage(fixture, kind, &stage)`; the new `fn try_launch_stage(fixture: &Fixture, kind: SessionType, stage: &Stage) -> Result<(Session, String)>` holds the rest of today's body unchanged (cwd: the worktree for a Stage session, the repository root for every other kind). Every existing caller of `try_launch` stays as it is. Then add `an_ultracode_stage_session_launches_with_the_workflow_tool_and_no_other_kind_does`: three launches through `try_launch_stage` on `fixture()`: a Stage session of a `stage-1` stage with `ultracode: true` (its `/permissions/deny` has no `Workflow` entry), a Contract session of the same stage (it runs in the repository root, as `try_launch_stage` places it; it has a `Workflow` entry), and a Stage session of a `stage-1` stage with `ultracode: false` (it has a `Workflow` entry). Read each capsule with `capsule_of`. This test fails if `write_capsule` passes a constant instead of the predicate. Keep the file at or under 400 lines (357 at 6fee4ecd; the split adds about four).
8. **`../skills/loom-refactoring/SKILL.md`**, the line that starts "Prefer tool-driven moves when available": keep its advice and add one sentence: "Loom stage sessions deny the LSP tool, so in a stage use `rg` to find every reference and Edit each one." At 6fee4ecd `rg -n -w LSP ../skills ../agents ../CLAUDE.md.template` finds only that line (and a code comment, `src/fs/knowledge/mod.rs:3`, which describes Claude Code's tool set and stays). Plan 3 and gate-dedup edit the plan-writer skill and `agents/*.md` before this stage: re-run the same `rg` first. A hit that tells an agent to use the LSP tool in a file you do not own goes in your report, naming the file and line; the main agent records it with `loom memory note`.

## Traps

- `capsule_settings` must stay at most 50 lines (42 after host verification; this one-for-one replacement keeps it there) and `tests_session_settings.rs` at or under 400 (398 at 50aa8336 plus your one line; host verification removes about five).
- Never deny a tool by a `Read`-shaped entry, and never add `--disallowedTools` anywhere.
- `Workflow` follows the same predicate as the prompt keyword. A Contract, Merge, Knowledge or Adjudication session of an ultracode stage still gets the deny, since its prompt carries no keyword.
- Deny only the five names. Monitor, AppifactRepl, the self-hosted-runner tools, `EnterWorktree` and `ExitWorktree` stay allowed (common.md).

## Your one check

`cargo test --lib orchestrator::terminal::native::` from `loom/`, after `cargo fmt`. Report its summary line and `wc -l` of every file you changed.
